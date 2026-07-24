;;; audio.scm — non-blocking cell audio playback (wavplay clips)

(require "common.scm")
(require "string-utils.scm")
(require "cell-boundaries.scm")
(require "cell-state.scm")
(require "output-store.scm")
(require "output-render.scm")
(require "kernel-widget.scm")
(require "project-config.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require-builtin helix/core/text as text.)
(require-builtin helix/components)
(require-builtin steel/time)

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          audio-play audio-play-from audio-stop audio-stop-all
                          audio-playing audio-position audio-waveform audio-info
                          json-get-audio))

(provide play-cell-audio
         stop-audio
         audio-stop-all!
         audio-auto-play-from-result!
         cell-has-stored-audio?
         recompose-cell!
         audio-seek-forward
         audio-seek-back
         scrub-audio
         waveform-group-for
         start-playhead-ticker!
         parse-audio-artifacts
         audio-artifact-path
         audio-artifact-duration
         format-audio-clock
         format-audio-khz
         audio-waveform-header
         audio-status-text
         ladder-next
         ladder-step-index
         waveform-playhead-col
         waveform-bracket-cols
         waveform-cols)

;;@doc
;; One active playback slot: #false, or
;; (list pid-string cell-index duration-ms wav-path).
;; Starting any playback stops whatever this slot held.
(define *audio-slot* (box #false))

(define (slot-pid s) (list-ref s 0))
(define (slot-cell s) (list-ref s 1))
(define (slot-duration s) (list-ref s 2))
(define (slot-wav s) (if (>= (length s) 4) (list-ref s 3) #false))

(define (slot-playing? slot)
  (and slot (equal? (audio-playing (slot-pid slot)) "true")))

(define (clear-slot!)
  (set-box! *audio-slot* #false)
  (clear-audio-playing-cell!))

;;@doc
;; Redraw a cell's output rows with the playhead cleared (column -1).
(define (clear-cell-playhead! idx)
  (when idx (recompose-cell! idx -1 -1 -1)))

(define (stop-current-playback!)
  (define slot (unbox *audio-slot*))
  (cond
    [slot
     (define idx (slot-cell slot))
     (audio-stop (slot-pid slot))
     (clear-slot!)
     (clear-cell-playhead! idx)]
    [else (clear-slot!)]))

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
                     "")
                 " · <space>ns scrub"))

;;@doc
;; Whether a cell has a replayable stored clip, judged against the stored
;; hash so an edited-but-not-rerun cell keeps its badge.
(define (cell-has-stored-audio? path idx)
  (define raw (store-get-for path (cell-id idx)))
  (define blob (decode-stored-audio-blob raw (stored-source-hash raw)))
  (not (null? (parse-audio-artifacts blob))))

(define (schedule-audio-clear! pid delay-ms)
  (enqueue-thread-local-callback-with-delay delay-ms
    (lambda () (maybe-clear-audio-slot! pid))))

(define (maybe-clear-audio-slot! pid)
  (define slot (unbox *audio-slot*))
  (when (and slot (equal? (slot-pid slot) pid))
    (define idx (slot-cell slot))
    (audio-playing pid)
    (clear-slot!)
    (clear-cell-playhead! idx)))

(define (play-artifacts! idx arts)
  (stop-current-playback!)
  (define first-art (car arts))
  (define wav (audio-artifact-path first-art))
  (define pid (audio-play wav))
  (cond
    [(string-starts-with? pid "ERROR:")
     (set-status! (string-append "cell " (number->string idx) " audio: " pid))]
    [else
     (define duration-ms (audio-artifact-duration first-art))
     (set-box! *audio-slot* (list pid idx duration-ms wav))
     (set-audio-playing-cell! idx)
     (set-status! (audio-status-text idx duration-ms (length arts)))
     (schedule-audio-clear! pid (+ duration-ms 300))
     (start-playhead-ticker!)]))

;;@doc
;; Play the cell under the cursor's stored audio artifacts (first clip). Reads
;; the output store so it replays exactly what the last run produced. When the
;; cell under the cursor is already the one playing, opens scrub mode instead.
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
     (define slot (unbox *audio-slot*))
     (cond
       [(not idx) (set-status! "play-cell-audio: no cell at cursor")]
       [(and slot (equal? (slot-cell slot) idx) (slot-playing? slot))
        (open-scrub-for-slot! slot)]
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
;; Stop the active playback slot, clear its picker glyph, and clear the
;; waveform playhead on that cell.
(define (stop-audio)
  (define slot (unbox *audio-slot*))
  (cond
    [(not slot) (set-status! "No audio playing")]
    [else
     (define idx (slot-cell slot))
     (audio-stop (slot-pid slot))
     (clear-slot!)
     (clear-cell-playhead! idx)
     (set-status! "♪ stopped")]))

;;@doc
;; Stop every player process and clear the slot (used on editor quit).
(define (audio-stop-all!)
  (audio-stop-all)
  (clear-slot!))

;;@doc
;; When a just-executed cell's result JSON carries audio artifacts, play the
;; first — a wavplay call is itself the play intent. Called by the
;; interactive single-cell completion path only; a batch run records the
;; artifact and badges the cell without performing it. Suppressed when the
;; project config sets `audio-autoplay = false`.
(define (audio-auto-play-from-result! result-json cell-index)
  (when (audio-autoplay?)
    (define blob (json-get-audio result-json))
    (define usable
      (if (and (> (string-length blob) 0) (not (string-starts-with? blob "ERROR:")))
          blob
          #false))
    (define arts (parse-audio-artifacts usable))
    (when (not (null? arts))
      (play-artifacts! cell-index arts))))

;; --- waveform geometry, header formatting, and seek ladder (pure) ---

(define *waveform-cols* 60)

;;@doc
;; Braille cell width of a cell's waveform, matching the text-plot width
;; convention (the braille chart's default column count).
(define (waveform-cols) *waveform-cols*)

;;@doc
;; Format a sample rate in Hz as a kHz label, one decimal, trailing ".0"
;; dropped: 44100 -> "44.1", 8000 -> "8".
(define (format-audio-khz rate)
  (define tenths (quotient (+ rate 50) 100))
  (define whole (quotient tenths 10))
  (define frac (remainder tenths 10))
  (if (= frac 0)
      (number->string whole)
      (string-append (number->string whole) "." (number->string frac))))

;;@doc
;; The one-line waveform header: "♪ m:ss · <rate>kHz · <mono|stereo>".
(define (audio-waveform-header duration-ms rate channels elapsed-ms)
  (define playing? (>= elapsed-ms 0))
  (string-append "♪ "
                 (if playing?
                     (string-append (format-audio-clock elapsed-ms) " / ")
                     "")
                 (format-audio-clock duration-ms)
                 " · " (format-audio-khz rate) "kHz"
                 " · " (if (> channels 1) "stereo" "mono")
                 (if playing?
                     " · <space>ns scrub · <space>nx stop"
                     " · <space>ns play")))

;;@doc
;; The braille column a playhead sits in for `position-ms` into a clip of
;; `duration-ms`, clamped to [0, width-1]. A zero/negative duration pins it
;; to column 0.
(define (waveform-playhead-col position-ms duration-ms width)
  (if (<= duration-ms 0)
      0
      (max 0 (min (- width 1) (quotient (* (max 0 position-ms) width) duration-ms)))))

;;@doc
;; Braille columns a `step-ms` seek window spans across a `duration-ms` clip,
;; rounded, at least 1.
(define (waveform-bracket-cols step-ms duration-ms width)
  (if (<= duration-ms 0)
      1
      (max 1 (quotient (+ (* step-ms width) (quotient duration-ms 2)) duration-ms))))

;;@doc
;; Next (index . step-ms) along `ladder` for a quick seek at `now-ms`: a press
;; within `accel-window` of `last-ms` escalates one rung (clamped to the top),
;; a longer gap resets to the base rung.
(define (ladder-next ladder cur-idx last-ms now-ms accel-window)
  (define within? (<= (- now-ms last-ms) accel-window))
  (define max-idx (- (length ladder) 1))
  (define new-idx (if within? (min max-idx (+ cur-idx 1)) 0))
  (cons new-idx (list-ref ladder new-idx)))

;;@doc
;; Move a ladder index by `delta` rungs, clamped to [0, len-1].
(define (ladder-step-index cur-idx delta len)
  (max 0 (min (- len 1) (+ cur-idx delta))))

;; --- waveform row composition ---

;;@doc
;; Build a cell's waveform output group from a decoded audio blob's first clip:
;; a header row plus the braille envelope styled rows, or '() when the clip is
;; missing or unreadable. `playhead-col`/`bracket-lo`/`bracket-hi` are braille
;; columns, or -1 to omit. The blob is regenerated from the wav each render, so
;; it is never persisted — the wav is the artifact.
(define (waveform-group-for audio-blob playhead-col bracket-lo bracket-hi)
  (define arts (parse-audio-artifacts audio-blob))
  (if (null? arts)
      '()
      (let ([art (car arts)])
        (cell-waveform-rows (audio-artifact-path art)
                            (audio-artifact-duration art)
                            playhead-col bracket-lo bracket-hi))))

;;@doc
;; The one seam that crosses the audio-waveform FFI. Steel keeps the -1
;; no-playhead/no-bracket convention; the wire carries col+1 with 0 as
;; none, because steel's dylib argument conversion rejects negatives.
(define (audio-waveform-blob wav-path playhead-col bracket-lo bracket-hi)
  (audio-waveform wav-path (waveform-cols) (audio-waveform-rows)
                  (+ playhead-col 1) (+ bracket-lo 1) (+ bracket-hi 1)))

(define (cell-waveform-rows wav-path duration-ms playhead-col bracket-lo bracket-hi)
  (define blob (audio-waveform-blob wav-path playhead-col bracket-lo bracket-hi))
  (cond
    [(or (not (string? blob))
         (= (string-length blob) 0)
         (string-starts-with? blob "ERROR:"))
     '()]
    [else
     (define groups (decode-text-plots-blob blob))
     (if (null? groups)
         '()
         (let ([plot (car groups)])
           (cons (waveform-header-for wav-path duration-ms)
                 (text-plot->styled-rows (car plot) (cdr plot)))))]))

(define (header-elapsed-for wav-path)
  (define slot (unbox *audio-slot*))
  (if (and slot (equal? (slot-wav slot) wav-path))
      (let ([pos (audio-position (slot-pid slot))])
        (if (string-starts-with? pos "ERROR:") -1 (or (string->number pos) -1)))
      -1))

(define (waveform-header-for wav-path duration-ms)
  (define info (audio-info wav-path))
  (define fallback (string-append "♪ " (format-audio-clock duration-ms)))
  (cond
    [(or (not (string? info)) (string-starts-with? info "ERROR:")) fallback]
    [else
     (define parts (split-once info "\t"))
     (if (list? parts)
         (audio-waveform-header duration-ms
                                (or (string->number (car parts)) 0)
                                (or (string->number (cadr parts)) 1)
                                (header-elapsed-for wav-path))
         fallback)]))

(define (waveform-braille-lines wav-path playhead-col bracket-lo bracket-hi)
  (define blob (audio-waveform-blob wav-path playhead-col bracket-lo bracket-hi))
  (cond
    [(or (not (string? blob))
         (= (string-length blob) 0)
         (string-starts-with? blob "ERROR:"))
     '()]
    [else
     (define groups (decode-text-plots-blob blob))
     (if (null? groups) '() (car (car groups)))]))

;;@doc
;; Recompose one cell's output rows from the output store, re-deriving its
;; waveform group with the given playhead/bracket columns. The single-cell,
;; playhead-aware counterpart to `restore-cell-outputs-on-open!`.
(define (recompose-cell! cell-index playhead-col bracket-lo bracket-hi)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (and path (string-suffix? path ".jl"))
    (define rope (editor->text doc-id))
    (define total (text.rope-len-lines rope))
    (define (get-line idx) (doc-get-line rope total idx))
    (define marker-line (find-cell-marker-by-index rope total cell-index))
    (when marker-line
      (define code-end (find-cell-code-end get-line total (+ marker-line 1)))
      (define anchor-line (- code-end 1))
      (define code (string-join (extract-cell-code get-line marker-line code-end) "\n"))
      (define stored (store-get-for path (cell-id cell-index)))
      (define hash (cell-source-hash code))
      (define legacy (legacy-source-hash code))
      (define rows (decode-stored-rows stored hash legacy))
      (define tp-groups
        (map (lambda (plot) (text-plot->styled-rows (car plot) (cdr plot)))
             (decode-text-plots-blob (or (decode-stored-text-plots-blob stored hash legacy) ""))))
      (define audio-blob (or (decode-stored-audio-blob stored hash legacy) ""))
      (define wf-group (waveform-group-for audio-blob playhead-col bracket-lo bracket-hi))
      (define widget-group
        (widget-group-for (or (decode-stored-widgets-blob stored hash legacy) "")))
      (define groups
        (append (list (if (list? rows) rows '()))
                tp-groups
                (if (null? wf-group) '() (list wf-group))
                (if (null? widget-group) '() (list widget-group))))
      (try-set-output-lines-below! anchor-line (assign-cycling-bars groups)))))

;; --- playhead ticker ---

(define *playhead-ticking?* (box #false))

;;@doc
;; Start the once-a-second playhead ticker if it is not already running. Each
;; tick advances the playing cell's waveform playhead; the loop self-terminates
;; when the slot empties or playback stops, clearing the playhead.
(define (start-playhead-ticker!)
  (define slot (unbox *audio-slot*))
  (when slot (recompose-cell! (slot-cell slot) 0 -1 -1))
  (when (not (unbox *playhead-ticking?*))
    (set-box! *playhead-ticking?* #true)
    (schedule-playhead-tick!)))

(define (schedule-playhead-tick!)
  (enqueue-thread-local-callback-with-delay 1000 playhead-tick!))

(define (playhead-tick!)
  (define slot (unbox *audio-slot*))
  (cond
    [(not slot) (set-box! *playhead-ticking?* #false)]
    [(not (slot-playing? slot))
     (define idx (slot-cell slot))
     (set-box! *playhead-ticking?* #false)
     (clear-slot!)
     (clear-cell-playhead! idx)]
    [else
     (define pos-str (audio-position (slot-pid slot)))
     (define pos-ms (if (string-starts-with? pos-str "ERROR:") 0 (or (string->number pos-str) 0)))
     (define playhead (waveform-playhead-col pos-ms (slot-duration slot) (waveform-cols)))
     (recompose-cell! (slot-cell slot) playhead -1 -1)
     (schedule-playhead-tick!)]))

;; --- quick seeks (]a / [a) ---

(define *seek-step-idx* (box 0))
(define *seek-last-ms* (box 0))

(define (restart-playback-at! cell wav duration offset-ms)
  (when wav
    (define slot (unbox *audio-slot*))
    (define pid
      (begin
        (when slot (audio-stop (slot-pid slot)))
        (audio-play-from wav (max 0 offset-ms))))
    (cond
      [(string-starts-with? pid "ERROR:")
       (set-status! (string-append "audio seek: " pid))]
      [else
       (set-box! *audio-slot* (list pid cell duration wav))
       (set-audio-playing-cell! cell)
       (schedule-audio-clear! pid (+ (max 0 (- duration offset-ms)) 300))
       (start-playhead-ticker!)
       (set-status! (string-append "♪ " (format-audio-clock offset-ms)))])))

(define (audio-seek! direction)
  (define slot (unbox *audio-slot*))
  (cond
    [(not slot) (set-status! "No audio playing")]
    [(not (slot-wav slot)) (set-status! "audio seek: this clip cannot be scrubbed")]
    [else
     (define now (current-milliseconds))
     (define pair (ladder-next (audio-seek-ladder) (unbox *seek-step-idx*)
                               (unbox *seek-last-ms*) now (audio-accel-window-ms)))
     (set-box! *seek-step-idx* (car pair))
     (set-box! *seek-last-ms* now)
     (define step (cdr pair))
     (define duration (slot-duration slot))
     (define pos-str (audio-position (slot-pid slot)))
     (define pos (if (string-starts-with? pos-str "ERROR:") 0 (or (string->number pos-str) 0)))
     (define target (max 0 (min duration (+ pos (* direction step)))))
     (restart-playback-at! (slot-cell slot) (slot-wav slot) duration target)]))

;;@doc
;; Seek the playing clip forward by the current ladder step and resume there.
(define (audio-seek-forward) (audio-seek! 1))

;;@doc
;; Seek the playing clip back by the current ladder step and resume there.
(define (audio-seek-back) (audio-seek! -1))

;; --- scrub mode (modal component, chart-viewer style) ---

(struct ScrubState
  (wav-path
   duration-ms
   cell-index
   position-ms
   step-idx
   display-col)
  #:mutable)

(define (safe-substring s a b)
  (define len (string-length s))
  (define lo (max 0 (min a len)))
  (define hi (max lo (min b len)))
  (substring s lo hi))

(define (scrub-step-ms state)
  (define ladder (audio-seek-ladder))
  (list-ref ladder (max 0 (min (- (length ladder) 1) (ScrubState-step-idx state)))))

(define (render-scrub state rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))
  (define cols (waveform-cols))
  (define rows (audio-waveform-rows))
  (define wav (ScrubState-wav-path state))
  (define duration (ScrubState-duration-ms state))
  (define position (ScrubState-position-ms state))
  (define step (scrub-step-ms state))
  (define playhead (waveform-playhead-col position duration cols))
  (define display-col (max 0 (min (- cols 1) (ScrubState-display-col state))))
  (define bracket-cols (waveform-bracket-cols step duration cols))
  (define bracket-hi (min cols (+ playhead bracket-cols)))
  (define lines (waveform-braille-lines wav -1 -1 -1))

  (define popup-w (min (- rw 2) (+ cols 6)))
  (define popup-h (min (- rh 2) (+ rows 6)))
  (define px (max 0 (quotient (- rw popup-w) 2)))
  (define py (max 0 (quotient (- rh popup-h) 2)))
  (define popup-area (area px py popup-w popup-h))

  (define bg-style (style))
  (define border-style (style-fg (style) Color/Cyan))
  (define title-style (style-with-bold (style-fg (style) Color/White)))
  (define base-style (style-fg (style) Color/Cyan))
  (define bracket-style (style-fg (style) Color/Gray))
  (define playhead-style (style-fg (style) Color/Red))
  (define footer-style (style-fg (style) Color/Gray))

  (buffer/clear buf popup-area)
  (block/render buf popup-area (make-block bg-style border-style "all" "rounded"))

  (frame-set-string! buf (+ px 2) (+ py 1)
    (string-append "Scrub  " (format-audio-clock position) " / " (format-audio-clock duration))
    title-style)

  (define chart-x (+ px 2))
  (define chart-y (+ py 2))
  (let loop ([rs lines] [r 0])
    (when (and (not (null? rs)) (< r rows))
      (define line (car rs))
      (define glyph (safe-substring line display-col (+ display-col 1)))
      (frame-set-string! buf chart-x (+ chart-y r) line base-style)
      (when (> bracket-hi playhead)
        (define seg (safe-substring line playhead bracket-hi))
        (when (> (string-length seg) 0)
          (frame-set-string! buf (+ chart-x playhead) (+ chart-y r) seg bracket-style)))
      (when (> (string-length glyph) 0)
        (frame-set-string! buf (+ chart-x display-col) (+ chart-y r) glyph playhead-style))
      (loop (cdr rs) (+ r 1))))

  (define footer-y (+ py popup-h -2))
  (frame-set-string! buf (+ px 2) footer-y
    (string-append "step " (number->string step) "ms · h/l seek · j/k step · enter play")
    footer-style))

(define (animate-scrub-sweep! state from-col to-col)
  (define frames 4)
  (define per-frame (max 1 (quotient (audio-sweep-ms) frames)))
  (set-ScrubState-display-col! state from-col)
  (let loop ([i 1])
    (when (<= i frames)
      (enqueue-thread-local-callback-with-delay (* i per-frame)
        (lambda ()
          (set-ScrubState-display-col! state
            (+ from-col (quotient (* i (- to-col from-col)) frames)))
          (helix.redraw)))
      (loop (+ i 1)))))

(define (scrub-seek! state direction)
  (define cols (waveform-cols))
  (define duration (ScrubState-duration-ms state))
  (define step (scrub-step-ms state))
  (define old-col (waveform-playhead-col (ScrubState-position-ms state) duration cols))
  (define new-pos (max 0 (min duration (+ (ScrubState-position-ms state) (* direction step)))))
  (set-ScrubState-position-ms! state new-pos)
  (animate-scrub-sweep! state old-col (waveform-playhead-col new-pos duration cols)))

(define (scrub-step! state delta)
  (set-ScrubState-step-idx! state
    (ladder-step-index (ScrubState-step-idx state) delta (length (audio-seek-ladder)))))

(define (scrub-commit! state)
  (restart-playback-at! (ScrubState-cell-index state)
                        (ScrubState-wav-path state)
                        (ScrubState-duration-ms state)
                        (ScrubState-position-ms state)))

;; The audio vtable for the shared modal shell: h/l seek, j/k step the ladder,
;; Enter resumes playback at the scrubbed position, Esc leaves. render-scrub
;; owns the popup so the bracket, sweep, and footer stay pixel-identical.
(define scrub-vtable
  (hash 'render render-scrub
        'move   scrub-seek!
        'step   scrub-step!
        'apply  scrub-commit!))

(define (open-scrub! wav duration cell start-pos)
  (define ladder (audio-seek-ladder))
  (define init-idx (min 1 (- (length ladder) 1)))
  (define state
    (ScrubState wav duration cell start-pos init-idx
                (waveform-playhead-col start-pos duration (waveform-cols))))
  (open-widget-modal! scrub-vtable state "scrub-audio"))

(define (open-scrub-for-slot! slot)
  (define wav (slot-wav slot))
  (if (not wav)
      (set-status! "scrub-audio: this clip cannot be scrubbed")
      (let ([pos-str (audio-position (slot-pid slot))])
        (define pos (if (string-starts-with? pos-str "ERROR:") 0 (or (string->number pos-str) 0)))
        (open-scrub! wav (slot-duration slot) (slot-cell slot) pos))))

;;@doc
;; Open the scrub-mode modal for the cell under the cursor: on the live slot if
;; that cell is playing, otherwise on its stored audio positioned at the start.
(define (scrub-audio)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "scrub-audio: only runs on .jl notebook files")]
    [else
     (define slot (unbox *audio-slot*))
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define (get-line idx) (doc-get-line rope total idx))
     (define cell-start (find-cell-start-line get-line (current-line-number)))
     (define idx (marker-line-cell-index (get-line cell-start)))
     (cond
       [(not idx) (set-status! "scrub-audio: no cell at cursor")]
       [(and slot (equal? (slot-cell slot) idx) (slot-wav slot))
        (open-scrub-for-slot! slot)]
       [else
        (define raw (store-get-for path (cell-id idx)))
        (define blob (decode-stored-audio-blob raw (stored-source-hash raw)))
        (define arts (parse-audio-artifacts blob))
        (cond
          [(null? arts)
           (set-status! (string-append "cell " (number->string idx)
                                        ": no audio — run it first"))]
          [else
           (define art (car arts))
           (open-scrub! (audio-artifact-path art)
                        (audio-artifact-duration art) idx 0)])])]))

;; --- widget-kind registration (scrub: an output widget on a cell's audio) ---

(define (discover-audio-widgets scan)
  (define path (WidgetScan-path scan))
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (if (not path)
      '()
      (let loop ([i 0] [acc '()])
        (if (>= i total)
            (reverse acc)
            (let ([line (get-line i)])
              (if (cell-marker? line)
                  (let ([idx (marker-line-cell-index line)])
                    (if (and idx (cell-has-stored-audio? path idx))
                        (loop (+ i 1) (cons (cons i idx) acc))
                        (loop (+ i 1) acc)))
                  (loop (+ i 1) acc)))))))

(register-widget-kind! 'scrub "audio" "]a/[a seek · <space>ns scrub" discover-audio-widgets)
