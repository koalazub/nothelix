;;; audio.scm — non-blocking cell audio playback (wavplay clips)

(require "common.scm")
(require "string-utils.scm")
(require "cell-boundaries.scm")
(require "cell-state.scm")
(require "output-store.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          audio-play audio-stop audio-stop-all audio-playing
                          json-get-audio))

(provide play-cell-audio
         stop-audio
         audio-stop-all!
         audio-auto-play-from-result!
         parse-audio-artifacts
         audio-artifact-path
         audio-artifact-duration
         format-audio-clock
         audio-status-text)

;;@doc
;; One active playback slot: #false, or (list pid-string cell-index duration-ms).
;; Starting any playback stops whatever this slot held.
(define *audio-slot* (box #false))

(define (slot-pid s) (list-ref s 0))

(define (clear-slot!)
  (set-box! *audio-slot* #false)
  (clear-audio-playing-cell!))

(define (stop-current-playback!)
  (define slot (unbox *audio-slot*))
  (when slot (audio-stop (slot-pid slot)))
  (clear-slot!))

;;@doc
;; Path of a parsed audio artifact `(path . duration-ms)`.
(define (audio-artifact-path a) (car a))

;;@doc
;; Duration in milliseconds of a parsed audio artifact `(path . duration-ms)`.
(define (audio-artifact-duration a) (cdr a))

(define (parse-one-audio-line line)
  (cond
    [(equal? (string-trim line) "") #false]
    [else
     (define parts (split-once line "\t"))
     (if (list? parts)
         (cons (car parts) (or (string->number (cadr parts)) 0))
         (cons line 0))]))

;;@doc
;; Decode a `json-get-audio` / stored audio blob ("<path>\t<duration_ms>" lines)
;; into a list of `(path . duration-ms)` artifacts — '() for "" or #false.
(define (parse-audio-artifacts blob)
  (if (or (not blob) (equal? blob ""))
      '()
      (filter (lambda (x) x)
              (map parse-one-audio-line (string-split blob "\n")))))

;;@doc
;; Format a millisecond duration as an "m:ss" wall clock (seconds floored).
(define (format-audio-clock ms)
  (define total-s (quotient (max 0 ms) 1000))
  (define mins (quotient total-s 60))
  (define secs (remainder total-s 60))
  (string-append (number->string mins) ":"
                 (if (< secs 10) (string-append "0" (number->string secs)) (number->string secs))))

;;@doc
;; Status line for a playing cell: "♪ cell N (m:ss)", with " (k clips)" when the
;; cell produced more than one clip.
(define (audio-status-text idx duration-ms clips)
  (string-append "♪ cell " (number->string idx) " (" (format-audio-clock duration-ms) ")"
                 (if (> clips 1)
                     (string-append " (" (number->string clips) " clips)")
                     "")))

(define (schedule-audio-clear! pid delay-ms)
  (enqueue-thread-local-callback-with-delay delay-ms
    (lambda () (maybe-clear-audio-slot! pid))))

(define (maybe-clear-audio-slot! pid)
  (define slot (unbox *audio-slot*))
  (when (and slot (equal? (slot-pid slot) pid))
    (audio-playing pid)
    (clear-slot!)))

(define (play-artifacts! idx arts)
  (stop-current-playback!)
  (define first-art (car arts))
  (define pid (audio-play (audio-artifact-path first-art)))
  (cond
    [(string-starts-with? pid "ERROR:")
     (set-status! (string-append "cell " (number->string idx) " audio: " pid))]
    [else
     (define duration-ms (audio-artifact-duration first-art))
     (set-box! *audio-slot* (list pid idx duration-ms))
     (set-audio-playing-cell! idx)
     (set-status! (audio-status-text idx duration-ms (length arts)))
     (schedule-audio-clear! pid (+ duration-ms 300))]))

;;@doc
;; Play the cell under the cursor's stored audio artifacts (first clip). Reads
;; the output store so it replays exactly what the last run produced.
(define (play-cell-audio)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "play-cell-audio: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define (get-line idx) (doc-get-line rope total idx))
     (define cell-start (find-cell-start-line get-line (current-line-number)))
     (define idx (marker-line-cell-index (get-line cell-start)))
     (cond
       [(not idx) (set-status! "play-cell-audio: no cell at cursor")]
       [else
        (define raw (store-get-for path (cell-id idx)))
        (define blob (decode-stored-audio-blob raw (stored-source-hash raw)))
        (define arts (parse-audio-artifacts blob))
        (cond
          [(null? arts)
           (set-status! (string-append "cell " (number->string idx)
                                        ": no audio — run it first"))]
          [else (play-artifacts! idx arts)])])]))

;;@doc
;; Stop the active playback slot and clear its picker glyph.
(define (stop-audio)
  (define slot (unbox *audio-slot*))
  (cond
    [(not slot) (set-status! "No audio playing")]
    [else
     (audio-stop (slot-pid slot))
     (clear-slot!)
     (set-status! "♪ stopped")]))

;;@doc
;; Stop every player process and clear the slot (used on editor quit).
(define (audio-stop-all!)
  (audio-stop-all)
  (clear-slot!))

;;@doc
;; When a just-executed cell's result JSON carries audio artifacts, play the
;; first — a wavplay call is itself the play intent.
(define (audio-auto-play-from-result! result-json cell-index)
  (define blob (json-get-audio result-json))
  (define usable
    (if (and (> (string-length blob) 0) (not (string-starts-with? blob "ERROR:")))
        blob
        #false))
  (define arts (parse-audio-artifacts usable))
  (when (not (null? arts))
    (play-artifacts! cell-index arts)))
