;;; output-insert-test.scm — unit tests for the MG3 multi-image stacking helpers:
;;; take-first-n's truncation semantics, the images-truncated? cap predicate, and the
;;; cell-img->image-id band clear-cell-output! relies on to clear exactly the slots
;;; insertion used, with no overlap into a neighboring cell's band.

(require "test-framework.scm")
(require "../nothelix/output-insert.scm")
(require "../nothelix/image-cache.scm")

(provide run-output-insert-tests)

(define (ids-for-cell cell ppc)
  (let loop ([i 0] [acc '()])
    (if (>= i ppc)
        (reverse acc)
        (loop (+ i 1) (cons (cell-img->image-id cell i) acc)))))

(define (contiguous-by-1? lst)
  (cond
    [(or (null? lst) (null? (cdr lst))) #t]
    [(= (cadr lst) (+ (car lst) 1)) (contiguous-by-1? (cdr lst))]
    [else #f]))

(define (run-output-insert-tests)
  (reset-test-counters!)
  (print-test-suite-header "output-insert")

  (assert-equal '(1 2) (take-first-n '(1 2) 5)
                "take-first-n under-count: list shorter than n returns the whole list")
  (assert-equal '(1 2 3) (take-first-n '(1 2 3) 3)
                "take-first-n exact count returns the whole list")
  (assert-equal '(1 2) (take-first-n '(1 2 3 4 5) 2)
                "take-first-n over-count returns only the first n")
  (assert-equal '() (take-first-n '(1 2 3) 0)
                "take-first-n n=0 returns empty")
  (assert-equal '() (take-first-n '() 5)
                "take-first-n on an empty list returns empty")
  (assert-equal '() (take-first-n '(1 2 3) -1)
                "take-first-n negative n returns empty")

  (assert-false (images-truncated? 2 5) "images-truncated? raw < cap is #f")
  (assert-false (images-truncated? 5 5) "images-truncated? raw = cap is #f")
  (assert-true (images-truncated? 7 5) "images-truncated? raw > cap is #t")
  (assert-false (images-truncated? 0 0) "images-truncated? raw = 0, cap = 0 is #f")

  (assert-equal 5 (length (take-first-n (list 1 2 3 4 5 6 7) 5))
                "rendered count = min(raw, cap): raw=7 cap=5 -> 5")
  (assert-equal 2 (length (take-first-n (list 1 2) 5))
                "rendered count = min(raw, cap): raw=2 cap=5 -> 2")
  (assert-equal 0 (length (take-first-n '() 5))
                "rendered count = min(raw, cap): raw=0 cap=5 -> 0")

  (define default-ppc (plots-per-cell))

  (define band3 (ids-for-cell 3 default-ppc))
  (assert-equal default-ppc (length band3)
                "clear-cell-output! band for a cell has plots-per-cell slots")
  (assert-true (contiguous-by-1? band3)
               "clear-cell-output! band is contiguous by 1 (=> all ids distinct)")
  (assert-equal (cell-img->image-id 3 0) (car band3)
                "band's first id matches cell-img->image-id(cell,0)")
  (assert-equal (cell-img->image-id 3 (- default-ppc 1)) (list-ref band3 (- default-ppc 1))
                "band's last id matches cell-img->image-id(cell,ppc-1)")
  (assert-equal (+ 1 (cell-img->image-id 3 (- default-ppc 1))) (cell-img->image-id 4 0)
                "cell 3's band ends exactly where cell 4's band begins (disjoint, adjacent)")
  (assert-equal (+ 1 (cell-img->image-id 0 (- default-ppc 1))) (cell-img->image-id 1 0)
                "cell 0's band ends exactly where cell 1's band begins (disjoint, adjacent)")

  (set-plots-per-cell! 4)
  (define small-ppc (plots-per-cell))
  (define band7-small (ids-for-cell 7 small-ppc))
  (assert-equal 4 (length band7-small) "with plots-per-cell=4, cell 7's band has 4 slots")
  (assert-true (contiguous-by-1? band7-small)
               "with plots-per-cell=4, cell 7's band is contiguous by 1")
  (assert-equal (+ 1 (cell-img->image-id 7 (- small-ppc 1))) (cell-img->image-id 8 0)
                "with plots-per-cell=4, cell 7's band ends exactly where cell 8's begins")
  (set-plots-per-cell! default-ppc)

  (print-test-suite-footer "output-insert"))
