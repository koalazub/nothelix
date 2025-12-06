;;; kernel.scm - Kernel lifecycle management

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For set-status!
(require (prefix-in helix. "helix/commands.scm"))

;; FFI imports for kernel functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          find-julia-executable
                          kernel-start-macro
                          kernel-stop
                          kernel-stop-all-processes))

(provide kernel-start
         kernel-get-for-notebook
         stop-kernel
         stop-all-kernels
         *kernels*
         *executing-kernel-dir*)

(define *kernels* (hash))

;; Track currently executing kernel dir for cancellation
(define *executing-kernel-dir* #f)

(define (kernel-start lang notebook-path)
  ;; Use fixed kernel directory (kernel-1) to avoid orphaned processes
  ;; The Rust side will kill any existing process before starting a new one
  (define kernel-dir "/tmp/helix-kernel-1")
  (set-status! (string-append "Starting kernel in " kernel-dir "..."))

  ;; Start kernel using Rust FFI (uses kernel/runner.jl)
  (define result-json (kernel-start-macro kernel-dir))

  ;; Check for error in the JSON result (look for "status":"error" pattern)
  ;; Don't just check for "error" substring as it appears in stack traces too
  (when (string-contains? result-json "\"status\":\"error\"")
    (define error-msg (sanitise-error-message result-json))
    (set-status! (string-append "✗ Kernel failed: " error-msg))
    ;; Return #f instead of throwing - caller should check for this
    #f)

  ;; File paths match new runner.jl format
  (define input-file (string-append kernel-dir "/input.json"))
  (define output-file (string-append kernel-dir "/output.json"))
  (define pid-file (string-append kernel-dir "/pid"))
  (define ready-file (string-append kernel-dir "/ready"))

  ;; Wait for kernel to be ready (runner.jl creates ready file)
  (helix.run-shell-command "sleep 0.5")

  ;; Check if kernel directory was actually created
  (define dir-exists
    (string-trim (helix.run-shell-command (string-append "[ -d " kernel-dir " ] && echo 'yes' || echo 'no'"))))

  (when (equal? dir-exists "no")
    (set-status! (string-append "✗ Kernel dir not created: " kernel-dir))
    #f)

  (define ready-check
    (string-trim (helix.run-shell-command (string-append "[ -f " ready-file " ] && echo 'yes' || echo 'no'"))))

  (when (equal? ready-check "no")
    (define log-contents
      (string-trim (helix.run-shell-command (string-append "tail -3 " kernel-dir "/kernel.log 2>&1 || echo 'No log'"))))
    (set-status! (string-append "✗ Kernel not ready: " (sanitise-error-message log-contents)))
    #f)

  (define kernel-state
    (hash 'lang lang
          'kernel-dir kernel-dir
          'input-file input-file
          'output-file output-file
          'pid-file pid-file
          'ready #t))

  (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
  (set-status! (string-append "✓ Started " lang " kernel in " kernel-dir))
  kernel-state)

;; Get or start a kernel for a notebook
;; Returns kernel-state hash on success, #f on failure
(define (kernel-get-for-notebook notebook-path lang)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing 
      existing 
      (kernel-start lang notebook-path)))

;;@doc
;; Stop kernel for a specific notebook
(define (stop-kernel notebook-path)
  (define kernel-state (hash-try-get *kernels* notebook-path))
  (if (not kernel-state)
      (set-status! "No kernel running for this notebook")
      (let ([kernel-dir (hash-get kernel-state 'kernel-dir)])
        (define result (kernel-stop kernel-dir))
        (if (equal? result "ok")
            (begin
              (set! *kernels* (hash-remove *kernels* notebook-path))
              (set-status! "✓ Kernel stopped"))
            (set-status! result)))))

;;@doc
;; Stop all running kernels (cleanup)
;; This does TWO things:
;; 1. Stop tracked kernels in *kernels* hash
;; 2. Kill ALL orphaned kernel processes (in case of crashes)
(define (stop-all-kernels)
  ;; First, stop all tracked kernels
  (define keys (hash-keys->list *kernels*))
  (for-each
    (lambda (notebook-path)
      (define kernel-state (hash-get *kernels* notebook-path))
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (kernel-stop kernel-dir))
    keys)
  (set! *kernels* (hash))

  ;; Second, aggressively kill any orphaned processes
  (define result (kernel-stop-all-processes))
  (set-status! (string-append "✓ " result)))
