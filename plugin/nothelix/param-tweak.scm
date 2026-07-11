;;; param-tweak.scm — declare a numeric @param in a cell, nudge it, re-render.

(require "string-utils.scm")
(require "common.scm")

(provide parse-param-line
         format-number
         nudge-param-value
         decimals-of
         find-param-target-line
         collect-assigned-names
         token-references?
         scan-stale-lines)

(define (split-on-first s ch)
  (let loop ([i 0])
    (cond
      [(>= i (string-length s)) #false]
      [(char=? (string-ref s i) ch)
       (cons (substring s 0 i) (substring s (+ i 1) (string-length s)))]
      [else (loop (+ i 1))])))

(define (literal-kind value-str)
  (if (string-contains? value-str ".") 'float 'int))

(define (tokens-of s)
  (filter (lambda (t) (> (string-length t) 0))
          (string-split (string-replace-all (string-trim s) "\t" " ") " ")))

(define (parse-range tok)
  (define parts (split-on-first tok #\:))
  (and parts
       (let ([lo (string->number (string-trim (car parts)))]
             [hi (string->number (string-trim (cdr parts)))])
         (and lo hi (cons lo hi)))))

(define (default-step lo hi kind)
  (if (eq? kind 'int) 1 (/ (- hi lo) 100)))

(define (parse-param-line line)
  (define halves (split-on-first line #\#))
  (and halves
       (let* ([code (car halves)]
              [comment (string-trim (cdr halves))])
         (and (string-starts-with? comment "@param")
              (let* ([spec (string-trim (substring comment 6 (string-length comment)))]
                     [code-parts (split-on-first code #\=)])
                (and code-parts
                     (let* ([name (string-trim (car code-parts))]
                            [value-str (string-trim (cdr code-parts))]
                            [toks (tokens-of spec)]
                            [rng (and (pair? toks) (parse-range (car toks)))])
                       (and rng
                            (> (string-length name) 0)
                            (string->number value-str)
                            (let* ([lo (car rng)]
                                   [hi (cdr rng)]
                                   [kind (literal-kind value-str)]
                                   [step (parse-step toks lo hi kind)])
                              (list name value-str lo hi step kind))))))))))

(define (parse-step toks lo hi kind)
  (let loop ([ts toks])
    (cond
      [(null? ts) (default-step lo hi kind)]
      [(and (equal? (car ts) "step") (pair? (cdr ts)))
       (or (string->number (cadr ts)) (default-step lo hi kind))]
      [else (loop (cdr ts))])))

(define *decimals-of-max* 12)
(define *decimals-of-epsilon* 1e-6)

(define (decimals-of step)
  (define mag (abs step))
  (if (>= mag 1)
      0
      (let loop ([n mag] [count 0])
        (if (or (>= count *decimals-of-max*)
                (< (abs (- n (round n))) *decimals-of-epsilon*))
            count
            (loop (* n 10) (+ count 1))))))

(define (format-number n decimals)
  (if (<= decimals 0)
      (number->string (inexact->exact (round n)))
      (let* ([scale (expt 10 decimals)]
             [scaled (inexact->exact (round (* n scale)))]
             [neg (< scaled 0)]
             [mag (abs scaled)]
             [int-part (quotient mag scale)]
             [frac-part (remainder mag scale)]
             [frac-str (number->string frac-part)]
             [padded (string-append
                       (make-string (- decimals (string-length frac-str)) #\0)
                       frac-str)])
        (string-append (if neg "-" "")
                       (number->string int-part) "." padded))))

(define (nudge-param-value current lo hi step dir)
  (define steps (round (/ (- current lo) step)))
  (define next (+ steps dir))
  (define max-steps (floor (/ (- hi lo) step)))
  (define clamped (max 0 (min next max-steps)))
  (define raw (+ lo (* clamped step)))
  (if (and (integer? lo) (integer? step)) (inexact->exact (round raw)) raw))

(define (find-param-target-line get-line total-lines cursor-line)
  (let loop ([i (min cursor-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(cell-marker? (string-trim (get-line i))) #false]
      [(parse-param-line (get-line i)) i]
      [else (loop (- i 1))])))

(define (collect-assigned-names get-line cell-start cell-end)
  (let loop ([i cell-start] [acc '()])
    (if (>= i cell-end)
        (reverse acc)
        (let ([p (parse-param-line (get-line i))])
          (loop (+ i 1) (if p (cons (car p) acc) acc))))))

(define *ident-chars* "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_!")

(define (ident-char? c)
  (or (string-contains? *ident-chars* (string c))
      (>= (char->integer c) 128)))

(define (token-references? code-line name)
  (define nlen (string-length name))
  (define llen (string-length code-line))
  (let loop ([i 0])
    (cond
      [(> (+ i nlen) llen) #false]
      [(and (equal? (substring code-line i (+ i nlen)) name)
            (or (= i 0) (not (ident-char? (string-ref code-line (- i 1)))))
            (or (= (+ i nlen) llen) (not (ident-char? (string-ref code-line (+ i nlen))))))
       #true]
      [else (loop (+ i 1))])))

(define (any-name-referenced? code-line names)
  (cond
    [(null? names) #false]
    [(token-references? code-line (car names)) #true]
    [else (any-name-referenced? code-line (cdr names))]))

(define (scan-stale-lines get-line total-lines from-line names)
  (let loop ([i (+ from-line 1)] [current-marker #false] [hit #false] [acc '()])
    (cond
      [(>= i total-lines)
       (reverse (if (and current-marker hit) (cons current-marker acc) acc))]
      [(cell-marker? (string-trim (get-line i)))
       (loop (+ i 1) i #false
             (if (and current-marker hit) (cons current-marker acc) acc))]
      [(and current-marker (any-name-referenced? (get-line i) names))
       (loop (+ i 1) current-marker #true acc)]
      [else (loop (+ i 1) current-marker hit acc)])))
