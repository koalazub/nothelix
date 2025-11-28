;;; string-utils.scm - String manipulation and JSON parsing utilities

(provide string-find
         string-trim-left
         string-trim
         string-starts-with?
         string-suffix?
         string-contains?
         string-join
         string->bytes
         string->number
         string-replace-all
         char->number
         json-get-string)

(define (string-find str substr start)
  (let loop ([pos start])
    (cond
      [(>= pos (string-length str)) #f]
      [(>= (+ pos (string-length substr)) (string-length str)) #f]
      [(equal? (substring str pos (+ pos (string-length substr))) substr) pos]
      [else (loop (+ pos 1))])))

(define (string-trim-left s)
  (if (not (string? s))
      ""
      (let loop ([i 0])
        (cond
          [(>= i (string-length s)) ""]
          [(char-whitespace? (string-ref s i)) (loop (+ i 1))]
          [else (substring s i (string-length s))]))))

(define (string-trim str)
  (define (trim-start s)
    (let loop ([i 0])
      (cond
        [(>= i (string-length s)) ""]
        [(char-whitespace? (string-ref s i)) (loop (+ i 1))]
        [else (substring s i (string-length s))])))

  (define (trim-end s)
    (let loop ([i (- (string-length s) 1)])
      (cond
        [(< i 0) ""]
        [(char-whitespace? (string-ref s i)) (loop (- i 1))]
        [else (substring s 0 (+ i 1))])))

  (cond
    [(not str) ""]
    [(void? str) ""]
    [(not (string? str)) ""]
    [else (trim-end (trim-start str))]))

(define (string-starts-with? str prefix)
  (and (string? str)
       (string? prefix)
       (>= (string-length str) (string-length prefix))
       (equal? (substring str 0 (string-length prefix)) prefix)))

(define (string-suffix? str suffix)
  (and (string? str)
       (string? suffix)
       (let [(str-len (string-length str))
             (suf-len (string-length suffix))]
         (and (>= str-len suf-len)
              (equal? (substring str (- str-len suf-len) str-len) suffix)))))

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

(define (string->bytes str)
  (if (not (string? str))
      (list->vector '())
      (list->vector (map char->integer (string->list str)))))

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

(define (string->number s)
  (if (not (string? s))
      #f
      (let loop ([chars (string->list s)] [acc 0])
        (cond
          [(null? chars) acc]
          [else
           (define digit (char->number (car chars)))
           (if digit
               (loop (cdr chars) (+ (* acc 10) digit))
               #f)]))))

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

;; Find the end of a JSON string, properly handling escape sequences
;; Returns the index of the closing quote, or #f if not found
(define (find-json-string-end str start)
  (let loop ([pos start])
    (cond
      [(>= pos (string-length str)) #f]
      [(eqv? (string-ref str pos) #\\)
       ;; Escape sequence - skip the next character
       (loop (+ pos 2))]
      [(eqv? (string-ref str pos) #\")
       pos]  ; Found unescaped closing quote
      [else (loop (+ pos 1))])))

;; Decode JSON escape sequences in a string
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
           [else next]))  ; Unknown escape, keep as-is
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
           ;; Find end quote properly (skip escaped quotes)
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
