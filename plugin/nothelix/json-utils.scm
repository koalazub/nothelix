;;; json-utils.scm - Minimal JSON parsing helpers shared across modules
;;;
;;; conceal.scm parses a JSON array of `{"offset": N, "replacement": "X"}`
;;; records produced by the Rust FFI. chart-viewer.scm parses a JSON array
;;; of string literals produced by the chart renderer. Both need the same
;;; low-level primitives: walk past whitespace, find a character,
;;; decode a \-escaped string body, and count its on-the-wire length.
;;; Those primitives live here so neither module ships its own copy.
;;;
;;; These helpers are deliberately small and allocation-free (they work on
;;; the source string directly with position indices). None of them handle
;;; the full JSON grammar — the inputs come from `serde_json::to_string`
;;; in libnothelix, which produces predictable ASCII-only output.

(provide json-skip-whitespace
         json-find-char
         json-find-non-digit
         json-find-string-end
         json-extract-string
         json-string-raw-length)

;;@doc
;; Advance `start` past ASCII whitespace (space and tab).
;; Returns the first non-whitespace position, or `(string-length str)` if
;; the rest of the string is whitespace.
(define (json-skip-whitespace str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(or (char=? (string-ref str i) #\space)
           (char=? (string-ref str i) #\tab))
       (loop (+ i 1))]
      [else i])))

;;@doc
;; Return the first position `>= start` where `ch` appears in `str`,
;; or `(string-length str)` if `ch` is not found in the remainder.
(define (json-find-char str ch start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(char=? (string-ref str i) ch) i]
      [else (loop (+ i 1))])))

;;@doc
;; Return the first position `>= start` where a non-digit (and non-minus)
;; character appears. Used to read the end of a JSON integer literal.
(define (json-find-non-digit str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(and (char>=? (string-ref str i) #\0)
            (char<=? (string-ref str i) #\9))
       (loop (+ i 1))]
      [(char=? (string-ref str i) #\-) (loop (+ i 1))]
      [else i])))

;;@doc
;; Given a position `start` that points JUST past an opening `"`, return
;; the position of the closing `"`, skipping over `\-escaped characters.
;; Returns `#false` if no closing quote is found.
(define (json-find-string-end str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) #false]
      [(char=? (string-ref str i) #\\) (loop (+ i 2))]
      [(char=? (string-ref str i) #\") i]
      [else (loop (+ i 1))])))

;;@doc
;; Given a position `start` that points JUST past an opening `"`, decode
;; the string body up to the closing quote and return it.
;; `\x` escapes drop the backslash and keep the following character,
;; which is sufficient for the ASCII payloads we produce in Rust.
(define (json-extract-string str start)
  (let loop ([i start] [chars '()])
    (cond
      [(>= i (string-length str)) (list->string (reverse chars))]
      [(char=? (string-ref str i) #\\)
       (if (< (+ i 1) (string-length str))
           (loop (+ i 2) (cons (string-ref str (+ i 1)) chars))
           (list->string (reverse chars)))]
      [(char=? (string-ref str i) #\") (list->string (reverse chars))]
      [else (loop (+ i 1) (cons (string-ref str i) chars))])))

;;@doc
;; Count the raw (on-the-wire) length of a JSON string from position
;; `start` up to (but not including) the closing quote. Used when the
;; caller needs to know how far to advance in the source text after
;; extracting the decoded content.
(define (json-string-raw-length str start)
  (let loop ([i start] [len 0])
    (cond
      [(>= i (string-length str)) len]
      [(char=? (string-ref str i) #\\)
       (if (< (+ i 1) (string-length str))
           (loop (+ i 2) (+ len 2))
           len)]
      [(char=? (string-ref str i) #\") len]
      [else (loop (+ i 1) (+ len 1))])))
