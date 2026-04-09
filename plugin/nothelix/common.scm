;;; common.scm - Shared helpers used across multiple nothelix modules
;;;
;;; Provides document accessors and cell marker predicates that are needed
;;; by navigation, execution, and selection modules. Centralised here to
;;; avoid duplication.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(provide current-line-number
         doc-get-line
         cell-marker?
         cell-marker-line?)

;;@doc
;; Return the 0-indexed line number at the cursor position.
(define (current-line-number)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (text.rope-char->line rope pos))

;;@doc
;; Get the text content of a line by index.
;; Returns an empty string if the index is out of bounds.
;; (-> rope? integer? integer? string?)
(define (doc-get-line rope total-lines line-idx)
  (if (< line-idx total-lines)
      (text.rope->string (text.rope->line rope line-idx))
      ""))

;;@doc
;; Return #true if `line-text` starts with a cell or markdown marker.
;; (-> string? boolean?)
(define (cell-marker? line-text)
  (or (string-starts-with? line-text "@cell ")
      (string-starts-with? line-text "@markdown ")))

;;@doc
;; Return #true if the line at `line-idx` in the rope is a cell marker.
;; Checks the prefix directly on the rope slice to avoid allocating a String
;; per iteration — rope-starts-with? does the check without materialising the
;; line contents.
;; (-> rope? integer? integer? boolean?)
(define (cell-marker-line? rope total-lines line-idx)
  (if (< line-idx total-lines)
      (let ([line (text.rope->line rope line-idx)])
        (or (text.rope-starts-with? line "@cell ")
            (text.rope-starts-with? line "@markdown ")))
      #false))
