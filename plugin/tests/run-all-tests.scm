;;; run-all-tests.scm - Master test runner for Nothelix
;;;
;;; Loads each test suite, enables math-image test mode so no binary
;;; Kitty graphics payloads are emitted, runs every suite, and prints an
;;; aggregated PASS/FAIL summary.

(require "cell-extraction-test.scm")
(require "kernel-persistence-test.scm")
(require "execution-flow-test.scm")
(require "math-image-test.scm")
(require "string-utils-test.scm")
(require "../nothelix/math-image.scm")

(provide run-all-nothelix-tests
         run-cell-extraction-tests
         run-kernel-persistence-tests
         run-execution-flow-tests
         run-math-image-tests
         run-string-utils-tests)

(define (run-all-nothelix-tests)
  ;; Suppress image rendering for the duration of the test run so the
  ;; terminal is not polluted with binary Kitty graphics data.
  (set-math-image-test-mode! #t)

  (displayln "")
  (displayln "============================================================")
  (displayln "                 NOTHELIX TEST SUITE")
  (displayln "============================================================")

  (define suite-results
    (list
      (cons "cell-extraction" (run-cell-extraction-tests))
      (cons "kernel-persistence" (run-kernel-persistence-tests))
      (cons "execution-flow" (run-execution-flow-tests))
      (cons "math-image" (run-math-image-tests))
      (cons "string-utils" (run-string-utils-tests))))

  ;; Suites that don't return a boolean are reported as unknown.
  (define (suite-status passed?)
    (cond
      [(eq? passed? #t) "PASS"]
      [(eq? passed? #f) "FAIL"]
      [else "UNKNOWN"]))

  (displayln "")
  (displayln "------------------------------------------------------------")
  (displayln "Suite summary:")
  (for-each
    (lambda (pair)
      (displayln (string-append "  " (car pair) ": " (suite-status (cdr pair)))))
    suite-results)
  (displayln "------------------------------------------------------------")

  (define all-passed
    (let loop ([xs suite-results])
      (cond
        [(null? xs) #t]
        [(eq? (cdar xs) #t) (loop (cdr xs))]
        [else #f])))

  (displayln "")
  (if all-passed
      (displayln "RESULT: ALL TESTS PASSED")
      (displayln "RESULT: SOME TESTS FAILED"))
  (displayln "============================================================")
  (displayln "")

  ;; Restore normal image rendering.
  (set-math-image-test-mode! #f)

  all-passed)
