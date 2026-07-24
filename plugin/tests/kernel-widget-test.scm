;;; kernel-widget-test.scm — pure-logic tests for kernel-declared widgets: spec
;;; parse/serialize (malformed lines dropped), slider/choice row text at boundary
;;; values, the apply-path payload formatting (slider nudge + choice cycle), and
;;; the output-store round trip for the widgets section incl. absent-byte-identity,
;;; audio co-existence, and stale-hash rejection.

(require "test-framework.scm")
(require "../nothelix/kernel-widget.scm")
(require "../nothelix/output-store.scm")
(require "../nothelix/param-tweak.scm")
(require "../nothelix/project-config.scm")
(require "../nothelix/kernel.scm")

(provide run-kernel-widget-tests)

(define (run-kernel-widget-tests)
  (reset-test-counters!)
  (print-test-suite-header "kernel-widget")

  ;; spec parse: two well-formed specs, fields recovered
  (define blob "slider\tfreq\t220:880:10\t440\nchoice\twave\tsin|cos|tan\tsin")
  (define specs (parse-widget-specs blob))
  (assert-equal 2 (length specs) "parse: two lines yield two specs")
  (assert-equal "slider" (widget-spec-kind (car specs)) "parse: slider kind")
  (assert-equal "freq" (widget-spec-name (car specs)) "parse: slider name")
  (assert-equal "220:880:10" (widget-spec-params (car specs)) "parse: slider params")
  (assert-equal "440" (widget-spec-current (car specs)) "parse: slider current")
  (assert-equal "choice" (widget-spec-kind (cadr specs)) "parse: choice kind")
  (assert-equal "sin|cos|tan" (widget-spec-params (cadr specs)) "parse: choice params")
  (assert-equal "sin" (widget-spec-current (cadr specs)) "parse: choice current")

  ;; spec parse: empty current field survives (trailing tab)
  (define bare (parse-widget-specs "slider\tg\t0:1:0\t"))
  (assert-equal 1 (length bare) "parse: a slider with an empty current is kept")
  (assert-equal "" (widget-spec-current (car bare)) "parse: empty current is the empty string")

  ;; spec parse: malformed lines dropped
  (assert-equal '() (parse-widget-specs "") "parse: empty blob yields no specs")
  (assert-equal '() (parse-widget-specs #false) "parse: #false blob yields no specs")
  (assert-equal 1 (length (parse-widget-specs "slider\tfreq\t0:9:1\t3\nbad-line"))
                "parse: a line with too few fields is dropped")
  (assert-equal 1 (length (parse-widget-specs "\tfreq\t0:9:1\t3\nslider\tg\t0:9:1\t3"))
                "parse: a line with an empty kind is dropped")

  ;; serialize round trip
  (assert-equal blob (serialize-widget-specs specs) "serialize: round trips a parsed blob")
  (assert-equal "slider\tfreq\t220:880:10\t450"
                (serialize-widget-specs (list (spec-with-current (car specs) "450")))
                "spec-with-current: only the current field changes")

  ;; params grammar
  (assert-equal (list 220 880 10) (parse-slider-params "220:880:10") "slider params: int triple")
  (assert-equal (list 0.0 1.0 0.1) (parse-slider-params "0.0:1.0:0.1") "slider params: float triple")
  (assert-false (parse-slider-params "220:880") "slider params: too few fields -> #false")
  (assert-equal (list "sin" "cos" "tan") (parse-choice-options "sin|cos|tan") "choice options: split on pipe")

  ;; effective step
  (assert-equal 10 (slider-step 220 880 10) "step: a declared step wins")
  (assert-equal 1 (slider-step 220 880 0) "step: an integer range defaults to 1")
  (assert-equal (/ 1.0 100) (slider-step 0.0 1.0 0) "step: a fractional range defaults to (hi-lo)/100")

  ;; slider-track geometry at boundaries
  (assert-equal 0 (widget-track-position 220 880 220 20) "track: value at lo sits at column 0")
  (assert-equal 19 (widget-track-position 220 880 880 20) "track: value at hi sits at the last column")
  (assert-equal 0 (widget-track-position 5 5 5 20) "track: a degenerate range clamps to column 0")

  ;; slider row text at boundaries
  (define row-lo (widget-slider-row "freq" 220 880 220 "220"))
  (define row-hi (widget-slider-row "freq" 220 880 880 "880"))
  (assert-contains row-lo "[●" "slider row: marker sits at the track start at lo")
  (assert-contains row-hi "●]" "slider row: marker sits at the track end at hi")
  (assert-contains row-lo "freq" "slider row: carries the name")
  (assert-contains row-lo "220" "slider row: carries the current literal")
  (assert-contains row-lo "]p/[p" "slider row: names its keys")

  ;; choice row text: current option bracketed, first and last
  (assert-equal "⊞ wave  [sin] cos tan · ]s/[s"
                (widget-choice-row "wave" (list "sin" "cos" "tan") "sin")
                "choice row: the first option marked current")
  (assert-equal "⊞ wave  sin cos [tan] · ]s/[s"
                (widget-choice-row "wave" (list "sin" "cos" "tan") "tan")
                "choice row: the last option marked current")
  (assert-equal "⊞ wave  sin cos tan · ]s/[s"
                (widget-choice-row "wave" (list "sin" "cos" "tan") "off")
                "choice row: an off-set value marks nothing")

  ;; row-for-spec dispatch, and #false for an unknown kind
  (assert-contains (widget-row-for-spec (car specs)) "freq" "row-for-spec: slider dispatch")
  (assert-contains (widget-row-for-spec (cadr specs)) "wave" "row-for-spec: choice dispatch")
  (assert-false (widget-row-for-spec (list "mystery" "x" "" "")) "row-for-spec: unknown kind -> #false")

  ;; apply-path payload formatting: slider nudge value snapped, formatted
  (assert-equal 450 (slider-nudge-value 220 880 10 440 1) "slider nudge: forward one step")
  (assert-equal 430 (slider-nudge-value 220 880 10 440 -1) "slider nudge: back one step")
  (assert-equal 880 (slider-nudge-value 220 880 10 880 1) "slider nudge: forward clamps at hi")
  (assert-equal 220 (slider-nudge-value 220 880 10 220 -1) "slider nudge: back clamps at lo")
  (assert-equal "450" (format-number (slider-nudge-value 220 880 10 440 1) 0)
                "payload: an integer nudge formats without a decimal point")
  (assert-equal "0.6" (format-number (slider-nudge-value 0.0 1.0 0.1 0.5 1) (decimals-of 0.1))
                "payload: a fractional nudge formats to the step's precision")

  ;; apply-path payload formatting: choice cycle wraps both ways
  (define opts (list "sin" "cos" "tan"))
  (assert-equal "cos" (choice-cycle-value opts "sin" 1) "choice cycle: forward one option")
  (assert-equal "sin" (choice-cycle-value opts "tan" 1) "choice cycle: forward wraps to the first")
  (assert-equal "tan" (choice-cycle-value opts "sin" -1) "choice cycle: back wraps to the last")
  (assert-equal "cos" (choice-cycle-value opts "off" 1) "choice cycle: a stray value cycles from index 0")

  ;; output-store round trip through the widgets section
  (define hash "9999")
  (define encoded
    (encode-outputs+rows+text-plots+audio+widgets "[]" (list "out line") "" "" blob))
  (define raw (string-append hash "\t" encoded))
  (assert-equal blob (decode-stored-widgets-blob raw hash)
                "decode-stored-widgets-blob: recovers the exact stored blob")
  (assert-equal (list "out line") (decode-stored-rows raw hash)
                "decode-stored-rows: rows survive a trailing widgets section")

  ;; absent-widgets byte identity
  (define encoded-none
    (encode-outputs+rows+text-plots+audio+widgets "[]" (list "out line") "" "" ""))
  (assert-equal (encode-outputs+rows+text-plots+audio "[]" (list "out line") "" "")
                encoded-none
                "encode+widgets: an empty widgets blob is byte-identical to the audio encoder")
  (define raw-none (string-append hash "\t" encoded-none))
  (assert-equal #false (decode-stored-widgets-blob raw-none hash)
                "decode-stored-widgets-blob: #false when no widgets were stored")

  ;; widgets and audio co-exist without polluting each other
  (define audio-blob "/k/a.wav\t1500")
  (define encoded-both
    (encode-outputs+rows+text-plots+audio+widgets "[]" (list "out line") "" audio-blob blob))
  (define raw-both (string-append hash "\t" encoded-both))
  (assert-equal (list "out line") (decode-stored-rows raw-both hash)
                "decode-stored-rows: rows survive an audio AND widgets section")
  (assert-equal audio-blob (decode-stored-audio-blob raw-both hash)
                "decode-stored-audio-blob: audio survives a trailing widgets section")
  (assert-equal blob (decode-stored-widgets-blob raw-both hash)
                "decode-stored-widgets-blob: widgets survive a preceding audio section")

  ;; stale hash drops the stored widgets
  (assert-equal #false (decode-stored-widgets-blob raw "stale-hash")
                "decode-stored-widgets-blob: #false on a stale (mismatched) hash")

  ;; the widgets knob gates the row surface
  (assert-equal '() (widget-group-for "") "group: an empty blob renders nothing")
  (assert-equal 2 (length (widget-group-for blob)) "group: one row per spec when enabled")
  (apply-project-config! (list (cons "widgets" #false)))
  (assert-equal '() (widget-group-for blob) "group: the widgets knob off suppresses the rows")
  (apply-project-config! (list (cons "widgets" #true)))
  (assert-equal 2 (length (widget-group-for blob)) "group: re-enabling restores the rows")

  ;; the stale-runner status line names the one command that upgrades a kernel
  (assert-equal "kernel predates the installed runner — :kernel-shutdown to upgrade"
                (kernel-stale-status-line)
                "stale runner: the status line points at :kernel-shutdown")

  (print-test-suite-footer "kernel-widget"))
