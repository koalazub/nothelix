#!/usr/bin/env steel

;; Cell Navigation Test
;; Run with: :scm (require "plugins/tests/navigation-test.scm")

(require "plugins/nothelix.scm")

(displayln "")
(displayln "=== CELL NAVIGATION TEST ===")
(displayln "")

(displayln "This test requires a converted .jl file to be open.")
(displayln "Expected cell markers like: @cell 0, @cell 1, @markdown 2, etc.")
(displayln "")

(define focus (editor-focus))
(define doc-id (editor->doc-id focus))
(define path (editor-document->path doc-id))
(define rope (editor->text doc-id))
(define total-lines (text.rope-len-lines rope))

(displayln (string-append "File: " (if path path "NO PATH")))
(displayln (string-append "Total lines: " (number->string total-lines)))
(displayln "")

(displayln "Scanning for cell markers...")
(define (get-line idx)
  (if (< idx total-lines)
      (text.rope->string (text.rope->line rope idx))
      ""))

(define cell-count 0)
(define (scan-cells idx)
  (when (< idx total-lines)
    (define line (get-line idx))
    (when (or (string-starts-with? line "@cell ")
              (string-starts-with? line "@markdown "))
      (set! cell-count (+ cell-count 1))
      (displayln (string-append "  Line " (number->string (+ idx 1)) ": " line)))
    (scan-cells (+ idx 1))))

(scan-cells 0)

(displayln "")
(displayln (string-append "Found " (number->string cell-count) " cells"))
(displayln "")

(if (> cell-count 0)
    (begin
      (displayln "Testing next-cell function...")
      (displayln (string-append "Current line: " (number->string (current-line-number))))

      (with-handler
        (lambda (e)
          (displayln (string-append "✗ Error calling next-cell: " (error-object-message e))))
        (next-cell)
        (displayln (string-append "After next-cell: " (number->string (current-line-number)))))

      (displayln "")
      (displayln "Testing previous-cell function...")
      (with-handler
        (lambda (e)
          (displayln (string-append "✗ Error calling previous-cell: " (error-object-message e))))
        (previous-cell)
        (displayln (string-append "After previous-cell: " (number->string (current-line-number))))))
    (displayln "✗ No cells found. Open a converted .jl file first with :convert-notebook"))

(displayln "")
(displayln "=== END TEST ===")
(displayln "")
