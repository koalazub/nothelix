;;; audio-test.scm — pure parts of non-blocking cell audio: artifact TSV parse,
;;; clock/status/kHz formatting, waveform geometry, seek-ladder logic (audio.scm),
;;; the display-knob defaults/overrides (project-config.scm), and the output-store
;;; audio encode/decode round trip incl. absent-audio byte-identity and
;;; rows/text-plots co-existence.

(require "test-framework.scm")
(require "../nothelix/audio.scm")
(require "../nothelix/output-store.scm")
(require "../nothelix/project-config.scm")

(provide run-audio-tests)

(define (run-audio-tests)
  (reset-test-counters!)
  (print-test-suite-header "audio")

  ;; parse-audio-artifacts
  (assert-equal '() (parse-audio-artifacts #false)
                "parse: #false blob yields no artifacts")
  (assert-equal '() (parse-audio-artifacts "")
                "parse: empty blob yields no artifacts")
  (define one (parse-audio-artifacts "/k/audio/cell_0.wav\t1500"))
  (assert-equal 1 (length one) "parse: one line yields one artifact")
  (assert-equal "/k/audio/cell_0.wav" (audio-artifact-path (car one))
                "parse: recovers the path")
  (assert-equal 1500 (audio-artifact-duration (car one))
                "parse: recovers the duration in ms")
  (define two (parse-audio-artifacts "/k/a.wav\t1500\n/k/b.wav\t800"))
  (assert-equal 2 (length two) "parse: two lines yield two artifacts")
  (assert-equal "/k/b.wav" (audio-artifact-path (cadr two))
                "parse: keeps clip order")
  (assert-equal 800 (audio-artifact-duration (cadr two))
                "parse: second clip's duration")
  (assert-equal '() (parse-audio-artifacts "   ")
                "parse: a blank-only line yields no artifacts")

  ;; format-audio-clock
  (assert-equal "0:00" (format-audio-clock 0) "clock: zero")
  (assert-equal "0:00" (format-audio-clock 500) "clock: sub-second floors to 0:00")
  (assert-equal "0:02" (format-audio-clock 2000) "clock: two seconds")
  (assert-equal "0:02" (format-audio-clock 2999) "clock: seconds are floored")
  (assert-equal "1:05" (format-audio-clock 65000) "clock: minutes and seconds")
  (assert-equal "0:00" (format-audio-clock -10) "clock: a negative duration clamps to 0:00")

  ;; audio-status-text
  (assert-equal "♪ cell 3 (0:02)" (audio-status-text 3 2000 1)
                "status: a single clip shows no clip count")
  (assert-equal "♪ cell 3 (0:02) (2 clips)" (audio-status-text 3 2000 2)
                "status: more than one clip appends the count")

  ;; encode/decode round trip through the output store
  (define hash "12345")
  (define audio-blob "/k/audio/cell_0.wav\t1500\n/k/audio/cell_0_1.wav\t800")
  (define encoded
    (encode-outputs+rows+text-plots+audio "[]" (list "stdout line") "" audio-blob))
  (define raw (string-append hash "\t" encoded))

  (assert-equal audio-blob (decode-stored-audio-blob raw hash)
                "decode-stored-audio-blob: recovers the exact stored blob")
  (assert-equal (list "stdout line") (decode-stored-rows raw hash)
                "decode-stored-rows: rows are unaffected by a trailing audio section")

  ;; absent-audio byte-identity
  (define encoded-no-audio
    (encode-outputs+rows+text-plots+audio "[]" (list "stdout line") "" ""))
  (assert-equal (encode-outputs+rows+text-plots "[]" (list "stdout line") "")
                encoded-no-audio
                "encode+audio: an empty audio blob is byte-identical to the text-plots encoder")
  (define raw-no-audio (string-append hash "\t" encoded-no-audio))
  (assert-equal #false (decode-stored-audio-blob raw-no-audio hash)
                "decode-stored-audio-blob: #false when no audio was stored")

  ;; audio and text-plots co-exist without polluting each other
  (define tp-blob (string-append "AB\nCD" (make-string 1 (integer->char 29)) "0,0,1,2"))
  (define encoded-both
    (encode-outputs+rows+text-plots+audio "[]" (list "stdout line") tp-blob audio-blob))
  (define raw-both (string-append hash "\t" encoded-both))
  (assert-equal (list "stdout line") (decode-stored-rows raw-both hash)
                "decode-stored-rows: rows survive a text-plots AND audio section")
  (assert-equal tp-blob (decode-stored-text-plots-blob raw-both hash)
                "decode-stored-text-plots-blob: text-plots survive a trailing audio section")
  (assert-equal audio-blob (decode-stored-audio-blob raw-both hash)
                "decode-stored-audio-blob: audio survives a preceding text-plots section")

  ;; stale hash drops the stored audio
  (assert-equal #false (decode-stored-audio-blob raw "stale-hash")
                "decode-stored-audio-blob: #false on a stale (mismatched) hash")

  ;; format-audio-khz
  (assert-equal "44.1" (format-audio-khz 44100) "khz: 44100 -> 44.1")
  (assert-equal "8" (format-audio-khz 8000) "khz: 8000 -> 8 (trailing .0 dropped)")
  (assert-equal "22.1" (format-audio-khz 22050) "khz: 22050 rounds to 22.1")
  (assert-equal "48" (format-audio-khz 48000) "khz: 48000 -> 48")

  ;; audio-waveform-header
  (assert-equal "♪ 1:05 · 44.1kHz · stereo" (audio-waveform-header 65000 44100 2)
                "header: minutes, khz, and stereo channel count")
  (assert-equal "♪ 0:02 · 8kHz · mono" (audio-waveform-header 2000 8000 1)
                "header: mono single channel")

  ;; waveform-playhead-col geometry
  (assert-equal 0 (waveform-playhead-col 0 1000 60) "playhead: start pins to column 0")
  (assert-equal 30 (waveform-playhead-col 500 1000 60) "playhead: half-way lands mid-width")
  (assert-equal 59 (waveform-playhead-col 1000 1000 60) "playhead: end clamps to width-1")
  (assert-equal 0 (waveform-playhead-col 100 0 60) "playhead: zero duration pins to 0")

  ;; waveform-bracket-cols geometry
  (assert-equal 30 (waveform-bracket-cols 500 1000 60) "bracket: half the clip spans half the width")
  (assert-equal 6 (waveform-bracket-cols 100 1000 60) "bracket: a tenth spans a tenth")
  (assert-equal 1 (waveform-bracket-cols 10 100000 60) "bracket: a sub-column step still spans at least 1")

  ;; ladder-next: escalation within the accel window, reset after a gap, top clamp
  (define ladder (list 100 500 1000 5000 30000))
  (assert-equal (cons 0 100) (ladder-next ladder 0 0 10000 700)
                "ladder: a first press (gap) starts at the base rung")
  (assert-equal (cons 1 500) (ladder-next ladder 0 1000 1500 700)
                "ladder: a press within the window escalates one rung")
  (assert-equal (cons 2 1000) (ladder-next ladder 1 1000 1500 700)
                "ladder: escalation keeps climbing")
  (assert-equal (cons 4 30000) (ladder-next ladder 4 1000 1500 700)
                "ladder: escalation clamps at the top rung")
  (assert-equal (cons 0 100) (ladder-next ladder 3 1000 3000 700)
                "ladder: a press past the window resets to the base rung")

  ;; ladder-step-index: coarser/finer stepping, clamped both ends
  (assert-equal 1 (ladder-step-index 0 1 5) "step-index: coarser moves up one")
  (assert-equal 0 (ladder-step-index 0 -1 5) "step-index: finer clamps at 0")
  (assert-equal 4 (ladder-step-index 4 1 5) "step-index: coarser clamps at the top")
  (assert-equal 1 (ladder-step-index 2 -1 5) "step-index: finer moves down one")

  ;; knob defaults (project-config.scm)
  (assert-equal #true (audio-autoplay?) "knob default: autoplay on")
  (assert-equal 4 (audio-waveform-rows) "knob default: 4 waveform rows")
  (assert-equal (list 100 500 1000 5000 30000) (audio-seek-ladder)
                "knob default: the standard ladder")
  (assert-equal 700 (audio-accel-window-ms) "knob default: 700ms accel window")
  (assert-equal 100 (audio-sweep-ms) "knob default: 100ms sweep")

  ;; knob overrides through apply-project-config!
  (apply-project-config!
    (list (cons "audio-autoplay" #false)
          (cons "audio-waveform-rows" 6)
          (cons "audio-seek-ladder" "50,250,1000")
          (cons "audio-accel-window-ms" 400)
          (cons "audio-sweep-ms" 200)))
  (assert-equal #false (audio-autoplay?) "knob override: autoplay off")
  (assert-equal 6 (audio-waveform-rows) "knob override: 6 waveform rows")
  (assert-equal (list 50 250 1000) (audio-seek-ladder) "knob override: parsed comma ladder")
  (assert-equal 400 (audio-accel-window-ms) "knob override: accel window")
  (assert-equal 200 (audio-sweep-ms) "knob override: sweep")

  ;; a malformed knob value is ignored, leaving the prior value in place
  (apply-project-config!
    (list (cons "audio-waveform-rows" "wide")
          (cons "audio-seek-ladder" "0,bad")))
  (assert-equal 6 (audio-waveform-rows) "knob guard: non-integer rows rejected")
  (assert-equal (list 50 250 1000) (audio-seek-ladder) "knob guard: bad ladder rejected")

  ;; restore the defaults so later suites see a clean config
  (apply-project-config!
    (list (cons "audio-autoplay" #true)
          (cons "audio-waveform-rows" 4)
          (cons "audio-seek-ladder" "100,500,1000,5000,30000")
          (cons "audio-accel-window-ms" 700)
          (cons "audio-sweep-ms" 100)))

  (print-test-suite-footer "audio"))
