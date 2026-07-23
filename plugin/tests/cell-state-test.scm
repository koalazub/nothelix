;;; cell-state-test.scm — classification cache parse, glyph/label presentation,
;;; edited-since-run override, and marker-index parsing (cell-state.scm).

(require "test-framework.scm")
(require "../nothelix/cell-state.scm")

(provide run-cell-state-tests)

(define (run-cell-state-tests)
  (reset-test-counters!)
  (print-test-suite-header "cell-state")

  (define blob
    "0\tfresh\t\t12\n3\tout-of-order\tA,5,below\t1400\n7\tstale-input\tB,2,stale;C,4,fresh\t")
  (define h (parse-cell-states blob))

  (assert-equal (list "fresh" '() 12) (hash-try-get h 0)
                "parse: a fresh cell has state fresh, no inputs, and its duration")
  (assert-equal (list "out-of-order" (list (list "A" 5 "below")) 1400) (hash-try-get h 3)
                "parse: an out-of-order cell keeps its single input triple and duration")
  (assert-equal (list "stale-input" (list (list "B" 2 "stale") (list "C" 4 "fresh")) #false)
                (hash-try-get h 7)
                "parse: multiple inputs split on the semicolon, blank duration is #false")
  (assert-equal 12 (cell-state-record-duration (hash-try-get h 0))
                "duration accessor: reads the parsed milliseconds")
  (assert-equal #false (cell-state-record-duration (hash-try-get h 7))
                "duration accessor: a blank duration is #false")
  (assert-equal #false (hash-try-get (parse-cell-states "") 0)
                "parse: an empty blob yields no entries")
  (assert-equal #false (hash-try-get (parse-cell-states "ERROR: boom") 0)
                "parse: an ERROR blob is never turned into rows")

  (assert-equal "" (cell-state-glyph "fresh") "glyph: fresh shows nothing")
  (assert-equal "↕" (cell-state-glyph "out-of-order") "glyph: out-of-order")
  (assert-equal "○" (cell-state-glyph "stale-input") "glyph: stale-input")
  (assert-equal "∅" (cell-state-glyph "orphan-input") "glyph: orphan-input")
  (assert-equal "✎" (cell-state-glyph "edited-since-run") "glyph: edited-since-run")
  (assert-false (cell-state-nonfresh? "fresh") "nonfresh?: fresh is fresh")
  (assert-false (cell-state-nonfresh? "") "nonfresh?: empty is fresh")
  (assert-true (cell-state-nonfresh? "out-of-order") "nonfresh?: out-of-order is non-fresh")

  (assert-equal "uses A from cell 76, below"
                (cell-state-label "out-of-order" (list (list "A" 76 "below")))
                "label: out-of-order wording")
  (assert-equal "input A changed in cell 74"
                (cell-state-label "stale-input" (list (list "A" 74 "stale")))
                "label: stale-input wording")
  (assert-equal "A has no defining cell"
                (cell-state-label "orphan-input" (list (list "A" 3 "orphan")))
                "label: orphan-input wording")
  (assert-equal "edited since last run"
                (cell-state-label "edited-since-run" '())
                "label: edited-since-run wording")
  (assert-equal "  ↕ uses A from cell 76, below"
                (cell-state-tag-text "out-of-order" (list (list "A" 76 "below")))
                "tag text: glyph then label")

  (assert-equal "out of order" (input-freshness-word "below") "freshness word: below")
  (assert-equal "stale" (input-freshness-word "stale") "freshness word: stale")
  (assert-equal "no defining cell" (input-freshness-word "orphan") "freshness word: orphan")
  (assert-equal "fresh" (input-freshness-word "fresh") "freshness word: fresh")

  (assert-equal 5 (marker-line-cell-index "@cell 5 :julia # foo") "marker index: @cell")
  (assert-equal 3 (marker-line-cell-index "@markdown 3") "marker index: @markdown")
  (assert-equal #false (marker-line-cell-index "@cell") "marker index: bare marker -> #false")

  (set-cell-states! h)
  (assert-equal (list "out-of-order" (list (list "A" 5 "below")) 1400) (cell-state-for 3)
                "cache: lookup returns the record")
  (assert-equal "↕" (cell-glyph-for 3) "glyph-for: reads the cache")
  (assert-equal "" (cell-glyph-for 999) "glyph-for: an unknown cell shows no glyph")
  (assert-equal 1400 (cell-duration-for 3) "duration-for: reads the cache")
  (assert-equal #false (cell-duration-for 999) "duration-for: an unknown cell has no duration")

  (assert-false (cell-running? 3) "running?: nothing runs by default")
  (set-running-cell! 3)
  (assert-true (cell-running? 3) "running?: the marked cell is running")
  (assert-false (cell-running? 7) "running?: only the marked cell is running")
  (clear-running-cell!)
  (assert-false (cell-running? 3) "running?: clearing stops the marker")

  (apply-edited-overrides! (list 7))
  (assert-equal "edited-since-run" (car (cell-state-for 7))
                "override: an edited cell becomes edited-since-run")
  (assert-equal (list (list "B" 2 "stale") (list "C" 4 "fresh")) (cadr (cell-state-for 7))
                "override: the input list survives the override")
  (apply-edited-overrides! (list 42))
  (assert-equal "edited-since-run" (car (cell-state-for 42))
                "override: an edited cell with no prior record still classifies")

  (clear-cell-states!)
  (assert-equal #false (cell-state-for 3) "clear: the cache empties")

  (print-test-suite-footer "cell-state"))
