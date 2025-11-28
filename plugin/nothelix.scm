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

;; Nothelix modules
(require "nothelix/string-utils.scm")
(require "nothelix/graphics.scm")
(require "nothelix/kernel.scm")
(require "nothelix/conversion.scm")
(require "nothelix/navigation.scm")
(require "nothelix/execution.scm")
(require "nothelix/selection.scm")
(require "nothelix/picker.scm")

(provide convert-notebook sync-to-ipynb execute-cell execute-all-cells execute-cells-above
         cancel-cell  ;; Interrupt running execution
         next-cell previous-cell cell-picker
         select-cell select-cell-code select-output
         render-image render-cell-image
         ;; Kernel lifecycle
         kernel-shutdown kernel-shutdown-all
         ;; Protocol API
         graphics-protocol graphics-check nothelix-status)

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
      ("g" ("n" ("r" ":execute-cell")))
      (space ("n" ("j" ":cell-picker")
                  ("c" ":select-cell")
                  ("s" ":select-cell-code")
                  ("o" ":select-output"))))))

;; Register for .ipynb files (raw notebooks)
(helix.keymaps.#%add-extension-or-labeled-keymap
  "ipynb"
  (merge-keybindings (get-keybindings) notebook-bindings))

;; Register for .jl files (converted notebooks)
(helix.keymaps.#%add-extension-or-labeled-keymap
  "jl"
  (merge-keybindings (get-keybindings) notebook-bindings))
