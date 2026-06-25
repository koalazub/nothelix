;;; string-utils-test.scm - Tests for the native-backed string helpers.
;;;
;;; string-trim / string-starts-with? / string-suffix? now delegate to
;;; Steel's native `steel/strings` builtins (trim / starts-with? /
;;; ends-with?). These assertions pin the edge cases the old hand-rolled
;;; loops guarded — non-string input, and a prefix/suffix longer than the
;;; subject string — so a regression surfaces in `:run-all-nothelix-tests`.

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

  (print-test-suite-footer "string-utils"))
