;;; string-utils.scm — String manipulation and JSON parsing utilities

(provide string-trim
         string-starts-with?
         string-suffix?
         string-contains?
         string-join
         string-split
         string-replace-all
         char->number
         json-get-string
         sanitise-error-message
         truncate-string)

(define (string-find str substr start)
  (let loop ([pos start])
    (cond
      [(>= pos (string-length str)) #f]
      [(>= (+ pos (string-length substr)) (string-length str)) #f]
      [(equal? (substring str pos (+ pos (string-length substr))) substr) pos]
      [else (loop (+ pos 1))])))

(define (string-trim-left s)
  (if (string? s) (trim-start s) ""))

(define (string-trim str)
  (if (string? str) (trim str) ""))

(define (string-starts-with? str prefix)
  (and (string? str) (string? prefix) (starts-with? str prefix)))

(define (string-suffix? str suffix)
  (and (string? str) (string? suffix) (ends-with? str suffix)))

(define (string-contains? str substr)
  (and (string? str)
       (string? substr)
       (>= (string-length str) (string-length substr))
       (let loop ([i 0])
         (cond
           [(> (+ i (string-length substr)) (string-length str)) #f]
           [(equal? (substring str i (+ i (string-length substr))) substr) #t]
           [else (loop (+ i 1))]))))

(define (string-join strings sep)
  (if (null? strings)
      ""
      (let loop ([rest (cdr strings)] [result (car strings)])
        (if (null? rest)
            result
            (loop (cdr rest) (string-append result sep (car rest)))))))

(define (string-split str delim)
  (if (or (not str) (equal? str ""))
      '()
      (split-many str delim)))

(define (char->number c)
  (cond
    [(eqv? c #\0) 0]
    [(eqv? c #\1) 1]
    [(eqv? c #\2) 2]
    [(eqv? c #\3) 3]
    [(eqv? c #\4) 4]
    [(eqv? c #\5) 5]
    [(eqv? c #\6) 6]
    [(eqv? c #\7) 7]
    [(eqv? c #\8) 8]
    [(eqv? c #\9) 9]
    [else #f]))

;; Don't shadow Steel's native string->number: an integer-only shadow
;; breaks negative/float parsing. char->number stays (picker.scm uses it).

(define (string-replace-all str old new)
  (define old-len (string-length old))
  (define (replace-at-pos s pos)
    (string-append (substring s 0 pos) new (substring s (+ pos old-len) (string-length s))))
  (let loop ([s str] [pos 0])
    (if (>= pos (string-length s))
        s
        (if (and (<= (+ pos old-len) (string-length s))
                 (equal? (substring s pos (+ pos old-len)) old))
            (loop (replace-at-pos s pos) (+ pos (string-length new)))
            (loop s (+ pos 1))))))

(define (find-json-string-end str start)
  (let loop ([pos start])
    (cond
      [(>= pos (string-length str)) #f]
      [(eqv? (string-ref str pos) #\\)
       (loop (+ pos 2))]
      [(eqv? (string-ref str pos) #\")
       pos]
      [else (loop (+ pos 1))])))

(define (json-decode-string str)
  (let loop ([chars (string->list str)] [acc '()])
    (cond
      [(null? chars) (list->string (reverse acc))]
      [(and (eqv? (car chars) #\\) (not (null? (cdr chars))))
       (define next (cadr chars))
       (define decoded
         (cond
           [(eqv? next #\n) #\newline]
           [(eqv? next #\t) #\tab]
           [(eqv? next #\r) #\return]
           [(eqv? next #\\) #\\]
           [(eqv? next #\") #\"]
           [else next]))
       (loop (cddr chars) (cons decoded acc))]
      [else (loop (cdr chars) (cons (car chars) acc))])))

(define (json-get-string json-str key)
  (define pattern (string-append "\"" key "\":"))
  (define key-pos (string-find json-str pattern 0))
  (if (not key-pos)
      #f
      (let* ([value-start (+ key-pos (string-length pattern))]
             [rest (substring json-str value-start (string-length json-str))]
             [trimmed (string-trim-left rest)])
        (cond
          [(string-starts-with? trimmed "\"")
           (define end-quote (find-json-string-end trimmed 1))
           (if end-quote
               (json-decode-string (substring trimmed 1 end-quote))
               "")]
          [(string-starts-with? trimmed "true") "true"]
          [(string-starts-with? trimmed "false") "false"]
          [(string-starts-with? trimmed "null") ""]
          [else
           (define end-pos (or (string-find trimmed "," 0)
                              (string-find trimmed "}" 0)
                              (string-length trimmed)))
           (string-trim (substring trimmed 0 end-pos))]))))

;; Error message utilities

(define (truncate-string str max-len)
  (if (or (not (string? str)) (<= (string-length str) max-len))
      str
      (string-append (substring str 0 (- max-len 3)) "...")))

(define (sanitise-error-message msg)
  (if (not (string? msg))
      "Unknown error"
      (let* ([error-text (or (json-get-string msg "error") msg)]
             [clean1 (string-replace-all error-text "\\n" " ")]
             [clean2 (string-replace-all clean1 "\n" " ")]
             [clean3 (string-replace-all clean2 "\\r" "")]
             [clean4 (string-replace-all clean3 "\r" "")]
             [clean5 (string-replace-all clean4 "\\t" " ")]
             [clean6 (string-replace-all clean5 "\t" " ")]
             [trimmed (string-trim clean6)])
        (truncate-string trimmed 120))))
