;;; image-cache-test.scm — boundary tests for image-id config/derivation, pinning the
;;; exact-integer? guard against untrusted-config float injection (`plots-per-cell = 32.0`).

(require "test-framework.scm")
(require "../nothelix/image-cache.scm")

(provide run-image-cache-tests)

(define (run-image-cache-tests)
  (reset-test-counters!)
  (print-test-suite-header "image-cache")

  (define default-ppc (plots-per-cell))

  (set-plots-per-cell! 32.5)
  (assert-equal default-ppc (plots-per-cell) "set-plots-per-cell! rejects 32.5 (not exact-integer?)")

  (set-plots-per-cell! 32.0)
  (assert-equal default-ppc (plots-per-cell) "set-plots-per-cell! rejects 32.0 (not exact-integer?)")

  (set-plots-per-cell! -5)
  (assert-equal default-ppc (plots-per-cell) "set-plots-per-cell! rejects -5 (not > 0)")

  (set-plots-per-cell! 0)
  (assert-equal default-ppc (plots-per-cell) "set-plots-per-cell! rejects 0 (not > 0)")

  (set-plots-per-cell! 32)
  (assert-equal 32 (plots-per-cell) "set-plots-per-cell! accepts exact-integer 32")

  (set-plots-per-cell! 300)
  (assert-equal 256 (plots-per-cell) "set-plots-per-cell! clamps 300 to 256")

  (set-plots-per-cell! default-ppc)

  (define (id-ok? cell img)
    (define id (cell-img->image-id cell img))
    (and (exact-integer? id) (< id 4000000)))

  (assert-true (id-ok? 0 0) "cell-img->image-id (0,0) is an exact integer under 4M")
  (assert-true (id-ok? 5 3) "cell-img->image-id (5,3) is an exact integer under 4M")
  (assert-true (id-ok? 100 5) "cell-img->image-id (100,5) is an exact integer under 4M")
  (assert-true (id-ok? 99999 999) "cell-img->image-id (99999,999) is an exact integer under 4M")
  (assert-true (id-ok? 0 -1) "cell-img->image-id tolerates a negative img-index")

  (assert-equal (cons 3 1) (extract-cell-and-img-from-path "cell-3-1.png")
                "extract-cell-and-img-from-path parses cell-3-1.png -> (3 . 1)")
  (assert-equal (cons 42 0) (extract-cell-and-img-from-path "cell-42.png")
                "extract-cell-and-img-from-path parses legacy cell-42.png -> (42 . 0)")
  (assert-equal #false (extract-cell-and-img-from-path "notacell.png")
                "extract-cell-and-img-from-path rejects a non-matching path")

  (print-test-suite-footer "image-cache"))
