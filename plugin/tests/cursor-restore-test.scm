;;; cursor-restore-test.scm — unit tests for the with-cursor-restore
;;; unwind-protect seam that update-cell-output relies on: the restore + after
;;; must run on BOTH a normal return and a mid-render throw, and a throw must
;;; still propagate (never swallowed, never stranding the cursor). Uses a
;;; doc-id with no pending-restore entry so restore-cursor-for! is a no-op and
;;; no live editor state is needed.

(require "test-framework.scm")
(require "../nothelix/cursor-restore.scm")

(provide run-cursor-restore-tests)

(define (run-cursor-restore-tests)
  (reset-test-counters!)
  (print-test-suite-header "cursor-restore")

  ;; normal return: thunk runs, after runs, value flows through
  (let ([after-count 0] [body-ran #f])
    (with-cursor-restore "no-such-doc"
      (lambda () (set! after-count (+ after-count 1)))
      (lambda () (set! body-ran #t)))
    (assert-true body-ran "normal: render body ran")
    (assert-equal 1 after-count "normal: after ran exactly once"))

  ;; throwing thunk: after still runs AND the error propagates to the caller
  (let ([after-count 0] [caught #f])
    (with-handler
      (lambda (e) (set! caught #t))
      (with-cursor-restore "no-such-doc"
        (lambda () (set! after-count (+ after-count 1)))
        (lambda () (error "render blew up"))))
    (assert-equal 1 after-count "throw: after (restore) still ran")
    (assert-true caught "throw: error propagated, not swallowed"))

  (print-test-suite-footer "cursor-restore"))
