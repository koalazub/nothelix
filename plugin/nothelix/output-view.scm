(require "string-utils.scm")
(require "common.scm")
(require "cell-boundaries.scm")
(require "cell-state.scm")
(require "output-store.scm")
(require "kernel.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require-builtin helix/core/text as text.)
(require-builtin helix/components)

(#%require-dylib "libnothelix"
                 (only-in nothelix read-file-tail))

(provide cell-output-view
         wrap-line
         wrap-rows
         tail-lines-for
         output-view-footer
         no-output-status)

;;@doc
;; Break `line` into a list of chunks each at most `width` characters wide, so a
;; long output line wraps to the popup instead of being truncated. An empty line
;; stays a single "" chunk; a non-positive width returns the line unsplit.
(define (wrap-line line width)
  (cond
    [(<= width 0) (list line)]
    [(= (string-length line) 0) (list "")]
    [else
     (let loop ([start 0] [acc '()])
       (define len (string-length line))
       (if (>= start len)
           (reverse acc)
           (let ([end (min len (+ start width))])
             (loop end (cons (substring line start end) acc)))))]))

;;@doc
;; Wrap every row in `rows` to `width`, flattening to one display-line list.
(define (wrap-rows rows width)
  (apply append (map (lambda (r) (wrap-line r width)) rows)))

;;@doc
;; Number of trailing lines to request from live.out, sized to the popup's
;; content height (at least one).
(define (tail-lines-for content-height)
  (max 1 content-height))

;;@doc
;; The footer key legend plus the visible state word ("live" or "stored").
(define (output-view-footer live?)
  (string-append "j/k scroll · ctrl-d/ctrl-u page · q close · "
                 (if live? "live" "stored")))

;;@doc
;; The status line shown when a cell has no output to blow up.
(define (no-output-status idx)
  (string-append "cell " (number->string idx) ": no output — run it first"))

(struct OutputViewState
  (doc-path cell-idx live? live-path rows scroll content-height)
  #:mutable)

(define *output-view-open?* (box #false))

(define (blob->rows blob)
  (if (or (equal? blob "") (string-starts-with? blob "ERROR"))
      '()
      (string-split blob "\n")))

(define (reload-live! state)
  (define n (tail-lines-for (OutputViewState-content-height state)))
  (define blob (read-file-tail (OutputViewState-live-path state) n))
  (set-OutputViewState-rows! state (blob->rows blob))
  (set-OutputViewState-scroll! state 1000000))

(define (switch-to-stored! state)
  (define raw (store-get-for (OutputViewState-doc-path state)
                             (cell-id (OutputViewState-cell-idx state))))
  (define rows (decode-stored-rows raw (stored-source-hash raw)))
  (set-OutputViewState-live?! state #false)
  (when (and rows (not (null? rows)))
    (set-OutputViewState-rows! state rows)
    (set-OutputViewState-scroll! state 0)))

(define (output-view-refresh! state)
  (when (unbox *output-view-open?*)
    (cond
      [(cell-running? (OutputViewState-cell-idx state))
       (reload-live! state)
       (helix.redraw)
       (enqueue-thread-local-callback-with-delay 500
         (lambda () (output-view-refresh! state)))]
      [else
       (switch-to-stored! state)
       (helix.redraw)])))

(define (render-output-view state rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))
  (define popup-w (max 20 (- rw 8)))
  (define popup-h (max 6 (- rh 6)))
  (define px (max 0 (quotient (- rw popup-w) 2)))
  (define py (max 0 (quotient (- rh popup-h) 2)))
  (define popup-area (area px py popup-w popup-h))
  (define content-w (max 1 (- popup-w 4)))
  (define content-h (max 1 (- popup-h 4)))

  (set-OutputViewState-content-height! state content-h)

  (define wrapped (wrap-rows (OutputViewState-rows state) content-w))
  (define total (length wrapped))
  (define max-scroll (max 0 (- total content-h)))
  (define scroll (min (OutputViewState-scroll state) max-scroll))
  (set-OutputViewState-scroll! state scroll)

  (define bg-style (style))
  (define border-style (style-fg (style) Color/Cyan))
  (define title-style (style-with-bold (style-fg (style) Color/White)))
  (define text-style (style))
  (define footer-style (style-fg (style) Color/Gray))

  (buffer/clear buf popup-area)
  (block/render buf popup-area (make-block bg-style border-style "all" "rounded"))

  (define title
    (string-append "Cell " (number->string (OutputViewState-cell-idx state)) " output"))
  (frame-set-string! buf (+ px 2) (+ py 1) title title-style)

  (let loop ([xs wrapped] [i 0])
    (cond
      [(null? xs) (void)]
      [(< i scroll) (loop (cdr xs) (+ i 1))]
      [(>= (- i scroll) content-h) (void)]
      [else
       (frame-set-string! buf (+ px 2) (+ py 2 (- i scroll)) (car xs) text-style)
       (loop (cdr xs) (+ i 1))]))

  (frame-set-string! buf (+ px 2) (+ py popup-h -2)
                     (output-view-footer (OutputViewState-live? state)) footer-style))

(define (ctrl-key? event ch)
  (define m (key-event-modifier event))
  (and m (equal? m key-modifier-ctrl) (eqv? (key-event-char event) ch)))

(define (scroll-by! state delta)
  (set-OutputViewState-scroll! state
    (max 0 (+ (OutputViewState-scroll state) delta))))

(define (handle-output-view-event state event)
  (define char (key-event-char event))
  (define half (max 1 (quotient (OutputViewState-content-height state) 2)))
  (cond
    [(or (key-event-escape? event) (eqv? char #\q))
     (set-box! *output-view-open?* #false)
     event-result/close]
    [(ctrl-key? event #\d)
     (scroll-by! state half)
     event-result/consume]
    [(ctrl-key? event #\u)
     (scroll-by! state (- half))
     event-result/consume]
    [(or (eqv? char #\j) (key-event-down? event))
     (scroll-by! state 1)
     event-result/consume]
    [(or (eqv? char #\k) (key-event-up? event))
     (scroll-by! state -1)
     event-result/consume]
    [else event-result/consume]))

(define (push-output-view! state)
  (set-box! *output-view-open?* #true)
  (define component
    (new-component! "output-view" state render-output-view
      (hash "handle_event" handle-output-view-event)))
  (push-component! (overlaid component))
  (when (OutputViewState-live? state)
    (enqueue-thread-local-callback-with-delay 500
      (lambda () (output-view-refresh! state)))))

(define (open-output-view path idx)
  (define kernel-dir (running-kernel-dir path))
  (define live? (and (cell-running? idx) kernel-dir #true))
  (cond
    [live?
     (define state (OutputViewState path idx #true
                     (string-append kernel-dir "/live.out") '() 1000000 40))
     (reload-live! state)
     (push-output-view! state)]
    [else
     (define raw (store-get-for path (cell-id idx)))
     (define rows (decode-stored-rows raw (stored-source-hash raw)))
     (if (or (not rows) (null? rows))
         (set-status! (no-output-status idx))
         (push-output-view! (OutputViewState path idx #false #false rows 0 40)))]))

;;@doc
;; Open the output popup for the cell under the cursor: a live tail of the
;; kernel's output while the cell runs, otherwise the stored output blown up.
(define (cell-output-view)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "cell-output-view: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define (get-line idx) (doc-get-line rope total idx))
     (define cell-start (find-cell-start-line get-line (current-line-number)))
     (define idx (marker-line-cell-index (get-line cell-start)))
     (if (not idx)
         (set-status! "cell-output-view: no cell at cursor")
         (open-output-view path idx))]))
