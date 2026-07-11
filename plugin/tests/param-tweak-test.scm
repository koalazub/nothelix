;;; param-tweak-test.scm — pure-logic tests for the @param primitive.

(require "test-framework.scm")
(require "../nothelix/param-tweak.scm")

(provide run-param-tweak-tests)

(define (run-param-tweak-tests)
  (reset-test-counters!)
  (print-test-suite-header "param-tweak")

  (assert-equal (list "freq" "440" 220 880 10 'int)
                (parse-param-line "freq = 440      # @param 220:880 step 10")
                "parse int param with step")
  (assert-equal (list "amp" "0.8" 0.0 1.0 0.05 'float)
                (parse-param-line "amp  = 0.8 # @param 0.0:1.0 step 0.05")
                "parse float param with step")
  (assert-equal "int"
                (symbol->string (list-ref (parse-param-line "n = 5 # @param 1:10") 5))
                "default int kind, no step")
  (assert-false (parse-param-line "x = 5") "no annotation -> #false")
  (assert-false (parse-param-line "# just a comment") "no assignment -> #false")
  (assert-false (parse-param-line "name = foo # @param 1:10") "non-numeric literal -> #false")

  (assert-equal "5" (format-number 5 0) "format int")
  (assert-equal "0.80" (format-number 0.8 2) "format float 2dp")
  (assert-equal "0.05" (format-number 0.05 2) "format small float 2dp")
  (assert-equal "1.00" (format-number 1 2) "format whole as float 2dp")
  (assert-equal 2 (decimals-of 0.05) "decimals of 0.05 is 2")
  (assert-equal 0 (decimals-of 10) "decimals of int is 0")
  (assert-true (>= (decimals-of 1e-5) 5) "decimals of scientific-notation step 1e-5 is >= 5")
  (assert-equal "0.00050" (format-number 0.0005 (decimals-of 1e-5))
                "tiny step preserves float literal instead of truncating to int")

  (assert-equal 450 (nudge-param-value 440 220 880 10 1) "nudge int up by step")
  (assert-equal 430 (nudge-param-value 440 220 880 10 -1) "nudge int down by step")
  (assert-equal 880 (nudge-param-value 875 220 880 10 1) "nudge clamps to hi on grid")
  (assert-equal 220 (nudge-param-value 220 220 880 10 -1) "nudge clamps to lo")
  (assert-true (< (abs (- 0.85 (nudge-param-value 0.8 0.0 1.0 0.05 1))) 0.0001)
               "nudge float up by step")

  (let* ([lines (list "@cell 0 :julia"
                      "freq = 440 # @param 220:880 step 10"
                      "plot(freq)"
                      "@cell 1 :julia"
                      "y = freq * 2"
                      "@cell 2 :julia"
                      "z = frequency + 1")]
         [vec (list->vector lines)]
         [get-line (lambda (i) (if (< i (vector-length vec)) (vector-ref vec i) ""))]
         [total (vector-length vec)])
    (assert-equal 1 (find-param-target-line get-line total 2) "target = @param above cursor")
    (assert-equal 1 (find-param-target-line get-line total 1) "target = @param on cursor line")
    (assert-false (find-param-target-line get-line total 0) "no @param above cell marker -> #false")
    (assert-equal (list "freq") (collect-assigned-names get-line 0 3) "collect assigned names")
    (assert-true (token-references? "y = freq * 2" "freq") "whole-token reference")
    (assert-false (token-references? "z = frequency + 1" "freq") "substring is NOT a reference")
    (assert-equal (list 3) (scan-stale-lines get-line total 1 (list "freq"))
                  "stale cell marker line indices below"))

  (assert-false (token-references? "z = αβ + 1" "α") "unicode-adjacent identifier is NOT a reference")
  (assert-true (token-references? "y = θ * 2" "θ") "standalone unicode identifier is a reference")

  (print-test-suite-footer "param-tweak"))
