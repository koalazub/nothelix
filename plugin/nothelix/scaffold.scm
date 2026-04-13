;;; scaffold.scm - Cell marker autofill, renumbering, and notebook scaffolding
;;;
;;; Goal: a non-programmer opens a new `.jl` file and just starts
;;; typing. They write `@cell` followed by a space — the plugin fills
;;; in the cell index and language. Same for `@md`, `@mark`,
;;; `@markdown`. No manual numbering, no marker syntax to memorise;
;;; cell indices get tidied up when the buffer is saved.
;;;
;;; Two expansion paths:
;;;
;;;   * **Direct expansion** for unambiguous markdown aliases
;;;     (`@md`, `@mark`, `@markdown`). Typing any of these followed
;;;     by a space rewrites the line in place to the full
;;;     `@markdown N` marker plus a heading prefix `# ` on the next
;;;     line.
;;;
;;;   * **Picker** for `@cell` and any other `@<word>` at the start
;;;     of a line (option B from the design doc — forgiving of typos
;;;     and unknown words). A small popup asks "Code cell (julia)"
;;;     or "Markdown cell"; the chosen marker is inserted in place.
;;;
;;; Aliases are checked before the picker opens, so users who have
;;; learned the shortcuts pay zero UI cost.
;;;
;;; Scaffold file creation (`new-notebook`) and renumber-on-save
;;; (`renumber-cells!`) live here too because they share the marker
;;; parsing helpers.

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
                          write-string-to-file
                          path-exists))

(provide notebook-file?
         file-lang
         next-cell-index
         maybe-expand-cell-marker!
         renumber-cells!
         new-notebook
         open-cell-type-picker)

;; ─── File-type predicates ─────────────────────────────────────────────────────

;;@doc
;; `#true` when `path` looks like a nothelix-managed notebook source.
;; We only autofill / renumber in these files so editing a plain `.jl`
;; script that just happens to contain `@cell` somewhere (in a comment,
;; a string literal, etc.) isn't accidentally rewritten.
(define (notebook-file? path)
  (and path
       (or (string-suffix? path ".jl")
           (string-suffix? path ".py")
           (string-suffix? path ".ipynb"))))

;;@doc
;; Map a notebook file's extension to the language annotation that
;; goes into the `@cell N :LANG` marker. Defaults to `julia` when in
;; doubt — everything in nothelix is Julia-first today.
(define (file-lang path)
  (cond
    [(and path (string-suffix? path ".py")) "python"]
    [else "julia"]))

;; ─── Marker index parsing ─────────────────────────────────────────────────────

;;@doc
;; Parse the leading decimal integer from a string. Returns `-1` when
;; the string doesn't start with a digit — used as a sentinel by
;; callers that want to fold with `max`.
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
;; Scan the buffer for the highest `N` in any `@cell N` or
;; `@markdown N` marker. Returns `N + 1` so the caller can use it as
;; the next free index. Returns `0` when the buffer has no markers
;; yet (fresh notebook). Holes in the sequence are fine — we just
;; take the max.
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
            [(string-starts-with? line "@typst ")
             (loop (+ line-idx 1) (max max-idx (scan-after-prefix line "@typst ")))]
            [else (loop (+ line-idx 1) max-idx)])))))

;; ─── Line helpers ─────────────────────────────────────────────────────────────

;;@doc
;; Return the text of the line the cursor is on, without its trailing
;; newline. Used by the expansion hook to check whether the user just
;; typed a complete `@<word> ` on an otherwise empty line.
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
;; Does `str` look like `@<word> ` — an `@` sign, at least one
;; identifier character, and then exactly a trailing space, with no
;; other content? This is the shape of a freshly typed-in marker
;; prefix waiting to be expanded.
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
;; Extract the word between the `@` and the trailing space. Assumes
;; `cell-marker-prefix?` already returned `#true` for the input.
(define (cell-marker-word str)
  (substring str 1 (- (string-length str) 1)))

;; ─── Line replacement ─────────────────────────────────────────────────────────

;;@doc
;; Replace the entire current line (including its trailing newline)
;; with `replacement`. Used by the direct-expansion path and the
;; picker's selection callback. Caller is responsible for positioning
;; the cursor on the target line first via `helix.goto`.
(define (replace-current-line-with! replacement)
  (helix.static.goto_line_start)
  (helix.static.extend_to_line_bounds)
  (helix.static.delete_selection)
  (helix.static.insert_string replacement))

;; ─── Expansion ────────────────────────────────────────────────────────────────

;;@doc
;; Insert a code cell marker at the current line. Called from either
;; the direct-expansion path or the picker. `next-idx` is the index
;; to stamp on the marker; `lang` is the language annotation.
(define (expand-code-marker! next-idx lang)
  (replace-current-line-with!
    (string-append "@cell " (number->string next-idx) " :" lang "\n\n"))
  ;; After inserting `@cell N :lang\n\n`, cursor sits at the start of
  ;; the line after the blank line. Move it back up one so the user
  ;; lands on the empty line immediately below the marker, ready to
  ;; type code.
  (helix.static.move_line_up))

;;@doc
;; Insert a markdown cell marker at the current line. The heading
;; prefix `# ` is seeded on the next line with the cursor parked
;; right after the space, so the user immediately starts typing the
;; heading text.
(define (expand-markdown-marker! next-idx)
  (replace-current-line-with!
    (string-append "@markdown " (number->string next-idx) "\n# "))
  #true)

;;@doc
;; Expand a `@typst` marker. Same comment-prefix style as markdown.
(define (expand-typst-marker! next-idx)
  (replace-current-line-with!
    (string-append "@typst " (number->string next-idx) "\n# "))
  #true)

;;@doc
;; Called from the `post-insert-char` hook every time the user types
;; a space. If the current line is exactly `@<word> ` in a notebook
;; file, route it to the direct expander (for unambiguous markdown
;; aliases) or to the cell-type picker (for `@cell` and any unknown
;; `@<word>`).
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
        ;; Markdown aliases expand directly, no picker.
        ;; Everything else (@cell, @foo, …) opens the picker so the
        ;; user can pick code or markdown.
        (cond
          [(or (string=? word "md")
               (string=? word "mark")
               (string=? word "markdown"))
           (expand-markdown-marker! next-idx)]
          [(string=? word "typst")
           (expand-typst-marker! next-idx)]
          [else
           (open-cell-type-picker line-idx next-idx (file-lang path))])))))

;; ─── Cell-type picker ─────────────────────────────────────────────────────────

;; A minimal popup with two items: code or markdown. Driven by
;; arrow/j/k/Enter/Esc just like the cell-picker. The state struct
;; captures the line index and index-to-stamp so the picker can do
;; the insertion on its own without needing global state.
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
         ;; Pull styles from the active Helix theme via the raw
         ;; built-in `theme-scope` (takes the Context as its first
         ;; arg). Using `(style)` — the empty style — would render
         ;; as a black block regardless of the user's colourscheme,
         ;; which is what made the `<space>nn` popup look
         ;; permanently dark even in light themes.
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
  ;; Navigate back to the line the user typed `@<word> ` on, delete
  ;; it, and stamp the expanded marker. `helix.goto` is 1-indexed;
  ;; our stored line index is 0-indexed, so add 1.
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
      [(or (eqv? char #\j) (eqv? char #\n))
       (when (< (CellTypePickerState-selected state) (- (length items) 1))
         (set-CellTypePickerState-selected! state
           (+ (CellTypePickerState-selected state) 1)))
       event-result/consume]
      [(or (eqv? char #\k) (eqv? char #\p))
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
;; Push a new cell-type picker component onto the component stack.
;; Called from `maybe-expand-cell-marker!` when the user types
;; `@cell ` or any other non-markdown `@<word> ` on an otherwise
;; empty line.
(define (open-cell-type-picker line-idx next-idx lang)
  (push-component! (make-cell-type-picker-component line-idx next-idx lang)))

;; ─── Renumber cells ───────────────────────────────────────────────────────────

;;@doc
;; Walk the buffer top-to-bottom and rewrite every `@cell N …` and
;; `@markdown N` marker so the `N`s form a contiguous 0-indexed
;; sequence. Called automatically after file saves (`:write` and
;; friends) and `:sync-to-ipynb`, and also exposed as an explicit
;; `:renumber-cells` command.
;;
;; Saves and restores the cursor's (line, column) before/after the
;; edit pass so `:write` / `:fmt` can't fling the cursor back to the
;; top of the file. Also short-circuits when every marker is already
;; at its correct index so repeated saves on an unchanged buffer
;; don't churn any transactions at all.
;;
;; Iterates markers in REVERSE line order so each in-place line
;; replacement doesn't shift the positions of later markers.
(define (renumber-cells!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (notebook-file? path)
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))

    ;; Snapshot the cursor (line + column) up front so we can put it
    ;; back after the rewrites land. Without this the user's cursor
    ;; ends up at the start of whichever marker line was rewritten
    ;; last — typically near the top of the file for a save over a
    ;; many-cell notebook, which feels like the editor randomly
    ;; scrolling to top on every `:w`.
    (define saved-char (cursor-position))
    (define saved-line (text.rope-char->line rope saved-char))
    (define saved-line-start (text.rope-line->char rope saved-line))
    (define saved-col (- saved-char saved-line-start))

    ;; First pass: collect (line-idx, kind, rest-of-line) triples
    ;; in forward order so we can compute the final 0-indexed sequence.
    (define markers
      (let loop ([line-idx 0] [acc '()])
        (if (>= line-idx total-lines)
            (reverse acc)
            (let ([line (doc-get-line rope total-lines line-idx)])
              (cond
                [(string-starts-with? line "@cell ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx 'code line) acc))]
                [(string-starts-with? line "@markdown ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx 'md line) acc))]
                [(string-starts-with? line "@typst ")
                 (loop (+ line-idx 1)
                       (cons (list line-idx 'typst line) acc))]
                [else (loop (+ line-idx 1) acc)])))))
    ;; Compute the new content for each marker with its target index
    ;; in forward order, then reverse to apply back-to-front.
    (define indexed
      (let loop ([ms markers] [i 0] [acc '()])
        (if (null? ms)
            (reverse acc)
            (let* ([m (car ms)]
                   [line-idx (car m)]
                   [kind (cadr m)]
                   [current (caddr m)]
                   [current-trimmed
                    (if (string-suffix? current "\n")
                        (substring current 0 (- (string-length current) 1))
                        current)]
                   [new-line
                    (case kind
                      [(code)
                       ;; Preserve everything after the index (e.g.
                       ;; the ` :julia` suffix, any trailing comments).
                       ;; Parse "@cell OLD REST" → "@cell NEW REST".
                       (define after-prefix
                         (substring current
                                    (string-length "@cell ")
                                    (string-length current)))
                       (define trimmed
                         (if (string-suffix? after-prefix "\n")
                             (substring after-prefix 0
                                        (- (string-length after-prefix) 1))
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
                       (string-append "@cell " (number->string i) rest-after-digits)]
                      [(md)
                       ;; Preserve any trailing label after the index.
                       (define md-after
                         (substring current
                                    (string-length "@markdown ")
                                    (string-length current)))
                       (define md-trimmed
                         (if (string-suffix? md-after "\n")
                             (substring md-after 0
                                        (- (string-length md-after) 1))
                             md-after))
                       (define md-rest
                         (let scan ([j 0])
                           (cond
                             [(>= j (string-length md-trimmed))
                              (substring md-trimmed j (string-length md-trimmed))]
                             [(let ([c (string-ref md-trimmed j)])
                                (and (char>=? c #\0) (char<=? c #\9)))
                              (scan (+ j 1))]
                             [else
                              (substring md-trimmed j (string-length md-trimmed))])))
                       (string-append "@markdown " (number->string i) md-rest)]
                      [(typst)
                       (define ty-after
                         (substring current
                                    (string-length "@typst ")
                                    (string-length current)))
                       (define ty-trimmed
                         (if (string-suffix? ty-after "\n")
                             (substring ty-after 0 (- (string-length ty-after) 1))
                             ty-after))
                       (define ty-rest
                         (let scan ([j 0])
                           (cond
                             [(>= j (string-length ty-trimmed))
                              (substring ty-trimmed j (string-length ty-trimmed))]
                             [(let ([c (string-ref ty-trimmed j)])
                                (and (char>=? c #\0) (char<=? c #\9)))
                              (scan (+ j 1))]
                             [else
                              (substring ty-trimmed j (string-length ty-trimmed))])))
                       (string-append "@typst " (number->string i) ty-rest)])])
              (loop (cdr ms) (+ i 1)
                    ;; Only queue the line for rewrite if the new
                    ;; content actually differs from what's there —
                    ;; skip no-ops so repeated saves don't touch the
                    ;; buffer.
                    (if (string=? new-line current-trimmed)
                        acc
                        (cons (list line-idx new-line) acc)))))))

    (cond
      [(null? indexed)
       ;; Nothing to do — every marker is already at its correct
       ;; index. Don't touch the buffer, don't move the cursor.
       (debug-log "scaffold.renumber: nothing to renumber")]
      [else
       ;; Apply the rewrites in reverse line order so earlier edits
       ;; don't shift later line indices.
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
       (helix.static.commit-changes-to-history)

       ;; Restore the cursor to where it was before we started. The
       ;; line number is still valid because we only rewrote marker
       ;; lines in place (no line insertions or deletions), and
       ;; columns within non-marker lines are untouched.
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
                        " restored-col=" (number->string saved-col)))])))

;; ─── New-notebook scaffold ────────────────────────────────────────────────────

;;@doc
;; Create a brand-new .jl notebook file and open it. Seeds a tiny
;; template (one markdown heading + one empty code cell) that the
;; autofill flow takes over from. With no argument, creates
;; `notebook.jl` in the current working directory.
(define (new-notebook . args)
  (define path (if (null? args) "notebook.jl" (car args)))
  (cond
    [(string=? (path-exists path) "yes")
     (set-status! (string-append "nothelix: " path " already exists"))]
    [else
     (define template
       (string-append
         "using NothelixMacros\n"
         "\n"
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
         "\n"
         "@cell 1 :julia\n"
         "\n"))
     (define err (write-string-to-file path template))
     (cond
       [(> (string-length err) 0)
        (set-status! (string-append "nothelix: failed to create notebook: " err))]
       [else
        ;; Generate a Julia Project.toml alongside the notebook if one
        ;; doesn't exist in the directory. The LSP resolves packages
        ;; from this env, so without it every `using` shows as
        ;; "Missing reference". Seed with CellMarkers + LinearAlgebra
        ;; as sensible defaults.
        (define project-toml "Project.toml")
        (when (string=? (path-exists project-toml) "no")
          (write-string-to-file project-toml
            (string-append
              "[deps]\n"
              "CellMarkers = \"019d8495-069e-72c6-9285-251bb2f95da1\"\n"
              "LinearAlgebra = \"37e2e46d-f89d-539d-b4ee-838fcccc9c8e\"\n")))
        (helix.open path)
        (set-status! (string-append "nothelix: created " path))])]))
