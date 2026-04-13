;;; selection.scm - Cell and output selection text objects
;;;
;;; Provides text-object-style selections for cells: select the whole cell,
;;; just the code portion, or just the output section.

(require "common.scm")
(require "cell-boundaries.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(provide select-cell
         select-cell-code
         select-output)

;;@doc
;; Find the full cell end including any output section.
;; If an output section follows the code, returns the line after its footer;
;; otherwise returns `code-end`.
(define (find-full-cell-end get-line total-lines code-end)
  (define output-start (find-output-start get-line total-lines code-end))
  (if output-start
      (find-output-end-line get-line total-lines (+ output-start 1))
      code-end))

;;@doc
;; Select a range of lines in the document (0-indexed, end exclusive).
(define (select-line-range start-line end-line)
  (helix.goto (number->string (+ start-line 1)))
  (helix.static.goto_line_start)
  (helix.static.extend_to_line_bounds)
  (let ([lines-to-extend (- end-line start-line 1)])
    (when (> lines-to-extend 0)
      (let loop ([i 0])
        (when (< i lines-to-extend)
          (helix.static.extend_line_below)
          (loop (+ i 1)))))))

;;@doc
;; Select the entire current cell (header + code + output).
(define (select-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define cell-end (find-full-cell-end get-line total-lines cell-code-end))

  (select-line-range cell-start cell-end)
  (set-status! (string-append "Selected cell: lines "
                              (number->string (+ cell-start 1))
                              "-"
                              (number->string cell-end))))

;;@doc
;; Select just the code portion of the current cell (excluding header and output).
(define (select-cell-code)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define code-start (+ cell-start 1))

  (if (< code-start cell-code-end)
      (begin
        (select-line-range code-start cell-code-end)
        (set-status! (string-append "Selected code: lines "
                                    (number->string (+ code-start 1))
                                    "-"
                                    (number->string cell-code-end))))
      (set-status! "Cell has no code")))

;;@doc
;; Select the output section of the current cell.
(define (select-output)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define output-start (find-output-start get-line total-lines cell-code-end))

  (if output-start
      (let ([output-end (find-output-end-line get-line total-lines (+ output-start 1))])
        (select-line-range output-start output-end)
        (set-status! (string-append "Selected output: lines "
                                    (number->string (+ output-start 1))
                                    "-"
                                    (number->string output-end))))
      (set-status! "No output section found")))
