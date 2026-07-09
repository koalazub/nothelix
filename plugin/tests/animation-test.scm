;;; animation-test.scm — behavioural tests mirroring nothelix/animation.scm's lifecycle predicates and hook handlers (kept in sync by hand).

(provide run-animation-tests)

(define *tests-passed* 0)
(define *tests-failed* 0)
(define *failures* '())

(define (assert-equal actual expected description)
  (if (equal? actual expected)
      (begin
        (set! *tests-passed* (+ *tests-passed* 1))
        (displayln (string-append "  ✓ " description)))
      (begin
        (set! *tests-failed* (+ *tests-failed* 1))
        (set! *failures*
              (cons (string-append description
                                   "\n    expected: " (to-string expected)
                                   "\n    actual:   " (to-string actual))
                    *failures*))
        (displayln (string-append "  ✗ " description))
        (display "    Expected: ") (displayln expected)
        (display "    Actual:   ") (displayln actual))))

(define (assert-true v description) (assert-equal v #t description))
(define (assert-false v description) (assert-equal v #f description))

(define (to-string v)
  (cond
    [(string? v) v]
    [(number? v) (number->string v)]
    [(boolean? v) (if v "#t" "#f")]
    [(symbol? v) (symbol->string v)]
    [(null? v) "()"]
    [else "<...>"]))

(define (animation-state-active? st)
  (and st
       (hash-try-get st 'focused?)
       (hash-try-get st 'visible?)
       (not (hash-try-get st 'manual-paused?))
       (eq? (hash-try-get st 'status) 'playing)))

(define (make-state)
  (hash 'char-idx 0
        'doc-id 0
        'height 1
        'focused? #t
        'visible? #t
        'manual-paused? #f
        'status 'playing))

(define (state-with st . kvs)
  (let loop ([s st] [pairs kvs])
    (cond
      [(null? pairs) s]
      [(null? (cdr pairs)) s]
      [else (loop (hash-insert s (car pairs) (cadr pairs)) (cddr pairs))])))

(define (test-active-predicate)
  (displayln "animation-state-active? truth table")
  (assert-true (animation-state-active? (make-state))
    "all-true state is active")
  (assert-false (animation-state-active? (state-with (make-state) 'focused? #f))
    "unfocused state is inactive")
  (assert-false (animation-state-active? (state-with (make-state) 'visible? #f))
    "invisible state is inactive")
  (assert-false (animation-state-active? (state-with (make-state) 'manual-paused? #t))
    "manual-paused state is inactive")
  (assert-false (animation-state-active? (state-with (make-state) 'status 'finished))
    "finished state is inactive")
  (assert-false (animation-state-active? (state-with (make-state) 'status 'errored))
    "errored state is inactive")
  (assert-false (animation-state-active? #f)
    "nil state is inactive (no panic)"))

(define (apply-focus-lost! anims doc-id)
  (define result anims)
  (for-each
    (lambda (eid)
      (define st (hash-try-get result eid))
      (when (equal? (hash-try-get st 'doc-id) doc-id)
        (set! result (hash-insert result eid (hash-insert st 'focused? #f)))))
    (hash-keys->list result))
  result)

(define (apply-focus-gained! anims doc-id)
  (define result anims)
  (for-each
    (lambda (eid)
      (define st (hash-try-get result eid))
      (when (equal? (hash-try-get st 'doc-id) doc-id)
        (set! result (hash-insert result eid (hash-insert st 'focused? #t)))))
    (hash-keys->list result))
  result)

(define (test-focus-handlers)
  (displayln "focus-lost / focus-gained handlers")
  (define anims
    (hash 100 (state-with (make-state) 'doc-id 1)
          200 (state-with (make-state) 'doc-id 1)
          300 (state-with (make-state) 'doc-id 2)))
  (define after-lost (apply-focus-lost! anims 1))
  (assert-false (hash-try-get (hash-try-get after-lost 100) 'focused?)
    "engine 100 (doc 1) loses focus")
  (assert-false (hash-try-get (hash-try-get after-lost 200) 'focused?)
    "engine 200 (doc 1) loses focus")
  (assert-true (hash-try-get (hash-try-get after-lost 300) 'focused?)
    "engine 300 (doc 2) keeps focus")
  (define after-gained (apply-focus-gained! after-lost 1))
  (assert-true (hash-try-get (hash-try-get after-gained 100) 'focused?)
    "engine 100 regains focus on doc 1 focus-gained"))

(define (apply-viewport-changed! anims doc-id anchor height)
  (define visible-end (+ anchor (* (max 1 height) 200)))
  (define result anims)
  (for-each
    (lambda (eid)
      (define st (hash-try-get result eid))
      (when (equal? (hash-try-get st 'doc-id) doc-id)
        (define cell-anchor (hash-try-get st 'char-idx))
        (define newly-visible?
          (and (>= cell-anchor anchor) (< cell-anchor visible-end)))
        (set! result (hash-insert result eid
                                  (hash-insert st 'visible? newly-visible?)))))
    (hash-keys->list result))
  result)

(define (test-viewport-handler)
  (displayln "viewport-changed visibility logic")
  (define anims
    (hash 1 (state-with (make-state) 'doc-id 1 'char-idx 50)
          2 (state-with (make-state) 'doc-id 1 'char-idx 5000)
          3 (state-with (make-state) 'doc-id 2 'char-idx 50)))
  (define after (apply-viewport-changed! anims 1 0 10))
  (assert-true (hash-try-get (hash-try-get after 1) 'visible?)
    "engine in viewport stays visible")
  (assert-false (hash-try-get (hash-try-get after 2) 'visible?)
    "engine outside viewport becomes invisible")
  (assert-true (hash-try-get (hash-try-get after 3) 'visible?)
    "engine in different doc unaffected")
  (define after2 (apply-viewport-changed! after 1 10000 10))
  (assert-false (hash-try-get (hash-try-get after2 1) 'visible?)
    "engine becomes invisible when scrolled past"))

(define (toggle-manual-pause anims eid)
  (define st (hash-try-get anims eid))
  (cond
    [(not st) anims]
    [else
     (define cur (hash-try-get st 'manual-paused?))
     (hash-insert anims eid (hash-insert st 'manual-paused? (not cur)))]))

(define (test-toggle)
  (displayln "manual-pause toggle")
  (define anims (hash 7 (make-state)))
  (define after (toggle-manual-pause anims 7))
  (assert-true (hash-try-get (hash-try-get after 7) 'manual-paused?)
    "first toggle pauses")
  (assert-false (animation-state-active? (hash-try-get after 7))
    "manually-paused state is no longer active")
  (define after2 (toggle-manual-pause after 7))
  (assert-false (hash-try-get (hash-try-get after2 7) 'manual-paused?)
    "second toggle resumes")
  (assert-true (animation-state-active? (hash-try-get after2 7))
    "resumed state is active again"))

(define (test-gate-composition)
  (displayln "compound gate: any false = inactive")
  (define st0 (make-state))
  (define st1 (hash-insert st0 'focused? #f))
  (define st2 (hash-insert st1 'manual-paused? #t))
  (define st3 (hash-insert st2 'focused? #t))
  (define st4 (hash-insert st3 'visible? #f))
  (assert-false (animation-state-active? st1) "after focus-lost: inactive")
  (assert-false (animation-state-active? st2) "after manual-pause: inactive")
  (assert-false (animation-state-active? st3) "still manual-paused: inactive")
  (assert-false (animation-state-active? st4) "and offscreen: inactive")
  (define st5 (hash-insert st4 'manual-paused? #f))
  (define st6 (hash-insert st5 'visible? #t))
  (assert-true (animation-state-active? st6)
    "all gates cleared: active again"))

(define *tick-count* 0)
(define *reschedule-count* 0)

(define (simulate-tick-loop initial-state max-iters)
  (set! *tick-count* 0)
  (set! *reschedule-count* 0)
  (define st initial-state)
  (let loop ([iter 0] [active? (animation-state-active? st)])
    (cond
      [(or (not active?) (>= iter max-iters)) #f]
      [else
       (set! *tick-count* (+ *tick-count* 1))
       (set! *reschedule-count* (+ *reschedule-count* 1))
       (loop (+ iter 1) (animation-state-active? st))])))

(define (test-schedule-tick-gate)
  (displayln "schedule-tick gate exits when state goes inactive")
  (simulate-tick-loop (make-state) 5)
  (assert-equal *tick-count* 5 "active engine ticks max-iters times")
  (simulate-tick-loop (state-with (make-state) 'focused? #f) 5)
  (assert-equal *tick-count* 0 "unfocused engine never ticks")
  (simulate-tick-loop (state-with (make-state) 'visible? #f) 5)
  (assert-equal *tick-count* 0 "offscreen engine never ticks")
  (simulate-tick-loop (state-with (make-state) 'manual-paused? #t) 5)
  (assert-equal *tick-count* 0 "manually-paused engine never ticks")
  (simulate-tick-loop (state-with (make-state) 'status 'finished) 5)
  (assert-equal *tick-count* 0 "finished engine never ticks"))

(define (run-animation-tests)
  (displayln "")
  (displayln "── Animation plugin behavioural tests ──")
  (set! *tests-passed* 0)
  (set! *tests-failed* 0)
  (set! *failures* '())

  (test-active-predicate)
  (test-focus-handlers)
  (test-viewport-handler)
  (test-toggle)
  (test-gate-composition)
  (test-schedule-tick-gate)

  (displayln "")
  (displayln (string-append "Animation: "
                            (number->string *tests-passed*) " passed, "
                            (number->string *tests-failed*) " failed"))
  (when (> *tests-failed* 0)
    (displayln "Failures:")
    (for-each (lambda (f)
                (displayln (string-append "  - " f)))
              *failures*))
  (= *tests-failed* 0))

(run-animation-tests)
