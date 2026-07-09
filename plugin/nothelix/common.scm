;;; common.scm — Shared document accessors and cell-marker predicates

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(provide current-line-number
         doc-get-line
         cell-marker?
         cell-marker-line?
         *plot-rows*
         *plot-max-rows*
         *plot-cols*)

;; Terminal cell grid for inline plots. Override in init.scm via set!.
(define *plot-rows* 12)
(define *plot-max-rows* 60)
(define *plot-cols* 40)

;;@doc
;; Return the 0-indexed line number at the cursor position.
(define (current-line-number)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (text.rope-char->line rope pos))

;;@doc
;; Get the text content of a line by index, or "" if out of bounds.
(define (doc-get-line rope total-lines line-idx)
  (if (< line-idx total-lines)
      (text.rope->string (text.rope->line rope line-idx))
      ""))

;;@doc
;; Return #true if line-text is (or starts with) an @cell / @markdown / @raw / @typst marker.
(define (cell-marker? line-text)
  (or (string=? line-text "@cell")
      (string=? line-text "@cell\n")
      (string=? line-text "@markdown")
      (string=? line-text "@markdown\n")
      (string=? line-text "@raw")
      (string=? line-text "@raw\n")
      (string=? line-text "@typst")
      (string=? line-text "@typst\n")
      (string-starts-with? line-text "@cell ")
      (string-starts-with? line-text "@markdown ")
      (string-starts-with? line-text "@raw ")
      (string-starts-with? line-text "@typst ")))

;;@doc
;; Return #true if the line at line-idx in the rope is a cell marker.
(define (cell-marker-line? rope total-lines line-idx)
  (if (< line-idx total-lines)
      (let ([line (text.rope->line rope line-idx)])
        (or (text.rope-starts-with? line "@cell ")
            (text.rope-starts-with? line "@markdown ")
            (text.rope-starts-with? line "@raw ")
            (text.rope-starts-with? line "@typst ")
            (let ([s (text.rope->string line)])
              (or (string=? s "@cell")
                  (string=? s "@cell\n")
                  (string=? s "@markdown")
                  (string=? s "@markdown\n")
                  (string=? s "@raw")
                  (string=? s "@raw\n")
                  (string=? s "@typst")
                  (string=? s "@typst\n")))))
      #false))
