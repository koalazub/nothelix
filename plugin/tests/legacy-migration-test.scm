;;; legacy-migration-test.scm — unit tests for the stale legacy-output-block
;;; migration. Exercises the pure `legacy-output-block-range` helper
;;; (cell-boundaries.scm) on mock buffers: a real cell-output block is
;;; located exactly, a clean cell yields no range, user `# ─── … ───` prose
;;; and an unterminated header are left untouched, and a block flanked by
;;; blanks folds one trailing blank into the range. No live editor state.

(require "test-framework.scm")
(require "../nothelix/cell-boundaries.scm")

(provide run-legacy-migration-tests)

(define (buffer->get-line lines)
  (lambda (idx) (list-ref lines idx)))

(define (range-of lines search-start)
  (legacy-output-block-range (buffer->get-line lines) (length lines) search-start))

(define cell-with-block
  (list "# ═══"
        "@cell 1 10"
        "x = 42"
        "# ─── Output ───"
        "# 42"
        "# ─────────────"))

(define clean-cell
  (list "# ═══"
        "@cell 1 10"
        "x = 42"))

(define prose-section
  (list "# ═══"
        "@cell 1 10"
        "x = 1"
        "# ─── Section ───"
        "some prose"))

(define unterminated-header
  (list "# ═══"
        "@cell 1 10"
        "x = 1"
        "# ─── Output ───"
        "# stuff"))

(define block-between-blanks
  (list "# ═══"
        "@cell 1 10"
        "x = 1"
        ""
        "# ─── Output ───"
        "# 1"
        "# ─────────────"
        ""
        "# ═══"
        "@cell 2 20"))

(define (run-legacy-migration-tests)
  (reset-test-counters!)
  (print-test-suite-header "legacy-migration")

  (define block-range (range-of cell-with-block 2))
  (assert-equal 3 (car block-range) "block range starts at the Output header")
  (assert-equal 6 (cdr block-range) "block range ends past the footer (exclusive)")

  (assert-false (range-of clean-cell 2)
                "clean cell yields no legacy range")
  (assert-false (range-of prose-section 2)
                "user '# ─── Section ───' prose is preserved")
  (assert-false (range-of unterminated-header 2)
                "unterminated Output header (no footer) is preserved")

  (define fold-range (range-of block-between-blanks 2))
  (assert-equal 4 (car fold-range) "flanked block range starts at header")
  (assert-equal 8 (cdr fold-range)
                "flanked block folds one trailing blank into the range")

  ;; Anchor: after the block is removed, find-cell-code-end lands on the true
  ;; code end so the virtual-row anchor (code-end - 1) is the last code line.
  (define cleaned (list "# ═══" "@cell 1 10" "x = 42"))
  (define code-end (find-cell-code-end (buffer->get-line cleaned) (length cleaned) 2))
  (assert-equal 3 code-end "cleaned cell code-end is EOF")
  (assert-equal 2 (- code-end 1) "anchor is the last code line (x = 42)")

  (print-test-suite-footer "legacy-migration"))
