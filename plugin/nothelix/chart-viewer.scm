;;; chart-viewer.scm - Interactive braille chart viewer component
;;;
;;; Opens as a popup overlay rendering plot data as braille unicode characters.
;;; Supports zoom (+ / -), pan (h/j/k/l or arrows), reset (r), and close (q/Esc).
;;; The Rust FFI `render-braille-chart` does the heavy lifting; this module
;;; manages the viewport state and draws the result using `frame-set-string!`.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/components)

;; FFI imports
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          render-braille-chart
                          json-get))

(provide view-chart
         *last-plot-data*)

;; The most recently captured plot_data JSON string.
;; Stored by execution.scm after a cell produces a plot.
(define *last-plot-data* #false)

;; ─── State ────────────────────────────────────────────────────────────────────

(struct ChartState
  (plot-data    ;; JSON string of the plot_data array
   x-min        ;; current viewport bounds (or #false for auto)
   x-max
   y-min
   y-max
   chart-cols   ;; width of the chart area in columns
   chart-rows)  ;; height of the chart area in rows
  #:mutable)

;;@doc
;; Build the JSON parameters string for the Rust renderer.
(define (build-render-params state)
  (define pd (ChartState-plot-data state))
  (define cols (ChartState-chart-cols state))
  (define rows (ChartState-chart-rows state))
  (define x0 (ChartState-x-min state))
  (define x1 (ChartState-x-max state))
  (define y0 (ChartState-y-min state))
  (define y1 (ChartState-y-max state))

  ;; Build JSON by hand to avoid needing a JSON serialiser in Scheme.
  (string-append
    "{\"plot_data\":" pd
    ",\"cols\":" (number->string cols)
    ",\"rows\":" (number->string rows)
    (if x0 (string-append ",\"x_min\":" (number->string x0)) "")
    (if x1 (string-append ",\"x_max\":" (number->string x1)) "")
    (if y0 (string-append ",\"y_min\":" (number->string y0)) "")
    (if y1 (string-append ",\"y_max\":" (number->string y1)) "")
    "}"))

;; ─── Rendering ────────────────────────────────────────────────────────────────

(define (render-chart state rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))

  ;; Leave room for border, title, axis labels, and help bar.
  (define chart-w (max 10 (- rw 14)))  ;; 8 chars for y-axis labels + 6 padding
  (define chart-h (max 4 (- rh 6)))    ;; 2 for title, 1 for x-axis, 1 for help, 2 border

  (set-ChartState-chart-cols! state chart-w)
  (set-ChartState-chart-rows! state chart-h)

  ;; Call Rust renderer.
  (define params (build-render-params state))
  (define result-json (render-braille-chart params))
  (define err (json-get result-json "error"))

  ;; Compute popup position (centred).
  (define popup-w (min (- rw 2) (+ chart-w 12)))
  (define popup-h (min (- rh 2) (+ chart-h 5)))
  (define px (max 0 (quotient (- rw popup-w) 2)))
  (define py (max 0 (quotient (- rh popup-h) 2)))
  (define popup-area (area px py popup-w popup-h))

  ;; Clear and draw border.
  (define bg-style (style))
  (define border-style (style-fg (style) Color/Cyan))
  (define title-style (style-with-bold (style-fg (style) Color/White)))
  (define axis-style (style-fg (style) Color/Gray))
  (define help-style (style-fg (style) Color/Gray))
  (define chart-style (style-fg (style) Color/Green))
  (define err-style (style-fg (style) Color/Red))

  (buffer/clear buf popup-area)
  (block/render buf popup-area (make-block bg-style border-style "all" "rounded"))

  ;; Title line.
  (define labels-json (json-get result-json "series_labels"))
  (define title "Plot Viewer")
  (frame-set-string! buf (+ px 3) (+ py 1) title title-style)

  (cond
    [(and (string? err) (> (string-length err) 0))
     ;; Error state.
     (frame-set-string! buf (+ px 3) (+ py 3) err err-style)]
    [else
     ;; Draw y-axis labels.
     (define y-top (json-get result-json "y_label_top"))
     (define y-bot (json-get result-json "y_label_bottom"))
     (define label-x (+ px 2))
     (define chart-x (+ px 10))
     (define chart-y (+ py 2))

     (frame-set-string! buf label-x chart-y y-top axis-style)
     (frame-set-string! buf label-x (+ chart-y chart-h -1) y-bot axis-style)

     ;; Draw chart lines.
     (define lines-json (json-get result-json "lines"))
     ;; Parse the lines array from JSON.  The result is a JSON array of strings.
     ;; We need to iterate it.  Since we don't have a JSON array parser in Scheme,
     ;; we parse the render result as individual line strings from the JSON.
     ;; The Rust side returns {"lines": ["...", "..."], ...}.
     ;; We'll use a simple extraction: split on the "lines" field.
     (draw-chart-lines buf result-json chart-x chart-y chart-w chart-h chart-style)

     ;; Draw x-axis labels.
     (define x-left (json-get result-json "x_label_left"))
     (define x-right (json-get result-json "x_label_right"))
     (define x-axis-y (+ chart-y chart-h))
     (frame-set-string! buf chart-x x-axis-y x-left axis-style)
     (when (> (string-length x-right) 0)
       (define right-pos (max chart-x (- (+ chart-x chart-w) (string-length x-right))))
       (frame-set-string! buf right-pos x-axis-y x-right axis-style))

     ;; Help bar at bottom.
     (define help-y (+ py popup-h -2))
     (frame-set-string! buf (+ px 2) help-y "+/- zoom  h/j/k/l pan  r reset  q close" help-style)]))

;;@doc
;; Draw chart lines from the render result JSON.
;; Extracts the "lines" array and writes each string at the correct position.
(define (draw-chart-lines buf result-json chart-x chart-y chart-w chart-h chart-style)
  ;; Extract lines from JSON.  The "lines" field is a JSON array of strings.
  ;; We parse it by finding the array brackets and splitting.
  (define raw (json-get result-json "lines"))
  ;; json-get returns the array as a JSON string like ["...", "..."]
  ;; We need to parse this array.  Simple approach: the render result has
  ;; exactly chart-h lines, each of length chart-w braille characters.
  ;; We can extract them by position from the raw JSON array string.
  (when (string-starts-with? raw "[")
    (define inner (substring raw 1 (- (string-length raw) 1)))
    (define line-strings (parse-json-string-array inner))
    (let loop ([idx 0] [lines line-strings])
      (when (and (not (null? lines)) (< idx chart-h))
        (frame-set-string! buf chart-x (+ chart-y idx) (car lines) chart-style)
        (loop (+ idx 1) (cdr lines))))))

;;@doc
;; Parse a comma-separated list of JSON strings into a list of decoded strings.
;; Input: "\"abc\",\"def\",\"ghi\""
;; Output: ("abc" "def" "ghi")
(define (parse-json-string-array s)
  (if (or (not s) (equal? s ""))
      '()
      (let loop ([pos 0] [acc '()])
        (cond
          [(>= pos (string-length s)) (reverse acc)]
          ;; Skip whitespace and commas.
          [(or (eqv? (string-ref s pos) #\,)
               (eqv? (string-ref s pos) #\space))
           (loop (+ pos 1) acc)]
          ;; Found a quoted string.
          [(eqv? (string-ref s pos) #\")
           (define end-pos (find-json-str-end s (+ pos 1)))
           (if end-pos
               (let ([str-content (substring s (+ pos 1) end-pos)])
                 (loop (+ end-pos 1) (cons str-content acc)))
               (reverse acc))]
          [else (loop (+ pos 1) acc)]))))

;; Find the closing quote of a JSON string, handling escapes.
(define (find-json-str-end s pos)
  (cond
    [(>= pos (string-length s)) #false]
    [(eqv? (string-ref s pos) #\\) (find-json-str-end s (+ pos 2))]
    [(eqv? (string-ref s pos) #\") pos]
    [else (find-json-str-end s (+ pos 1))]))

;; ─── Event handling ───────────────────────────────────────────────────────────

(define (handle-chart-event state event)
  (define char (key-event-char event))
  (cond
    ;; Close.
    [(or (key-event-escape? event) (eqv? char #\q))
     event-result/close]

    ;; Zoom in.
    [(or (eqv? char #\+) (eqv? char #\=))
     (zoom-viewport! state 0.8)
     event-result/consume]

    ;; Zoom out.
    [(eqv? char #\-)
     (zoom-viewport! state 1.25)
     event-result/consume]

    ;; Pan left.
    [(or (eqv? char #\h) (key-event-left? event))
     (pan-viewport! state -0.2 0.0)
     event-result/consume]

    ;; Pan right.
    [(or (eqv? char #\l) (key-event-right? event))
     (pan-viewport! state 0.2 0.0)
     event-result/consume]

    ;; Pan up.
    [(or (eqv? char #\k) (key-event-up? event))
     (pan-viewport! state 0.0 0.2)
     event-result/consume]

    ;; Pan down.
    [(or (eqv? char #\j) (key-event-down? event))
     (pan-viewport! state 0.0 -0.2)
     event-result/consume]

    ;; Reset to auto viewport.
    [(eqv? char #\r)
     (set-ChartState-x-min! state #false)
     (set-ChartState-x-max! state #false)
     (set-ChartState-y-min! state #false)
     (set-ChartState-y-max! state #false)
     event-result/consume]

    ;; Mouse scroll zoom.
    [(mouse-event? event)
     (cond
       [(= (event-mouse-kind event) 11)  ;; ScrollUp = zoom in
        (zoom-viewport! state 0.9)
        event-result/consume]
       [(= (event-mouse-kind event) 10)  ;; ScrollDown = zoom out
        (zoom-viewport! state 1.1)
        event-result/consume]
       [else event-result/consume])]

    [else event-result/consume]))

;;@doc
;; Zoom the viewport by a factor around its centre.
;; factor < 1.0 zooms in, > 1.0 zooms out.
(define (zoom-viewport! state factor)
  ;; If viewport is auto, first resolve to concrete values by doing a
  ;; dummy render to get the auto-computed bounds.
  (ensure-concrete-viewport! state)
  (define x0 (ChartState-x-min state))
  (define x1 (ChartState-x-max state))
  (define y0 (ChartState-y-min state))
  (define y1 (ChartState-y-max state))
  (define cx (/ (+ x0 x1) 2))
  (define cy (/ (+ y0 y1) 2))
  (define hw (* (/ (- x1 x0) 2) factor))
  (define hh (* (/ (- y1 y0) 2) factor))
  (set-ChartState-x-min! state (- cx hw))
  (set-ChartState-x-max! state (+ cx hw))
  (set-ChartState-y-min! state (- cy hh))
  (set-ChartState-y-max! state (+ cy hh)))

;;@doc
;; Pan the viewport by a fraction of its current range.
(define (pan-viewport! state dx-frac dy-frac)
  (ensure-concrete-viewport! state)
  (define x0 (ChartState-x-min state))
  (define x1 (ChartState-x-max state))
  (define y0 (ChartState-y-min state))
  (define y1 (ChartState-y-max state))
  (define dx (* (- x1 x0) dx-frac))
  (define dy (* (- y1 y0) dy-frac))
  (set-ChartState-x-min! state (+ x0 dx))
  (set-ChartState-x-max! state (+ x1 dx))
  (set-ChartState-y-min! state (+ y0 dy))
  (set-ChartState-y-max! state (+ y1 dy)))

;;@doc
;; If the viewport is still in auto mode (#false), resolve it by calling
;; the renderer once to get the auto-computed bounds.
(define (ensure-concrete-viewport! state)
  (when (not (ChartState-x-min state))
    (define params (build-render-params state))
    (define result-json (render-braille-chart params))
    (define x0-str (json-get result-json "x_min"))
    (define x1-str (json-get result-json "x_max"))
    (define y0-str (json-get result-json "y_min"))
    (define y1-str (json-get result-json "y_max"))
    (set-ChartState-x-min! state (string->number x0-str))
    (set-ChartState-x-max! state (string->number x1-str))
    (set-ChartState-y-min! state (string->number y0-str))
    (set-ChartState-y-max! state (string->number y1-str))))

;; ─── Public API ───────────────────────────────────────────────────────────────

;;@doc
;; Open the interactive chart viewer with the given plot data JSON string.
;; If no argument is given, uses *last-plot-data*.
(define (view-chart . args)
  (define pd (if (and (not (null? args)) (car args))
                 (car args)
                 *last-plot-data*))
  (when (not pd)
    (set-status! "No plot data available. Execute a cell that produces a plot first.")
    (error "No plot data"))

  (define state (ChartState pd #false #false #false #false 60 15))

  (define component
    (new-component! "chart-viewer"
      state
      render-chart
      (hash "handle_event" handle-chart-event)))

  (push-component! (overlaid component)))
