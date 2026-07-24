(require "test-framework.scm")
(require "../nothelix/string-utils.scm")
(require "../nothelix/output-view.scm")

(provide run-output-view-tests)

(define (run-output-view-tests)
  (reset-test-counters!)
  (print-test-suite-header "output-view")

  (assert-equal (list "hello")
                (wrap-line "hello" 10)
                "wrap-line: a line under the width stays one chunk")
  (assert-equal (list "hello")
                (wrap-line "hello" 5)
                "wrap-line: a line exactly the width stays one chunk")
  (assert-equal (list "hel" "lo")
                (wrap-line "hello" 3)
                "wrap-line: a long line splits into width-sized chunks")
  (assert-equal (list "aa" "bb" "cc" "d")
                (wrap-line "aabbccd" 2)
                "wrap-line: the final chunk keeps the remainder")
  (assert-equal (list "")
                (wrap-line "" 4)
                "wrap-line: an empty line stays a single empty chunk")
  (assert-equal (list "hello")
                (wrap-line "hello" 0)
                "wrap-line: a non-positive width returns the line unsplit")

  (assert-equal (list "ab" "c" "de" "f")
                (wrap-rows (list "abc" "def") 2)
                "wrap-rows: rows wrap and flatten in order")
  (assert-equal '()
                (wrap-rows '() 5)
                "wrap-rows: no rows wrap to nothing")

  (assert-equal 40 (tail-lines-for 40)
                "tail-lines-for: sizes the request to the content height")
  (assert-equal 1 (tail-lines-for 0)
                "tail-lines-for: a zero height still asks for one line")
  (assert-equal 1 (tail-lines-for -5)
                "tail-lines-for: a negative height clamps to one line")

  (assert-true (string-suffix? (output-view-footer #true) "live")
               "output-view-footer: the live state word trails the footer")
  (assert-true (string-suffix? (output-view-footer #false) "stored")
               "output-view-footer: the stored state word trails the footer")
  (assert-contains (output-view-footer #true) "j/k"
                   "output-view-footer: names the line-scroll keys")
  (assert-contains (output-view-footer #false) "ctrl-d/ctrl-u"
                   "output-view-footer: names the half-page keys")
  (assert-contains (output-view-footer #false) "q close"
                   "output-view-footer: names the close key")

  (assert-equal "cell 7: no output — run it first"
                (no-output-status 7)
                "no-output-status: names the cell and the fix")

  (print-test-suite-footer "output-view"))
