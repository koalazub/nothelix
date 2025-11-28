;;; conversion.scm - Notebook conversion between .ipynb and .jl formats

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For set-status!
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

;; FFI imports for conversion functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          notebook-validate
                          notebook-convert-sync
                          notebook-cell-count
                          convert-to-ipynb))

(provide convert-notebook
         sync-to-ipynb
         replace-document-contents!)

(define (replace-document-contents! content)
  (helix.static.select_all)
  (helix.static.delete_selection)
  (helix.static.insert_string content)
  (helix.static.goto_file_start))

;;@doc
;; Convert current .ipynb to readable cell format (fast Rust parsing)
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
     ;; Use detailed validation with error message
     (define validation-error (notebook-validate path))
     (if (not (equal? validation-error ""))
         ;; Show detailed error
         (set-status! (string-append "Invalid notebook: " validation-error))
         ;; Valid, proceed with conversion
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
                         ;; Generate output path: notebook.ipynb -> notebook.jl
                         (define output-path
                           (string-append
                             (substring path 0 (- (string-length path) 6))  ; Remove ".ipynb"
                             ".jl"))
                         ;; Write converted content to .jl file
                         (helix.run-shell-command
                           (string-append "cat > " output-path " <<'NOTHELIXEOF'\n"
                                         result
                                         "\nNOTHELIXEOF"))
                         (set-status! (string-append "Converted to " output-path ": "
                                                    (number->string cell-count)
                                                    " cells. Run :open " output-path))))))))))]))

;;@doc
;; Sync changes from .jl file back to .ipynb file
;; Updates cell sources in the original .ipynb from the edited .jl file
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
         (set-status! "✓ Synced changes back to .ipynb"))]))
