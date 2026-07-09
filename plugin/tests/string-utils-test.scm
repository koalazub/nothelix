;;; string-utils-test.scm — tests for the native-backed string helpers, pinning their edge cases.

(require "test-framework.scm")
(require "../nothelix/string-utils.scm")

(provide run-string-utils-tests)

(define (run-string-utils-tests)
  (reset-test-counters!)
  (print-test-suite-header "string-utils")

  (assert-equal "foo" (string-trim "  foo  ") "string-trim strips both ends")
  (assert-equal "" (string-trim "   ") "string-trim all-whitespace -> empty")
  (assert-equal "" (string-trim #false) "string-trim non-string -> empty")
  (assert-equal "bar" (string-trim "bar") "string-trim no-op")

  (assert-true (string-starts-with? "hello.jl" "hello") "starts-with? positive")
  (assert-false (string-starts-with? "hello" "world") "starts-with? negative")
  (assert-false (string-starts-with? "hi" "hello") "starts-with? prefix longer than subject")
  (assert-false (string-starts-with? #false "x") "starts-with? non-string -> #f")

  (assert-true (string-suffix? "notebook.jl" ".jl") "suffix? positive")
  (assert-false (string-suffix? "notebook.py" ".jl") "suffix? negative")
  (assert-false (string-suffix? "x" "longer") "suffix? suffix longer than subject")
  (assert-false (string-suffix? #false ".jl") "suffix? non-string -> #f")

  (assert-true (string-contains? "abcdef" "cde") "contains? positive")
  (assert-false (string-contains? "abcdef" "xyz") "contains? negative")

  (assert-equal '() (string-split "" ",") "split empty -> '()")
  (assert-equal '() (string-split #false ",") "split non-string -> '()")
  (assert-equal (list "abc") (string-split "abc" ",") "split no-match -> one piece")
  (assert-equal (list "a" "b" "c") (string-split "a,b,c" ",") "split basic")
  (assert-equal (list "a" "" "b") (string-split "a,,b" ",") "split keeps empty field")
  (assert-equal (list "a" "") (string-split "a," ",") "split trailing delim")
  (assert-equal (list "" "a") (string-split ",a" ",") "split leading delim")
  (assert-equal (list "a" "b" "") (string-split "a\nb\n" "\n") "split on newline")

  (let* ([rs (make-string 1 (integer->char 30))]
         [seg (make-string 120 #\x)]
         [reply (build-rs-batch 200 seg rs)]
         [parts (string-split reply rs)])
    (assert-equal 200 (length parts) "split RS-batch -> all pieces")
    (assert-equal seg (car parts) "split RS-batch first piece intact")
    (assert-equal seg (list-ref parts 199) "split RS-batch last piece intact"))

  (print-test-suite-footer "string-utils"))

(define (build-rs-batch n seg sep)
  (define (pieces k acc) (if (<= k 0) acc (pieces (- k 1) (cons seg acc))))
  (apply string-append
         (let loop ([ps (pieces n '())] [out '()] [first #true])
           (cond
             [(null? ps) (reverse out)]
             [first (loop (cdr ps) (cons (car ps) out) #false)]
             [else (loop (cdr ps) (cons (car ps) (cons sep out)) #false)]))))
