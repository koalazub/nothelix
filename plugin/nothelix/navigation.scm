;;; navigation.scm - Cell navigation commands
;;;
;;; Provides :next-cell and :previous-cell for jumping between @cell / @markdown
;;; markers in a converted notebook.

(require "common.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(provide next-cell
         previous-cell)

;;@doc
;; Jump to the next @cell or @markdown marker below the cursor.
(define (next-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (find-next line-idx)
    (cond
      [(>= line-idx total-lines) #false]
      [(cell-marker-line? rope total-lines line-idx) line-idx]
      [else (find-next (+ line-idx 1))]))

  (define target (find-next (+ current-line 1)))

  (if target
      (begin
        (helix.goto (number->string (+ target 1)))
        (set-status! (string-append "Cell at line " (number->string (+ target 1)))))
      (set-status! "No next cell")))

;;@doc
;; Jump to the previous @cell or @markdown marker above the cursor.
(define (previous-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (find-prev line-idx)
    (cond
      [(< line-idx 0) #false]
      [(cell-marker-line? rope total-lines line-idx) line-idx]
      [else (find-prev (- line-idx 1))]))

  (define target (find-prev (- current-line 1)))

  (if target
      (begin
        (helix.goto (number->string (+ target 1)))
        (set-status! (string-append "Cell at line " (number->string (+ target 1)))))
      (set-status! "No previous cell")))
