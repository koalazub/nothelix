;;; kernel.scm - Kernel lifecycle management
;;;
;;; Manages Julia kernel processes.  Each notebook gets at most one kernel,
;;; tracked in the `*kernels*` hash keyed by notebook path.  The Rust FFI
;;; handles the actual process spawning and IPC via kernel-start-macro /
;;; kernel-stop / kernel-stop-all-processes.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

;; FFI imports for kernel functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kernel-start-macro
                          kernel-stop
                          kernel-stop-all-processes
                          sleep-ms
                          path-exists
                          read-file-tail))

(provide kernel-start
         kernel-get-for-notebook
         stop-kernel
         stop-all-kernels
         *kernels*
         *executing-kernel-dir*)

;; Hash of notebook-path -> kernel-state for all running kernels.
(define *kernels* (hash))

;; The kernel directory of the currently executing cell, or #false.
;; Used by `cancel-cell` to know which process to SIGINT.
(define *executing-kernel-dir* #false)

;;@doc
;; Start a new kernel for the given language and notebook path.
;; Returns a kernel-state hash on success, or #false on failure (with a
;; status message shown to the user).
(define (kernel-start lang notebook-path)
  ;; Fixed directory avoids orphaned processes across restarts.
  ;; The Rust side SIGTERMs any existing process before spawning.
  (define kernel-dir "/tmp/helix-kernel-1")
  (set-status! (string-append "Starting kernel in " kernel-dir "..."))

  (define result-json (kernel-start-macro kernel-dir))

  ;; Each check returns #false on failure; success falls through to the end.
  (cond
    [(string-contains? result-json "julia not found")
     (set-status! "Julia not found. Install Julia (https://julialang.org) and make sure it is on your PATH.")
     #false]

    [(string-contains? result-json "\"status\":\"error\"")
     (set-status! (string-append "Kernel failed to start: " (sanitise-error-message result-json)))
     #false]

    [else
     ;; Poll for the ready file instead of a fixed sleep.  Julia startup
     ;; time varies widely (500 ms to several seconds on first load), so a
     ;; single sleep is unreliable.  We check every 200 ms for up to 30 s.
     (define (wait-for-ready attempts)
       (cond
         [(equal? (path-exists (string-append kernel-dir "/ready")) "yes")
          ;; Kernel is up.
          (define kernel-state
            (hash 'lang lang
                  'kernel-dir kernel-dir
                  'input-file (string-append kernel-dir "/input.json")
                  'output-file (string-append kernel-dir "/output.json")
                  'pid-file (string-append kernel-dir "/pid")
                  'ready #true))

          (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
          (set-status! (string-append "Started " lang " kernel in " kernel-dir))
          kernel-state]

         [(<= attempts 0)
          ;; Timed out — show whatever the kernel log says.
          (define log-tail (read-file-tail (string-append kernel-dir "/kernel.log") 3))
          (define msg (sanitise-error-message log-tail))
          (if (> (string-length msg) 0)
              (set-status! (string-append "Kernel not ready after 30 s. Julia output: " msg))
              (set-status! "Kernel not ready after 30 s. Check kernel.log in /tmp/helix-kernel-1/ for details."))
          #false]

         [else
          (sleep-ms 200)
          (wait-for-ready (- attempts 1))]))

     (cond
       [(equal? (path-exists kernel-dir) "no")
        (set-status! (string-append "Kernel directory was not created at " kernel-dir ". Check file permissions."))
        #false]

       [else
        ;; 150 attempts * 200 ms = 30 s max wait
        (wait-for-ready 150)])]))

;;@doc
;; Get or start a kernel for a notebook.
;; Returns the existing kernel-state if one is already running, otherwise
;; starts a new one.  Returns #false if the kernel fails to start.
(define (kernel-get-for-notebook notebook-path lang)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing
      existing
      (kernel-start lang notebook-path)))

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
;; Stop all running kernels.
;; First stops every tracked kernel, then kills any orphaned runner.jl processes.
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
