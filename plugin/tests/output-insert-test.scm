;;; output-insert-test.scm — unit tests for the MG3 multi-image stacking helpers:
;;; take-first-n's truncation semantics, the images-truncated? cap predicate, and the
;;; cell-img->image-id band clear-cell-output! relies on to clear exactly the fixed
;;; *image-slots-per-cell*-wide slots insertion used, with no overlap into a
;;; neighboring cell's band, and unaffected by the mutable plots-per-cell cap.

(require "test-framework.scm")
(require "../nothelix/output-insert.scm")
(require "../nothelix/output-render.scm")
(require "../nothelix/image-cache.scm")

(provide run-output-insert-tests)

(define (ids-for-cell cell n)
  (let loop ([i 0] [acc '()])
    (if (>= i n)
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
  (define slots *image-slots-per-cell*)

  (define band3 (ids-for-cell 3 slots))
  (assert-equal slots (length band3)
                "clear-cell-output! band for a cell has *image-slots-per-cell* slots")
  (assert-true (contiguous-by-1? band3)
               "clear-cell-output! band is contiguous by 1 (=> all ids distinct)")
  (assert-equal (cell-img->image-id 3 0) (car band3)
                "band's first id matches cell-img->image-id(cell,0)")
  (assert-equal (cell-img->image-id 3 (- slots 1)) (list-ref band3 (- slots 1))
                "band's last id matches cell-img->image-id(cell,slots-1)")
  (assert-equal (+ 1 (cell-img->image-id 3 (- slots 1))) (cell-img->image-id 4 0)
                "cell 3's band ends exactly where cell 4's band begins (disjoint, adjacent)")
  (assert-equal (+ 1 (cell-img->image-id 0 (- slots 1))) (cell-img->image-id 1 0)
                "cell 0's band ends exactly where cell 1's band begins (disjoint, adjacent)")

  (define ids-before (ids-for-cell 7 slots))
  (set-plots-per-cell! 4)
  (assert-equal ids-before (ids-for-cell 7 slots)
                "cell-img->image-id is unaffected by shrinking plots-per-cell")
  (set-plots-per-cell! 300)
  (assert-equal ids-before (ids-for-cell 7 slots)
                "cell-img->image-id is unaffected by growing plots-per-cell")
  (set-plots-per-cell! default-ppc)

  (assert-equal '() (notes-blob->group "")
                "notes-blob->group: empty blob -> no note rows")
  (assert-equal '() (notes-blob->group #false)
                "notes-blob->group: #false blob -> no note rows")
  (assert-equal '() (notes-blob->group "ERROR: json-get-notes: invalid JSON: x")
                "notes-blob->group: an ERROR reply never fabricates a note row")
  (assert-equal (list "note: A was last assigned by cell 76, below this cell")
                (notes-blob->group "note: A was last assigned by cell 76, below this cell")
                "notes-blob->group: single note -> one row")
  (assert-equal (list "note: A below" "note: B stale")
                (notes-blob->group "note: A below\nnote: B stale")
                "notes-blob->group: newline-joined notes split into rows")

  (assert-equal
    (list (list "bar" "ui.virtual.output.series0" "note: A below")
          (list "bar" "ui.virtual.output.series1" "out line"))
    (assign-cycling-bars
      (list (notes-blob->group "note: A below") (list "out line")))
    "notes group renders as its own bar group (series0), shifting stdout to series1")

  (assert-equal
    (list (list "bar" "ui.virtual.output.series0" "out line"))
    (assign-cycling-bars
      (list (notes-blob->group "") (list "out line")))
    "no notes: empty notes group consumes no color, stdout stays series0")

  (print-test-suite-footer "output-insert"))
