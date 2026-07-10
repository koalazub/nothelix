;;; nothelix.scm — Jupyter notebooks for Helix

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

;; FFI version handshake — must be the FIRST nothelix require.
(require "nothelix/ffi-version.scm")

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
(require "nothelix/plot-resize.scm")
(require "nothelix/execution.scm")
(require "nothelix/selection.scm")
(require "nothelix/picker.scm")
(require "nothelix/chart-viewer.scm")
(require "nothelix/backslash.scm")
(require "nothelix/conceal-state.scm")
(require "nothelix/conceal.scm")
(require "nothelix/scaffold.scm")
(require "nothelix/math-format.scm")
(require "nothelix/math-render.scm")
(require "nothelix/math-image.scm")
(require "nothelix/table-image.scm")
(require "nothelix/project-config.scm")
(require "nothelix/resume.scm")
(require "nothelix/markdown-render.scm")
(require "nothelix/animation.scm")
(require "nothelix/health.scm")
(require "nothelix/lsp-statusline.scm")

;; Test modules loaded at startup so the :run-*-tests commands can invoke them.
(require "tests/run-all-tests.scm")

(provide convert-notebook sync-to-ipynb export-markdown export-typst export-pdf
         execute-cell execute-all-cells execute-cells-above cancel-cell
         next-cell previous-cell cell-picker
         select-cell select-cell-code select-output
         view-chart
         insert-image
         plot-grow plot-shrink
         format-math-buffer
         math-render-buffer
         math-render-clear
         render-math-at-cursor
         render-all-display-math
         render-all-tables
         clear-math-images
         kernel-shutdown kernel-shutdown-all
         nothelix-trust-project nothelix-untrust-project nothelix-project-trust-status
         graphics-protocol graphics-check nothelix-status
         julia-tab-complete
         conceal-math clear-conceal
         animation-toggle-at-cursor animation-pause-all animation-resume-all
         new-notebook renumber-cells new-cell
         nothelix-debug-on nothelix-debug-off nothelix-debug-toggle
         run-all-tests run-cell-tests run-kernel-tests run-execution-tests
         ;; Shorthands
         xc xca nc)

;; Conceal — shims for backwards-compatible provided names (logic in conceal.scm)

;;@doc
;; Apply LaTeX unicode concealment to the current buffer.
(define (conceal-math) (conceal-math!))

;;@doc
;; Remove LaTeX unicode concealment overlays from the current buffer.
(define (clear-conceal) (clear-conceal!))

;; Command shorthands

(define (xc) (execute-cell))
(define (xca) (execute-all-cells))
(define (nc) (next-cell))

;; Debug mode command shims

;;@doc
;; Turn nothelix debug logging on.
(define (nothelix-debug-on) (nothelix-debug-enable!))

;;@doc
;; Turn nothelix debug logging off.
(define (nothelix-debug-off) (nothelix-debug-disable!))

;;@doc
;; Toggle nothelix debug logging on/off.
(define (nothelix-debug-toggle) (nothelix-debug-toggle!))

;;@doc
;; Print the current nothelix health-check status (re-runs the check).
(define (nothelix-status) (nothelix-status-command))

;; Scaffolding command shims

;;@doc
;; Insert a new cell (code or markdown) at the cursor.
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
;; Renumber every @cell / @markdown marker to a contiguous 0-indexed sequence.
(define (renumber-cells) (renumber-cells!))

;; Kernel lifecycle

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

;; Project trust — gate for .nothelix.conf executable settings (julia-bin /
;; julia-project). Display settings never need trust; these do.

;;@doc
;; Trust the current notebook's project so its .nothelix.conf may launch a
;; custom Julia binary / project env. Restarts the kernel so it takes effect.
(define (nothelix-trust-project)
  (define path (focused-notebook-path))
  (if (not path)
      (set-status! "nothelix: no file in focus")
      (let ([dir (project-dir-for path)])
        (if (not dir)
            (set-status! "nothelix: no .nothelix.conf found for this project")
            (let ([res (trust-project! dir)])
              (if (> (string-length res) 0)
                  (set-status! (string-append "nothelix: " res))
                  (begin
                    (stop-kernel path)
                    (set-status!
                      (string-append "nothelix: trusted " dir
                        " — its Julia runtime applies on the next cell run")))))))))

;;@doc
;; Revoke trust for the current notebook's project; restarts the kernel.
(define (nothelix-untrust-project)
  (define path (focused-notebook-path))
  (if (not path)
      (set-status! "nothelix: no file in focus")
      (let ([dir (project-dir-for path)])
        (if (not dir)
            (set-status! "nothelix: no .nothelix.conf found for this project")
            (begin
              (untrust-project! dir)
              (stop-kernel path)
              (set-status!
                (string-append "nothelix: untrusted " dir
                  " — kernel reverts to PATH julia on the next run")))))))

;;@doc
;; Report whether the current project is trusted and the runtime it would use.
(define (nothelix-project-trust-status)
  (define path (focused-notebook-path))
  (if (not path)
      (set-status! "nothelix: no file in focus")
      (let ([dir (project-dir-for path)])
        (if (not dir)
            (set-status! "nothelix: no .nothelix.conf for this project")
            (let ([rt (project-runtime-for path)])
              (set-status!
                (string-append "nothelix: " dir
                  (if (project-trusted? dir) " [trusted]" " [untrusted]")
                  (if (> (string-length (car rt)) 0)
                      (string-append " julia-bin=" (car rt)) "")
                  (if (> (string-length (cdr rt)) 0)
                      (string-append " julia-project=" (cdr rt)) ""))))))))

;; Keybindings

;; keymaps.scm's (inherit-from ...) is buggy (map vs kmap), so call functions directly.
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
                  ("o" ":select-output")
                  ("=" ":plot-grow")
                  ("-" ":plot-shrink"))
             ("p" ":animation-toggle-at-cursor")))))

;; Command docs for the keymap help UI (Helix can't read Steel doc strings).
(define nothelix-command-docs
  (hash
    ;; Notebook lifecycle
    "convert-notebook" "Convert an .ipynb into editable cell format (.jl)."
    "sync-to-ipynb"    "Sync edits in the .jl file back to the source .ipynb."
    "export-markdown"  "Export the .jl notebook to Markdown (.md)."
    "export-typst"     "Export the .jl notebook to Typst (.typ)."
    "export-pdf"       "Export the .jl notebook to a PDF (.pdf) via Typst."
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

    ;; Image insertion
    "insert-image" "Insert a `# @image <path>` marker + blank canvas at the cursor."

    ;; Plot resizing
    "plot-grow"   "Grow the @image plot block under the cursor and re-render."
    "plot-shrink" "Shrink the @image plot block under the cursor and re-render."

    ;; Math formatting
    "format-math-buffer" "Expand single-line \\begin{cases}/pmatrix/aligned envs into multi-line \\$\\$ blocks."
    "math-render-buffer" "Stack big-operator limits (\\int / \\sum / \\prod …) above/below their glyph via virtual rows."
    "math-render-clear"  "Remove every virtual-row math annotation from the current buffer."
    "render-math-at-cursor" "Render the # $$ display-math block under the cursor as a Typst SVG image."
    "render-all-display-math" "Render every # $$ display-math block in the buffer as Typst SVG images."
    "render-all-tables"     "Render every markdown pipe table in the buffer as a transparent Typst image."
    "clear-math-images"     "Remove all inline math-image renderings from the current buffer."

    ;; Kernel lifecycle
    "kernel-shutdown"     "Stop the kernel for the current document."
    "kernel-shutdown-all" "Stop every running kernel."

    ;; Project trust (.nothelix.conf executable settings)
    "nothelix-trust-project"        "Trust this project's .nothelix.conf to launch a custom Julia bin/env."
    "nothelix-untrust-project"      "Revoke trust for this project's custom Julia runtime."
    "nothelix-project-trust-status" "Show whether this project is trusted and the runtime it would use."

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

(define (nothelix-document-keymap! keymap)
  (helix.keymaps.keymap-update-documentation! keymap nothelix-command-docs)
  keymap)

;; Register for .ipynb files
(helix.keymaps.#%add-extension-or-labeled-keymap
  "ipynb"
  (nothelix-document-keymap!
    (merge-keybindings (get-keybindings) notebook-bindings)))

;; Tab completion bindings for .jl files (insert mode only)
(define jl-tab-bindings
  (keymap
    (insert
      ("tab" ":julia-tab-complete"))))

;; Register for .jl files (notebook keys + Tab completion)
(let ((km (deep-copy-global-keybindings)))
  (merge-keybindings km notebook-bindings)
  (merge-keybindings km jl-tab-bindings)
  (nothelix-document-keymap! km)
  (helix.keymaps.#%add-extension-or-labeled-keymap "jl" km))

;; Auto-conceal (orchestration lives in conceal.scm; this file wires the hooks)

;;@doc
;; Apply conceal if the current buffer is a markdown/tex/jl file.
(define (maybe-conceal-current-buffer)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (file-has-conceal-extension? path)
    (conceal-math!)))

;; Exit cleanup hook

(define *quit-commands*
  '("quit" "force-quit" "quit-all" "force-quit-all"
    "write-quit" "force-write-quit" "write-quit-all" "force-write-quit-all"
    "cquit" "force-cquit"))

;; Debounce generation counter; callbacks compare their captured gen and skip if stale.
(define *conceal-generation* 0)

;; Commands that mutate the buffer and invalidate the conceal cache.
(define *mutating-commands*
  '("convert-notebook" "sync-to-ipynb" "export-markdown" "export-typst"
    "insert-image" "format-math-buffer" "math-render-buffer" "math-render-clear"
    "paste_after" "paste_before" "paste_clipboard_after" "paste_clipboard_before"
    "replace_with_yanked" "replace_selections_with_clipboard"
    "delete_selection" "delete_selection_noyank"
    "change_selection" "change_selection_noyank"))

;; Commands that write the buffer to disk; we hook these to renumber cells.
(define *save-commands*
  '("write" "force-write" "write-quit" "force-write-quit"
    "write-all" "force-write-all" "write-quit-all" "force-write-quit-all"
    "write-buffer-close"))

(define (nothelix-post-command-hook command-name)
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
       (format-math-buffer #true)
       (math-render-buffer)
       (when (not (math-image-test-mode?))
         (render-all-display-math)
         (render-all-tables))
       (renumber-cells!)
       (save-resume-position!)
       (schedule-reconceal 50)]
    [(member command-name *mutating-commands*)
     (renumber-cells!)
     (schedule-reconceal 50)]))

(register-hook! "post-command" nothelix-post-command-hook)
(register-hook! "document-opened"
  (lambda (doc-id)
    (set! *conceal-generation* (+ *conceal-generation* 1))
    (define my-gen *conceal-generation*)
    (enqueue-thread-local-callback-with-delay 200
      (lambda ()
        (when (= my-gen *conceal-generation*)
          ;; Apply per-project display config before the first render so
          ;; font/colour/width settings take effect immediately.
          (maybe-apply-project-config!)
          (render-cached-images)
          (when (conceal-on-open?)
            (maybe-conceal-current-buffer))
          (when (not (math-image-test-mode?))
            (render-all-display-math)
            (render-all-tables))
          (restore-resume-position! doc-id))))))

;; Cursor-aware conceal: reveal raw LaTeX on the cursor's line while editing.
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

;; post-insert-char: cell-marker autofill + marker-count sync + debounced reconceal.
(register-hook! "post-insert-char"
  (lambda (char)
    (define path (editor-document->path (editor->doc-id (editor-focus))))
    (maybe-expand-cell-marker! char)
    (sync-images-if-markers-changed!)
    (when (file-has-conceal-extension? path)
      (schedule-reconceal 400))))

;; Test commands

;;@doc
;; Run all Nothelix tests.
(define (run-all-tests)
  (run-all-nothelix-tests))

;;@doc
;; Run cell extraction tests only.
(define (run-cell-tests)
  (run-cell-extraction-tests))

;;@doc
;; Run kernel persistence tests only.
(define (run-kernel-tests)
  (run-kernel-persistence-tests))

;;@doc
;; Run execution flow integration tests only.
(define (run-execution-tests)
  (run-execution-flow-tests))
