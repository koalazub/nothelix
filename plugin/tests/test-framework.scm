;;; test-framework.scm — shared assert helpers and PASS/FAIL reporting for the test suite.

(provide assert-equal
         assert-true
         assert-false
         assert-contains
         assert-not-contains
         reset-test-counters!
         print-test-suite-header
         print-test-suite-footer)

(define *framework-tests-passed* 0)
(define *framework-tests-failed* 0)

(define (reset-test-counters!)
  (set! *framework-tests-passed* 0)
  (set! *framework-tests-failed* 0))

(define (test-passed!)
  (set! *framework-tests-passed* (+ *framework-tests-passed* 1)))

(define (test-failed!)
  (set! *framework-tests-failed* (+ *framework-tests-failed* 1)))

(define (assert-equal expected actual description)
  (if (equal? expected actual)
      (begin
        (test-passed!)
        (displayln (string-append "  PASS: " description)))
      (begin
        (test-failed!)
        (displayln (string-append "  FAIL: " description))
        (display "    Expected: ")
        (displayln expected)
        (display "    Actual:   ")
        (displayln actual))))

(define (assert-true condition description)
  (assert-equal #t condition description))

(define (assert-false condition description)
  (assert-equal #f condition description))

(define (assert-contains haystack needle description)
  (assert-true (string-contains? haystack needle) description))

(define (assert-not-contains haystack needle description)
  (assert-false (string-contains? haystack needle) description))

(define (print-test-suite-header name)
  (displayln "")
  (displayln (string-append "-- " name " --")))

(define (print-test-suite-footer name)
  (displayln
    (string-append "-- " name " done: "
                   (number->string *framework-tests-passed*) " passed, "
                   (number->string *framework-tests-failed*) " failed --"))
  (= *framework-tests-failed* 0))
