;;; cell-extraction-test.scm - Tests for cell code extraction from .jl and .ipynb files

(require (prefix-in helix. "helix/commands.scm"))
(require "../nothelix/kernel.scm")

(provide run-cell-extraction-tests)

;; Test counter
(define *tests-passed* 0)
(define *tests-failed* 0)

;; Test assertion helper
(define (assert-equal actual expected description)
  (if (equal? actual expected)
      (begin
        (set! *tests-passed* (+ *tests-passed* 1))
        (displayln (string-append "  ✓ " description)))
      (begin
        (set! *tests-failed* (+ *tests-failed* 1))
        (displayln (string-append "  ✗ " description))
        (display "    Expected: ")
        (displayln expected)
        (display "    Actual:   ")
        (displayln actual))))

(define (assert-true condition description)
  (assert-equal condition #t description))

(define (assert-contains haystack needle description)
  (assert-true (string-contains? haystack needle) description))

;; Create a test .jl file
(define test-jl-path "/tmp/test-notebook.jl")
(define test-jl-content "#=
# Test Notebook
=#

# ═══════════════════════════════════════════════════════════════════
@cell 1 10
x = 42
y = x + 8

# ═══════════════════════════════════════════════════════════════════
@markdown 2
#=
Some markdown content
=#

# ═══════════════════════════════════════════════════════════════════
@cell 3 20
z = x * 2
result = y + z

# ═══════════════════════════════════════════════════════════════════
@cell 5 30
# This is cell 5
using Statistics
mean([1, 2, 3])
")

;; Create a test .ipynb file
(define test-ipynb-path "/tmp/test-notebook.ipynb")
(define test-ipynb-content "{
  \"cells\": [
    {
      \"cell_type\": \"markdown\",
      \"source\": [\"# Test Notebook\"]
    },
    {
      \"cell_type\": \"code\",
      \"source\": [\"x = 100\\n\", \"y = 200\"]
    },
    {
      \"cell_type\": \"code\",
      \"source\": [\"z = x + y\\n\", \"println(z)\"]
    }
  ]
}
")

;; Write test files
(define (setup-test-files)
  (displayln "Setting up test files...")
  ;; Write .jl file
  (helix.run-shell-command
    (string-append "echo '" test-jl-content "' > " test-jl-path))
  ;; Write .ipynb file
  (helix.run-shell-command
    (string-append "echo '" test-ipynb-content "' > " test-ipynb-path))
  (displayln "Test files created"))

;; Clean up test files
(define (cleanup-test-files)
  (helix.run-shell-command (string-append "rm -f " test-jl-path))
  (helix.run-shell-command (string-append "rm -f " test-ipynb-path))
  (displayln "Test files cleaned up"))

;; Test get-cell-code-from-jl
(define (test-jl-cell-extraction)
  (displayln "\n## Testing .jl cell extraction (get-cell-code-from-jl)")
  (displayln "  ⚠ SKIPPED - Rebuild Helix with get-cell-code-from-jl registered first")
  (displayln "    After rebuild, uncomment test in cell-extraction-test.scm")

  ;; TODO: Uncomment after rebuilding Helix
  ;; ;; Test cell 1
  ;; (define cell1-json (get-cell-code-from-jl test-jl-path 1))
  ;; (define cell1-code (json-get cell1-json "code"))
  ;; (assert-contains cell1-code "x = 42" "Cell 1 should contain 'x = 42'")
  ;; (assert-contains cell1-code "y = x + 8" "Cell 1 should contain 'y = x + 8'")
  ;; (assert-true (not (string-contains? cell1-code "@cell")) "Cell 1 should not contain marker")
  ;; (assert-true (not (string-contains? cell1-code "═══")) "Cell 1 should not contain separator")

  ;; ;; Test cell 3
  ;; (define cell3-json (get-cell-code-from-jl test-jl-path 3))
  ;; (define cell3-code (json-get cell3-json "code"))
  ;; (assert-contains cell3-code "z = x * 2" "Cell 3 should contain 'z = x * 2'")
  ;; (assert-contains cell3-code "result = y + z" "Cell 3 should contain 'result = y + z'")

  ;; ;; Test cell 5
  ;; (define cell5-json (get-cell-code-from-jl test-jl-path 5))
  ;; (define cell5-code (json-get cell5-json "code"))
  ;; (assert-contains cell5-code "using Statistics" "Cell 5 should contain 'using Statistics'")
  ;; (assert-contains cell5-code "mean([1, 2, 3])" "Cell 5 should contain 'mean([1, 2, 3])'")

  ;; ;; Test non-existent cell
  ;; (define cell99-json (get-cell-code-from-jl test-jl-path 99))
  ;; (define cell99-error (json-get cell99-json "error"))
  ;; (assert-true (> (string-length cell99-error) 0) "Non-existent cell should return error")
  )

;; Test notebook-get-cell-code
(define (test-ipynb-cell-extraction)
  (displayln "\n## Testing .ipynb cell extraction (notebook-get-cell-code)")

  ;; Test cell 0 (markdown - should return type)
  (define cell0-json (notebook-get-cell-code test-ipynb-path 0))
  (define cell0-type (json-get cell0-json "type"))
  (assert-equal cell0-type "markdown" "Cell 0 should be markdown")

  ;; Test cell 1 (code)
  (define cell1-json (notebook-get-cell-code test-ipynb-path 1))
  (define cell1-code (json-get cell1-json "code"))
  (define cell1-type (json-get cell1-json "type"))
  (assert-equal cell1-type "code" "Cell 1 should be code")
  (assert-contains cell1-code "x = 100" "Cell 1 should contain 'x = 100'")
  (assert-contains cell1-code "y = 200" "Cell 1 should contain 'y = 200'")

  ;; Test cell 2 (code)
  (define cell2-json (notebook-get-cell-code test-ipynb-path 2))
  (define cell2-code (json-get cell2-json "code"))
  (assert-contains cell2-code "z = x + y" "Cell 2 should contain 'z = x + y'")
  (assert-contains cell2-code "println(z)" "Cell 2 should contain 'println(z)'"))

;; Test get-cell-at-line for .jl files
(define (test-cell-at-line-jl)
  (displayln "\n## Testing get-cell-at-line for .jl files")

  ;; Line 7 should be in cell 1 (the "@cell 1 10" line)
  (define result1 (get-cell-at-line test-jl-path 7))
  (define idx1 (json-get result1 "cell_index"))
  (assert-equal idx1 "1" "Line 7 should be in cell 1")

  ;; Line 8 should also be in cell 1 (x = 42 line)
  (define result2 (get-cell-at-line test-jl-path 8))
  (define idx2 (json-get result2 "cell_index"))
  (assert-equal idx2 "1" "Line 8 should be in cell 1")

  ;; Line 16 should be in cell 3
  (define result3 (get-cell-at-line test-jl-path 16))
  (define idx3 (json-get result3 "cell_index"))
  (assert-equal idx3 "3" "Line 16 should be in cell 3")

  ;; Test source_path extraction
  (define source-path (json-get result1 "source_path"))
  (assert-equal source-path test-jl-path "source_path should match input path"))

;; Test that code extraction preserves whitespace and structure
(define (test-code-structure-preservation)
  (displayln "\n## Testing code structure preservation")

  (define cell1-json (get-cell-code-from-jl test-jl-path 1))
  (define cell1-code (json-get cell1-json "code"))

  ;; Should have newline between statements
  (assert-true (string-contains? cell1-code "\n") "Code should preserve newlines")

  ;; Should not have leading/trailing whitespace
  (define trimmed (string-trim cell1-code))
  (assert-equal (string-length cell1-code) (string-length trimmed)
                "Code should be trimmed"))

;; Main test runner
(define (run-cell-extraction-tests)
  (displayln "\n╔════════════════════════════════════════════════════════╗")
  (displayln "║  Cell Extraction Tests                                 ║")
  (displayln "╚════════════════════════════════════════════════════════╝")

  (set! *tests-passed* 0)
  (set! *tests-failed* 0)

  (setup-test-files)

  (test-jl-cell-extraction)
  (test-ipynb-cell-extraction)
  (test-cell-at-line-jl)
  (test-code-structure-preservation)

  (cleanup-test-files)

  (displayln "\n╔════════════════════════════════════════════════════════╗")
  (displayln (string-append "║  Results: "
                           (number->string *tests-passed*) " passed, "
                           (number->string *tests-failed*) " failed"
                           (string-repeat " " (- 38
                                               (string-length (number->string *tests-passed*))
                                               (string-length (number->string *tests-failed*))))
                           "║"))
  (displayln "╚════════════════════════════════════════════════════════╝")

  (if (equal? *tests-failed* 0)
      (displayln "✓ All tests passed!")
      (displayln (string-append "✗ " (number->string *tests-failed*) " test(s) failed"))))

;; Helper to repeat string n times
(define (string-repeat str n)
  (if (<= n 0)
      ""
      (string-append str (string-repeat str (- n 1)))))
