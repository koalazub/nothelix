;; Jupyter Notebook Plugin for Helix
;; Auto-conversion from .ipynb JSON format to cell format

(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/static)
(require-builtin helix/core/text as text.)
(require-builtin steel/json)

;; Helper
(define (string-suffix? str suffix)
  (let [(str-len (string-length str))
        (suf-len (string-length suffix))]
    (and (>= str-len suf-len)
         (equal? (substring str (- str-len suf-len) str-len) suffix))))

;; Convert value to string
(define (to-string val)
  (cond
    [(string? val) val]
    [(number? val) (number->string val)]
    [else ""]))

;; Helper: check if string contains substring
(define (string-contains? str substr)
  (and (>= (string-length str) (string-length substr))
       (let loop ([i 0])
         (cond
           [(> (+ i (string-length substr)) (string-length str)) #f]
           [(equal? (substring str i (+ i (string-length substr))) substr) #t]
           [else (loop (+ i 1))]))))

;; Convert notebook to readable format
(define (convert-notebook doc-id)
  (set-status! "Converting notebook...")

  ;; Get the text from the document
  (define text (editor->text doc-id))
  (define json-str (text.rope->string text))

  ;; Only convert if it's valid JSON and not already converted
  (cond
    ;; Already converted - skip
    [(or (string-contains? json-str "# ─── Code Cell")
         (string-contains? json-str "# ─── Markdown Cell"))
     (set-status! "Already converted, skipping...")]

    ;; Not valid JSON - skip
    [(not (and (> (string-length json-str) 0)
               (equal? (substring json-str 0 1) "{")))
     (set-status! "Not a valid JSON notebook, skipping...")]

    ;; Valid JSON - convert it
    [else
     ;; Parse JSON
     (define nb (string->jsexpr json-str))
     (define cells (hash-try-get nb 'cells))

     (set-status! (string-append "Found " (number->string (length cells)) " cells"))

     ;; Format cells
     (define formatted
       (string-join
         (map (lambda (cell)
                (define cell-type (hash-try-get cell 'cell_type))
                (define source-raw (hash-try-get cell 'source))
                (define source (cond
                                [(list? source-raw) (string-join (map to-string source-raw) "")]
                                [(string? source-raw) source-raw]
                                [else ""]))
                (define exec-count (hash-try-get cell 'execution_count))
                (define header (if (equal? cell-type "code")
                                  (string-append "# ─── Code Cell ["
                                               (if (number? exec-count) (number->string exec-count) " ")
                                               "] ───")
                                  "# ─── Markdown Cell ───"))
                (string-append header "\n" source "\n"))
              cells)
         "\n"))

     ;; Replace buffer content
     (select_all *helix.cx*)
     (replace-selection-with *helix.cx* formatted)
     (set-status! "✓ Notebook converted!")]))

;; Convert all currently open .ipynb files on startup
(define all-docs (editor-all-documents))

(for-each
  (lambda (doc-id)
    (define path (editor-document->path doc-id))
    (when (and path (string-suffix? path ".ipynb"))
      (convert-notebook doc-id)))
  all-docs)

;; Hook for newly opened files
(register-hook! "document-opened"
  (lambda (doc-id)
    (define path (editor-document->path doc-id))
    (when (and path (string-suffix? path ".ipynb"))
      (convert-notebook doc-id))))
