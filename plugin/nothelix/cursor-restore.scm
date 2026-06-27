;;; cursor-restore.scm - Cursor preservation across async buffer mutations
;;;
;;; Stashes and restores (line, col) pairs keyed by doc-id so that
;;; execute-cell and execute-all-cells can mutate the buffer (insert
;;; output headers, result bodies, footers, image padding) without
;;; leaving the cursor parked at the bottom of the output block.
;;; Concurrent executions on different documents don't clobber each
;;; other because each save is keyed by its own doc-id.

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))

(provide save-cursor-for-restore!
         restore-cursor-for!
         clear-cursor-restore!
         move-to-line-start-no-center!)

;;; ============================================================================
;;; CURSOR SAVE / RESTORE
;;; ============================================================================

;; Hash of (doc-id -> (marker-ordinal offset col)) entries. The position is
;; stored RELATIVE to the cursor's enclosing cell marker (the Nth `@cell` /
;; `@markdown` line at or above it) rather than as an absolute line, because
;; execute-all-cells inserts output below each cell and shifts every absolute
;; line below it. The enclosing marker's ordinal and the cursor's offset within
;; the cell are invariant under those inserts, so the cursor lands where the
;; user actually was instead of drifting up into an earlier output block.
(define *pending-cursor-restore* (hash))

;; Count the cell markers at indices 0..line inclusive — the 1-based ordinal of
;; the cell the cursor sits in (0 when the cursor is above the first marker).
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
;; Move the cursor to the start of `line` WITHOUT recentering the viewport —
;; `helix.goto` aligns the view to centre, which is what makes the page lurch
;; while output is inserted cell-by-cell. A plain selection set only scrolls if
;; the target is off-screen, so on-screen insert points stay visually put.
(define (move-to-line-start-no-center! rope line)
  (define total (text.rope-len-lines rope))
  (define safe (max 0 (min line (max 0 (- total 1)))))
  (define c (text.rope-line->char rope safe))
  (helix.static.set-current-selection-object!
    (helix.static.range->selection (helix.static.range c c))))

;; Visible length of a line (excluding the trailing newline), for col clamping.
(define (line-visible-length rope line)
  (let ([s (text.rope->string (text.rope->line rope line))])
    (if (string-suffix? s "\n")
        (- (string-length s) 1)
        (string-length s))))

;;@doc
;; Snapshot the current cursor position for `doc-id`, anchored to its enclosing
;; cell marker. Called before any buffer mutation so the captured position is
;; the user's true cursor, not a mid-insert intermediate.
(define (save-cursor-for-restore! doc-id)
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
  (set! *pending-cursor-restore*
        (hash-insert *pending-cursor-restore* doc-id (list ord offset col))))

;;@doc
;; Move the cursor back to the position saved by `save-cursor-for-restore!`
;; for `doc-id`, resolving the enclosing marker's CURRENT line so output
;; inserted above doesn't misplace it. Uses a direct selection set rather than
;; `helix.goto`, which recenters the viewport (the "page jumping" the user
;; sees); a plain selection only scrolls if the target is off-screen. Does
;; nothing if no entry exists; deliberately does NOT clear the entry so
;; successive cells in `execute-all-cells` each pull back to the same spot.
(define (restore-cursor-for! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (define entry (hash-get *pending-cursor-restore* doc-id))
    (define ord (list-ref entry 0))
    (define offset (list-ref entry 1))
    (define col (list-ref entry 2))
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
    (helix.static.collapse_selection)))

;;@doc
;; Discard the pending cursor-restore entry for `doc-id`. Called by
;; `execute-cell-list` after the whole run finishes so the stash
;; doesn't leak across unrelated executions.
(define (clear-cursor-restore! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (set! *pending-cursor-restore*
          (hash-remove *pending-cursor-restore* doc-id))))
