;;; output-render.scm — Deferred wrappers over the fork's output-lines
;;; virtual-line annotation, so the plugin loads on an hx without it.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/static as helix.static.)

(provide try-set-output-lines-below!
         try-clear-output-lines-at!
         try-clear-all-output-lines!
         output-lines-ffi-available?
         try-commit-output-changes!
         ansi-color->scope
         text-plot->styled-rows
         series-scope
         assign-cycling-bars)

(define (try-set-output-lines-below! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.set-output-lines-below! *helix.cx* ,line-idx ',lines)))
    #true))

(define (try-clear-output-lines-at! line-idx)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.clear-output-lines-at! *helix.cx* ,line-idx)))
    #true))

(define (try-clear-all-output-lines!)
  (with-handler
    (lambda (_) #false)
    (eval '(begin (require-builtin helix/core/static as hs.)
                  (hs.clear-all-output-lines! *helix.cx*)))
    #true))

(define (output-lines-ffi-available?)
  (with-handler
    (lambda (_) #false)
    (eval '(begin (require-builtin helix/core/static as hs.)
                  hs.set-output-lines-below!))
    #true))

;;@doc
;; Commit pending buffer changes as a tagged `output` revision (skipped by
;; user undo/redo) when the fork binding exists; falls back to a plain
;; commit on an hx without it, so behavior matches today exactly.
(define (try-commit-output-changes!)
  (with-handler
    (lambda (_) (helix.static.commit-changes-to-history *helix.cx*))
    (eval '(begin (require-builtin helix/core/static as hs.)
                  (hs.commit-output-changes-to-history! *helix.cx*)))
    #true))

(define *text-plot-series-scopes*
  (vector "ui.virtual.output.series0" "ui.virtual.output.series1"
          "ui.virtual.output.series2" "ui.virtual.output.series3"
          "ui.virtual.output.series4" "ui.virtual.output.series5"
          "ui.virtual.output.series6" "ui.virtual.output.series7"))

;;@doc
;; ANSI palette index (0-15) -> a themable series scope name, or #false for
;; an out-of-range index (rendered with the decoration's default style).
;; Bright variants (8-15) reuse their base color's scope: 8+n -> series<n>.
(define (ansi-color->scope idx)
  (cond
    [(not (exact-integer? idx)) #false]
    [(and (>= idx 0) (<= idx 7)) (vector-ref *text-plot-series-scopes* idx)]
    [(and (>= idx 8) (<= idx 15)) (vector-ref *text-plot-series-scopes* (- idx 8))]
    [else #false]))

(define *series-scope-count* 8)

;;@doc
;; The bar/series theme scope for cycle index `i`, wrapping every
;; `*series-scope-count*` (series0 .. series7 .. series0 ...).
(define (series-scope i)
  (vector-ref *text-plot-series-scopes* (modulo i *series-scope-count*)))

;;@doc
;; Wrap one output row (a plain string or a list of `(text scope)` span pairs)
;; as a bar-tagged row for `set-output-lines-below!`: `("bar" bar-scope row)`.
;; The fork disambiguates on the leading marker string; an untagged row keeps
;; rendering with no gutter bar.
(define (bar-row bar-scope row)
  (list "bar" bar-scope row))

;;@doc
;; Flatten a list of output groups (each a list of rows) into one row list,
;; tagging every row in group N with series-scope N so distinct outputs get a
;; cycling gutter-bar color. Empty groups are dropped and do not consume a
;; color index, so a matrix (one multi-row group) is one bar color and three
;; separate outputs get series0/series1/series2.
(define (assign-cycling-bars groups)
  (let loop ([gs (filter (lambda (g) (not (null? g))) groups)] [i 0] [acc '()])
    (if (null? gs)
        (apply append (reverse acc))
        (let ([scope (series-scope i)])
          (loop (cdr gs) (+ i 1)
                (cons (map (lambda (r) (bar-row scope r)) (car gs)) acc))))))

(define (tp-span-row sp) (list-ref sp 0))
(define (tp-span-start sp) (list-ref sp 1))
(define (tp-span-end sp) (list-ref sp 2))
(define (tp-span-color sp) (list-ref sp 3))

;;@doc
;; Segment `row-text` into `(text scope-or-#false)` spans using
;; `row-spans` — spans already filtered to this row, and assumed sorted by
;; `start` ascending (true of the kernel's left-to-right ANSI scan; see
;; `kernel/output_capture.jl`'s `parse_ansi_rows`, which appends spans in
;; scan order). A span that is zero/negative-width, out of `row-text`'s
;; bounds, or overlaps a position already emitted by an earlier span is
;; skipped defensively rather than raising or corrupting layout.
(define (text-plot-segment-row row-text row-spans)
  (define row-len (string-length row-text))
  (define (clamp v) (max 0 (min v row-len)))
  (let loop ([spans row-spans] [pos 0] [acc '()])
    (cond
      [(null? spans)
       (reverse (if (< pos row-len)
                    (cons (list (substring row-text pos row-len) #false) acc)
                    acc))]
      [else
       (define sp (car spans))
       (define start (clamp (tp-span-start sp)))
       (define end (clamp (tp-span-end sp)))
       (cond
         [(or (<= end start) (< start pos))
          (loop (cdr spans) pos acc)]
         [else
          (define with-gap
            (if (> start pos)
                (cons (list (substring row-text pos start) #false) acc)
                acc))
          (define with-span
            (cons (list (substring row-text start end)
                        (ansi-color->scope (tp-span-color sp)))
                  with-gap))
          (loop (cdr spans) end with-span)])])))

;;@doc
;; Build styled rows (for `set-output-lines-below!`, via
;; `try-set-output-lines-below!`) from a text-plot's `rows` (list of
;; plain-text lines, ANSI already stripped) and `spans` (list of `(row
;; start end color)`, 0-based half-open char offsets into the stripped
;; row, color an ANSI palette index — matching `parse_ansi_rows`'s output
;; shape exactly). A row with no matching spans is returned as its plain
;; string (identical to a monochrome output row, so plain rows round-trip
;; through the fork's existing string-row path unchanged); a row with
;; spans becomes a list of `(text scope-or-#false)` pairs, gaps tagged
;; #false so they paint with the decoration's default style.
(define (text-plot->styled-rows rows spans)
  (let build ([rs rows] [row-idx 0] [acc '()])
    (if (null? rs)
        (reverse acc)
        (let* ([row-text (car rs)]
               [row-spans (filter (lambda (sp) (= (tp-span-row sp) row-idx)) spans)])
          (build (cdr rs) (+ row-idx 1)
                 (cons (if (null? row-spans)
                           row-text
                           (text-plot-segment-row row-text row-spans))
                       acc))))))
