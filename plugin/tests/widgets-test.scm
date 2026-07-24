;;; widgets-test.scm — pure-logic tests for the shared widget contract: the
;;; ]w/[w walk ordering + wrap-around, the modal-shell vtable dispatch, the
;;; seek-ladder plumbing that dispatch drives (reusing audio's ladder-next /
;;; ladder-step-index), the debounced re-run generation counter, the `widgets`
;;; config knob (default on + the disabled short-circuit), and the Phase 2 leaf
;;; kinds: @select parse/cycle, @toggle parse/flip, the number slider-track
;;; geometry, and the cell-picker widget-marker presence + precedence.

(require "test-framework.scm")
(require "../nothelix/widgets.scm")
(require "../nothelix/audio.scm")
(require "../nothelix/param-tweak.scm")
(require "../nothelix/choice.scm")
(require "../nothelix/flag.scm")
(require "../nothelix/picker.scm")
(require "../nothelix/cell-state.scm")
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

  ;; @select parse: quoted string, bare identifier, and malformed rejections
  (assert-equal (list "wave" "\"sin\"" "sin" (list "sin" "cos" "tan") #true)
                (parse-select-line "wave = \"sin\"  # @select sin|cos|tan")
                "select parse: quoted value keeps its literal, current token unquoted")
  (assert-equal (list "wave" "sin" "sin" (list "sin" "cos" "tan") #false)
                (parse-select-line "wave = sin # @select sin|cos|tan")
                "select parse: bare identifier stays bare")
  (assert-false (parse-select-line "wave = sin") "select parse: no annotation -> #false")
  (assert-false (parse-select-line "wave = sin # @param 1:10")
                "select parse: wrong annotation -> #false")
  (assert-false (parse-select-line "wave = sin # @select")
                "select parse: empty option set -> #false")
  (assert-false (parse-select-line "# @select sin|cos")
                "select parse: no assignment -> #false")

  ;; @select cycle: wrap forward and back, and shape-preserving rewrite
  (define opts (list "sin" "cos" "tan"))
  (assert-equal "cos" (next-option opts "sin" 1) "select cycle: forward one option")
  (assert-equal "sin" (next-option opts "tan" 1) "select cycle: forward wraps to the first")
  (assert-equal "tan" (next-option opts "sin" -1) "select cycle: back wraps to the last")
  (assert-equal "cos" (next-option opts "tan" -1) "select cycle: back one option")
  (assert-equal 0 (option-index opts "unknown") "select cycle: a stray value starts at index 0")
  (assert-equal "\"cos\"" (select-value-string "cos" #true) "select rewrite: string value re-quoted")
  (assert-equal "cos" (select-value-string "cos" #false) "select rewrite: identifier stays bare")
  (assert-equal "sin [cos] tan" (choice-options-row opts 1) "select modal: current option marked")

  ;; @toggle flip: parse both booleans, reject non-booleans, flip both ways
  (assert-equal (list "loop" "true") (parse-toggle-line "loop = true  # @toggle")
                "toggle parse: a true boolean")
  (assert-equal (list "loop" "false") (parse-toggle-line "loop = false # @toggle")
                "toggle parse: a false boolean")
  (assert-false (parse-toggle-line "n = 5 # @toggle") "toggle parse: non-boolean -> #false")
  (assert-false (parse-toggle-line "loop = true # @param 0:1") "toggle parse: wrong annotation -> #false")
  (assert-equal "false" (toggle-flip-value "true") "toggle flip: true -> false")
  (assert-equal "true" (toggle-flip-value "false") "toggle flip: false -> true")

  ;; slider-track geometry: marker column at lo / hi / mid, and width clamping
  (assert-equal 0 (param-track-position 0 100 0 20) "track: value at lo sits at column 0")
  (assert-equal 19 (param-track-position 0 100 100 20) "track: value at hi sits at the last column")
  (assert-equal 10 (param-track-position 0 100 50 21) "track: value at mid sits mid-track")
  (assert-equal 0 (param-track-position 0 100 -50 20) "track: below lo clamps to column 0")
  (assert-equal 19 (param-track-position 0 100 200 20) "track: above hi clamps to the last column")
  (assert-equal 0 (param-track-position 5 5 5 20) "track: a degenerate range clamps to column 0")
  (assert-equal 0 (param-track-position 0 100 0 0) "track: a zero width clamps to a single column")
  (assert-contains (param-track-string 220 880 440 24 "440") "●" "track string: carries the marker")
  (assert-contains (param-track-string 220 880 440 24 "440") "440" "track string: carries the literal")
  (assert-contains (param-track-string 220 880 440 24 "440") "]p/[p" "track string: names its keys")

  ;; picker glyph presence: widget declarations recognised, marker at lowest precedence
  (assert-true (line-declares-widget? "freq = 440 # @param 220:880") "declares: @param line")
  (assert-true (line-declares-widget? "wave = sin # @select sin|cos") "declares: @select line")
  (assert-true (line-declares-widget? "loop = true # @toggle") "declares: @toggle line")
  (assert-true (line-declares-widget? "# @image plot.png") "declares: @image block")
  (assert-false (line-declares-widget? "y = freq * 2") "declares: a plain assignment does not")
  (set-cell-states! (parse-cell-states "58\tfresh\t\t1400"))
  (set-widget-cells! (list 58))
  (assert-equal "⊞" (picker-glyph 58) "picker glyph: a fresh cell with a widget shows the marker")
  (set-cell-states! (parse-cell-states "58\tstale-input\tA,2,stale\t"))
  (assert-equal "○" (picker-glyph 58) "picker glyph: freshness outranks the widget marker")
  (clear-widget-cells!)
  (clear-cell-states!)

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
