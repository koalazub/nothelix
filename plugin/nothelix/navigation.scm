;;; navigation.scm - Cell navigation commands

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For cursor-position, set-status!
(require-builtin helix/core/text as text.)
(require (prefix-in helix. "helix/commands.scm"))

(provide next-cell
         previous-cell)

;; Helper: Get current line number (0-indexed)
(define (current-line-number)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (text.rope-char->line rope pos))

;;@doc
;; Jump to next cell in the notebook
(define (next-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-starts-with? line "@cell ")
          (string-starts-with? line "@markdown "))))

  (define (find-next-cell line-idx)
    (cond
      [(>= line-idx total-lines) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-next-cell (+ line-idx 1))]))

  (define next-cell-line (find-next-cell (+ current-line 1)))

  (if next-cell-line
      (begin
        (helix.goto (number->string (+ next-cell-line 1)))
        (set-status! (string-append "Cell at line " (number->string (+ next-cell-line 1)))))
      (set-status! "No next cell")))

;;@doc
;; Jump to previous cell in the notebook
(define (previous-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-starts-with? line "@cell ")
          (string-starts-with? line "@markdown "))))

  (define (find-prev-cell line-idx)
    (cond
      [(< line-idx 0) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-prev-cell (- line-idx 1))]))

  (define prev-cell-line (find-prev-cell (- current-line 1)))

  (if prev-cell-line
      (begin
        (helix.goto (number->string (+ prev-cell-line 1)))
        (set-status! (string-append "Cell at line " (number->string (+ prev-cell-line 1)))))
      (set-status! "No previous cell")))
