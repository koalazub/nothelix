;;; debug.scm — Opt-in debug logging for nothelix

(require "helix/misc.scm")

(provide *nothelix-debug*
         nothelix-debug?
         nothelix-debug-enable!
         nothelix-debug-disable!
         nothelix-debug-toggle!
         debug-log)

(define *nothelix-debug* #false)

(define (nothelix-debug?) *nothelix-debug*)

(define (nothelix-debug-enable!)
  (set! *nothelix-debug* #true)
  (log::info! "nothelix.debug: enabled")
  (set-status! "nothelix debug: on"))

(define (nothelix-debug-disable!)
  (log::info! "nothelix.debug: disabled")
  (set! *nothelix-debug* #false)
  (set-status! "nothelix debug: off"))

(define (nothelix-debug-toggle!)
  (if *nothelix-debug*
      (nothelix-debug-disable!)
      (nothelix-debug-enable!)))

;;@doc
;; Emit message to the helix log under nothelix's namespace when debug is on.
(define (debug-log message)
  (when *nothelix-debug*
    (log::info! (string-append "nothelix: " message))))
