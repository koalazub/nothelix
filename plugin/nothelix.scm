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
(require "nothelix/cell-boundaries.scm")
(require "nothelix/cursor-restore.scm")
(require "nothelix/image-cache.scm")
(require "nothelix/output-insert.scm")
(require "nothelix/execution.scm")
(require "nothelix/selection.scm")
(require "nothelix/picker.scm")
(require "nothelix/chart-viewer.scm")
(require "nothelix/backslash.scm")
(require "nothelix/conceal-state.scm")
(require "nothelix/conceal.scm")
(require "nothelix/scaffold.scm")

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
         new-notebook renumber-cells new-cell
         nothelix-debug-on nothelix-debug-off nothelix-debug-toggle
         run-all-tests run-cell-tests run-kernel-tests run-execution-tests
         ;; Shorthands
         xc xca nc)

;;; ============================================================================
;;; CONCEAL — thin shim for backwards-compatible provided names
;;; ============================================================================
;;;
;;; All conceal logic lives in nothelix/conceal.scm. These aliases keep the
;;; top-level provide list stable so users of the plugin don't need to
;;; re-import anything.

;;@doc
;; Apply LaTeX unicode concealment to the current buffer.
(define (conceal-math) (conceal-math!))

;;@doc
;; Remove LaTeX unicode concealment overlays from the current buffer.
(define (clear-conceal) (clear-conceal!))

;;; ============================================================================
;;; COMMAND SHORTHANDS
;;; ============================================================================

(define (xc) (execute-cell))          ;; :xc   = :execute-cell
(define (xca) (execute-all-cells))    ;; :xca  = :execute-all-cells
(define (nc) (next-cell))             ;; :nc   = :next-cell

;;; ============================================================================
;;; DEBUG MODE — thin command shims for the `nothelix/debug.scm` module
;;; ============================================================================
;;;
;;; When toggled on, modules across the plugin start emitting
;;; `nothelix: …` lines via `debug-log`, which are routed to the
;;; helix log at info level. Off by default.

;;@doc
;; Turn nothelix debug logging on. Every cell execution and image
;; registration then logs to ~/.cache/helix/helix.log (needs -v / -vv
;; on the hx command line to surface info-level lines).
(define (nothelix-debug-on) (nothelix-debug-enable!))

;;@doc
;; Turn nothelix debug logging off.
(define (nothelix-debug-off) (nothelix-debug-disable!))

;;@doc
;; Toggle nothelix debug logging on/off.
(define (nothelix-debug-toggle) (nothelix-debug-toggle!))

;;; ============================================================================
;;; SCAFFOLDING — thin command shims for the `nothelix/scaffold.scm` module
;;; ============================================================================

;;@doc
;; Insert a new cell at the cursor. Opens a small picker to choose
;; between a code cell (using the file's language, typically Julia)
;; or a markdown cell. The picked marker is stamped with the next
;; available cell index, and the cursor is positioned ready to type.
(define (new-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define line-idx (text.rope-char->line rope (cursor-position)))
  (define next-idx (next-cell-index rope total-lines))
  (open-cell-type-picker line-idx next-idx (file-lang (or path ""))))

;;@doc
;; Renumber every @cell / @markdown marker in the current buffer to
;; a contiguous 0-indexed sequence. Runs automatically on save and
;; on :sync-to-ipynb; exposed here so you can also invoke it manually
;; after a bunch of mid-file cell deletions.
(define (renumber-cells) (renumber-cells!))

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
                  ("n" ":new-cell")
                  ("j" ":cell-picker")
                  ("a" ":select-cell")
                  ("i" ":select-cell-code")
                  ("o" ":select-output"))))))

;; ─── Plugin command documentation ─────────────────────────────────────────────
;;
;; Helix's keybinding help tries to look up each bound command's
;; docstring via `#%function-ptr-table-get`, but that table is only
;; populated by commands registered through Helix's Rust-side
;; `template_function_arityN` macros — Steel plugin `define`s don't
;; write into it even with a `;;@doc` block attached. So every
;; nothelix command falls through to the "Undocumented plugin
;; command" default no matter what we annotate.
;;
;; Fix: maintain the doc hash ourselves and push it into the
;; keymap directly via `keymap-update-documentation!` right after
;; `merge-keybindings` has stamped the bindings. The Rust side
;; applies the hash to every matching command in the trie, so the
;; `<space>n…` menu and the command palette both show real
;; descriptions instead of the fallback.
(define nothelix-command-docs
  (hash
    ;; Notebook lifecycle
    "convert-notebook" "Convert an .ipynb into editable cell format (.jl)."
    "sync-to-ipynb"    "Sync edits in the .jl file back to the source .ipynb."
    "new-notebook"     "Create a new .jl notebook file and open it."
    "renumber-cells"   "Renumber @cell / @markdown markers to 0, 1, 2, …"

    ;; Cell execution
    "execute-cell"        "Run the code cell under the cursor."
    "execute-all-cells"   "Run every code cell top-to-bottom."
    "execute-cells-above" "Run every code cell from the top to the current cell."
    "cancel-cell"         "Interrupt the currently running cell."

    ;; Cell navigation / selection
    "next-cell"        "Jump to the next cell."
    "previous-cell"    "Jump to the previous cell."
    "cell-picker"      "Open the interactive cell navigator."
    "new-cell"         "Insert a new cell (code or markdown) at the cursor."
    "select-cell"      "Select around cell (header + code + output)."
    "select-cell-code" "Select inside cell (code only)."
    "select-output"    "Select output block."

    ;; Chart viewer
    "view-chart" "Open the last-executed plot in the interactive chart viewer."

    ;; Kernel lifecycle
    "kernel-shutdown"     "Stop the kernel for the current document."
    "kernel-shutdown-all" "Stop every running kernel."

    ;; Graphics
    "graphics-protocol" "Show which graphics protocol nothelix detected."
    "graphics-check"    "Run a quick diagnostic of the active graphics protocol."
    "nothelix-status"   "Show full nothelix status (kernels, graphics, LSP, …)."

    ;; LaTeX / unicode
    "conceal-math"       "Apply LaTeX → unicode concealment to the current buffer."
    "clear-conceal"      "Remove LaTeX concealment overlays from the current buffer."
    "julia-tab-complete" "Expand a \\<name> Julia LaTeX shortcut at the cursor."

    ;; Debug
    "nothelix-debug-on"     "Enable nothelix debug logging (writes to ~/.cache/helix/helix.log)."
    "nothelix-debug-off"    "Disable nothelix debug logging."
    "nothelix-debug-toggle" "Flip the nothelix debug logging on/off."

    ;; Tests
    "run-all-tests"        "Run every nothelix Steel test suite."
    "run-cell-tests"       "Run the cell-extraction tests only."
    "run-kernel-tests"     "Run the kernel-persistence tests only."
    "run-execution-tests"  "Run the execution-flow tests only."))

;; Helper: apply nothelix-command-docs to a keymap after it's been
;; built via `merge-keybindings`. Wraps the existing Rust-side hook.
(define (nothelix-document-keymap! keymap)
  (helix.keymaps.keymap-update-documentation! keymap nothelix-command-docs)
  keymap)

;; Register for .ipynb files (raw notebooks)
(helix.keymaps.#%add-extension-or-labeled-keymap
  "ipynb"
  (nothelix-document-keymap!
    (merge-keybindings (get-keybindings) notebook-bindings)))

;; Tab completion bindings for .jl files (insert mode only)
(define jl-tab-bindings
  (keymap
    (insert
      ("tab" ":julia-tab-complete"))))

;; Register for .jl files (converted notebooks) — notebook keys + Tab completion
(let ((km (deep-copy-global-keybindings)))
  (merge-keybindings km notebook-bindings)
  (merge-keybindings km jl-tab-bindings)
  (nothelix-document-keymap! km)
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

;; Commands that write the buffer to disk. We hook these to run the
;; cell renumber pass so holes that accumulate during editing (from
;; deleting or rearranging cells) get cleaned up at the natural "I
;; committed to this" moment. The renumber runs *after* the write,
;; so the on-disk copy lags by one save — the next `:w` syncs the
;; clean numbers. In practice you don't notice because the file
;; content is identical apart from the integers in the markers.
(define *save-commands*
  '("write" "force-write" "write-quit" "force-write-quit"
    "write-all" "force-write-all" "write-quit-all" "force-write-quit-all"
    "write-buffer-close"))

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
  ;; Every non-quit command gets the image-marker sync check. It's
  ;; cheap (one buffer scan + count compare) and only escalates to a
  ;; full re-register when the `# @image` line count actually changed,
  ;; so typing stays snappy and backspacing a marker line really does
  ;; make the image disappear.
  (when (not (member command-name *quit-commands*))
    (sync-images-if-markers-changed!))
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
    [(member command-name *save-commands*)
     ;; Renumber cells to a contiguous 0-indexed sequence whenever
     ;; the user saves. Holes from mid-file deletion/rearrangement
     ;; get swept up here so the file on disk stays tidy between
     ;; sessions. `:sync-to-ipynb` also drops into this branch
     ;; indirectly because `renumber-cells!` is idempotent.
     ;; `renumber-cells!` itself now saves and restores the cursor
     ;; so `:w` no longer flings the view to the top of the file.
     (renumber-cells!)]
    [(member command-name *mutating-commands*)
     ;; Cache is stale from the moment the command started running.
     ;; apply-conceal-for-cursor will fail closed until reconceal completes.
     (renumber-cells!)
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

;; Insert-driven reconceal *and* cell marker autofill. Both run from
;; the same `post-insert-char` hook because they want the same event
;; and both no-op cheaply when the typed character isn't interesting.
;;
;; - Autofill: checks for a just-typed space after `@<word>` on an
;;   otherwise empty line in a notebook file and rewrites the line
;;   (either directly for `@md`/`@mark`/`@markdown`, or via the
;;   cell-type picker for `@cell` and any unknown `@<word>`).
;; - Reconceal: debounces a LaTeX-overlay refresh so conceal stays
;;   in sync with buffer edits in files that opt in.
(register-hook! "post-insert-char"
  (lambda (char)
    (define path (editor-document->path (editor->doc-id (editor-focus))))
    (maybe-expand-cell-marker! char)
    ;; Marker-count sync on insert. The check is O(lines) and is
    ;; a no-op when nothing about the `# @image` markers changed,
    ;; so rapid typing doesn't pay the full re-register cost.
    (sync-images-if-markers-changed!)
    (when (file-has-conceal-extension? path)
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
