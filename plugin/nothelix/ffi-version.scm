;;; ffi-version.scm — libnothelix ↔ plugin FFI version handshake
;;;
;;; Required FIRST from nothelix.scm so a stale dylib hard-fails here
;;; with an actionable message, ahead of any other module's symbol imports.

(require "helix/misc.scm")

;; Must equal NOTHELIX_FFI_VERSION in libnothelix/src/lib.rs. The
;; ffi-version-mismatch doctor in health.rs scans for this exact
;; `(define EXPECTED-FFI-VERSION <n>)` shape — keep it.
(define EXPECTED-FFI-VERSION 27)

;; Probe at runtime: %#maybe-module-get returns #false for a missing
;; symbol (unlike %module-get%, which panics); any other failure → v0.
(define (installed-ffi-version)
  (define raw
    (with-handler
      (lambda (_) 0)
      (eval '(let ((probe (%#maybe-module-get (#%get-dylib "libnothelix")
                                              (quote nothelix-ffi-version))))
               (if probe (probe) 0)))))
  (if (number? raw) raw 0))

(define (assert-ffi-version!)
  (define got (installed-ffi-version))
  (when (not (equal? got EXPECTED-FFI-VERSION))
    ;; Keep this message in lockstep with health.rs's ffi-version-mismatch.
    (define msg (string-append "libnothelix FFI v" (number->string got)
                               ", plugin expects v"
                               (number->string EXPECTED-FFI-VERSION)
                               " — run just install"))
    ;; Status first — the raised error may be swallowed; the status line survives.
    (set-status! msg)
    (error msg)))

(assert-ffi-version!)
