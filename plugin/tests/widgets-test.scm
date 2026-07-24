;;; widgets-test.scm — pure-logic tests for the shared widget contract: the
;;; ]w/[w walk ordering + wrap-around, the modal-shell vtable dispatch, the
;;; seek-ladder plumbing that dispatch drives (reusing audio's ladder-next /
;;; ladder-step-index), the debounced re-run generation counter, and the
;;; `widgets` config knob (default on + the disabled short-circuit).

(require "test-framework.scm")
(require "../nothelix/widgets.scm")
(require "../nothelix/audio.scm")
(require "../nothelix/project-config.scm")

(provide run-widgets-tests)

(define (run-widgets-tests)
  (reset-test-counters!)
  (print-test-suite-header "widgets")

  ;; walk-line-order: registry anchors sort ascending and de-duplicate
  (assert-equal (list 2 5 9) (walk-line-order (list 9 2 5)) "walk order: sorted ascending")
  (assert-equal (list 2 5 9) (walk-line-order (list 5 9 2 5 9)) "walk order: duplicate anchors dropped")
  (assert-equal '() (walk-line-order '()) "walk order: empty stays empty")

  ;; next-/prev-walk-line: wrap-around walk over sorted anchors
  (define anchors (list 2 5 9))
  (assert-equal 5 (next-walk-line anchors 3) "walk next: first anchor below the cursor")
  (assert-equal 2 (prev-walk-line anchors 3) "walk prev: first anchor above the cursor")
  (assert-equal 2 (next-walk-line anchors 9) "walk next: wraps past the last to the first")
  (assert-equal 5 (prev-walk-line anchors 9) "walk prev: anchor just above the last")
  (assert-equal 9 (prev-walk-line anchors 0) "walk prev: wraps before the first to the last")
  (assert-equal 2 (next-walk-line anchors 0) "walk next: from the top lands on the first")
  (assert-equal 2 (next-walk-line anchors 10) "walk next: past the end wraps to the first")
  (assert-equal 9 (prev-walk-line anchors 10) "walk prev: from past the end lands on the last")
  (assert-equal 5 (next-walk-line anchors 2) "walk next: on an anchor moves to the next")
  (assert-equal 2 (prev-walk-line anchors 5) "walk prev: on an anchor moves to the previous")
  (assert-false (next-walk-line '() 3) "walk next: no anchors yields #false")
  (assert-false (prev-walk-line '() 3) "walk prev: no anchors yields #false")

  ;; modal-shell vtable dispatch: h/l -> move ±1, j/k -> step ±1, enter -> apply+close, esc -> close
  (define moved (box 0))
  (define stepped (box 0))
  (define applied (box #false))
  (define vt (hash 'move (lambda (st d) (set-box! moved (+ (unbox moved) d)))
                   'step (lambda (st d) (set-box! stepped (+ (unbox stepped) d)))
                   'apply (lambda (st) (set-box! applied #true))))
  (assert-equal 'consume (dispatch-modal-action vt #false 'right) "dispatch: right consumes")
  (assert-equal 1 (unbox moved) "dispatch: right moves the value +1")
  (dispatch-modal-action vt #false 'left)
  (assert-equal 0 (unbox moved) "dispatch: left moves the value -1")
  (assert-equal 'consume (dispatch-modal-action vt #false 'down) "dispatch: down consumes")
  (assert-equal 1 (unbox stepped) "dispatch: down steps the granularity +1")
  (dispatch-modal-action vt #false 'up)
  (assert-equal 0 (unbox stepped) "dispatch: up steps the granularity -1")
  (assert-equal 'close (dispatch-modal-action vt #false 'apply) "dispatch: apply closes the modal")
  (assert-true (unbox applied) "dispatch: apply ran the vtable apply")
  (set-box! applied #false)
  (assert-equal 'close (dispatch-modal-action vt #false 'close) "dispatch: close closes the modal")
  (assert-false (unbox applied) "dispatch: close leaves without applying")
  (assert-equal 'consume (dispatch-modal-action vt #false 'noop) "dispatch: an unmapped key is consumed")

  ;; ladder plumbing: the shell's step drives a seek ladder via audio's primitives
  (define ladder (list 100 500 1000 5000 30000))
  (define idx (box 0))
  (define ladder-vt
    (hash 'step (lambda (st d) (set-box! idx (ladder-step-index (unbox idx) d (length ladder))))
          'move (lambda (st d) (void))
          'apply (lambda (st) (void))))
  (dispatch-modal-action ladder-vt #false 'down)
  (assert-equal 1 (unbox idx) "ladder: down coarsens one rung")
  (dispatch-modal-action ladder-vt #false 'up)
  (assert-equal 0 (unbox idx) "ladder: up finens one rung")
  (dispatch-modal-action ladder-vt #false 'up)
  (assert-equal 0 (unbox idx) "ladder: finer clamps at the base rung")
  (assert-equal (cons 1 500) (ladder-next ladder 0 1000 1500 700)
                "ladder-next: a press within the accel window escalates one rung")

  ;; debounce generation counter: only the latest scheduled re-run survives
  (define g1 (bump-widget-rerun-generation!))
  (define g2 (bump-widget-rerun-generation!))
  (assert-true (> g2 g1) "debounce: each bump increases the generation")
  (assert-false (widget-rerun-current? g1) "debounce: a superseded generation is stale")
  (assert-true (widget-rerun-current? g2) "debounce: the latest generation is current")
  (define g3 (bump-widget-rerun-generation!))
  (assert-false (widget-rerun-current? g2) "debounce: a later bump invalidates the prior generation")
  (assert-true (widget-rerun-current? g3) "debounce: the newest generation is current")

  ;; config knob default: on, so the walk/modal proceed (guard returns #false, no scan skipped)
  (assert-true (widgets-enabled?) "knob default: widgets enabled")
  (assert-false (widget-walk-guard) "knob default: the guard lets the walk proceed")

  ;; disabled path: the guard short-circuits with the status BEFORE any registry scan
  (apply-project-config! (list (cons "widgets" #false)))
  (assert-false (widgets-enabled?) "knob override: widgets = false disables them")
  (assert-equal (widgets-disabled-status) (widget-walk-guard)
                "disabled path: the guard returns the status, skipping the scan")
  (assert-equal "widgets are disabled in .nothelix.conf" (widgets-disabled-status)
                "disabled path: the status names the .nothelix.conf knob")

  ;; restore the default so later suites see widgets enabled
  (apply-project-config! (list (cons "widgets" #true)))
  (assert-true (widgets-enabled?) "knob restore: widgets re-enabled")

  (print-test-suite-footer "widgets"))
