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
         run-all-tests run-cell-tests run-kernel-tests run-execution-tests)

;;; ============================================================================
;;; CONCEAL (overlay calls live here — misc.scm is already loaded)
;;; ============================================================================

;;@doc
;; Apply LaTeX math concealment to the current buffer.
;; Runs synchronously but is fast: .jl files only scan comment lines,
;; and the debounce guard prevents redundant calls.
(define (conceal-math)
  (define overlays (compute-conceal-overlays))
  (if (null? overlays)
      (clear-conceal)
      (begin
        (set-overlays! overlays)
        (set-status! (string-append "nothelix: " (number->string (length overlays)) " overlays")))))

;;@doc
;; Remove all conceal overlays.
(define (clear-conceal)
  (clear-overlays!))

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

;; File extensions that should get LaTeX math concealment
(define *conceal-extensions* '("md" "markdown" "tex" "jl" "qmd" "rmd"))

(define (ends-with? str suffix)
  (define slen (string-length suffix))
  (define tlen (string-length str))
  (and (>= tlen slen)
       (string=? (substring str (- tlen slen) tlen) suffix)))

(define (file-has-conceal-extension? path)
  (and path
       (let loop ((exts *conceal-extensions*))
         (if (null? exts) #f
             (or (ends-with? path (string-append "." (car exts)))
                 (loop (cdr exts)))))))

;;@doc
;; Apply conceal if the current buffer is a markdown/tex/jl file.
(define (maybe-conceal-current-buffer)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (file-has-conceal-extension? path)
    (conceal-math)))

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

;; Hook for kernel cleanup on exit and image rendering on buffer switch
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
           (render-cached-images)
           (maybe-conceal-current-buffer))))]))

(register-hook! "post-command" nothelix-post-command-hook)
(register-hook! "document-opened"
  (lambda (_doc-id)
    (set! *conceal-generation* (+ *conceal-generation* 1))
    (define my-gen *conceal-generation*)
    (enqueue-thread-local-callback-with-delay 200
      (lambda ()
        (when (= my-gen *conceal-generation*)
          (maybe-conceal-current-buffer))))))

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
