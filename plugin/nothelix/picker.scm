;;; picker.scm — interactive cell picker popup

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/components)
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(provide cell-picker)

(struct CellPickerState (cells selected digits) #:mutable)

;; Marker parsing

;;@doc
;; Extract the label from the `# comment` portion of a marker line.
(define (extract-comment-label str)
  (define parts (string-split str "#"))
  (if (< (length parts) 2)
      ""
      (string-trim (list-ref parts 1))))

(define (parse-cell-header line)
  (define (strip-trailing-newline s)
    (if (string-suffix? s "\n")
        (substring s 0 (- (string-length s) 1))
        s))
  (cond
    [(string-starts-with? line "@cell ")
     (define rest (strip-trailing-newline
                    (substring line (string-length "@cell ") (string-length line))))
     (define label (extract-comment-label rest))
     (define before-hash (string-trim (car (string-split rest "#"))))
     (define parts (string-split before-hash " "))
     (define first (if (null? parts) "" (car parts)))
     (define maybe-num (string->number first))
     (define idx (if maybe-num maybe-num 0))
     (define lang-tok
       (cond
         [maybe-num (if (> (length parts) 1) (cadr parts) ":julia")]
         [else first]))
     (define lang
       (cond
         [(and (> (string-length lang-tok) 0) (char=? (string-ref lang-tok 0) #\:))
          (substring lang-tok 1 (string-length lang-tok))]
         [(> (string-length lang-tok) 0) lang-tok]
         [else "julia"]))
     (list (string-append "code (" lang ")") idx label)]
    [(string-starts-with? line "@markdown ")
     (define rest (strip-trailing-newline
                    (substring line (string-length "@markdown ") (string-length line))))
     (define label (extract-comment-label rest))
     (define before-hash (string-trim (car (string-split rest "#"))))
     (define idx (or (string->number before-hash) 0))
     (list "markdown" idx label)]
    [(string-starts-with? line "@raw ")
     (define rest (strip-trailing-newline
                    (substring line (string-length "@raw ") (string-length line))))
     (define label (extract-comment-label rest))
     (define before-hash (string-trim (car (string-split rest "#"))))
     (define idx (or (string->number before-hash) 0))
     (list "raw" idx label)]
    [(string-starts-with? line "@typst ")
     (define rest (strip-trailing-newline
                    (substring line (string-length "@typst ") (string-length line))))
     (define label (extract-comment-label rest))
     (define before-hash (string-trim (car (string-split rest "#"))))
     (define idx (or (string->number before-hash) 0))
     (list "typst" idx label)]
    [else (list "unknown" 0 "")]))

;;@doc
;; Scan the document for all @cell/@markdown/@raw/@typst markers.
(define (get-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (find-cells line-idx acc)
    (if (>= line-idx total-lines)
        (reverse acc)
        (let ([line (doc-get-line rope total-lines line-idx)])
          (cond
            [(or (string-starts-with? line "@cell ")
                 (string-starts-with? line "@markdown ")
                 (string-starts-with? line "@raw ")
                 (string-starts-with? line "@typst "))
             (define parsed (parse-cell-header line))
             (define kind-label (list-ref parsed 0))
             (define idx (list-ref parsed 1))
             (define user-label (list-ref parsed 2))
             (find-cells (+ line-idx 1)
                         (cons (list line-idx kind-label idx line user-label) acc))]
            [else (find-cells (+ line-idx 1) acc)]))))

  (find-cells 0 '()))

;;@doc
;; Get up to `max-lines` lines of cell content starting after `line-num`.
(define (get-cell-preview line-num max-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (let loop ([idx (+ line-num 1)] [collected 0] [lines '()])
    (if (or (>= idx total-lines) (>= collected max-lines))
        (reverse lines)
        (let ([line (doc-get-line rope total-lines idx)])
          (if (or (string-starts-with? line "@cell ")
                  (string-starts-with? line "@markdown ")
                  (string-starts-with? line "@raw ")
                  (string-starts-with? line "@typst ")
                  (string-starts-with? line "# ═══")
                  (string-contains? line "# ─── Output"))
              (reverse lines)
              (loop (+ idx 1) (+ collected 1) (cons line lines)))))))

(define (picker-theme-styles)
  (list
    (theme-scope *helix.cx* "ui.popup")
    (theme-scope *helix.cx* "ui.text")
    (theme-scope *helix.cx* "ui.menu.selected")))

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
         [styles (picker-theme-styles)]
         [popup-style (list-ref styles 0)]
         [text-style (list-ref styles 1)]
         [selected-style (list-ref styles 2)])

    (buffer/clear buf list-area)
    (block/render buf list-area (make-block popup-style popup-style "all" "plain"))

    (define selected-cell-idx
      (if (and (>= selected 0) (< selected (length cells)))
          (list-ref (list-ref cells selected) 2)
          0))
    (define digits (CellPickerState-digits state))
    (define title
      (if (> (string-length digits) 0)
          (string-append "Jump to Cell: " digits "_")
          (string-append "Jump to Cell: " (number->string selected-cell-idx))))
    (frame-set-string! buf (+ x 2) y title text-style)

    (let loop ([i 0])
      (when (< i (length cells))
        (let* ([cell (list-ref cells i)]
               [kind-label (list-ref cell 1)]
               [user-label (if (>= (length cell) 5) (list-ref cell 4) "")]
               [row-style (if (= i selected) selected-style text-style)]
               [row-text
                (if (> (string-length user-label) 0)
                    user-label
                    kind-label)])
          (frame-set-string! buf (+ x 2) (+ y i 1) row-text row-style)
          (loop (+ i 1)))))

    (buffer/clear buf preview-area)
    (block/render buf preview-area (make-block popup-style popup-style "all" "plain"))
    (frame-set-string! buf (+ x list-width 4) y "Preview" text-style)

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
              (frame-set-string! buf (+ x list-width 4) (+ y i 1) truncated text-style)
              (loop (+ i 1)))))))))

(define (find-row-for-cell-index cells idx)
  (let loop ([i 0])
    (cond
      [(>= i (length cells)) #f]
      [(= (list-ref (list-ref cells i) 2) idx) i]
      [else (loop (+ i 1))])))

(define (jump-to-row cells row)
  (when (and (>= row 0) (< row (length cells)))
    (let* ([cell (list-ref cells row)]
           [line-num (list-ref cell 0)])
      ;; line-num is 0-indexed, helix.goto expects 1-indexed
      (helix.goto (number->string (+ line-num 1))))))

(define (handle-cell-picker-event state event)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [digits (CellPickerState-digits state)]
         [char (key-event-char event)]
         [digit-value (and char (char->number char))])
    (cond
      [(or (key-event-escape? event) (eqv? char #\q))
       (set-CellPickerState-digits! state "")
       event-result/close]

      [(or (eqv? char #\j) (key-event-down? event))
       (set-CellPickerState-digits! state "")
       (when (< selected (- (length cells) 1))
         (set-CellPickerState-selected! state (+ selected 1)))
       event-result/consume]
      [(or (eqv? char #\k) (key-event-up? event))
       (set-CellPickerState-digits! state "")
       (when (> selected 0)
         (set-CellPickerState-selected! state (- selected 1)))
       event-result/consume]

      [digit-value
       (let* ([new-buf (string-append digits (list->string (list char)))]
              [num (string->number new-buf)]
              [match-row (and num (find-row-for-cell-index cells num))])
         (set-CellPickerState-digits! state new-buf)
         (when match-row
           (set-CellPickerState-selected! state match-row))
         event-result/consume)]

      [(key-event-enter? event)
       (cond
         [(> (string-length digits) 0)
          (let* ([num (string->number digits)]
                 [row (and num (find-row-for-cell-index cells num))])
            (when row (jump-to-row cells row)))]
         [else (jump-to-row cells selected)])
       (set-CellPickerState-digits! state "")
       event-result/close]

      [else
       (set-CellPickerState-digits! state "")
       event-result/consume])))

(define (make-cell-picker-component)
  (define cells (get-all-cells))
  (new-component! "cell-picker"
    (CellPickerState cells (initial-selection cells) "")
    render-cell-picker
    (hash "handle_event" handle-cell-picker-event)))

;;@doc
;; Pick the cell to highlight when the picker opens (the one the cursor is inside).
(define (initial-selection cells)
  (cond
    [(null? cells) 0]
    [else
     (define cursor (current-line-number))
     (let loop ([i 0] [best 0])
       (cond
         [(>= i (length cells)) best]
         [(<= (car (list-ref cells i)) cursor)
          (loop (+ i 1) i)]
         [else best]))]))

;;@doc
;; Open interactive cell picker
(define (cell-picker)
  (push-component! (make-cell-picker-component)))
