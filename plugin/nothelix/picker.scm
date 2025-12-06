;;; picker.scm - Interactive cell picker component

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For push-component!
(require-builtin helix/components)  ; Import all component functions globally
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(provide cell-picker
         get-all-cells
         get-cell-preview
         CellPickerState
         make-cell-picker-component)

(struct CellPickerState (cells selected) #:mutable)

(define (get-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (find-cells line-idx acc)
    (if (>= line-idx total-lines)
        (reverse acc)
        (let ([line (get-line line-idx)])
          (cond
            [(string-starts-with? line "@cell ")
             (find-cells (+ line-idx 1) (cons (list line-idx "Code" line) acc))]
            [(string-starts-with? line "@markdown ")
             (find-cells (+ line-idx 1) (cons (list line-idx "Markdown" line) acc))]
            [else (find-cells (+ line-idx 1) acc)]))))

  (find-cells 0 '()))

(define (get-cell-preview line-num max-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (let loop ([idx (+ line-num 1)] [collected 0] [lines '()])
    (if (or (>= idx total-lines) (>= collected max-lines))
        (reverse lines)
        (let ([line (get-line idx)])
          (if (or (string-starts-with? line "@cell ")
                  (string-starts-with? line "@markdown ")
                  (string-starts-with? line "# ═══")
                  (string-contains? line "# ─── Output"))
              (reverse lines)
              (loop (+ idx 1) (+ collected 1) (cons line lines)))))))

(define (render-cell-picker state rect buf)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [rect-width (area-width rect)]
         [rect-height (area-height rect)]
         [total-width (min 100 (- rect-width 4))]
         [list-width 35]
         [preview-width (- total-width list-width 2)]
         [height (min (+ (max (length cells) 5) 2) (- rect-height 4))]
         [x (ceiling (max 0 (- (ceiling (/ rect-width 2)) (floor (/ total-width 2)))))]
         [y (ceiling (max 0 (- (ceiling (/ rect-height 2)) (floor (/ height 2)))))]
         [list-area (area x y list-width height)]
         [preview-area (area (+ x list-width 2) y preview-width height)]
         ;; Use basic styles for now
         [popup-style (style)]
         [active-style (style)]
         [preview-style (style)])

    (buffer/clear buf list-area)
    (block/render buf list-area (make-block popup-style (style) "all" "plain"))
    (frame-set-string! buf (+ x 2) y "Jump to Cell" active-style)

    (let loop ([i 0])
      (when (< i (length cells))
        (let* ([cell (list-ref cells i)]
               [line-num (list-ref cell 0)]
               [cell-type (list-ref cell 1)]
               [current-style (if (= i selected) active-style popup-style)])
          (frame-set-string! buf (+ x 2) (+ y i 1)
            (string-append (number->string (+ i 1)) ". " cell-type " [" (number->string line-num) "]")
            current-style)
          (loop (+ i 1)))))

    (buffer/clear buf preview-area)
    (block/render buf preview-area (make-block popup-style (style) "all" "plain"))
    (frame-set-string! buf (+ x list-width 4) y "Preview" active-style)

    (when (and (>= selected 0) (< selected (length cells)))
      (let* ([cell (list-ref cells selected)]
             [line-num (list-ref cell 0)]
             [preview-lines (get-cell-preview line-num (- height 3))]
             [max-preview-width (- preview-width 4)])
        (let loop ([i 0])
          (when (< i (length preview-lines))
            (let* ([line (list-ref preview-lines i)]
                   [truncated (if (> (string-length line) max-preview-width)
                                  (string-append (substring line 0 (- max-preview-width 3)) "...")
                                  line)])
              (frame-set-string! buf (+ x list-width 4) (+ y i 1) truncated preview-style)
              (loop (+ i 1)))))))))

(define (handle-cell-picker-event state event)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [char (key-event-char event)])
    (cond
      [(or (key-event-escape? event) (eqv? char #\q))
       event-result/close]
      [(eqv? char #\j)
       (when (< selected (- (length cells) 1))
         (set-CellPickerState-selected! state (+ selected 1)))
       event-result/consume]
      [(eqv? char #\k)
       (when (> selected 0)
         (set-CellPickerState-selected! state (- selected 1)))
       event-result/consume]
      [(key-event-enter? event)
       (when (< selected (length cells))
         (let* ([cell (list-ref cells selected)]
                [line-num (list-ref cell 0)])
           ;; line-num is 0-indexed, helix.goto expects 1-indexed
           (helix.goto (number->string (+ line-num 1)))))
       event-result/close]
      [else
       (let ([num (char->number (or char #\null))])
         (if (and (not (eqv? num #false)) (>= num 1) (<= num (length cells)))
             (begin
               (let* ([cell (list-ref cells (- num 1))]
                      [line-num (list-ref cell 0)])
                 ;; line-num is 0-indexed, helix.goto expects 1-indexed
                 (helix.goto (number->string (+ line-num 1))))
               event-result/close)
             event-result/consume))])))

(define (make-cell-picker-component)
  (new-component! "cell-picker"
    (CellPickerState (get-all-cells) 0)
    render-cell-picker
    (hash "handle_event" handle-cell-picker-event)))

;;@doc
;; Open interactive cell picker
(define (cell-picker)
  (push-component! (make-cell-picker-component)))
