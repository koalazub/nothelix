;;; audio-test.scm — pure parts of non-blocking cell audio: artifact TSV parse,
;;; clock/status formatting (audio.scm), and the output-store audio encode/decode
;;; round trip incl. absent-audio byte-identity and rows/text-plots co-existence.

(require "test-framework.scm")
(require "../nothelix/audio.scm")
(require "../nothelix/output-store.scm")

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

  (print-test-suite-footer "audio"))
