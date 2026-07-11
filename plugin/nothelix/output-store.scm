;;; output-store.scm — Steel layer over the per-cell output store FFI.

(require "string-utils.scm")
(require "image-cache.scm")
(require "helix/editor.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix output-store-put output-store-get output-store-clear))

(provide workspace-id cell-id cell-source-hash
         store-put! store-get-for store-clear!
         json-escape-string outputs-json-for-cell
         encode-outputs+rows decode-stored-rows
         encode-outputs+rows+text-plots
         decode-stored-text-plots-blob
         decode-text-plots-blob)

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
(define *text-plots-sep-line* "###NOTHELIX-TEXT-PLOTS###")

;;@doc
;; Bundle nbformat outputs-json with the exact text rows that were rendered
;; for it, so a later reopen can restore the rows without re-parsing JSON.
(define (encode-outputs+rows outputs-json rows)
  (string-append outputs-json "\n" *rows-sep-line* "\n" (string-join rows "\n")))

;;@doc
;; Like `encode-outputs+rows`, but also persists a text-plot's raw
;; delimiter-encoded blob (`json-get-text-plots`'s return value, or the
;; same shape reconstructed for storage) so `decode-stored-text-plots-blob`
;; can restore it on reopen without re-parsing the kernel's result JSON.
;; `text-plots-blob` may be "" or #false (no text-plots) — then the
;; encoding is byte-identical to plain `encode-outputs+rows`.
(define (encode-outputs+rows+text-plots outputs-json rows text-plots-blob)
  (define base (encode-outputs+rows outputs-json rows))
  (if (and text-plots-blob (string? text-plots-blob) (> (string-length text-plots-blob) 0))
      (string-append base "\n" *text-plots-sep-line* "\n" text-plots-blob)
      base))

;;@doc
;; Shared header parse for a stored raw "<hash>\t<body>" value: verifies
;; the hash against `current-hash` and strips the JSON prefix + rows
;; marker, returning whatever follows (rows blob, optionally followed by
;; the text-plots marker + blob) — or #false (missing, stale, or no rows
;; marker at all).
(define (stored-body-remainder raw current-hash)
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
                      (if (not (list? json+rows)) #false (cadr json+rows))))))))))

;;@doc
;; Split a `stored-body-remainder` result into (rows-blob . text-plots-blob)
;; — text-plots-blob is "" when the text-plots marker is absent (a cell
;; stored before this feature, or one with no text-plots).
(define (split-rows-and-text-plots remainder)
  (define marker (string-append "\n" *text-plots-sep-line* "\n"))
  (define parts (split-once remainder marker))
  (if (list? parts)
      (cons (car parts) (cadr parts))
      (cons remainder "")))

;;@doc
;; Given `store-get-for`'s raw "<hash>\t<body>" value and the cell's current
;; source hash, return the stored text rows when the hash matches and the
;; body carries a rows blob, or #false (missing, stale, or no rows).
(define (decode-stored-rows raw current-hash)
  (define remainder (stored-body-remainder raw current-hash))
  (if (not remainder)
      #false
      (let ([rows-blob (car (split-rows-and-text-plots remainder))])
        (if (equal? rows-blob "") '() (string-split rows-blob "\n")))))

;;@doc
;; Given `store-get-for`'s raw "<hash>\t<body>" value and the cell's current
;; source hash, return the stored text-plots blob (the same shape
;; `json-get-text-plots` returns, decodable by `decode-text-plots-blob`)
;; when the hash matches and a blob was stored, or #false (missing, stale,
;; or no text-plots).
(define (decode-stored-text-plots-blob raw current-hash)
  (define remainder (stored-body-remainder raw current-hash))
  (if (not remainder)
      #false
      (let ([tp-blob (cdr (split-rows-and-text-plots remainder))])
        (if (equal? tp-blob "") #false tp-blob))))

;;@doc
;; ASCII information-separator delimiters — must match libnothelix's
;; json_utils::{PLOT_SEP,SECTION_SEP,SPAN_SEP} exactly (json_utils.rs).
(define *tp-plot-sep* (make-string 1 (integer->char 30)))
(define *tp-section-sep* (make-string 1 (integer->char 29)))
(define *tp-span-sep* (make-string 1 (integer->char 31)))

(define (tp-all-numbers? lst)
  (cond
    [(null? lst) #true]
    [(number? (car lst)) (tp-all-numbers? (cdr lst))]
    [else #false]))

;;@doc
;; Decode one "<row>,<start>,<end>,<color>" span string into a 4-element
;; number list, or #false if it doesn't have exactly 4 numeric fields.
(define (decode-one-tp-span span-str)
  (define fields (map string->number (string-split span-str ",")))
  (if (and (= (length fields) 4) (tp-all-numbers? fields)) fields #false))

;;@doc
;; Decode one plot's "<rows>SECTION_SEP<spans>" section into `(rows .
;; spans)` — rows a list of strings, spans a list of 4-number lists ready
;; for `text-plot->styled-rows` (output-render.scm). Malformed spans are
;; dropped defensively rather than raising.
(define (decode-one-text-plot plot-str)
  (define parts (split-once plot-str *tp-section-sep*))
  (define rows-blob (if (list? parts) (car parts) plot-str))
  (define spans-blob (if (list? parts) (cadr parts) ""))
  (define rows (if (equal? rows-blob "") '() (string-split rows-blob "\n")))
  (define spans
    (if (equal? spans-blob "")
        '()
        (filter (lambda (sp) sp)
                (map decode-one-tp-span (string-split spans-blob *tp-span-sep*)))))
  (cons rows spans))

;;@doc
;; Decode a `json-get-text-plots` / stored text-plots blob into a list of
;; `(rows . spans)` pairs, one per plot — '() for "" or #false (no
;; text-plots). The pure counterpart to `json-get-text-plots`'s Rust-side
;; encoding; used both for a freshly executed cell's result JSON and for a
;; stored blob restored on reopen, so the two paths share one decoder.
(define (decode-text-plots-blob blob)
  (if (or (not blob) (equal? blob ""))
      '()
      (map decode-one-text-plot (string-split blob *tp-plot-sep*))))

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
