;;; json-utils.scm — Minimal JSON parsing helpers shared across modules

(provide json-skip-whitespace
         json-find-char
         json-find-non-digit
         json-find-string-end
         json-extract-string
         json-string-raw-length)

;;@doc
;; Advance past ASCII whitespace; return the first non-whitespace position.
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
;; Return the first position >= start where a non-digit (non-minus) char appears.
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
;; From a position just past an opening quote, return the closing quote position, or #false.
(define (json-find-string-end str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) #false]
      [(char=? (string-ref str i) #\\) (loop (+ i 2))]
      [(char=? (string-ref str i) #\") i]
      [else (loop (+ i 1))])))

;;@doc
;; From a position just past an opening quote, decode the string body up to the closing quote.
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
;; Count the raw on-the-wire length of a JSON string body up to the closing quote.
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
