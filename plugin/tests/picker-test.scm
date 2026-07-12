;;; picker-test.scm — cell-picker row summaries: first meaningful line per cell.

(require "test-framework.scm")
(require "../nothelix/picker.scm")

(provide run-picker-tests)

(define (run-picker-tests)
  (reset-test-counters!)
  (print-test-suite-header "picker")

  (assert-equal "Eigenvalues and eigenvectors"
                (cell-summary "markdown" (list "" "# ## Eigenvalues and eigenvectors" "# prose"))
                "markdown: heading markers and comment prefix stripped")

  (assert-equal "Plot $\\mathbf{b}$ as vectors"
                (cell-summary "markdown" (list "# Plot $\\mathbf{b}$ as vectors"))
                "markdown: plain prose line kept verbatim")

  (assert-equal "A = [1 0; 0 1]"
                (cell-summary "code (julia)" (list "" "# setup" "A = [1 0; 0 1]" "b = A \\ x"))
                "code: skips blanks and comments, takes first code line")

  (assert-equal "" (cell-summary "code (julia)" (list "" "# only a comment"))
                "code: all-comment cell summarizes empty")

  (assert-equal "x^2 + y^2"
                (cell-summary "markdown" (list "# $$" "# x^2 + y^2" "# $$"))
                "markdown: bare $$ fences skipped")

  (assert-equal "caption text"
                (cell-summary "markdown" (list "# @image plot.png" "# caption text"))
                "markdown: @image refs skipped")

  (assert-equal "" (cell-summary "markdown" '()) "empty cell summarizes empty")

  (assert-equal "md" (kind-tag "markdown") "markdown tag")
  (assert-equal "jl" (kind-tag "code (julia)") "julia code tag")
  (assert-equal "pytho" (kind-tag "code (python)") "other lang truncated to 5")
  (assert-equal "raw" (kind-tag "raw") "raw passes through")

  (print-test-suite-footer "picker"))
