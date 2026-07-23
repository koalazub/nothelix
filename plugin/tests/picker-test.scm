;;; picker-test.scm — cell-picker row summaries: first meaningful line per cell.

(require "test-framework.scm")
(require "../nothelix/picker.scm")
(require "../nothelix/cell-state.scm")

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

  (assert-equal 0 (picker-scroll-offset 0 20 60) "scroll: top stays 0")
  (assert-equal 0 (picker-scroll-offset 5 20 60) "scroll: early selection no scroll")
  (assert-equal 20 (picker-scroll-offset 30 20 60) "scroll: mid selection centers")
  (assert-equal 40 (picker-scroll-offset 59 20 60) "scroll: last row clamps to end")
  (assert-equal 0 (picker-scroll-offset 3 20 5) "scroll: short list never scrolls")

  (define (hay s) (string->list s))
  (assert-true (number? (fuzzy-score (string->list "sec7") (hay "57 md section 7: pseudoinverse")))
               "fuzzy: subsequence matches")
  (assert-false (fuzzy-score (string->list "xyz") (hay "57 md section 7"))
                "fuzzy: non-subsequence rejected")
  (assert-true (number? (fuzzy-score (string->list "SEC") (hay "section")))
               "fuzzy: query case-insensitive")
  (assert-true (> (fuzzy-score (string->list "sec") (hay "section"))
                  (fuzzy-score (string->list "sec") (hay "s-e-c-tion")))
               "fuzzy: contiguous run outscores scattered")

  (define cell-a (list 10 "markdown" 57 "line" "" "section 7: pseudoinverse"
                       (hay "57 md section 7: pseudoinverse")))
  (define cell-b (list 20 "code (julia)" 58 "line" "" "matrix 7: definition"
                       (hay "58 jl matrix 7: definition")))
  (define both (list cell-a cell-b))
  (assert-equal both (car (fuzzy-filter both "")) "filter: empty query keeps all")
  (assert-equal (list cell-a) (car (fuzzy-filter both "pseudo")) "filter: narrows to matches")
  (assert-equal 0 (cdr (fuzzy-filter both "pseudo")) "filter: best row selected")
  (assert-equal '() (car (fuzzy-filter both "zzz")) "filter: no matches yields empty view")

  (set-cell-states! (parse-cell-states "58\tout-of-order\tA,5,below"))
  (assert-equal "↕" (cell-glyph-for 58) "glyph column: a classified cell shows its status glyph")
  (assert-equal "" (cell-glyph-for 57) "glyph column: an unclassified cell shows no glyph")
  (clear-cell-states!)

  (print-test-suite-footer "picker"))
