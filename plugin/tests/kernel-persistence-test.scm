;;; kernel-persistence-test.scm - Tests for kernel persistence and variable sharing

(require (prefix-in helix. "helix/commands.scm"))
(require "../nothelix/kernel.scm")

(provide run-kernel-persistence-tests)

;; Test counter
(define *tests-passed* 0)
(define *tests-failed* 0)

;; Test assertion helpers
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

;; Test notebook path
(define test-notebook "/tmp/test-vars.jl")

;; Setup: Create test notebook
(define (setup-test-notebook)
  (displayln "Creating test notebook...")
  (define content "#=
# Variable Persistence Test
=#

# ═══════════════════════════════════════════════════════════════════
@cell 1 10
test_var_1 = 100
test_var_2 = 200

# ═══════════════════════════════════════════════════════════════════
@cell 3 20
# This cell uses variables from cell 1
test_var_3 = test_var_1 + test_var_2

# ═══════════════════════════════════════════════════════════════════
@cell 5 30
# This cell uses variables from cells 1 and 3
test_result = test_var_1 * test_var_2 + test_var_3
println(test_result)
")
  (helix.run-shell-command
    (string-append "echo '" content "' > " test-notebook)))

;; Cleanup
(define (cleanup-test-notebook)
  (helix.run-shell-command (string-append "rm -f " test-notebook))
  (displayln "Test notebook cleaned up"))

;; Test: Same kernel is reused for same notebook
(define (test-kernel-reuse)
  (displayln "\n## Testing kernel reuse")

  ;; Get kernel for notebook
  (define kernel1 (kernel-get-for-notebook test-notebook "julia"))
  (define dir1 (hash-get kernel1 'kernel-dir))

  ;; Get kernel again - should be same
  (define kernel2 (kernel-get-for-notebook test-notebook "julia"))
  (define dir2 (hash-get kernel2 'kernel-dir))

  (assert-equal dir1 dir2 "Same notebook should reuse same kernel directory"))

;; Test: Different notebooks get different kernels
(define (test-kernel-isolation)
  (displayln "\n## Testing kernel isolation")

  (define notebook-a "/tmp/notebook-a.jl")
  (define notebook-b "/tmp/notebook-b.jl")

  (define kernel-a (kernel-get-for-notebook notebook-a "julia"))
  (define kernel-b (kernel-get-for-notebook notebook-b "julia"))

  (define dir-a (hash-get kernel-a 'kernel-dir))
  (define dir-b (hash-get kernel-b 'kernel-dir))

  (assert-true (not (equal? dir-a dir-b))
               "Different notebooks should get different kernels")

  ;; Cleanup
  (stop-kernel notebook-a)
  (stop-kernel notebook-b))

;; Test: Variables persist across cell executions
(define (test-variable-persistence)
  (displayln "\n## Testing variable persistence across cells")

  ;; Get kernel
  (define kernel (kernel-get-for-notebook test-notebook "julia"))
  (define kernel-dir (hash-get kernel 'kernel-dir))

  ;; Execute cell 1 - define variables
  (define code1 "test_var_1 = 100\ntest_var_2 = 200")
  (kernel-execute-cell-start kernel-dir 1 code1)

  ;; Poll until done
  (define (wait-for-completion)
    (define result (kernel-poll-result kernel-dir))
    (define status (json-get result "status"))
    (if (equal? status "pending")
        (begin
          (helix.run-shell-command "sleep 0.1")
          (wait-for-completion))
        result))

  (define result1 (wait-for-completion))
  (define status1 (json-get result1 "status"))
  (assert-equal status1 "ok" "Cell 1 should execute successfully")

  ;; Execute cell 3 - use variables from cell 1
  (define code3 "test_var_3 = test_var_1 + test_var_2\ntest_var_3")
  (kernel-execute-cell-start kernel-dir 3 code3)
  (define result3 (wait-for-completion))
  (define status3 (json-get result3 "status"))
  (define error3 (json-get result3 "error"))
  (define output3 (json-get result3 "output_repr"))

  (assert-equal status3 "ok" "Cell 3 should execute successfully")
  (assert-not-contains error3 "UndefVarError" "Cell 3 should not have undefined variable error")
  (assert-contains output3 "300" "Cell 3 should compute 100 + 200 = 300")

  ;; Cleanup
  (stop-kernel test-notebook))

;; Test: Kernel state is tracked correctly
(define (test-kernel-state-tracking)
  (displayln "\n## Testing kernel state tracking")

  ;; Start fresh kernel
  (define kernel (kernel-get-for-notebook test-notebook "julia"))

  ;; Check kernel state has required fields
  (define has-lang (hash-contains? kernel 'lang))
  (define has-dir (hash-contains? kernel 'kernel-dir))
  (define has-ready (hash-contains? kernel 'ready))

  (assert-true has-lang "Kernel state should have 'lang' field")
  (assert-true has-dir "Kernel state should have 'kernel-dir' field")
  (assert-true has-ready "Kernel state should have 'ready' field")

  ;; Check kernel is in *kernels* hash
  (define kernel-exists (hash-try-get *kernels* test-notebook))
  (assert-true kernel-exists "Kernel should be tracked in *kernels* hash")

  ;; Stop kernel
  (stop-kernel test-notebook)

  ;; Check kernel is removed from hash
  (define kernel-removed (not (hash-try-get *kernels* test-notebook)))
  (assert-true kernel-removed "Stopped kernel should be removed from *kernels* hash"))

;; Test: Kernel execution with correct cell indices
(define (test-cell-index-tracking)
  (displayln "\n## Testing cell index tracking")

  ;; Get kernel
  (define kernel (kernel-get-for-notebook test-notebook "julia"))
  (define kernel-dir (hash-get kernel 'kernel-dir))

  ;; Execute cell 5
  (define code5 "println(\"Cell 5 executing\")\n5 * 10")
  (kernel-execute-cell-start kernel-dir 5 code5)

  ;; Wait for result
  (define (wait-for-result)
    (define result (kernel-poll-result kernel-dir))
    (if (equal? (json-get result "status") "pending")
        (begin
          (helix.run-shell-command "sleep 0.1")
          (wait-for-result))
        result))

  (define result (wait-for-result))
  (define status (json-get result "status"))

  (assert-equal status "ok" "Cell 5 should execute successfully")

  ;; Check that response has correct structure
  (define has-output (> (string-length (json-get result "output_repr")) 0))
  (assert-true (or has-output (> (string-length (json-get result "stdout")) 0))
               "Cell 5 should produce output")

  ;; Cleanup
  (stop-kernel test-notebook))

;; Main test runner
(define (run-kernel-persistence-tests)
  (displayln "\n╔════════════════════════════════════════════════════════╗")
  (displayln "║  Kernel Persistence Tests                              ║")
  (displayln "╚════════════════════════════════════════════════════════╝")

  (set! *tests-passed* 0)
  (set! *tests-failed* 0)

  (setup-test-notebook)

  ;; Stop all kernels to start fresh
  (stop-all-kernels)
  (helix.run-shell-command "sleep 1")

  (test-kernel-reuse)
  (test-kernel-isolation)
  (test-variable-persistence)
  (test-kernel-state-tracking)
  (test-cell-index-tracking)

  ;; Cleanup all kernels
  (stop-all-kernels)
  (cleanup-test-notebook)

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
