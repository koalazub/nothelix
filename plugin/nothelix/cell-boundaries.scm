;;; cell-boundaries.scm — Cell boundary detection and line-range manipulation

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(provide find-cell-start-line
         find-cell-code-end
         find-cell-region-end
         find-output-start
         find-output-end-line
         extract-cell-code
         line-blank?
         find-last-non-blank-line-before
         delete-line-range
         find-cell-marker-by-index
         output-image-line?)

;; Cell boundary detection

;;@doc
;; #t iff `line` belongs to a cell's output-image region: a rendered
;; `# @image ` marker, or a `# [Plot ` render-failure comment emitted by
;; update-cell-output when a plot fails to render.
(define (output-image-line? line)
  (or (string-starts-with? line "# @image ")
      (string-starts-with? line "# [Plot ")))

;;@doc
;; Find the cell start by searching backwards for a marker; 0 if none.
(define (find-cell-start-line get-line line-idx)
  (if (< line-idx 0) 0
      (let ([line (get-line line-idx)])
        (if (cell-marker? line)
            line-idx
            (find-cell-start-line get-line (- line-idx 1))))))

;;@doc
;; Find where cell code ends: the next marker, output header, output-image
;; line (`# @image ` or `# [Plot `), or EOF.
(define (find-cell-code-end get-line total-lines line-idx)
  (if (>= line-idx total-lines) total-lines
      (let ([line (get-line line-idx)])
        (if (or (cell-marker? line)
                (string-starts-with? line "# ═══")
                (string-starts-with? line "# ─── Output")
                (output-image-line? line))
            line-idx
            (find-cell-code-end get-line total-lines (+ line-idx 1))))))

;;@doc
;; Find the end of a cell's full region — code plus any stale output-image
;; lines (`# @image ` marker/blank canvas rows, or `# [Plot ` render-failure
;; comments) from a prior run — stopping at the next marker, output header,
;; or EOF. Unlike find-cell-code-end, does not stop at output-image lines.
(define (find-cell-region-end get-line total-lines line-idx)
  (if (>= line-idx total-lines) total-lines
      (let ([line (get-line line-idx)])
        (cond
          [(output-image-line? line)
           (find-cell-region-end get-line total-lines (+ line-idx 1))]
          [(string-starts-with? line "# ─── Output")
           (find-cell-region-end get-line total-lines
                                 (find-output-end-line get-line total-lines (+ line-idx 1)))]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══"))
           line-idx]
          [else (find-cell-region-end get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Find the "# ─── Output ───" header from line-idx, or #false.
(define (find-output-start get-line total-lines line-idx)
  (if (>= line-idx total-lines) #false
      (let ([line (get-line line-idx)])
        (cond
          [(string-starts-with? line "# ─── Output ───") line-idx]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) #false]
          [else (find-output-start get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Find the end of an output section (footer line or next marker).
(define (find-output-end-line get-line total-lines line-idx)
  (if (>= line-idx total-lines) line-idx
      (let ([line (get-line line-idx)])
        (cond
          [(string-starts-with? line "# ─────────────") (+ line-idx 1)]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) line-idx]
          [else (find-output-end-line get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Extract code lines from a cell, skipping markers and separator lines.
(define (extract-cell-code get-line start end)
  (let loop ([idx (+ start 1)] [acc '()])
    (if (>= idx end)
        (reverse acc)
        (let ([line (get-line idx)])
          (cond
            [(string-starts-with? line "# ═══") (loop (+ idx 1) acc)]
            [(string-starts-with? line "# ─── ") (loop (+ idx 1) acc)]
            [(string-starts-with? line "# @image ") (loop (+ idx 1) acc)]
            [(cell-marker? line) (loop (+ idx 1) acc)]
            [(string=? line "@cell") (loop (+ idx 1) acc)]
            [(string=? line "@markdown") (loop (+ idx 1) acc)]
            [else (loop (+ idx 1) (cons line acc))])))))

;; Blank line helpers

;;@doc
;; True when the rope line (possibly trailing \n) is empty or only spaces/tabs.
(define (line-blank? line)
  (define trimmed
    (if (string-suffix? line "\n")
        (substring line 0 (- (string-length line) 1))
        line))
  (define len (string-length trimmed))
  (let loop ([i 0])
    (cond
      [(>= i len) #true]
      [(or (char=? (string-ref trimmed i) #\space)
           (char=? (string-ref trimmed i) #\tab))
       (loop (+ i 1))]
      [else #false])))

;;@doc
;; Walk backward from start-1 to the first non-blank line, floored at floor-line.
(define (find-last-non-blank-line-before get-line floor-line start)
  (let loop ([i (- start 1)])
    (cond
      [(<= i floor-line) (+ floor-line 1)]
      [(line-blank? (get-line i)) (loop (- i 1))]
      [else i])))

;; Line range deletion

;;@doc
;; Delete lines from start-line (inclusive) to end-line (exclusive). Commits
;; the change as a plain (undo-visible) revision unless `commit?` is passed
;; and is #false, in which case the caller commits it.
(define (delete-line-range start-line end-line . commit?)
  (define should-commit? (if (null? commit?) #true (car commit?)))
  (when (> end-line start-line)
    (define focus (editor-focus))
    (define doc-id (editor->doc-id focus))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define doc-char-len (text.rope-len-chars rope))

    (define clamped-start (max 0 (min start-line total-lines)))
    (define clamped-end (max clamped-start (min end-line total-lines)))

    (define start-char (text.rope-line->char rope clamped-start))
    (define end-char
      (cond
        [(< clamped-end total-lines) (text.rope-line->char rope clamped-end)]
        [else doc-char-len]))

    (when (< start-char end-char)
      (define r (helix.static.range start-char end-char))
      (define sel (helix.static.range->selection r))
      (helix.static.set-current-selection-object! sel)
      (helix.static.delete_selection)
      (helix.static.collapse_selection)
      (when should-commit? (helix.static.commit-changes-to-history)))))

;; Cell marker lookup by index

;;@doc
;; Find the line of the @cell N / @markdown N marker, or #false.
(define (find-cell-marker-by-index rope total-lines cell-index)
  (define code-pattern (string-append "@cell " (number->string cell-index) " "))
  (define md-pattern (string-append "@markdown " (number->string cell-index)))

  (let loop ([line-idx 0])
    (cond
      [(>= line-idx total-lines) #false]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) code-pattern) line-idx]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) md-pattern) line-idx]
      [else (loop (+ line-idx 1))])))
