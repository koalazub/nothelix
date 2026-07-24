;;; picker.scm — interactive cell picker popup

(require "common.scm")
(require "string-utils.scm")
(require "image-cache.scm")
(require "output-store.scm")
(require "cell-state.scm")
(require "project-config.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/components)
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          slm-available
                          slm-refresh-summaries
                          slm-summary-for))

(provide cell-picker cell-summary kind-tag picker-scroll-offset
         fuzzy-score fuzzy-filter format-duration picker-glyph picker-duration
         line-declares-widget?)

(struct CellPickerState (cells view selected digits query filtering?) #:mutable)

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

(define (heading-strip s)
  (let loop ([t (string-trim s)])
    (if (and (> (string-length t) 0) (char=? (string-ref t 0) #\#))
        (loop (string-trim (substring t 1 (string-length t))))
        t)))

;;@doc
;; First meaningful content line of a cell, for the picker row: skips blanks,
;; code comments, bare $$ fences, and @image refs; strips markdown heading
;; markers and the `# ` comment prefix markdown cells carry in a .jl.
(define (cell-summary kind lines)
  (define code? (string-starts-with? kind "code"))
  (let loop ([ls lines])
    (if (null? ls)
        ""
        (let* ([raw (string-trim (car ls))]
               [t (if code? raw (heading-strip raw))])
          (cond
            [(= (string-length t) 0) (loop (cdr ls))]
            [(and code? (string-starts-with? t "#")) (loop (cdr ls))]
            [(equal? t "$$") (loop (cdr ls))]
            [(string-starts-with? t "@image") (loop (cdr ls))]
            [else t])))))

;; How many lines of a cell's preview participate in its SLM hash — must
;; stay in lockstep between the hash-read and refresh-blob call sites so
;; both agree on the same djb2 hash for the same cell.
(define *slm-preview-lines* 8)

;; ASCII record separator — must match libnothelix's slm.rs CELL_SEP.
(define *slm-cell-sep* (make-string 1 (integer->char 30)))

;;@doc
;; The ONE definition of "cell text" the SLM hash is computed over — shared
;; by the picker-row hash lookup and the refresh blob-builder so a cell's
;; hash always agrees between the two call sites.
(define (cell-text-for-hash line-num)
  (string-join (get-cell-preview line-num *slm-preview-lines*) "\n"))

;;@doc
;; Picker-row snippet for one cell: cached SLM summary (opt-in, when
;; present) over the heuristic first-meaningful-line summary. A stale or
;; not-yet-refreshed hash simply misses the cache and falls through.
(define (cell-picker-snippet kind line-num)
  (define heuristic (cell-summary kind (get-cell-preview line-num *slm-preview-lines*)))
  (cond
    [(not (slm-summaries?)) heuristic]
    [else
     (define hash (number->string (djb2-hash (cell-text-for-hash line-num))))
     (define slm (slm-summary-for (workspace-id) hash))
     (if (> (string-length slm) 0) slm heuristic)]))

;;@doc
;; Compact type tag for a picker row: markdown -> md, code (julia) -> jl.
(define (kind-tag kind)
  (cond
    [(equal? kind "markdown") "md"]
    [(string-starts-with? kind "code (")
     (let ([inner (substring kind 6 (- (string-length kind) 1))])
       (cond
         [(equal? inner "julia") "jl"]
         [(> (string-length inner) 5) (substring inner 0 5)]
         [else inner]))]
    [else kind]))

;;@doc
;; Scroll offset that keeps `selected` centered in a `visible`-row window,
;; clamped to the list bounds.
(define (picker-scroll-offset selected visible total)
  (min (max 0 (- total visible))
       (max 0 (- selected (quotient visible 2)))))

(define (ch-down c)
  (define n (char->integer c))
  (if (and (>= n 65) (<= n 90)) (integer->char (+ n 32)) c))

;;@doc
;; Case-insensitive subsequence match of `query-chars` against `hay-chars`.
;; Returns a score (contiguous runs and early hits rank higher) or #false
;; when the query is not a subsequence of the haystack.
(define (fuzzy-score query-chars hay-chars)
  (let loop ([q query-chars] [h hay-chars] [pos 0] [prev -2] [score 0] [first -1])
    (cond
      [(null? q) (+ score (if (>= first 0) (max 0 (- 20 first)) 0))]
      [(null? h) #false]
      [(char=? (ch-down (car q)) (car h))
       (loop (cdr q) (cdr h) (+ pos 1) pos
             (+ score (if (= prev (- pos 1)) 3 1))
             (if (< first 0) pos first))]
      [else (loop q (cdr h) (+ pos 1) prev score first)])))

;;@doc
;; Filter `cells` (each carrying a downcased haystack char list as its 7th
;; element) by fuzzy `query`, preserving document order. Returns
;; `(matching-cells . best-row-index)` — the row of the highest score.
(define (fuzzy-filter cells query)
  (define qchars (map ch-down (string->list query)))
  (if (null? qchars)
      (cons cells 0)
      (let loop ([cs cells] [kept '()] [row 0] [best-row 0] [best-score -1])
        (if (null? cs)
            (cons (reverse kept) best-row)
            (let* ([c (car cs)]
                   [hay (if (>= (length c) 7) (list-ref c 6) '())]
                   [s (fuzzy-score qchars hay)])
              (if s
                  (loop (cdr cs) (cons c kept) (+ row 1)
                        (if (> s best-score) row best-row)
                        (max s best-score))
                  (loop (cdr cs) kept row best-row best-score)))))))

(define (truncate-to s n)
  (if (> (string-length s) n)
      (string-append (substring s 0 (max 0 (- n 1))) "…")
      s))

(define (pad-right s n)
  (if (>= (string-length s) n)
      s
      (string-append s (make-string (- n (string-length s)) #\space))))

(define (pad-idx n)
  (define s (number->string n))
  (if (< (string-length s) 2) (string-append " " s) s))

(define *running-marker* "▸")
(define *audio-marker* "♪")
(define *widget-marker* "⊞")

;;@doc
;; #true when `line` declares a widget: a `# @image ` block, or an assignment
;; carrying a trailing `# @param` / `# @select` / `# @toggle` annotation.
(define (line-declares-widget? line)
  (define t (string-trim line))
  (cond
    [(string-starts-with? t "# @image ") #true]
    [else
     (define parts (string-split line "#"))
     (and (>= (length parts) 2)
          (let ([c (string-trim (list-ref parts 1))])
            (or (string-starts-with? c "@param")
                (string-starts-with? c "@select")
                (string-starts-with? c "@toggle"))))]))

(define (marker-cell-idx line)
  (if (or (string-starts-with? line "@cell ")
          (string-starts-with? line "@markdown ")
          (string-starts-with? line "@raw ")
          (string-starts-with? line "@typst "))
      (list-ref (parse-cell-header line) 1)
      #false))

;;@doc
;; Cell indices whose body contains a widget declaration, scanned in one pass.
(define (scan-widget-cells rope total-lines)
  (let loop ([i 0] [cur-idx #false] [acc '()])
    (if (>= i total-lines)
        (reverse acc)
        (let* ([line (doc-get-line rope total-lines i)]
               [midx (marker-cell-idx line)])
          (cond
            [midx (loop (+ i 1) midx acc)]
            [(and cur-idx (not (member cur-idx acc)) (line-declares-widget? line))
             (loop (+ i 1) cur-idx (cons cur-idx acc))]
            [else (loop (+ i 1) cur-idx acc)])))))

(define (refresh-widget-cells!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (set-widget-cells! (scan-widget-cells rope total-lines)))

;;@doc
;; Terse run-time for a picker row: sub-second as whole ms (12ms), a second
;; or longer as one-decimal seconds (1.4s), blank when the cell never ran.
(define (format-duration ms)
  (cond
    [(not ms) ""]
    [(< ms 1000) (string-append (number->string ms) "ms")]
    [else
     (let* ([tenths (quotient (+ ms 50) 100)]
            [whole (quotient tenths 10)]
            [frac (remainder tenths 10)])
       (string-append (number->string whole) "." (number->string frac) "s"))]))

;;@doc
;; Glyph-column content for a cell: the running marker while it executes, the
;; audio marker while its clip plays, its freshness glyph when non-fresh,
;; otherwise the widget marker when the cell declares a widget.
(define (picker-glyph idx)
  (cond
    [(cell-running? idx) *running-marker*]
    [(audio-playing-cell? idx) *audio-marker*]
    [else
     (let ([g (cell-glyph-for idx)])
       (cond
         [(> (string-length g) 0) g]
         [(cell-has-widget? idx) *widget-marker*]
         [else ""]))]))

;;@doc
;; Duration-column content for a cell: blank while it runs, otherwise its
;; formatted last run time.
(define (picker-duration idx)
  (if (cell-running? idx) "" (format-duration (cell-duration-for idx))))

;;@doc
;; Lay out one picker row: left content truncated to leave room for a
;; right-aligned duration column so the duration is never pushed off.
(define (compose-picker-row left dur width)
  (if (or (not dur) (equal? dur ""))
      (truncate-to left width)
      (let* ([dur-w (string-length dur)]
             [avail (max 0 (- width dur-w 1))])
        (string-append (pad-right (truncate-to left avail) avail) " " dur))))

(define (picker-theme-styles)
  (list
    (theme-scope *helix.cx* "ui.popup")
    (theme-scope *helix.cx* "ui.text")
    (theme-scope *helix.cx* "ui.menu.selected")))

(define (render-cell-picker state rect buf)
  (let* ([cells (CellPickerState-view state)]
         [selected (CellPickerState-selected state)]
         [rect-width (area-width rect)]
         [rect-height (area-height rect)]
         [total-width (min 100 (- rect-width 4))]
         [list-width 44]
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
      (cond
        [(CellPickerState-filtering? state)
         (string-append "Search: " (CellPickerState-query state) "_")]
        [(> (string-length digits) 0)
         (string-append "Jump to Cell: " digits "_")]
        [else (string-append "Jump to Cell: " (number->string selected-cell-idx))]))
    (frame-set-string! buf (+ x 2) y title text-style)

    (define hint
      (if (CellPickerState-filtering? state)
          " esc list · enter go "
          " h/l jump · / search · # cell · enter go "))
    (frame-set-string! buf (+ x 2) (+ y height -1) hint text-style)

    (define visible-rows (max 1 (- height 2)))
    (define scroll-offset (picker-scroll-offset selected visible-rows (length cells)))

    (let loop ([i scroll-offset])
      (when (and (< i (length cells)) (< (- i scroll-offset) visible-rows))
        (let* ([cell (list-ref cells i)]
               [kind-label (list-ref cell 1)]
               [idx (list-ref cell 2)]
               [user-label (if (>= (length cell) 5) (list-ref cell 4) "")]
               [summary (if (>= (length cell) 6) (list-ref cell 5) "")]
               [row-style (if (= i selected) selected-style text-style)]
               [snippet (if (> (string-length user-label) 0) user-label summary)]
               [glyph (picker-glyph idx)]
               [left (string-append (if (equal? glyph "") " " glyph) " "
                                    (pad-idx idx) " "
                                    (kind-tag kind-label)
                                    (if (> (string-length snippet) 0) "  " "")
                                    snippet)]
               [row-text (compose-picker-row left (picker-duration idx) (- list-width 4))])
          (frame-set-string! buf (+ x 2) (+ y (- i scroll-offset) 1) row-text row-style)
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

(define (apply-picker-query! state query)
  (define result (fuzzy-filter (CellPickerState-cells state) query))
  (set-CellPickerState-query! state query)
  (set-CellPickerState-view! state (car result))
  (set-CellPickerState-selected! state (cdr result)))

(define (exit-picker-filter! state)
  (define view (CellPickerState-view state))
  (define selected (CellPickerState-selected state))
  (define keep-idx
    (if (and (>= selected 0) (< selected (length view)))
        (list-ref (list-ref view selected) 2)
        #f))
  (set-CellPickerState-filtering?! state #f)
  (set-CellPickerState-query! state "")
  (set-CellPickerState-view! state (CellPickerState-cells state))
  (define row (and keep-idx (find-row-for-cell-index (CellPickerState-cells state) keep-idx)))
  (set-CellPickerState-selected! state (if row row 0)))

(define (handle-cell-picker-event state event)
  (let* ([view (CellPickerState-view state)]
         [selected (CellPickerState-selected state)]
         [digits (CellPickerState-digits state)]
         [char (key-event-char event)]
         [digit-value (and char (char->number char))])
    (cond
      [(CellPickerState-filtering? state)
       (cond
         [(key-event-escape? event)
          (exit-picker-filter! state)
          event-result/consume]
         [(key-event-enter? event)
          (jump-to-row view selected)
          (exit-picker-filter! state)
          event-result/close]
         [(key-event-backspace? event)
          (define q (CellPickerState-query state))
          (if (= (string-length q) 0)
              (exit-picker-filter! state)
              (apply-picker-query! state (substring q 0 (- (string-length q) 1))))
          event-result/consume]
         [(key-event-down? event)
          (when (< selected (- (length view) 1))
            (set-CellPickerState-selected! state (+ selected 1)))
          event-result/consume]
         [(key-event-up? event)
          (when (> selected 0)
            (set-CellPickerState-selected! state (- selected 1)))
          event-result/consume]
         [char
          (apply-picker-query! state
            (string-append (CellPickerState-query state) (list->string (list char))))
          event-result/consume]
         [else event-result/consume])]

      [(or (key-event-escape? event) (eqv? char #\q))
       (set-CellPickerState-digits! state "")
       event-result/close]

      [(eqv? char #\/)
       (set-CellPickerState-digits! state "")
       (set-CellPickerState-filtering?! state #t)
       (apply-picker-query! state "")
       event-result/consume]

      [(or (eqv? char #\j) (key-event-down? event))
       (set-CellPickerState-digits! state "")
       (when (< selected (- (length view) 1))
         (set-CellPickerState-selected! state (+ selected 1)))
       event-result/consume]
      [(or (eqv? char #\k) (key-event-up? event))
       (set-CellPickerState-digits! state "")
       (when (> selected 0)
         (set-CellPickerState-selected! state (- selected 1)))
       event-result/consume]
      [(eqv? char #\l)
       (set-CellPickerState-digits! state "")
       (when (> (length view) 0)
         (set-CellPickerState-selected! state
           (min (- (length view) 1) (+ selected (picker-jump)))))
       event-result/consume]
      [(eqv? char #\h)
       (set-CellPickerState-digits! state "")
       (when (> (length view) 0)
         (set-CellPickerState-selected! state (max 0 (- selected (picker-jump)))))
       event-result/consume]

      [digit-value
       (let* ([new-buf (string-append digits (list->string (list char)))]
              [num (string->number new-buf)]
              [match-row (and num (find-row-for-cell-index view num))])
         (set-CellPickerState-digits! state new-buf)
         (when match-row
           (set-CellPickerState-selected! state match-row))
         event-result/consume)]

      [(key-event-enter? event)
       (cond
         [(> (string-length digits) 0)
          (let* ([num (string->number digits)]
                 [row (and num (find-row-for-cell-index view num))])
            (when row (jump-to-row view row)))]
         [else (jump-to-row view selected)])
       (set-CellPickerState-digits! state "")
       event-result/close]

      [else
       (set-CellPickerState-digits! state "")
       event-result/consume])))

;;@doc
;; Fire-and-forget: kick off a background SLM refresh for every cell in the
;; buffer, when opted in and the on-device model is detected. Cheap no-op
;; otherwise. Never blocks — rows below read whatever's cached right now.
(define (maybe-refresh-slm-summaries! raw-cells)
  (define ws (workspace-id))
  (when (and (slm-summaries?) (equal? (slm-available ws) "yes"))
    (define blob
      (string-join (map (lambda (c) (cell-text-for-hash (car c))) raw-cells)
                   *slm-cell-sep*))
    (slm-refresh-summaries ws blob)))

(define (make-cell-picker-component)
  (define raw-cells (get-all-cells))
  (maybe-refresh-slm-summaries! raw-cells)
  (refresh-widget-cells!)
  (define cells
    (map (lambda (c)
           (define snippet (cell-picker-snippet (list-ref c 1) (car c)))
           (define user-label (list-ref c 4))
           (define label (if (> (string-length user-label) 0) user-label snippet))
           (define hay
             (map ch-down
                  (string->list
                    (string-append (number->string (list-ref c 2)) " "
                                   (kind-tag (list-ref c 1)) " " label))))
           (append c (list snippet hay)))
         raw-cells))
  (new-component! "cell-picker"
    (CellPickerState cells cells (initial-selection cells) "" "" #f)
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
