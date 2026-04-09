;;; nothelix.scm - Jupyter notebooks for Helix
;;;
;;; Commands:
;;;   :convert-notebook - Convert .ipynb to cell format (Rust, non-blocking)
;;;   :execute-cell     - Run current cell
;;;   :next-cell        - Jump to next cell
;;;   :previous-cell    - Jump to previous cell
;;;   :cell-picker      - Interactive cell navigation

;; Helix core imports
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/keymaps.scm")
(require "helix/configuration.scm")
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))
(require-builtin helix/core/text as text.)
(require-builtin helix/core/keymaps as helix.keymaps.)
(require (prefix-in helix. "helix/commands.scm"))

;; FFI import for shell-free utilities
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          resolve-symlink-dir
                          ensure-lsp-environment
                          lsp-environment-ready
                          lsp-project-dir
                          lsp-depot-dir))

;;; ============================================================================
;;; LSP ENVIRONMENT SETUP
;;; ============================================================================
;;; On first load, spawn Julia in the background to set up the minimal LSP
;;; environment (LanguageServer only, isolated depot). The editor is never
;;; blocked — Julia runs concurrently and the LSP becomes available once
;;; the environment is ready.

(define lsp-setup-result (ensure-lsp-environment))
(when (not (string=? lsp-setup-result ""))
  (displayln (string-append "nothelix: LSP setup failed: " lsp-setup-result)))

;; Nothelix modules (order matters: common first, then leaf modules)
(require "nothelix/string-utils.scm")
(require "nothelix/debug.scm")
(require "nothelix/common.scm")
(require "nothelix/graphics.scm")
(require "nothelix/kernel.scm")
(require "nothelix/conversion.scm")
(require "nothelix/navigation.scm")
(require "nothelix/execution.scm")
(require "nothelix/selection.scm")
(require "nothelix/picker.scm")
(require "nothelix/chart-viewer.scm")
(require "nothelix/backslash.scm")
(require "nothelix/conceal-state.scm")
(require "nothelix/conceal.scm")

;; Test modules are loaded dynamically (see test commands below).

(provide convert-notebook sync-to-ipynb
         execute-cell execute-all-cells execute-cells-above cancel-cell
         next-cell previous-cell cell-picker
         select-cell select-cell-code select-output
         view-chart
         kernel-shutdown kernel-shutdown-all
         graphics-protocol graphics-check nothelix-status
         julia-tab-complete
         conceal-math clear-conceal
         nothelix-debug-on nothelix-debug-off nothelix-debug-toggle
         run-all-tests run-cell-tests run-kernel-tests run-execution-tests)

;;; ============================================================================
;;; CONCEAL — thin shim for backwards-compatible provided names
;;; ============================================================================
;;;
;;; All conceal logic lives in nothelix/conceal.scm. These aliases keep the
;;; top-level provide list stable so users of the plugin don't need to
;;; re-import anything.

(define (conceal-math) (conceal-math!))
(define (clear-conceal) (clear-conceal!))

;;; ============================================================================
;;; DEBUG MODE — thin command shims for the `nothelix/debug.scm` module
;;; ============================================================================
;;;
;;; When toggled on, modules across the plugin start emitting
;;; `nothelix: …` lines via `debug-log`, which are routed to the
;;; helix log at info level. Off by default.

(define (nothelix-debug-on) (nothelix-debug-enable!))
(define (nothelix-debug-off) (nothelix-debug-disable!))
(define (nothelix-debug-toggle) (nothelix-debug-toggle!))

;;; ============================================================================
;;; KERNEL LIFECYCLE
;;; ============================================================================

;;@doc
;; Shutdown the kernel for the current document
(define (kernel-shutdown)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (if path
      (stop-kernel path)
      (set-status! "No document path")))

;;@doc
;; Shutdown all running kernels
(define (kernel-shutdown-all)
  (stop-all-kernels))

;;; ============================================================================
;;; KEYBINDINGS
;;; ============================================================================

;; Register extension-specific keybindings for notebook files
;; NOTE: The keymap macro's (inherit-from ...) has a bug in helix keymaps.scm:216-218
;; (pattern uses 'map' but body uses 'kmap'), so we call the functions directly.

(define notebook-bindings
  (keymap
    (normal
      ("]" ("l" ":next-cell"))
      ("[" ("l" ":previous-cell"))
      (space ("n" ("r" ":execute-cell")
                  ("j" ":cell-picker")
                  ("c" ":select-cell")
                  ("s" ":select-cell-code")
                  ("o" ":select-output"))))))

;; Register for .ipynb files (raw notebooks)
(helix.keymaps.#%add-extension-or-labeled-keymap
  "ipynb"
  (merge-keybindings (get-keybindings) notebook-bindings))

;; Tab completion bindings for .jl files (insert mode only)
(define jl-tab-bindings
  (keymap
    (insert
      ("tab" ":julia-tab-complete"))))

;; Register for .jl files (converted notebooks) — notebook keys + Tab completion
(let ((km (deep-copy-global-keybindings)))
  (merge-keybindings km notebook-bindings)
  (merge-keybindings km jl-tab-bindings)
  (helix.keymaps.#%add-extension-or-labeled-keymap "jl" km))

;;; ============================================================================
;;; AUTO-CONCEAL
;;; ============================================================================
;;;
;;; file-has-conceal-extension? and the conceal orchestration live in
;;; nothelix/conceal.scm. This file only wires the hooks.

;;@doc
;; Apply conceal if the current buffer is a markdown/tex/jl file.
(define (maybe-conceal-current-buffer)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (file-has-conceal-extension? path)
    (conceal-math!)))

;;; ============================================================================
;;; EXIT CLEANUP HOOK
;;; ============================================================================

;; List of quit commands that should trigger kernel cleanup
(define *quit-commands*
  '("quit" "force-quit" "quit-all" "force-quit-all"
    "write-quit" "force-write-quit" "write-quit-all" "force-write-quit-all"
    "cquit" "force-cquit"))

;; Debounce counter: each new trigger increments the generation.
;; Callbacks check their captured generation against the current one;
;; if stale, they skip execution.
(define *conceal-generation* 0)

;; Commands that mutate the buffer and therefore invalidate the conceal
;; cache when they run. post-command fires after the command returns, so
;; we schedule a reconceal to rebuild the cache against the new document
;; state. execute-cell lives in a different class — its output lands
;; asynchronously inside update-cell-output, which calls schedule-reconceal
;; directly at the end of that callback.
(define *mutating-commands*
  '("convert-notebook" "sync-to-ipynb"))

;; Hook for kernel cleanup, conceal refresh on buffer switch, and conceal
;; invalidation after mutating commands.
;;
;; Buffer switches used to also trigger `render-cached-images`, but that
;; accumulated duplicate RawContent entries on stock Helix (see
;; execution.scm for the full explanation). Images now register exactly
;; once per doc via `document-opened` and persist on the document for
;; the lifetime of the view, so this hook only needs to refresh
;; concealment on buffer switch.
(define (nothelix-post-command-hook command-name)
  (cond
    [(member command-name *quit-commands*)
     (stop-all-kernels)]
    [(member command-name '("buffer-next" "buffer-previous"))
     (set! *conceal-generation* (+ *conceal-generation* 1))
     (define my-gen *conceal-generation*)
     (enqueue-thread-local-callback-with-delay 150
       (lambda ()
         (when (= my-gen *conceal-generation*)
           (maybe-conceal-current-buffer))))]
    [(member command-name *mutating-commands*)
     ;; Cache is stale from the moment the command started running.
     ;; apply-conceal-for-cursor will fail closed until reconceal completes.
     (schedule-reconceal 50)]))

(register-hook! "post-command" nothelix-post-command-hook)
(register-hook! "document-opened"
  (lambda (_doc-id)
    (set! *conceal-generation* (+ *conceal-generation* 1))
    (define my-gen *conceal-generation*)
    (enqueue-thread-local-callback-with-delay 200
      (lambda ()
        (when (= my-gen *conceal-generation*)
          (render-cached-images)
          (maybe-conceal-current-buffer))))))

;; Cursor-aware conceal: when the cursor moves to a different line, re-filter
;; cached overlays to exclude that line so the user sees raw LaTeX while
;; editing. apply-conceal-for-cursor! validates the cache fingerprint and
;; fails closed if stale.
(define *conceal-cursor-line* -1)
(register-hook! "selection-did-change"
  (lambda (_doc-id)
    (when (not (conceal-cache-empty?))
      (define focus (editor-focus))
      (define doc-id (editor->doc-id focus))
      (define rope (editor->text doc-id))
      (define cursor-pos (cursor-position))
      (define cursor-line (text.rope-char->line rope cursor-pos))
      (when (not (= cursor-line *conceal-cursor-line*))
        (set! *conceal-cursor-line* cursor-line)
        (apply-conceal-for-cursor!)))))

;; Insert-driven reconceal. A short debounce lets rapid typing settle before
;; we pay for the FFI call.
(register-hook! "post-insert-char"
  (lambda (_char)
    (when (file-has-conceal-extension? (editor-document->path
            (editor->doc-id (editor-focus))))
      (schedule-reconceal 400))))

;;; ============================================================================
;;; TEST COMMANDS
;;; ============================================================================

(define *nothelix-plugin-dir* #false)

(define (get-nothelix-plugin-dir)
  (when (not *nothelix-plugin-dir*)
    (set! *nothelix-plugin-dir*
      (resolve-symlink-dir "~/.config/helix/nothelix.scm")))
  *nothelix-plugin-dir*)

;;@doc
;; Run all Nothelix tests
(define (run-all-tests)
  ;; Load tests dynamically at runtime (after FFI is initialized)
  ;; Use absolute path since eval doesn't inherit module search context
  (define test-path (string-append (get-nothelix-plugin-dir) "/tests/run-all-tests.scm"))
  (eval `(begin
           (require ,test-path)
           (run-all-nothelix-tests))))

;;@doc
;; Run cell extraction tests only
(define (run-cell-tests)
  (define test-path (string-append (get-nothelix-plugin-dir) "/tests/cell-extraction-test.scm"))
  (eval `(begin
           (require ,test-path)
           (run-cell-extraction-tests))))

;;@doc
;; Run kernel persistence tests only
(define (run-kernel-tests)
  (define test-path (string-append (get-nothelix-plugin-dir) "/tests/kernel-persistence-test.scm"))
  (eval `(begin
           (require ,test-path)
           (run-kernel-persistence-tests))))

;;@doc
;; Run execution flow integration tests only
(define (run-execution-tests)
  (define test-path (string-append (get-nothelix-plugin-dir) "/tests/execution-flow-test.scm"))
  (eval `(begin
           (require ,test-path)
           (run-execution-flow-tests))))
