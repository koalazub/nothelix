;;; kernel.scm — Kernel lifecycle management

(require "string-utils.scm")
(require "project-config.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kernel-start-macro
                          kernel-adopt-macro
                          kernel-stop
                          kernel-stop-all-processes
                          path-exists
                          read-file-tail))

(provide kernel-start
         kernel-get-for-notebook
         poll-kernel-ready
         stop-kernel
         stop-all-kernels
         *kernels*
         *executing-kernel-dir*)

;; Hash of notebook-path -> kernel-state for all running kernels.
(define *kernels* (hash))

;; Kernel directory of the currently executing cell, or #false.
(define *executing-kernel-dir* #false)

;; djb2 over the notebook path — same algorithm as the image-id hashes.
(define (kernel-path-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

;; Per-notebook runtime dir. A single shared dir let a second notebook's
;; kernel-start SIGTERM the first notebook's kernel and wipe its IPC files
;; (kernel_start_macro clears pid/ready/input.json/output.json on start), so
;; derive a stable, path-unique directory instead.
(define (kernel-dir-for notebook-path)
  (string-append "/tmp/helix-kernel-"
                 (number->string (kernel-path-hash (or notebook-path "scratch")))))

;;@doc
;; Start a kernel for `lang`/`notebook-path`, poll for readiness, then call `on-ready`; returns #true if the spawn began.
(define (kernel-start lang notebook-path on-ready)
  (define kernel-dir (kernel-dir-for notebook-path))
  (cond
    [(string-contains? (kernel-adopt-macro kernel-dir) "\"status\":\"ok\"")
     (define kernel-state
       (hash 'lang lang
             'kernel-dir kernel-dir
             'input-file (string-append kernel-dir "/input.json")
             'output-file (string-append kernel-dir "/output.json")
             'pid-file (string-append kernel-dir "/pid")
             'ready #true))
     (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
     (set-status! (string-append "Reattached to running " lang " kernel — session state preserved"))
     (on-ready kernel-state)
     #true]
    [else (kernel-start-fresh lang notebook-path kernel-dir on-ready)]))

(define (kernel-start-fresh lang notebook-path kernel-dir on-ready)
  ;; (julia-bin . julia-project) — empty strings unless this notebook's project
  ;; is trusted; the macro then falls back to PATH julia + default env.
  (define runtime (project-runtime-for notebook-path))
  (set-status! (string-append "Starting kernel in " kernel-dir "..."))

  ;; Pass the notebook file so the kernel runs in its directory — relative
  ;; paths in cells (data files, includes) resolve next to the notebook.
  (define result-json
    (kernel-start-macro kernel-dir (car runtime) (cdr runtime) (or notebook-path "")))

  (cond
    [(string-contains? result-json "julia not found")
     (set-status! "Julia not found. Install Julia (https://julialang.org) and make sure it is on your PATH.")
     #false]

    [(string-contains? result-json "\"status\":\"error\"")
     (set-status! (string-append "Kernel failed to start: " (sanitise-error-message result-json)))
     #false]

    [(equal? (path-exists kernel-dir) "no")
     (set-status! (string-append "Kernel directory was not created at " kernel-dir ". Check file permissions."))
     #false]

    [else
     ;; 150 attempts × 200 ms = 30 s max wait.
     (poll-kernel-ready kernel-dir lang notebook-path on-ready 150)
     #true]))

(define (poll-kernel-ready kernel-dir lang notebook-path on-ready attempts)
  (cond
    [(equal? (path-exists (string-append kernel-dir "/ready")) "yes")
     (define kernel-state
       (hash 'lang lang
             'kernel-dir kernel-dir
             'input-file (string-append kernel-dir "/input.json")
             'output-file (string-append kernel-dir "/output.json")
             'pid-file (string-append kernel-dir "/pid")
             'ready #true))

     (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
     (set-status! (string-append "Started " lang " kernel in " kernel-dir))
     (on-ready kernel-state)]

    [(<= attempts 0)
     (define log-tail (read-file-tail (string-append kernel-dir "/kernel.log") 3))
     (define msg (sanitise-error-message log-tail))
     (if (> (string-length msg) 0)
         (set-status! (string-append "Kernel not ready after 30 s. Julia output: " msg))
         (set-status! (string-append "Kernel not ready after 30 s. Check kernel.log in " kernel-dir "/ for details.")))
     (helix.redraw)]

    [else
     (enqueue-thread-local-callback-with-delay 200
       (lambda () (poll-kernel-ready kernel-dir lang notebook-path on-ready (- attempts 1))))]))

;;@doc
;; Get the existing kernel for `notebook-path` or start one, then call `on-ready` with the kernel-state.
(define (kernel-get-for-notebook notebook-path lang on-ready)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing
      (on-ready existing)
      (kernel-start lang notebook-path on-ready)))

;;@doc
;; Stop the kernel for a specific notebook path.
(define (stop-kernel notebook-path)
  (define kernel-state (hash-try-get *kernels* notebook-path))
  (if (not kernel-state)
      (set-status! "No kernel running for this notebook")
      (let ([kernel-dir (hash-get kernel-state 'kernel-dir)])
        (define result (kernel-stop kernel-dir))
        (if (equal? result "ok")
            (begin
              (set! *kernels* (hash-remove *kernels* notebook-path))
              (set-status! "Kernel stopped"))
            (set-status! result)))))

;;@doc
;; Stop all tracked kernels, then kill any orphaned runner.jl processes.
(define (stop-all-kernels)
  (define keys (hash-keys->list *kernels*))
  (for-each
    (lambda (notebook-path)
      (define kernel-state (hash-get *kernels* notebook-path))
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (kernel-stop kernel-dir))
    keys)
  (set! *kernels* (hash))
  (define result (kernel-stop-all-processes))
  (set-status! "All kernels stopped"))
