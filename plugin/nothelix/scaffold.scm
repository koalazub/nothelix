;;; scaffold.scm — Cell marker autofill, renumbering, and notebook scaffolding

(require "common.scm")
(require "string-utils.scm")
(require "debug.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require-builtin helix/components)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          write-string-to-file!
                          path-exists))

(provide notebook-file?
         file-lang
         next-cell-index
         maybe-expand-cell-marker!
         renumber-cells!
         new-notebook
         open-cell-type-picker)

;; File-type predicates

;;@doc
;; #true when `path` looks like a nothelix-managed notebook source (.jl/.py/.ipynb).
(define (notebook-file? path)
  (and path
       (or (string-suffix? path ".jl")
           (string-suffix? path ".py")
           (string-suffix? path ".ipynb"))))

;;@doc
;; Language annotation for `path`'s `@cell N :LANG` marker; defaults to julia.
(define (file-lang path)
  (cond
    [(and path (string-suffix? path ".py")) "python"]
    [else "julia"]))

;; Marker index parsing

;;@doc
;; Parse the leading decimal integer from `str`, or -1 if it doesn't start with a digit.
(define (parse-leading-int str)
  (define len (string-length str))
  (let loop ([i 0])
    (cond
      [(>= i len)
       (if (= i 0) -1 (string->number (substring str 0 i)))]
      [(let ([c (string-ref str i)])
         (and (char>=? c #\0) (char<=? c #\9)))
       (loop (+ i 1))]
      [else
       (if (= i 0) -1 (string->number (substring str 0 i)))])))

;;@doc
;; Highest N across all `@cell/@markdown/@raw/@typst N` markers, plus 1 (0 for a fresh notebook).
(define (next-cell-index rope total-lines)
  (define (scan-after-prefix line prefix)
    (define plen (string-length prefix))
    (if (< (string-length line) plen)
        -1
        (parse-leading-int (substring line plen (string-length line)))))
  (let loop ([line-idx 0] [max-idx -1])
    (if (>= line-idx total-lines)
        (+ max-idx 1)
        (let ([line (doc-get-line rope total-lines line-idx)])
          (cond
            [(string-starts-with? line "@cell ")
             (loop (+ line-idx 1) (max max-idx (scan-after-prefix line "@cell ")))]
            [(string-starts-with? line "@markdown ")
             (loop (+ line-idx 1) (max max-idx (scan-after-prefix line "@markdown ")))]
            [(string-starts-with? line "@raw ")
             (loop (+ line-idx 1) (max max-idx (scan-after-prefix line "@raw ")))]
            [(string-starts-with? line "@typst ")
             (loop (+ line-idx 1) (max max-idx (scan-after-prefix line "@typst ")))]
            [else (loop (+ line-idx 1) max-idx)])))))

;; Line helpers

;;@doc
;; Text of the cursor's current line, without the trailing newline.
(define (current-line-text)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define line-idx (current-line-number))
  (define line (doc-get-line rope total-lines line-idx))
  (if (string-suffix? line "\n")
      (substring line 0 (- (string-length line) 1))
      line))

;;@doc
;; #true when `str` is `@<word> ` — an @, identifier letters, then a single trailing space.
(define (cell-marker-prefix? str)
  (define len (string-length str))
  (and (> len 2)
       (char=? (string-ref str 0) #\@)
       (char=? (string-ref str (- len 1)) #\space)
       (let loop ([i 1])
         (cond
           [(= i (- len 1)) (> i 1)]
           [(let ([c (string-ref str i)])
              (or (and (char>=? c #\a) (char<=? c #\z))
                  (and (char>=? c #\A) (char<=? c #\Z))))
            (loop (+ i 1))]
           [else #false]))))

;;@doc
;; Extract the word between `@` and the trailing space (assumes `cell-marker-prefix?`).
(define (cell-marker-word str)
  (substring str 1 (- (string-length str) 1)))

;; Line replacement

;;@doc
;; Replace the entire current line (including newline) with `replacement`.
(define (replace-current-line-with! replacement)
  (helix.static.goto_line_start)
  (helix.static.extend_to_line_bounds)
  (helix.static.delete_selection)
  (helix.static.insert_string replacement))

;; Expansion

;;@doc
;; Insert a `@cell next-idx :lang` marker at the current line.
(define (expand-code-marker! next-idx lang)
  (replace-current-line-with!
    (string-append "@cell " (number->string next-idx) " :" lang "\n\n"))
  (helix.static.move_line_up))

;;@doc
;; Insert a `@markdown next-idx` marker, seeding a `# ` heading on the next line.
(define (expand-markdown-marker! next-idx)
  (replace-current-line-with!
    (string-append "@markdown " (number->string next-idx) "\n# "))
  #true)

;;@doc
;; Insert a `@typst next-idx` marker with a `# ` prefix line.
(define (expand-typst-marker! next-idx)
  (replace-current-line-with!
    (string-append "@typst " (number->string next-idx) "\n# "))
  #true)

;;@doc
;; On space in a notebook file, expand a complete `@<word> ` line via the markdown direct expander or the cell-type picker.
(define (maybe-expand-cell-marker! char)
  (when (char=? char #\space)
    (define focus (editor-focus))
    (define doc-id (editor->doc-id focus))
    (define path (editor-document->path doc-id))
    (when (notebook-file? path)
      (define line (current-line-text))
      (when (cell-marker-prefix? line)
        (define word (cell-marker-word line))
        (define rope (editor->text doc-id))
        (define total-lines (text.rope-len-lines rope))
        (define next-idx (next-cell-index rope total-lines))
        (define line-idx (current-line-number))
        (debug-log
          (string-append "scaffold.maybe-expand: word=" word
                         " next-idx=" (number->string next-idx)
                         " line-idx=" (number->string line-idx)))
        (cond
          [(or (string=? word "md")
               (string=? word "mark")
               (string=? word "markdown"))
           (expand-markdown-marker! next-idx)]
          [(string=? word "typst")
           (expand-typst-marker! next-idx)]
          [else
           (open-cell-type-picker line-idx next-idx (file-lang path))])))))

;; Cell-type picker

(struct CellTypePickerState (line-idx next-idx lang selected) #:mutable)

(define (cell-type-picker-items state)
  (list
    (string-append "Code cell (" (CellTypePickerState-lang state) ")")
    "Markdown cell"
    "Typst cell"))

(define (render-cell-type-picker state rect buf)
  (let* ([items (cell-type-picker-items state)]
         [selected (CellTypePickerState-selected state)]
         [rect-width (area-width rect)]
         [rect-height (area-height rect)]
         [width 36]
         [height (+ (length items) 2)]
         [x (ceiling (max 0 (- (ceiling (/ rect-width 2)) (floor (/ width 2)))))]
         [y (ceiling (max 0 (- (ceiling (/ rect-height 2)) (floor (/ height 2)))))]
         [list-area (area x y width height)]
         [popup-style (theme-scope *helix.cx* "ui.popup")]
         [text-style (theme-scope *helix.cx* "ui.text")]
         [selected-style (theme-scope *helix.cx* "ui.menu.selected")])
    (buffer/clear buf list-area)
    (block/render buf list-area (make-block popup-style popup-style "all" "plain"))
    (frame-set-string! buf (+ x 2) y "New cell" text-style)
    (let loop ([i 0])
      (when (< i (length items))
        (let* ([label (list-ref items i)]
               [marker (if (= i selected) "> " "  ")]
               [row-style (if (= i selected) selected-style text-style)]
               [text (string-append marker label)])
          (frame-set-string! buf (+ x 2) (+ y i 1) text row-style)
          (loop (+ i 1)))))))

(define (cell-type-picker-commit state)
  (helix.goto (number->string (+ (CellTypePickerState-line-idx state) 1)))
  (define sel (CellTypePickerState-selected state))
  (define idx (CellTypePickerState-next-idx state))
  (cond
    [(= sel 0) (expand-code-marker! idx (CellTypePickerState-lang state))]
    [(= sel 1) (expand-markdown-marker! idx)]
    [(= sel 2) (expand-typst-marker! idx)]))

(define (handle-cell-type-picker-event state event)
  (let ([char (key-event-char event)]
        [items (cell-type-picker-items state)])
    (cond
      [(or (key-event-escape? event) (eqv? char #\q))
       event-result/close]
      [(or (eqv? char #\j) (eqv? char #\n) (key-event-down? event))
       (when (< (CellTypePickerState-selected state) (- (length items) 1))
         (set-CellTypePickerState-selected! state
           (+ (CellTypePickerState-selected state) 1)))
       event-result/consume]
      [(or (eqv? char #\k) (eqv? char #\p) (key-event-up? event))
       (when (> (CellTypePickerState-selected state) 0)
         (set-CellTypePickerState-selected! state
           (- (CellTypePickerState-selected state) 1)))
       event-result/consume]
      [(key-event-enter? event)
       (cell-type-picker-commit state)
       event-result/close]
      [(eqv? char #\1)
       (set-CellTypePickerState-selected! state 0)
       (cell-type-picker-commit state)
       event-result/close]
      [(eqv? char #\2)
       (set-CellTypePickerState-selected! state 1)
       (cell-type-picker-commit state)
       event-result/close]
      [else event-result/consume])))

(define (make-cell-type-picker-component line-idx next-idx lang)
  (new-component! "cell-type-picker"
    (CellTypePickerState line-idx next-idx lang 0)
    render-cell-type-picker
    (hash "handle_event" handle-cell-type-picker-event)))

;;@doc
;; Push a cell-type picker component onto the stack.
(define (open-cell-type-picker line-idx next-idx lang)
  (push-component! (make-cell-type-picker-component line-idx next-idx lang)))

;; Renumber cells

;;@doc
;; Renumber all `@cell/@markdown/@raw/@typst N` markers into a contiguous 0-indexed sequence; runs after saves and as `:renumber-cells`. Pass #false as commit? to defer the undo-history commit to the caller. Returns #true if any markers were renumbered, #false otherwise.
(define (renumber-cells! . args)
  (define commit? (if (null? args) #true (car args)))
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (if (not (notebook-file? path))
      #false
      (let ()
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))

    (define saved-char (cursor-position))
    (define saved-line (text.rope-char->line rope saved-char))
    (define saved-line-start (text.rope-line->char rope saved-line))
    (define saved-col (- saved-char saved-line-start))

    (define markers
      (let loop ([line-idx 0] [acc '()])
        (if (>= line-idx total-lines)
            (reverse acc)
            (let ([line (doc-get-line rope total-lines line-idx)])
              (cond
                [(string-starts-with? line "@cell ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx "@cell " line) acc))]
                [(string-starts-with? line "@markdown ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx "@markdown " line) acc))]
                [(string-starts-with? line "@raw ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx "@raw " line) acc))]
                [(string-starts-with? line "@typst ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx "@typst " line) acc))]
                [else (loop (+ line-idx 1) acc)])))))
    (define (renumbered-marker-line prefix current i)
      (define after-prefix
        (substring current (string-length prefix) (string-length current)))
      (define trimmed
        (if (string-suffix? after-prefix "\n")
            (substring after-prefix 0 (- (string-length after-prefix) 1))
            after-prefix))
      (define rest-after-digits
        (let scan ([j 0])
          (cond
            [(>= j (string-length trimmed))
             (substring trimmed j (string-length trimmed))]
            [(let ([c (string-ref trimmed j)])
               (and (char>=? c #\0) (char<=? c #\9)))
             (scan (+ j 1))]
            [else
             (substring trimmed j (string-length trimmed))])))
      (string-append prefix (number->string i) rest-after-digits))
    (define indexed
      (let loop ([ms markers] [i 0] [acc '()])
        (if (null? ms)
            (reverse acc)
            (let* ([m (car ms)]
                   [line-idx (car m)]
                   [prefix (cadr m)]
                   [current (caddr m)]
                   [current-trimmed
                    (if (string-suffix? current "\n")
                        (substring current 0 (- (string-length current) 1))
                        current)]
                   [new-line (renumbered-marker-line prefix current i)])
              (loop (cdr ms) (+ i 1)
                    (if (string=? new-line current-trimmed)
                        acc
                        (cons (list line-idx new-line) acc)))))))

    (cond
      [(null? indexed)
       (debug-log "scaffold.renumber: nothing to renumber")
       #false]
      [else
       (for-each
         (lambda (entry)
           (define line-idx (car entry))
           (define new-line (cadr entry))
           (helix.goto (number->string (+ line-idx 1)))
           (helix.static.goto_line_start)
           (helix.static.extend_to_line_bounds)
           (helix.static.delete_selection)
           (helix.static.insert_string (string-append new-line "\n")))
         (reverse indexed))
       (when commit? (helix.static.commit-changes-to-history))

       (helix.goto (number->string (+ saved-line 1)))
       (helix.static.goto_line_start)
       (let loop ([i 0])
         (when (< i saved-col)
           (helix.static.move_char_right)
           (loop (+ i 1))))

       (debug-log
         (string-append "scaffold.renumber: rewrote="
                        (number->string (length indexed))
                        " restored-line=" (number->string saved-line)
                        " restored-col=" (number->string saved-col)))
       #true]))))

;; New-notebook scaffold

;;@doc
;; Create and open a new `.jl` notebook (default `notebook.jl`) seeded with a starter template.
(define (new-notebook . args)
  (define path (if (null? args) "notebook.jl" (car args)))
  (cond
    [(string=? (path-exists path) "yes")
     (set-status! (string-append "nothelix: " path " already exists"))]
    [else
     (define template
       (string-append
         "@markdown 0\n"
         "# # New notebook\n"
         "#\n"
         "# <space>nr  run cell  |  <space>nn  new cell  |  <space>nj  jump\n"
         "# <space>ni  select code | <space>na  select all | <space>no  select output\n"
         "# ]l / [l    next/prev |  dd on marker line to delete a cell\n"
         "#\n"
         "# :new-notebook        create a notebook\n"
         "# :execute-all-cells   run everything\n"
         "# :sync-to-ipynb       save as .ipynb\n"
         "#\n"
         "# Add packages: ! julia --project=. -e 'using Pkg; Pkg.add(\"Plots\")'\n"
         "\n"
         "@cell 1 :julia\n"
         "\n"))
     (define err (write-string-to-file! path template))
     (cond
       [(> (string-length err) 0)
        (set-status! (string-append "nothelix: failed to create notebook: " err))]
       [else
        (define project-toml "Project.toml")
        (cond
          [(string=? (path-exists project-toml) "no")
           (write-string-to-file! project-toml
             (string-append
               "[deps]\n"
               "LinearAlgebra = \"37e2e46d-f89d-539d-b4ee-838fcccc9c8e\"\n"))]
          [else #true])
        (helix.open path)
        (set-status! (string-append "nothelix: created " path))])]))
