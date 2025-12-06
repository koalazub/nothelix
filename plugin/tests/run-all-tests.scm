;;; run-all-tests.scm - Master test runner for Nothelix

(require "cell-extraction-test.scm")
(require "kernel-persistence-test.scm")
(require "execution-flow-test.scm")

(provide run-all-nothelix-tests)

(define (run-all-nothelix-tests)
  (displayln "\n")
  (displayln "████████████████████████████████████████████████████████████")
  (displayln "███                                                      ███")
  (displayln "███           NOTHELIX TEST SUITE                        ███")
  (displayln "███                                                      ███")
  (displayln "████████████████████████████████████████████████████████████")
  (displayln "\n")

  ;; Track overall results
  (define start-time (current-inexact-milliseconds))

  ;; Run all test suites
  (run-cell-extraction-tests)
  (displayln "\n")
  (run-kernel-persistence-tests)
  (displayln "\n")
  (run-execution-flow-tests)

  (define end-time (current-inexact-milliseconds))
  (define duration-ms (- end-time start-time))
  (define duration-s (/ duration-ms 1000.0))

  (displayln "\n")
  (displayln "████████████████████████████████████████████████████████████")
  (displayln "███                                                      ███")
  (displayln "███           ALL TESTS COMPLETED                        ███")
  (displayln (string-append "███           Duration: " (number->string duration-s) "s"
                           (string-repeat " " (- 40 (string-length (number->string duration-s))))
                           "███"))
  (displayln "███                                                      ███")
  (displayln "████████████████████████████████████████████████████████████")
  (displayln "\n"))

;; Helper to repeat string n times
(define (string-repeat str n)
  (if (<= n 0)
      ""
      (string-append str (string-repeat str (- n 1)))))
