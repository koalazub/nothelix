;;; debug.scm - Opt-in debug logging for nothelix
;;;
;;; Provides a single global flag and a small helper so the rest of
;;; the plugin can emit debug lines without spamming the helix log
;;; file on every normal operation.
;;;
;;; Usage:
;;;
;;;   (require "nothelix/debug.scm")
;;;   (debug-log (string-append "cell=" (number->string idx)))
;;;
;;; The `log::info!` call only happens when `*nothelix-debug*` is #t;
;;; otherwise `debug-log` is a cheap no-op. Toggle at runtime via
;;; `:nothelix-debug-on`, `:nothelix-debug-off`, or
;;; `:nothelix-debug-toggle` (registered in the plugin entrypoint),
;;; or from another module with `(nothelix-debug-enable!)` /
;;; `(nothelix-debug-disable!)`.
;;;
;;; Debug lines land in the same file as all other helix logs
;;; (`~/.cache/helix/helix.log`). To actually see them you still need
;;; helix running with `-v` (info) or higher; helix's default verbosity
;;; is `warn`.

(require "helix/misc.scm")

(provide *nothelix-debug*
         nothelix-debug?
         nothelix-debug-enable!
         nothelix-debug-disable!
         nothelix-debug-toggle!
         debug-log)

;; Single-cell-state flag. Mutated via `set!` from the enable/disable
;; helpers below. Defaults off so installs don't fill the log file on
;; every cell execution — flip on ad hoc via `:nothelix-debug-on`
;; when chasing down an image-rendering or conceal bug.
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
;; Emit `message` to the helix log under nothelix's namespace, but
;; only when the debug flag is on. Accepts a single string so callers
;; compose their message with `string-append`/`number->string` at the
;; call site — this mirrors how `log::info!` is shaped on the Rust
;; side and keeps the helper free of format-string parsing.
(define (debug-log message)
  (when *nothelix-debug*
    (log::info! (string-append "nothelix: " message))))
