;;; output-render-test.scm — unit tests for the colored braille text-plot
;;; helpers: `ansi-color->scope`'s ANSI-index -> theme-scope map,
;;; `text-plot->styled-rows`'s span segmentation (output-render.scm), and
;;; the flat-blob encode/decode round trip the store persists across a
;;; reopen (output-store.scm). Mirrors the image-cache/output-insert test
;;; pattern: pure-function assertions, no live editor state.

(require "test-framework.scm")
(require "../nothelix/output-render.scm")
(require "../nothelix/output-store.scm")

(provide run-output-render-tests)

(define tp-plot-sep (make-string 1 (integer->char 30)))
(define tp-section-sep (make-string 1 (integer->char 29)))
(define tp-span-sep (make-string 1 (integer->char 31)))

(define (run-output-render-tests)
  (reset-test-counters!)
  (print-test-suite-header "output-render")

  (assert-equal "ui.virtual.output.series0" (ansi-color->scope 0)
                "ansi-color->scope 0 -> series0")
  (assert-equal "ui.virtual.output.series3" (ansi-color->scope 3)
                "ansi-color->scope 3 -> series3")
  (assert-equal "ui.virtual.output.series7" (ansi-color->scope 7)
                "ansi-color->scope 7 -> series7")
  (assert-equal "ui.virtual.output.series0" (ansi-color->scope 8)
                "ansi-color->scope 8 (bright) reuses series0")
  (assert-equal "ui.virtual.output.series3" (ansi-color->scope 11)
                "ansi-color->scope 11 (bright) reuses series3")
  (assert-equal "ui.virtual.output.series7" (ansi-color->scope 15)
                "ansi-color->scope 15 (bright) reuses series7")
  (assert-equal #false (ansi-color->scope 16)
                "ansi-color->scope 16 (out of range) -> #false")
  (assert-equal #false (ansi-color->scope -1)
                "ansi-color->scope -1 (out of range) -> #false")
  (assert-equal #false (ansi-color->scope 2.5)
                "ansi-color->scope non-integer -> #false")

  (assert-equal (list "hello")
                (text-plot->styled-rows (list "hello") '())
                "text-plot->styled-rows: row with no spans stays a plain string")

  (assert-equal
    (list (list (list "AAA" "ui.virtual.output.series2")
                (list " BBB " #false)
                (list "CCC" "ui.virtual.output.series1")))
    (text-plot->styled-rows (list "AAA BBB CCC") (list (list 0 0 3 2) (list 0 8 11 1)))
    "text-plot->styled-rows: 2-color row segments colored runs + untagged gap")

  (assert-equal
    (list (list (list "AA" "ui.virtual.output.series2"))
          (list (list "BB" "ui.virtual.output.series4")))
    (text-plot->styled-rows (list "AA" "BB") (list (list 0 0 2 2) (list 1 0 2 4)))
    "text-plot->styled-rows: spans on different rows apply independently")

  (assert-equal
    (list (list (list "hi" "ui.virtual.output.series2") (list "there" #false)))
    (text-plot->styled-rows (list "hithere") (list (list 0 0 2 2)))
    "text-plot->styled-rows: a leading span leaves the remainder an untagged trailing gap")

  (assert-equal
    (list (list (list "hello" #false)))
    (text-plot->styled-rows (list "hello") (list (list 0 2 2 2)))
    "text-plot->styled-rows: a zero-width span is skipped defensively, whole row untagged")

  (assert-equal
    (list (list (list "AB" #false)))
    (text-plot->styled-rows (list "AB") (list (list 0 0 2 99)))
    "text-plot->styled-rows: an out-of-range color (99) composes to an untagged (#false) scope")

  (assert-equal '() (decode-text-plots-blob "")
                "decode-text-plots-blob: empty blob -> no plots")
  (assert-equal '() (decode-text-plots-blob #false)
                "decode-text-plots-blob: #false blob -> no plots")

  (define one-plot-blob
    (string-append "AB" "\n" "CD" tp-section-sep
                   "0,0,1,2" tp-span-sep "1,0,2,4"))
  (assert-equal
    (list (cons (list "AB" "CD") (list (list 0 0 1 2) (list 1 0 2 4))))
    (decode-text-plots-blob one-plot-blob)
    "decode-text-plots-blob: single plot decodes rows + spans")

  (define no-spans-blob (string-append "A" tp-section-sep))
  (assert-equal (list (cons (list "A") '()))
                (decode-text-plots-blob no-spans-blob)
                "decode-text-plots-blob: empty spans section -> '()")

  (define two-plot-blob
    (string-append "A" tp-section-sep tp-plot-sep "B" tp-section-sep "0,0,1,3"))
  (assert-equal
    (list (cons (list "A") '()) (cons (list "B") (list (list 0 0 1 3))))
    (decode-text-plots-blob two-plot-blob)
    "decode-text-plots-blob: multiple plots split on PLOT_SEP")

  (define malformed-span-blob
    (string-append "A" tp-section-sep "0,0,1" tp-span-sep "0,0,1,2"))
  (assert-equal
    (list (cons (list "A") (list (list 0 0 1 2))))
    (decode-text-plots-blob malformed-span-blob)
    "decode-text-plots-blob: a span with too few fields is dropped, well-formed ones kept")

  (define hash "12345")
  (define encoded-with-plots
    (encode-outputs+rows+text-plots "[]" (list "stdout line") one-plot-blob))
  (define raw-with-plots (string-append hash "\t" encoded-with-plots))

  (assert-equal (list "stdout line") (decode-stored-rows raw-with-plots hash)
                "decode-stored-rows: plain rows unaffected by a trailing text-plots section")
  (assert-equal one-plot-blob (decode-stored-text-plots-blob raw-with-plots hash)
                "decode-stored-text-plots-blob: recovers the exact stored blob")

  (define encoded-no-plots (encode-outputs+rows+text-plots "[]" (list "stdout line") ""))
  (assert-equal (encode-outputs+rows "[]" (list "stdout line")) encoded-no-plots
                "encode-outputs+rows+text-plots: an empty blob is byte-identical to encode-outputs+rows")
  (define raw-no-plots (string-append hash "\t" encoded-no-plots))
  (assert-equal #false (decode-stored-text-plots-blob raw-no-plots hash)
                "decode-stored-text-plots-blob: #false when no text-plots were stored")
  (assert-equal (list "stdout line") (decode-stored-rows raw-no-plots hash)
                "decode-stored-rows: still works on the no-text-plots (legacy) shape")

  (assert-equal #false (decode-stored-text-plots-blob raw-with-plots "stale-hash")
                "decode-stored-text-plots-blob: #false on a stale (mismatched) hash")
  (assert-equal #false (decode-stored-rows raw-with-plots "stale-hash")
                "decode-stored-rows: #false on a stale (mismatched) hash")

  (print-test-suite-footer "output-render"))
