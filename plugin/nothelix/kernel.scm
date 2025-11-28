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
                          kernel-stop))

(provide kernel-start
         kernel-get-for-notebook
         get-kernel-id
         stop-kernel
         stop-all-kernels
         *kernels*
         *executing-kernel-dir*)

(define *kernels* (hash))
(define *kernel-id-counter* 1)

;; Track currently executing kernel dir for cancellation
(define *executing-kernel-dir* #f)

(define (get-kernel-id)
  (define id *kernel-id-counter*)
  (set! *kernel-id-counter* (+ *kernel-id-counter* 1))
  id)

(define (kernel-start lang notebook-path)
  ;; Create kernel directory
  (define kernel-dir (string-append "/tmp/helix-kernel-" (number->string (get-kernel-id))))

  ;; Start kernel using Rust FFI (uses kernel/runner.jl)
  (define result-json (kernel-start-macro kernel-dir))

  ;; Parse the JSON result to check for errors
  (when (string-contains? result-json "\"error\"")
    (set-status! (string-append "Kernel failed: " result-json))
    (error result-json))

  ;; File paths match new runner.jl format
  (define input-file (string-append kernel-dir "/input.json"))
  (define output-file (string-append kernel-dir "/output.json"))
  (define pid-file (string-append kernel-dir "/pid"))
  (define ready-file (string-append kernel-dir "/ready"))

  ;; Wait for kernel to be ready (runner.jl creates ready file)
  (helix.run-shell-command "sleep 0.5")

  (define ready-check
    (string-trim (helix.run-shell-command (string-append "[ -f " ready-file " ] && echo 'yes' || echo 'no'"))))

  (when (equal? ready-check "no")
    (define log-contents
      (string-trim (helix.run-shell-command (string-append "tail -10 " kernel-dir "/kernel.log 2>&1 || echo 'No log file'"))))
    (set-status! (string-append "Kernel not ready. Log: " log-contents))
    (error "Kernel startup failed"))

  (define kernel-state
    (hash 'lang lang
          'kernel-dir kernel-dir
          'input-file input-file
          'output-file output-file
          'pid-file pid-file
          'ready #t))

  (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
  (set-status! (string-append "✓ Started " lang " kernel (runner.jl)"))
  kernel-state)

(define (kernel-get-for-notebook notebook-path lang)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing existing (kernel-start lang notebook-path)))

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
(define (stop-all-kernels)
  (define keys (hash-keys->list *kernels*))
  (for-each
    (lambda (notebook-path)
      (define kernel-state (hash-get *kernels* notebook-path))
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (kernel-stop kernel-dir))
    keys)
  (set! *kernels* (hash))
  (set-status! (string-append "✓ Stopped " (number->string (length keys)) " kernel(s)")))
