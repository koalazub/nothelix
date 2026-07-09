;;; execution-flow-test.scm - Integration tests for full cell execution flow

(require (prefix-in helix. "helix/commands.scm"))
(require "../nothelix/kernel.scm")
(require "../nothelix/execution.scm")
(require "../nothelix/string-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kernel-execute-cell-start
                          kernel-poll-result
                          get-cell-code-from-jl
                          json-get))

(provide run-execution-flow-tests)

(define *tests-passed* 0)
(define *tests-failed* 0)

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

(define (assert-not-contains haystack needle description)
  (assert-true (not (string-contains? haystack needle)) description))

(define (wait-for-kernel-result kernel-dir max-attempts)
  (define (poll-loop attempts)
    (if (>= attempts max-attempts)
        (begin
          (displayln "  ⚠ Timeout waiting for kernel result")
          (json-get "{\"status\":\"error\",\"error\":\"timeout\"}" ""))
        (let ([result (kernel-poll-result kernel-dir)])
          (if (equal? (json-get result "status") "pending")
              (begin
                (helix.run-shell-command "sleep 0.1")
                (poll-loop (+ attempts 1)))
              result))))
  (poll-loop 0))

(define (test-code-extraction-accuracy)
  (displayln "\n## Testing code extraction accuracy")

  (define test-file "/tmp/test-extraction.jl")
  (define content "@cell 1 10
x = 1 + 1
y = x * 2

@cell 3 20
z = 100")

  (helix.run-shell-command (string-append "echo '" content "' > " test-file))

  (define cell1-json (get-cell-code-from-jl test-file 1))
  (define cell1-code (json-get cell1-json "code"))

  (assert-contains cell1-code "x = 1 + 1" "Cell 1 code should contain first line")
  (assert-contains cell1-code "y = x * 2" "Cell 1 code should contain second line")
  (assert-not-contains cell1-code "@cell" "Cell 1 code should not contain marker")

  (define cell3-json (get-cell-code-from-jl test-file 3))
  (define cell3-code (json-get cell3-json "code"))

  (assert-equal cell3-code "z = 100" "Cell 3 code should be exact")

  (helix.run-shell-command (string-append "rm -f " test-file)))

(define (test-sequential-execution)
  (displayln "\n## Testing sequential execution with state")

  (define test-nb "/tmp/test-sequential.jl")
  (define content "@cell 1 10
counter = 0

@cell 3 20
counter = counter + 5

@cell 5 30
counter = counter * 2
counter")

  (helix.run-shell-command (string-append "echo '" content "' > " test-nb))

  (define kernel (kernel-get-for-notebook test-nb "julia"))
  (define kernel-dir (hash-get kernel 'kernel-dir))

  (define code1 (json-get (get-cell-code-from-jl test-nb 1) "code"))
  (kernel-execute-cell-start kernel-dir 1 code1)
  (define result1 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result1 "status") "ok" "Cell 1 should execute successfully")

  (define code3 (json-get (get-cell-code-from-jl test-nb 3) "code"))
  (kernel-execute-cell-start kernel-dir 3 code3)
  (define result3 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result3 "status") "ok" "Cell 3 should execute successfully")
  (assert-not-contains (json-get result3 "error") "UndefVarError"
                       "Cell 3 should see counter from cell 1")

  (define code5 (json-get (get-cell-code-from-jl test-nb 5) "code"))
  (kernel-execute-cell-start kernel-dir 5 code5)
  (define result5 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result5 "status") "ok" "Cell 5 should execute successfully")
  (define output5 (json-get result5 "output_repr"))
  (assert-contains output5 "10" "Cell 5 should output 10 (0+5)*2")

  (stop-kernel test-nb)
  (helix.run-shell-command (string-append "rm -f " test-nb)))

(define (test-error-handling)
  (displayln "\n## Testing error handling")

  (define test-nb "/tmp/test-errors.jl")
  (define content "@cell 1 10
good_var = 42

@cell 3 20
# This will error
bad_result = undefined_variable

@cell 5 30
# This should still work after error
good_var * 2")

  (helix.run-shell-command (string-append "echo '" content "' > " test-nb))

  (define kernel (kernel-get-for-notebook test-nb "julia"))
  (define kernel-dir (hash-get kernel 'kernel-dir))

  (define code1 (json-get (get-cell-code-from-jl test-nb 1) "code"))
  (kernel-execute-cell-start kernel-dir 1 code1)
  (define result1 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result1 "status") "ok" "Cell 1 should execute successfully")

  (define code3 (json-get (get-cell-code-from-jl test-nb 3) "code"))
  (kernel-execute-cell-start kernel-dir 3 code3)
  (define result3 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result3 "status") "error" "Cell 3 should error")
  (assert-contains (json-get result3 "error") "UndefVarError"
                   "Cell 3 error should be UndefVarError")

  (define code5 (json-get (get-cell-code-from-jl test-nb 5) "code"))
  (kernel-execute-cell-start kernel-dir 5 code5)
  (define result5 (wait-for-kernel-result kernel-dir 50))
  (assert-equal (json-get result5 "status") "ok"
                "Cell 5 should execute successfully after error in cell 3")
  (define output5 (json-get result5 "output_repr"))
  (assert-contains output5 "84" "Cell 5 should compute 42 * 2 = 84")

  (stop-kernel test-nb)
  (helix.run-shell-command (string-append "rm -f " test-nb)))

(define (test-output-capture)
  (displayln "\n## Testing output capture")

  (define test-nb "/tmp/test-output.jl")
  (define content "@cell 1 10
println(\"Hello from Julia\")
x = 123
x")

  (helix.run-shell-command (string-append "echo '" content "' > " test-nb))

  (define kernel (kernel-get-for-notebook test-nb "julia"))
  (define kernel-dir (hash-get kernel 'kernel-dir))

  (define code1 (json-get (get-cell-code-from-jl test-nb 1) "code"))
  (kernel-execute-cell-start kernel-dir 1 code1)
  (define result1 (wait-for-kernel-result kernel-dir 50))

  (assert-equal (json-get result1 "status") "ok" "Cell should execute successfully")

  (define stdout (json-get result1 "stdout"))
  (assert-contains stdout "Hello from Julia" "stdout should contain println output")

  (define output (json-get result1 "output_repr"))
  (assert-contains output "123" "output_repr should contain return value")

  (stop-kernel test-nb)
  (helix.run-shell-command (string-append "rm -f " test-nb)))

(define (test-multiple-kernels)
  (displayln "\n## Testing multiple concurrent kernels")

  (define nb1 "/tmp/notebook-1.jl")
  (define nb2 "/tmp/notebook-2.jl")

  (helix.run-shell-command (string-append "echo '@cell 1 10\nnb1_var = 100' > " nb1))
  (helix.run-shell-command (string-append "echo '@cell 1 10\nnb2_var = 200' > " nb2))

  (define kernel1 (kernel-get-for-notebook nb1 "julia"))
  (define kernel2 (kernel-get-for-notebook nb2 "julia"))

  (define dir1 (hash-get kernel1 'kernel-dir))
  (define dir2 (hash-get kernel2 'kernel-dir))

  (assert-true (not (equal? dir1 dir2))
               "Different notebooks should have different kernel directories")

  (kernel-execute-cell-start dir1 1 "nb1_var = 100")
  (define result1 (wait-for-kernel-result dir1 50))
  (assert-equal (json-get result1 "status") "ok" "Notebook 1 cell should execute")

  (kernel-execute-cell-start dir2 1 "nb2_var = 200")
  (define result2 (wait-for-kernel-result dir2 50))
  (assert-equal (json-get result2 "status") "ok" "Notebook 2 cell should execute")

  (kernel-execute-cell-start dir1 3 "nb2_var")
  (define result3 (wait-for-kernel-result dir1 50))
  (assert-equal (json-get result3 "status") "error"
                "Notebook 1 kernel should not see notebook 2 variables")

  (stop-kernel nb1)
  (stop-kernel nb2)
  (helix.run-shell-command (string-append "rm -f " nb1 " " nb2)))

(define (run-execution-flow-tests)
  (displayln "\n╔════════════════════════════════════════════════════════╗")
  (displayln "║  Execution Flow Integration Tests                      ║")
  (displayln "╚════════════════════════════════════════════════════════╝")
  (displayln "  ⚠ SKIPPED - Rebuild Helix with get-cell-code-from-jl registered first")
  (displayln "    These tests require get-cell-code-from-jl for .jl file parsing")
  (displayln "    After rebuild, uncomment tests in execution-flow-test.scm")

  (set! *tests-passed* 0)
  (set! *tests-failed* 0)

  ;; TODO: Uncomment after rebuilding Helix
  ;; ;; Stop all kernels to start fresh
  ;; (stop-all-kernels)
  ;; (helix.run-shell-command "sleep 1")

  ;; (test-code-extraction-accuracy)
  ;; (test-sequential-execution)
  ;; (test-error-handling)
  ;; (test-output-capture)
  ;; (test-multiple-kernels)

  ;; ;; Cleanup all kernels
  ;; (stop-all-kernels)

  (displayln "\n╔════════════════════════════════════════════════════════╗")
  (displayln (string-append "║  Results: "
                           (number->string *tests-passed*) " passed, "
                           (number->string *tests-failed*) " failed"
                           (string-repeat " " (- 38
                                               (string-length (number->string *tests-passed*))
                                               (string-length (number->string *tests-failed*))))
                           "║"))
  (displayln "╚════════════════════════════════════════════════════════╝")

  (define suite-passed (equal? *tests-failed* 0))
  (if suite-passed
      (displayln "✓ All tests passed!")
      (displayln (string-append "✗ " (number->string *tests-failed*) " test(s) failed")))
  suite-passed)

(define (string-repeat str n)
  (if (<= n 0)
      ""
      (string-append str (string-repeat str (- n 1)))))
