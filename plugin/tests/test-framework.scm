;;; test-framework.scm - Shared helpers for the Nothelix test suite.
;;;
;;; Each test suite resets the counters, runs its assertions through the
;;; helpers below, and returns the boolean from `print-test-suite-footer`.
;;; `run-all-tests` toggles math-image test mode so that no binary Kitty
;;; graphics payloads are emitted during the run.

(provide assert-equal
         assert-true
         assert-false
         assert-contains
         assert-not-contains
         reset-test-counters!
         print-test-suite-header
         print-test-suite-footer)

;; Global counters for the currently-running suite.
(define *framework-tests-passed* 0)
(define *framework-tests-failed* 0)

(define (reset-test-counters!)
  (set! *framework-tests-passed* 0)
  (set! *framework-tests-failed* 0))

(define (test-passed!)
  (set! *framework-tests-passed* (+ *framework-tests-passed* 1)))

(define (test-failed!)
  (set! *framework-tests-failed* (+ *framework-tests-failed* 1)))

;; Compare EXPECTED to ACTUAL and record the result. Uses PASS/FAIL
;; labels that remain readable when captured from a headless Helix run.
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

;; Plain-ASCII headers/footers so the report is readable even when box-
;; drawing characters would be mangled by the capturing terminal.
(define (print-test-suite-header name)
  (displayln "")
  (displayln (string-append "-- " name " --")))

;; Print the suite summary and return #true if everything passed.
(define (print-test-suite-footer name)
  (displayln
    (string-append "-- " name " done: "
                   (number->string *framework-tests-passed*) " passed, "
                   (number->string *framework-tests-failed*) " failed --"))
  (= *framework-tests-failed* 0))
