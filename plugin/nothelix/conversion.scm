;;; conversion.scm — Notebook conversion between .ipynb and .jl formats

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          notebook-validate
                          notebook-convert-sync!
                          notebook-cell-count
                          convert-to-ipynb!
                          export-to-markdown!
                          export-to-typst!
                          render-typst-to-pdf
                          read-file-tail
                          write-string-to-file!))

(provide convert-notebook
         sync-to-ipynb
         export-markdown
         export-typst
         export-pdf)

(define (path-basename p)
  (let ([parts (string-split p "/")])
    (if (null? parts)
        p
        (list-ref parts (- (length parts) 1)))))

(define (jl-sibling-path jl-path new-ext)
  (if (string-suffix? jl-path ".jl")
      (string-append (substring jl-path 0 (- (string-length jl-path) 3)) new-ext)
      (string-append jl-path new-ext)))

;;@doc
;; Convert the current .ipynb to the Nothelix .jl cell format.
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
     (set-status! "Converting…")
     (spawn-native-thread
       (lambda ()
         (define validation-error (notebook-validate path))
         (if (not (equal? validation-error ""))
             (hx.with-context
               (lambda () (set-status! (string-append "Invalid notebook: " validation-error))))
             (begin
               (define result (notebook-convert-sync! path))
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
                         (define write-err (write-string-to-file! output-path result))
                         (if (not (equal? write-err ""))
                             (set-status! write-err)
                             (set-status!
                               (string-append "✓ " (number->string cell-count)
                                              " cells · " (path-basename output-path)
                                              " — :open to view")))))))))))]))

;;@doc
;; Sync changes from the .jl file back to the original .ipynb.
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
     (set-status! "Syncing to .ipynb…")
     (spawn-native-thread
       (lambda ()
         (define result (convert-to-ipynb! path))
         (hx.with-context
           (lambda ()
             (if (string-starts-with? result "ERROR:")
                 (set-status! result)
                 (set-status! "Synced changes back to .ipynb"))))))]))

;;@doc
;; Export the current .jl notebook to Markdown (.md).
(define (export-markdown)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]
    [(not (string-suffix? path ".jl"))
     (set-status! "Error: Not a .jl file")]
    [else
     (set-status! "Exporting to Markdown…")
     (spawn-native-thread
       (lambda ()
         (define result (export-to-markdown! path))
         (hx.with-context (lambda () (set-status! result)))))]))

;;@doc
;; Export the current .jl notebook to Typst (.typ).
(define (export-typst)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]
    [(not (string-suffix? path ".jl"))
     (set-status! "Error: Not a .jl file")]
    [else
     (set-status! "Exporting to Typst…")
     (spawn-native-thread
       (lambda ()
         (define result (export-to-typst! path))
         (hx.with-context (lambda () (set-status! result)))))]))

;;@doc
;; Export the current .jl notebook to a PDF (.pdf) alongside its .typ.
(define (export-pdf)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]
    [(not (string-suffix? path ".jl"))
     (set-status! "Error: Not a .jl file")]
    [else
     (set-status! "Exporting to PDF…")
     (spawn-native-thread
       (lambda ()
         (define typ-result (export-to-typst! path))
         (cond
           [(string-starts-with? typ-result "ERROR:")
            (hx.with-context (lambda () (set-status! typ-result)))]
           [else
            (define typ-path (jl-sibling-path path ".typ"))
            (define pdf-path (jl-sibling-path path ".pdf"))
            (define source (read-file-tail typ-path 1000000000))
            (cond
              [(string-starts-with? source "ERROR:")
               (hx.with-context
                 (lambda () (set-status! (string-append "export-pdf: " source))))]
              [else
               (define pdf-err (render-typst-to-pdf source pdf-path))
               (hx.with-context
                 (lambda ()
                   (if (equal? pdf-err "")
                       (set-status!
                         (string-append "✓ Exported PDF: " (path-basename pdf-path)))
                       (set-status!
                         (string-append "✗ PDF export failed: " pdf-err)))))])])))]))
