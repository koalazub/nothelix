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

;; NOTE: Test modules are NOT required here - they're loaded dynamically
;; when test commands are run, AFTER FFI is initialized

(provide convert-notebook sync-to-ipynb execute-cell execute-all-cells execute-cells-above
         cancel-cell  ;; Interrupt running execution
         next-cell previous-cell cell-picker
         select-cell select-cell-code select-output
         render-image render-cell-image
         ;; Kernel lifecycle
         kernel-shutdown kernel-shutdown-all
         ;; Protocol API
         graphics-protocol graphics-check nothelix-status
         ;; Test commands
         run-all-tests run-cell-tests run-kernel-tests run-execution-tests)

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

;; Register for .jl files (converted notebooks)
(helix.keymaps.#%add-extension-or-labeled-keymap
  "jl"
  (merge-keybindings (get-keybindings) notebook-bindings))

;;; ============================================================================
;;; EXIT CLEANUP HOOK
;;; ============================================================================

;; List of quit commands that should trigger kernel cleanup
(define *quit-commands*
  '("quit" "force-quit" "quit-all" "force-quit-all"
    "write-quit" "force-write-quit" "write-quit-all" "force-write-quit-all"
    "cquit" "force-cquit"))

;; Hook to cleanup kernels on editor exit
(define (nothelix-post-command-hook command-name)
  (when (member command-name *quit-commands*)
    (stop-all-kernels)))

;; Register the post-command hook
(register-hook! "post-command" nothelix-post-command-hook)

;;; ============================================================================
;;; TEST COMMANDS
;;; ============================================================================

(define *nothelix-plugin-dir* #f)

(define (get-nothelix-plugin-dir)
  (when (not *nothelix-plugin-dir*)
    (set! *nothelix-plugin-dir*
      (string-trim (helix.run-shell-command
        "dirname $(readlink -f ~/.config/helix/plugins/nothelix.scm 2>/dev/null) 2>/dev/null || dirname ~/.config/helix/plugins/nothelix.scm"))))
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
