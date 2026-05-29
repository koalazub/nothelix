;;; ffi-version.scm - libnothelix ↔ plugin FFI version handshake.
;;;
;;; The plugin .scm files are live-linked from the repo while the dylib is
;;; a copied artifact, so a forgotten `just install` skews the two. Refuse
;;; to load against a mismatched dylib instead of degrading into
;;; half-broken commands.
;;;
;;; This module is required FIRST from nothelix.scm and provides nothing:
;;; Steel executes required modules before any body form of the requiring
;;; module (verified empirically — a mid-file require's body runs before
;;; the entry's first body form), so the load-time assert here is the
;;; earliest point in the plugin load, ahead of every other module's
;;; dylib symbol imports. A stale dylib therefore fails right here with
;;; an actionable message instead of panicking on a missing symbol
;;; import or silently misbehaving on changed semantics.

(require "helix/misc.scm")

;; Must equal NOTHELIX_FFI_VERSION in libnothelix/src/lib.rs (the bump
;; rule lives on that constant's doc comment). The `ffi-version-mismatch`
;; doctor check in libnothelix/src/health.rs scans this file — keep the
;; exact `(define EXPECTED-FFI-VERSION <n>)` shape.
(define EXPECTED-FFI-VERSION 1)

;; A dylib that predates the handshake doesn't export
;; `nothelix-ffi-version` at all, so naming it in an (only-in ...) import
;; would abort the load with an unhelpful free-identifier error before we
;; could explain the problem. Instead probe the dylib module at runtime:
;; `%#maybe-module-get` returns #false for a missing symbol (unlike
;; `%module-get%`, which panics), and the eval + with-handler wrapper —
;; same idiom as try-add-or-replace-… in animation.scm — maps any other
;; failure mode (including a missing dylib) to v0, which then hard-fails
;; below with the actionable message.
(define (installed-ffi-version)
  (with-handler
    (lambda (_) 0)
    (eval '(let ((probe (%#maybe-module-get (#%get-dylib "libnothelix")
                                            (quote nothelix-ffi-version))))
             (if probe (probe) 0)))))

(define (assert-ffi-version!)
  (define got (installed-ffi-version))
  (when (not (equal? got EXPECTED-FFI-VERSION))
    ;; Same sentence the ffi-version-mismatch doctor issue builds in
    ;; libnothelix/src/health.rs — keep the template in lockstep.
    (define msg (string-append "libnothelix FFI v" (number->string got)
                               ", plugin expects v"
                               (number->string EXPECTED-FFI-VERSION)
                               " — run just install"))
    ;; Status first: the raised error stops the plugin load, but module
    ;; loaders can swallow the error text; the status line survives.
    (set-status! msg)
    (error msg)))

(assert-ffi-version!)
