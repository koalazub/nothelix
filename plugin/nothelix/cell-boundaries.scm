;;; cell-boundaries.scm - Cell boundary detection and line-range manipulation
;;;
;;; Pure Scheme functions for locating cell markers, output sections, and
;;; code spans within a notebook buffer. No FFI dependencies — everything
;;; operates on the rope/line abstraction from common.scm.

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(provide find-cell-start-line
         find-cell-code-end
         find-output-start
         find-output-end-line
         extract-cell-code
         line-blank?
         expand-delete-start-backward
         expand-delete-end-forward
         find-last-non-blank-line-before
         delete-line-range
         find-cell-marker-by-index)

;;; ============================================================================
;;; CELL BOUNDARY DETECTION
;;; ============================================================================

;;@doc
;; Find the cell start line by searching backwards for an @cell or @markdown marker.
;; Returns 0 if no marker is found above `line-idx`.
(define (find-cell-start-line get-line line-idx)
  (if (< line-idx 0) 0
      (let ([line (get-line line-idx)])
        (if (cell-marker? line)
            line-idx
            (find-cell-start-line get-line (- line-idx 1))))))

;;@doc
;; Find where cell code ends: the next marker, output section
;; header, or EOF. Anchors the output-header match with
;; `string-starts-with?` instead of `string-contains?` so a raw
;; stdout fragment that happens to include "# --- Output" mid-line
;; can't fake a cell boundary.
(define (find-cell-code-end get-line total-lines line-idx)
  (if (>= line-idx total-lines) total-lines
      (let ([line (get-line line-idx)])
        (if (or (cell-marker? line)
                (string-starts-with? line "# ═══")
                (string-starts-with? line "# ─── Output"))
            line-idx
            (find-cell-code-end get-line total-lines (+ line-idx 1))))))

;;@doc
;; Find the "# --- Output ---" header line starting from `line-idx`.
;; Returns #false if no output section exists before the next cell
;; marker or EOF. Uses `string-starts-with?` so mid-line text that
;; happens to contain the header phrase is not mistaken for a
;; header line.
(define (find-output-start get-line total-lines line-idx)
  (if (>= line-idx total-lines) #false
      (let ([line (get-line line-idx)])
        (cond
          [(string-starts-with? line "# ─── Output ───") line-idx]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) #false]
          [else (find-output-start get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Find the end of an output section (the "# -----" footer, or next
;; marker). Anchored with `string-starts-with?` for the same reason
;; as `find-output-start`.
(define (find-output-end-line get-line total-lines line-idx)
  (if (>= line-idx total-lines) line-idx
      (let ([line (get-line line-idx)])
        (cond
          [(string-starts-with? line "# ─────────────") (+ line-idx 1)]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) line-idx]
          [else (find-output-end-line get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Extract code lines from a cell, skipping the `@cell` marker,
;; separator lines, and any stray `@cell` / `@markdown` / `@image`
;; marker lines that end up inside the cell body.
;;
;; The marker-stripping is a last line of defence: `find-cell-code-end`
;; already stops at the next cell boundary, but a bare `@cell` typed
;; by the user before the autofill hook got a chance to expand it
;; would sneak past older versions of that check and land in the
;; kernel, which then choked with `MethodError: no method matching
;; var"@cell"`. Stripping here means even if a new marker shape
;; shows up that extraction doesn't recognise, it still can't reach
;; the kernel.
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

;;; ============================================================================
;;; BLANK LINE HELPERS
;;; ============================================================================

;;@doc
;; True when the given rope line (possibly trailing `\n`) is empty or
;; contains only spaces/tabs. Used to absorb padding blanks around the
;; output block so re-runs don't let them compound.
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
;; Walk backward from `start - 1` while the line is blank; return the
;; lowest index that should still be deleted. Stops walking at
;; `floor-line` (exclusive lower bound).
(define (expand-delete-start-backward get-line floor-line start)
  (let loop ([i (- start 1)])
    (cond
      [(<= i floor-line) (+ floor-line 1)]
      [(line-blank? (get-line i)) (loop (- i 1))]
      [else (+ i 1)])))

;;@doc
;; Walk forward from `end` while the line is blank; return the lowest
;; index that should NOT be deleted (i.e. first non-blank line).
(define (expand-delete-end-forward get-line total-lines end)
  (let loop ([i end])
    (cond
      [(>= i total-lines) i]
      [(line-blank? (get-line i)) (loop (+ i 1))]
      [else i])))

;;@doc
;; Walk backward from `start - 1` looking for the first non-blank line
;; (the actual last line of code). Stops at `floor-line` so we never
;; cross past the cell header.
(define (find-last-non-blank-line-before get-line floor-line start)
  (let loop ([i (- start 1)])
    (cond
      [(<= i floor-line) (+ floor-line 1)]
      [(line-blank? (get-line i)) (loop (- i 1))]
      [else i])))

;;; ============================================================================
;;; LINE RANGE DELETION
;;; ============================================================================

;;@doc
;; Delete lines from `start-line` to `end-line` (start inclusive, end exclusive).
;;
;; Uses a single range-based selection and a single delete_selection call,
;; so the Helix command count is O(1) regardless of the number of lines.
;; Previous implementation did `goto + extend + delete` per line, which
;; dominated the cost of re-executing a cell with a large output section.
(define (delete-line-range start-line end-line)
  (when (> end-line start-line)
    (define focus (editor-focus))
    (define doc-id (editor->doc-id focus))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define doc-char-len (text.rope-len-chars rope))

    ;; Clamp to valid bounds so we never ask for a line past EOF.
    (define clamped-start (max 0 (min start-line total-lines)))
    (define clamped-end (max clamped-start (min end-line total-lines)))

    ;; Convert line numbers to char offsets. Leading position is start of
    ;; `start-line`; trailing position is start of `end-line` (which is the
    ;; char after the last newline of line `end-line - 1`). If `end-line`
    ;; is past EOF, snap to the end of the rope.
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
      (helix.static.commit-changes-to-history))))

;;; ============================================================================
;;; CELL MARKER LOOKUP BY INDEX
;;; ============================================================================

;;@doc
;; Find the line number of a cell marker with given index in a converted file.
;; Returns the line number of the "@cell N ..." or "@markdown N" marker, or
;; #false if not found.
(define (find-cell-marker-by-index rope total-lines cell-index)
  (define code-pattern (string-append "@cell " (number->string cell-index) " "))
  (define md-pattern (string-append "@markdown " (number->string cell-index)))

  (let loop ([line-idx 0])
    (cond
      [(>= line-idx total-lines) #false]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) code-pattern) line-idx]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) md-pattern) line-idx]
      [else (loop (+ line-idx 1))])))
