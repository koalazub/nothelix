;;; picker.scm - Interactive cell picker component
;;;
;;; Provides a popup UI that lists all cells in the current notebook.
;;; Navigate with j/k, press Enter or a digit to jump to that cell.
;;;
;;; Items in the list read as `N. code (julia)` or `N. markdown`,
;;; where N is the actual cell index parsed out of the `@cell N …`
;;; or `@markdown N` marker (not the picker's 1-based row number —
;;; those two diverge once you've renumbered or deleted cells mid-
;;; file). Styling is pulled from the current Helix theme via
;;; `theme-scope` so the picker matches whatever colourscheme the
;;; user has configured in `config.toml`.

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
;; `require-builtin helix/components` pulls in the raw Rust-side
;; names (area, buffer/clear, block/render, frame-set-string!,
;; new-component!, push-component!, theme-scope, …). Those names
;; take the Context as their first argument — calls to them have
;; to pass `*helix.cx*` explicitly. We do NOT also require the
;; `helix/components.scm` cog here because its wrapper for
;; `theme-scope` has the same name and would collide in resolution.
(require-builtin helix/components)
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(provide cell-picker)

(struct CellPickerState (cells selected) #:mutable)

;; ─── Marker parsing ───────────────────────────────────────────────────────────

;;@doc
;; Parse a `@cell N :lang` or `@markdown N` header line into a
;; `(cons label cell-index)` pair where label is the pretty text
;; shown in the picker ("code (julia)" / "markdown") and cell-index
;; is the integer parsed from the marker. Falls back to (0, kind)
;; when the header is malformed so the picker still renders
;; something rather than crashing.
(define (parse-cell-header line)
  (define (strip-trailing-newline s)
    (if (string-suffix? s "\n")
        (substring s 0 (- (string-length s) 1))
        s))
  (cond
    [(string-starts-with? line "@cell ")
     (define rest (strip-trailing-newline
                    (substring line
                               (string-length "@cell ")
                               (string-length line))))
     (define trimmed (string-trim rest))
     ;; `N :lang` or `N` or `:lang` (when autofill hasn't stamped
     ;; the index yet) — be forgiving.
     (define parts (string-split trimmed " "))
     (define first (if (null? parts) "" (car parts)))
     (define maybe-num (string->number first))
     (define idx (if maybe-num maybe-num 0))
     (define lang-tok
       (cond
         [maybe-num
          (if (> (length parts) 1) (cadr parts) ":julia")]
         [else first]))
     (define lang
       (cond
         [(and (> (string-length lang-tok) 0)
               (char=? (string-ref lang-tok 0) #\:))
          (substring lang-tok 1 (string-length lang-tok))]
         [(> (string-length lang-tok) 0) lang-tok]
         [else "julia"]))
     (cons (string-append "code (" lang ")") idx)]
    [(string-starts-with? line "@markdown ")
     (define rest (strip-trailing-newline
                    (substring line
                               (string-length "@markdown ")
                               (string-length line))))
     (define idx (or (string->number (string-trim rest)) 0))
     (cons "markdown" idx)]
    [else (cons "unknown" 0)]))

;;@doc
;; Scan the document for all @cell and @markdown markers.
;; Returns a list of (line-number kind-label cell-index header-text) tuples.
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
                 (string-starts-with? line "@markdown "))
             (define parsed (parse-cell-header line))
             (define label (car parsed))
             (define idx (cdr parsed))
             (find-cells (+ line-idx 1)
                         (cons (list line-idx label idx line) acc))]
            [else (find-cells (+ line-idx 1) acc)]))))

  (find-cells 0 '()))

;;@doc
;; Get a preview of the cell content starting at `line-num`.
;; Returns up to `max-lines` lines of code (stops at next marker or output).
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
                  (string-starts-with? line "# ═══")
                  (string-contains? line "# ─── Output"))
              (reverse lines)
              (loop (+ idx 1) (+ collected 1) (cons line lines)))))))

;; Pull styles from the active Helix theme. Four scopes used:
;;
;;   ui.popup         — background of the floating popup
;;   ui.text          — normal foreground for list items and preview
;;   ui.menu          — menu row fill (fallback when ui.popup is empty)
;;   ui.menu.selected — highlighted (currently-focused) list row
;;
;; `theme-scope` is the raw built-in from `require-builtin
;; helix/components`, which takes the Context as its first arg.
;; Each call returns a `Style` with whatever the user's
;; colourscheme has defined for that scope and falls back
;; gracefully on themes that don't define it.
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
    (frame-set-string! buf (+ x 2) y "Jump to Cell" text-style)

    (let loop ([i 0])
      (when (< i (length cells))
        (let* ([cell (list-ref cells i)]
               ;; cell = (line-num kind-label cell-index header-text)
               [label (list-ref cell 1)]
               [cell-idx (list-ref cell 2)]
               [row-style (if (= i selected) selected-style text-style)]
               [row-text
                (string-append (number->string cell-idx) ". " label)])
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
         (if (and num (>= num 1) (<= num (length cells)))
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
