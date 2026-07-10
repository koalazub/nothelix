;;; cursor-restore.scm — Cursor preservation across async buffer mutations

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))

(provide save-cursor-for-restore!
         restore-cursor-for!
         clear-cursor-restore!
         move-to-line-start-no-center!
         compute-cursor-anchor
         move-cursor-to-anchor!)

;; doc-id -> (marker-ordinal offset col): position stored relative to its
;; enclosing cell marker so output inserted below cells doesn't drift it.
(define *pending-cursor-restore* (hash))

;; 1-based ordinal of the cell marker enclosing `line` (0 if above the first marker).
(define (enclosing-marker-ordinal rope total line)
  (let loop ([i 0] [n 0])
    (cond
      [(or (> i line) (>= i total)) n]
      [(cell-marker-line? rope total i) (loop (+ i 1) (+ n 1))]
      [else (loop (+ i 1) n)])))

;; Line index of the `n`th (1-based) cell marker, or #false if there are fewer.
(define (nth-marker-line rope total n)
  (let loop ([i 0] [seen 0])
    (cond
      [(>= i total) #false]
      [(cell-marker-line? rope total i)
       (if (= (+ seen 1) n) i (loop (+ i 1) (+ seen 1)))]
      [else (loop (+ i 1) seen)])))

(define (clamp lo x hi) (max lo (min x hi)))

;;@doc
;; Move the cursor to the start of `line` without recentering the viewport.
(define (move-to-line-start-no-center! rope line)
  (define total (text.rope-len-lines rope))
  (define safe (max 0 (min line (max 0 (- total 1)))))
  (define c (text.rope-line->char rope safe))
  (helix.static.set-current-selection-object!
    (helix.static.range->selection (helix.static.range c c))))

;; Visible length of `line`, excluding the trailing newline.
(define (line-visible-length rope line)
  (let ([s (text.rope->string (text.rope->line rope line))])
    (if (string-suffix? s "\n")
        (- (string-length s) 1)
        (string-length s))))

;;@doc
;; The (ord offset col) cursor anchor for `doc-id`'s focused cursor, anchored to
;; the enclosing cell marker so output inserted below cells does not drift it.
(define (compute-cursor-anchor doc-id)
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define pos (cursor-position))
  (define line (text.rope-char->line rope pos))
  (define line-start (text.rope-line->char rope line))
  (define col (- pos line-start))
  (define ord (enclosing-marker-ordinal rope total line))
  (define offset
    (if (> ord 0)
        (let ([m (nth-marker-line rope total ord)]) (if m (- line m) line))
        line))
  (list ord offset col))

;;@doc
;; Move `doc-id`'s cursor to the (ord offset col) anchor, resolving the marker's
;; current line and clamping so a since-edited file lands nearby, not out of range.
(define (move-cursor-to-anchor! doc-id ord offset col)
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define base-line
    (if (> ord 0)
        (let ([m (nth-marker-line rope total ord)]) (if m m 0))
        0))
  (define target-line (clamp 0 (+ base-line offset) (max 0 (- total 1))))
  (define line-start (text.rope-line->char rope target-line))
  (define target-col (clamp 0 col (line-visible-length rope target-line)))
  (define char (+ line-start target-col))
  (define r (helix.static.range char char))
  (define sel (helix.static.range->selection r))
  (helix.static.set-current-selection-object! sel)
  (helix.static.collapse_selection))

;;@doc
;; Snapshot the cursor position for `doc-id`, anchored to its enclosing cell marker.
(define (save-cursor-for-restore! doc-id)
  (set! *pending-cursor-restore*
        (hash-insert *pending-cursor-restore* doc-id (compute-cursor-anchor doc-id))))

;;@doc
;; Restore `doc-id`'s saved cursor, resolving the enclosing marker's current line; leaves the entry in place for repeated restores.
(define (restore-cursor-for! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (define entry (hash-get *pending-cursor-restore* doc-id))
    (move-cursor-to-anchor! doc-id (list-ref entry 0) (list-ref entry 1) (list-ref entry 2))))

;;@doc
;; Discard the pending cursor-restore entry for `doc-id`.
(define (clear-cursor-restore! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (set! *pending-cursor-restore*
          (hash-remove *pending-cursor-restore* doc-id))))
