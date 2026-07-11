;;; output-store.scm — Steel layer over the per-cell output store FFI.

(require "string-utils.scm")
(require "image-cache.scm")
(require "helix/editor.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix output-store-put output-store-get output-store-clear))

(provide workspace-id cell-id cell-source-hash
         store-put! store-get-for store-clear!
         json-escape-string outputs-json-for-cell
         encode-outputs+rows decode-stored-rows)

;;@doc
;; The current document's path, used as the workspace key for the output store.
(define (workspace-id)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define p (editor-document->path doc-id))
  (if p p "unknown"))

;;@doc
;; Stable per-cell store key derived from the cell's index.
(define (cell-id cell-index)
  (string-append "cell-" (number->string cell-index)))

;;@doc
;; djb2 hash of the cell's source, used to detect stale stored output.
(define (cell-source-hash code) (number->string (djb2-hash code)))

(define (store-put! id source-hash outputs-json)
  (output-store-put (workspace-id) id source-hash outputs-json))

;;@doc
;; Fetch a cell's stored output, keyed off an explicit workspace path rather
;; than `editor-focus` — so restore works on a doc that isn't the focused one.
(define (store-get-for path id) (output-store-get (if path path "unknown") id))

(define (store-clear! id) (output-store-clear (workspace-id) id))

(define *rows-sep-line* "###NOTHELIX-OUTPUT-ROWS###")

;;@doc
;; Bundle nbformat outputs-json with the exact text rows that were rendered
;; for it, so a later reopen can restore the rows without re-parsing JSON.
(define (encode-outputs+rows outputs-json rows)
  (string-append outputs-json "\n" *rows-sep-line* "\n" (string-join rows "\n")))

;;@doc
;; Given `store-get-for`'s raw "<hash>\t<body>" value and the cell's current
;; source hash, return the stored text rows when the hash matches and the
;; body carries a rows blob, or #false (missing, stale, or no rows).
(define (decode-stored-rows raw current-hash)
  (if (or (not raw) (equal? raw ""))
      #false
      (let ([hash+body (split-once raw "\t")])
        (if (not (list? hash+body))
            #false
            (let ([stored-hash (car hash+body)]
                  [body (cadr hash+body)])
              (if (not (equal? stored-hash current-hash))
                  #false
                  (let ([marker (string-append "\n" *rows-sep-line* "\n")])
                    (let ([json+rows (split-once body marker)])
                      (if (not (list? json+rows))
                          #false
                          (let ([rows-blob (cadr json+rows)])
                            (if (equal? rows-blob "") '() (string-split rows-blob "\n"))))))))))))

;;@doc
;; Escape a string for embedding as a JSON string literal (no surrounding quotes).
(define (json-escape-string s)
  (define len (string-length s))
  (let loop ([i 0] [acc '()])
    (if (>= i len)
        (apply string-append (reverse acc))
        (let ([c (string-ref s i)])
          (loop (+ i 1)
                (cons
                  (cond
                    [(eqv? c #\") "\\\""]
                    [(eqv? c #\\) "\\\\"]
                    [(eqv? c #\newline) "\\n"]
                    [(eqv? c #\tab) "\\t"]
                    [(eqv? c #\return) "\\r"]
                    [else (string c)])
                  acc))))))

;;@doc
;; Build a minimal nbformat-style outputs JSON array from optional stdout/stderr/
;; result-repr/error text. Any argument may be #false or "" to omit that entry.
(define (outputs-json-for-cell stdout-text stderr-text repr-text error-text)
  (define entries '())
  (when (and error-text (> (string-length error-text) 0))
    (set! entries
          (cons
            (string-append "{\"output_type\":\"error\",\"ename\":\"\",\"evalue\":\""
                           (json-escape-string error-text)
                           "\",\"traceback\":[]}")
            entries)))
  (when (and stdout-text (> (string-length stdout-text) 0))
    (set! entries
          (cons
            (string-append "{\"output_type\":\"stream\",\"name\":\"stdout\",\"text\":\""
                           (json-escape-string stdout-text)
                           "\"}")
            entries)))
  (when (and stderr-text (> (string-length stderr-text) 0))
    (set! entries
          (cons
            (string-append "{\"output_type\":\"stream\",\"name\":\"stderr\",\"text\":\""
                           (json-escape-string stderr-text)
                           "\"}")
            entries)))
  (when (and repr-text (> (string-length repr-text) 0))
    (set! entries
          (cons
            (string-append "{\"output_type\":\"execute_result\",\"data\":{\"text/plain\":\""
                           (json-escape-string repr-text)
                           "\"}}")
            entries)))
  (string-append "[" (string-join (reverse entries) ",") "]"))
