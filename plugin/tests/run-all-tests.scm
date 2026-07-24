;;; run-all-tests.scm — master runner: runs every suite and prints an aggregated PASS/FAIL summary.

(require "cell-extraction-test.scm")
(require "kernel-persistence-test.scm")
(require "execution-flow-test.scm")
(require "math-image-test.scm")
(require "string-utils-test.scm")
(require "image-cache-test.scm")
(require "output-insert-test.scm")
(require "output-render-test.scm")
(require "param-tweak-test.scm")
(require "picker-test.scm")
(require "cell-state-test.scm")
(require "legacy-migration-test.scm")
(require "cursor-restore-test.scm")
(require "audio-test.scm")
(require "../nothelix/math-image.scm")

(provide run-all-nothelix-tests
         run-cell-extraction-tests
         run-kernel-persistence-tests
         run-execution-flow-tests
         run-math-image-tests
         run-string-utils-tests
         run-image-cache-tests
         run-output-insert-tests
         run-output-render-tests
         run-param-tweak-tests
         run-picker-tests
         run-cell-state-tests
         run-legacy-migration-tests
         run-cursor-restore-tests
         run-audio-tests)

(define (run-all-nothelix-tests)
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
      (cons "string-utils" (run-string-utils-tests))
      (cons "image-cache" (run-image-cache-tests))
      (cons "output-insert" (run-output-insert-tests))
      (cons "output-render" (run-output-render-tests))
      (cons "param-tweak" (run-param-tweak-tests))
      (cons "picker" (run-picker-tests))
      (cons "cell-state" (run-cell-state-tests))
      (cons "legacy-migration" (run-legacy-migration-tests))
      (cons "cursor-restore" (run-cursor-restore-tests))
      (cons "audio" (run-audio-tests))))

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

  (set-math-image-test-mode! #f)

  all-passed)
