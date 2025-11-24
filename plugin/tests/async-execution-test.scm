#!/usr/bin/env steel

;; Async Execution Test
;; Tests the background polling mechanism
;; Run with: :scm (require "plugins/tests/async-execution-test.scm")

(require "plugins/nothelix.scm")

(displayln "")
(displayln "=== ASYNC EXECUTION TEST ===")
(displayln "")

(define kernel-dir "/tmp/helix-kernel-1")

(displayln "Step 1: Starting async execution...")
(define code "x = [1, 2, 3]; println(x)")
(define start-result (kernel-execute-start kernel-dir code))
(displayln (string-append "  Start result: " start-result))

(if (string-starts-with? start-result "ERROR:")
    (begin
      (displayln "✗ FAILED: Could not start execution")
      (displayln start-result))
    (begin
      (displayln "✓ Execution started")

      (displayln "")
      (displayln "Step 2: Polling for completion (will try 10 times)...")
      (define (poll-loop count max-polls)
        (when (< count max-polls)
          (define status-json (kernel-execution-status kernel-dir))
          (define status (json-get-string status-json "status"))
          (displayln (string-append "  Poll " (number->string count) ": status=" (if status status "NULL")))

          (cond
            [(equal? status "done")
             (displayln "✓ Execution completed!")
             (displayln "")
             (displayln "Step 3: Reading output...")
             (define output-json (read-kernel-output kernel-dir))
             (define output-text (json-get-string output-json "text"))
             (displayln (string-append "  Output: " (if output-text output-text "NULL")))
             (displayln "")
             (displayln "=== TEST PASSED ===")]

            [(equal? status "error")
             (define err-msg (json-get-string status-json "message"))
             (displayln (string-append "✗ Execution error: " (if err-msg err-msg "Unknown")))
             (displayln "")
             (displayln "=== TEST FAILED ===")]

            [else
             ;; Still running - wait and poll again
             (helix.run-shell-command "sleep 0.1")
             (poll-loop (+ count 1) max-polls)])))

      (poll-loop 0 10)

      (displayln "")
      (displayln "Step 4: Testing background thread callback...")
      (displayln "  Spawning thread with hx.with-context...")

      (define callback-fired #f)
      (define callback-error #f)

      (with-handler
        (lambda (e)
          (set! callback-error (error-object-message e))
          (displayln (string-append "  ✗ Thread error: " callback-error)))

        (spawn-native-thread
          (lambda ()
            (displayln "  [Thread] Sleeping 0.1s...")
            (helix.run-shell-command "sleep 0.1")
            (displayln "  [Thread] Calling hx.with-context...")
            (with-handler
              (lambda (e)
                (displayln (string-append "  [Thread] ✗ hx.with-context error: " (error-object-message e))))
              (hx.with-context
                (lambda ()
                  (displayln "  [Thread] ✓ Inside hx.with-context callback!")
                  (set! callback-fired #t))))))

        (displayln "  Main thread: waiting for callback...")
        (helix.run-shell-command "sleep 0.5")

        (if callback-fired
            (displayln "  ✓ Background callback worked!")
            (if callback-error
                (displayln (string-append "  ✗ Background callback failed: " callback-error))
                (displayln "  ✗ Background callback did not fire (no error)")))
        )))

(displayln "")
(displayln "=== END TEST ===")
(displayln "")
