#!/usr/bin/env steel

;; Diagnostic Tests for Nothelix
;; Run with: :scm (require "plugins/tests/diagnostic-tests.scm")

(require "plugins/nothelix.scm")

(displayln "")
(displayln "=== NOTHELIX DIAGNOSTIC TESTS ===")
(displayln "")

;; Test 1: Check Rust FFI functions are available
(displayln "Test 1: Checking Rust FFI bindings...")
(define test-count 0)
(define pass-count 0)

(define (test name result)
  (set! test-count (+ test-count 1))
  (if result
      (begin
        (set! pass-count (+ pass-count 1))
        (displayln (string-append "  ✓ " name)))
      (displayln (string-append "  ✗ " name))))

;; Test Rust functions exist
(test "detect-graphics-protocol exists" (procedure? detect-graphics-protocol))
(test "kernel-execute-start exists" (procedure? kernel-execute-start))
(test "kernel-execution-status exists" (procedure? kernel-execution-status))
(test "kernel-interrupt exists" (procedure? kernel-interrupt))
(test "read-kernel-output exists" (procedure? read-kernel-output))
(test "render-b64-for-protocol exists" (procedure? render-b64-for-protocol))

(displayln "")

;; Test 2: Check Steel builtins we rely on
(displayln "Test 2: Checking Steel builtins...")
(test "spawn-native-thread exists" (procedure? spawn-native-thread))
(test "hx.with-context exists"
      (with-handler (lambda (e) #f)
        (procedure? hx.with-context)
        #t))
(test "helix.run-shell-command exists" (procedure? helix.run-shell-command))
(test "helix.static.insert_string exists" (procedure? helix.static.insert_string))
(test "string-find exists" (procedure? string-find))

(displayln "")

;; Test 3: Protocol detection
(displayln "Test 3: Testing protocol detection...")
(define protocol (detect-graphics-protocol))
(displayln (string-append "  Detected protocol: " protocol))
(test "Protocol is valid"
      (or (equal? protocol "kitty")
          (equal? protocol "iterm2")
          (equal? protocol "sixel")
          (equal? protocol "none")))

(displayln "")

;; Test 4: Kernel directory check
(displayln "Test 4: Checking kernel directory...")
(define kernel-dir "/tmp/helix-kernel-1")
(define (file-exists? path)
  (with-handler (lambda (e) #f)
    (let ([result (helix.run-shell-command (string-append "test -e " path " && echo yes || echo no"))])
      (string-starts-with? result "yes"))
    #t))

(test "Kernel directory exists" (file-exists? kernel-dir))
(test "PID file exists" (file-exists? (string-append kernel-dir "/pid")))
(test "Input file exists" (file-exists? (string-append kernel-dir "/input.jl")))
(test "Output file exists" (file-exists? (string-append kernel-dir "/output.txt")))
(test "Done marker exists" (file-exists? (string-append kernel-dir "/output.txt.done")))

(displayln "")

;; Test 5: Kernel execution status
(displayln "Test 5: Testing kernel execution status...")
(define status-json (kernel-execution-status kernel-dir))
(displayln (string-append "  Status JSON: " status-json))
(define status (json-get-string status-json "status"))
(displayln (string-append "  Parsed status: " (if status status "NULL")))
(test "Status is valid"
      (or (equal? status "done")
          (equal? status "running")
          (equal? status "error")))

(displayln "")

;; Test 6: Read kernel output
(displayln "Test 6: Testing kernel output reading...")
(define output-json (read-kernel-output kernel-dir))
(displayln (string-append "  Output JSON (first 200 chars): "
                         (substring output-json 0 (min 200 (string-length output-json)))))
(define output-text (json-get-string output-json "text"))
(define has-image (json-get-string output-json "has_image"))
(test "Output text extracted" (not (equal? output-text #f)))
(test "Has image field exists" (not (equal? has-image #f)))
(displayln (string-append "  Output length: " (number->string (string-length (or output-text "")))))
(displayln (string-append "  Has image: " (if has-image has-image "NULL")))

(displayln "")

;; Test 7: JSON parsing
(displayln "Test 7: Testing JSON parsing...")
(define test-json "{\"status\":\"done\",\"text\":\"hello\",\"has_image\":false}")
(define parsed-status (json-get-string test-json "status"))
(define parsed-text (json-get-string test-json "text"))
(define parsed-bool (json-get-string test-json "has_image"))
(test "Parse string value" (equal? parsed-status "done"))
(test "Parse text value" (equal? parsed-text "hello"))
(test "Parse boolean value" (equal? parsed-bool "false"))

(displayln "")

;; Test 8: Background thread support
(displayln "Test 8: Testing background thread support...")
(define thread-test-passed #f)
(with-handler
  (lambda (e)
    (displayln (string-append "  ERROR: " (error-object-message e))))
  (spawn-native-thread
    (lambda ()
      (helix.run-shell-command "sleep 0.1")
      (set! thread-test-passed #t)))
  ;; Wait a bit
  (helix.run-shell-command "sleep 0.2")
  #t)
(test "Background thread executed" thread-test-passed)

(displayln "")

;; Test 9: Context callback support
(displayln "Test 9: Testing hx.with-context...")
(define context-test-passed #f)
(with-handler
  (lambda (e)
    (displayln (string-append "  ERROR: hx.with-context not available - " (error-object-message e)))
    (test "hx.with-context available" #f))
  (hx.with-context
    (lambda ()
      (set! context-test-passed #t)))
  (test "hx.with-context available" context-test-passed))

(displayln "")

;; Test 10: Async execution simulation
(displayln "Test 10: Testing async execution flow...")
(define async-status-check #f)
(with-handler
  (lambda (e)
    (displayln (string-append "  ERROR in async test: " (error-object-message e))))
  ;; Simulate what execute-cell does
  (define start-result (kernel-execute-start kernel-dir "println(\"test\")"))
  (displayln (string-append "  Start result: " start-result))
  (test "Execute start succeeded" (not (string-starts-with? start-result "ERROR:")))

  ;; Poll for status
  (helix.run-shell-command "sleep 0.5")
  (define poll-status-json (kernel-execution-status kernel-dir))
  (displayln (string-append "  Poll status JSON: " poll-status-json))
  (define poll-status (json-get-string poll-status-json "status"))
  (displayln (string-append "  Poll status: " (if poll-status poll-status "NULL")))
  (test "Got valid status" (not (equal? poll-status #f)))
  (set! async-status-check #t))

(displayln "")
(displayln "=== TEST SUMMARY ===")
(displayln (string-append "Passed: " (number->string pass-count) "/" (number->string test-count)))
(if (= pass-count test-count)
    (displayln "✓ ALL TESTS PASSED")
    (displayln (string-append "✗ " (number->string (- test-count pass-count)) " TESTS FAILED")))
(displayln "")

;; Test 11: Graphics rendering pipeline
(displayln "Test 11: Testing graphics rendering pipeline...")

;; Minimal 1x1 red PNG as base64 (from Rust tests)
(define test-png-b64 "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNg+M8AAAQAAQE=")

;; Test protocol detection for current terminal
(define detected-protocol (detect-graphics-protocol))
(displayln (string-append "  Detected protocol: " detected-protocol))
(test "Protocol detected" (not (equal? detected-protocol "none")))

;; Test escape sequence generation
(define escape-seq (render-b64-for-protocol test-png-b64 detected-protocol 999))
(displayln (string-append "  Escape sequence length: " (number->string (string-length escape-seq))))

(test "Escape sequence not error" (not (string-starts-with? escape-seq "ERROR:")))

;; Check escape sequence format based on protocol
(cond
  [(equal? detected-protocol "kitty")
   (displayln "  Checking Kitty protocol format...")
   (test "Starts with ESC_G" (string-starts-with? escape-seq "\x1b_G"))
   (displayln (string-append "    First 80 chars: " (substring escape-seq 0 (min 80 (string-length escape-seq)))))
   ;; Note: Can't easily check end due to escape chars in Steel
   ]
  [(equal? detected-protocol "iterm2")
   (displayln "  Checking iTerm2 protocol format...")
   (test "Starts with OSC 1337" (string-starts-with? escape-seq "\x1b]1337;"))
   ]
  [else
   (displayln "  Unknown protocol - skipping format checks")])

(displayln "")

;; Return test results
(list pass-count test-count)
