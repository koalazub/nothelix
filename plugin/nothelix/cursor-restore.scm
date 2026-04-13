;;; cursor-restore.scm - Cursor preservation across async buffer mutations
;;;
;;; Stashes and restores (line, col) pairs keyed by doc-id so that
;;; execute-cell and execute-all-cells can mutate the buffer (insert
;;; output headers, result bodies, footers, image padding) without
;;; leaving the cursor parked at the bottom of the output block.
;;; Concurrent executions on different documents don't clobber each
;;; other because each save is keyed by its own doc-id.

(require "common.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(provide save-cursor-for-restore!
         restore-cursor-for!
         clear-cursor-restore!)

;;; ============================================================================
;;; CURSOR SAVE / RESTORE
;;; ============================================================================

;; Hash of (doc-id -> (line . col)) entries. Each execute-cell saves
;; the user's position before any buffer mutation; update-cell-output
;; restores it once all inserts are done.
(define *pending-cursor-restore* (hash))

;;@doc
;; Snapshot the current cursor position for `doc-id`. Called before any
;; buffer mutation so the captured position is the user's true cursor,
;; not a mid-insert intermediate.
(define (save-cursor-for-restore! doc-id)
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (define line (text.rope-char->line rope pos))
  (define line-start (text.rope-line->char rope line))
  (define col (- pos line-start))
  (set! *pending-cursor-restore*
        (hash-insert *pending-cursor-restore* doc-id (cons line col))))

;;@doc
;; Move the cursor back to the position saved by `save-cursor-for-restore!`
;; for `doc-id`. Does nothing if no entry exists. Deliberately does NOT
;; clear the entry so successive cells in `execute-all-cells` each pull
;; back to the same saved position.
(define (restore-cursor-for! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (define entry (hash-get *pending-cursor-restore* doc-id))
    (define target-line (car entry))
    (define target-col (cdr entry))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define safe-line
      (cond
        [(< target-line 0) 0]
        [(>= target-line total-lines) (- total-lines 1)]
        [else target-line]))
    ;; `helix.goto` is 1-indexed.
    (helix.goto (number->string (+ safe-line 1)))
    (helix.static.goto_line_start)
    (let loop ([i 0])
      (when (< i target-col)
        (helix.static.move_char_right)
        (loop (+ i 1))))))

;;@doc
;; Discard the pending cursor-restore entry for `doc-id`. Called by
;; `execute-cell-list` after the whole run finishes so the stash
;; doesn't leak across unrelated executions.
(define (clear-cursor-restore! doc-id)
  (when (hash-contains? *pending-cursor-restore* doc-id)
    (set! *pending-cursor-restore*
          (hash-remove *pending-cursor-restore* doc-id))))
