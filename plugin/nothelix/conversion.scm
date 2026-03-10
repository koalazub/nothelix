;;; conversion.scm - Notebook conversion between .ipynb and .jl formats
;;;
;;; :convert-notebook reads an .ipynb, parses it in Rust, and writes a .jl
;;; file in the Nothelix cell format.  :sync-to-ipynb does the reverse.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

;; FFI imports for conversion functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          notebook-validate
                          notebook-convert-sync
                          notebook-cell-count
                          convert-to-ipynb
                          write-string-to-file))

(provide convert-notebook
         sync-to-ipynb)

;;@doc
;; Convert the current .ipynb to the Nothelix .jl cell format.
;; Validates the notebook JSON, then spawns a native thread for the conversion
;; so the editor stays responsive.  Writes the result to a .jl file alongside
;; the original .ipynb.
(define (convert-notebook)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]

    [(not (string-suffix? path ".ipynb"))
     (set-status! "Error: Not a .ipynb file")]

    [else
     (define validation-error (notebook-validate path))
     (if (not (equal? validation-error ""))
         (set-status! (string-append "Invalid notebook: " validation-error))
         (begin
           (set-status! "Converting...")
           (spawn-native-thread
             (lambda ()
               (define result (notebook-convert-sync path))
               (define cell-count (notebook-cell-count path))
               (hx.with-context
                 (lambda ()
                   (if (string-starts-with? result "ERROR:")
                       (set-status! result)
                       (begin
                         (define output-path
                           (string-append
                             (substring path 0 (- (string-length path) 6))
                             ".jl"))
                         (define write-err (write-string-to-file output-path result))
                         (when (not (equal? write-err ""))
                           (set-status! write-err))
                         (set-status!
                           (string-append "Converted to " output-path ": "
                                          (number->string cell-count)
                                          " cells. Run :open " output-path))))))))))]))

;;@doc
;; Sync changes from the .jl file back to the original .ipynb.
;; Reads the @cell markers and code from the .jl, then updates the
;; corresponding cells in the .ipynb JSON.
(define (sync-to-ipynb)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]

    [(not (string-suffix? path ".jl"))
     (set-status! "Error: Not a .jl file. Only converted notebooks can be synced back.")]

    [else
     (set-status! "Syncing to .ipynb...")
     (define result (convert-to-ipynb path))
     (if (string-starts-with? result "ERROR:")
         (set-status! result)
         (set-status! "Synced changes back to .ipynb"))]))
