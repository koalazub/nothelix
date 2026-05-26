;;; health.scm — in-editor health check + first-focus notification.
;;;
;;; On plugin load we call the libnothelix `nothelix-health-check-tsv`
;;; FFI, which runs cheap static checks (dylib presence, BUILD_ID
;;; match, plugin cogs presence, fork-symbol probe of hx-nothelix).
;;; The result is cached in *health-issues*; a delayed callback fires
;;; ~500ms after load to surface the first issue via `set-status!` so
;;; the user sees the diagnostic instead of debugging silent
;;; degradation by hand.
;;;
;;; A `:nothelix-status` typable command (registered in nothelix.scm
;;; via the top-level `nothelix-status` define) dumps the full list at
;;; any time and re-runs the check, so the user can verify a fix
;;; without restarting Helix.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require "string-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          nothelix-health-check-tsv))

(provide health-issues
         run-health-check!
         install-first-focus-hint!
         nothelix-status-command)

;; Cached list of issues from the most recent check.
;; Each element is a list: (id message fix-hint).
(define *health-issues* (list))

;; Set after the first-focus hint has been surfaced so we don't spam
;; the status line on every buffer switch.
(define *health-hint-shown?* #f)

(define (health-issues)
  *health-issues*)

;; Parse the TSV blob the FFI returns. Empty string ⇒ empty list.
;; Otherwise each newline-separated line becomes a 3-element list.
;; Lines with fewer than 3 fields are skipped defensively so a future
;; FFI extension doesn't crash older plugin copies.
(define (parse-health-tsv tsv)
  (cond
    [(or (not tsv) (equal? tsv "")) (list)]
    [else
     (define lines (string-split tsv "\n"))
     (define non-empty
       (filter (lambda (s) (not (equal? s ""))) lines))
     (define parsed
       (map (lambda (line) (string-split line "\t")) non-empty))
     (filter (lambda (parts) (>= (length parts) 3)) parsed)]))

;;@doc
;; Re-run the static health check and update the cache. Returns the
;; list of issues for callers that want the structured form.
(define (run-health-check!)
  (set! *health-issues* (parse-health-tsv (nothelix-health-check-tsv)))
  *health-issues*)

;; Format a single issue for the status line. Issue is
;; (id message fix-hint).
(define (format-issue-line issue)
  (string-append "⚠ " (list-ref issue 1) " — " (list-ref issue 2)))

;; Show the first issue with a hint pointing at :nothelix-status for
;; more. No-op when the cache is empty (healthy install) or when the
;; hint has already been shown this session.
(define (surface-first-issue!)
  (when (and (not *health-hint-shown?*)
             (not (null? *health-issues*)))
    (set! *health-hint-shown?* #t)
    (define first (car *health-issues*))
    (define base (format-issue-line first))
    (define msg
      (if (> (length *health-issues*) 1)
          (string-append base " (more — :nothelix-status)")
          base))
    (set-status! msg)))

;;@doc
;; Schedule the first-issue surface after a short delay so Helix has
;; finished its initial draw and the status line is visible to the
;; user. Subsequent calls are cheap no-ops via *health-hint-shown?*.
(define (install-first-focus-hint!)
  (enqueue-thread-local-callback-with-delay 500
    (lambda () (surface-first-issue!))))

;;@doc
;; Body of the `:nothelix-status` typable command. Re-runs the check
;; (so the user can verify a fix mid-session), then prints either an
;; "all clear" line or the full set of issues joined by " | ". Resets
;; *health-hint-shown?* so a subsequent failure can re-surface on its
;; own.
(define (nothelix-status-command)
  (run-health-check!)
  (set! *health-hint-shown?* #f)
  (cond
    [(null? *health-issues*)
     (set-status! "nothelix: all health checks pass")]
    [else
     (define lines
       (map (lambda (issue)
              (string-append (list-ref issue 0)
                             ": "
                             (list-ref issue 1)
                             " (fix: "
                             (list-ref issue 2)
                             ")"))
            *health-issues*))
     (set-status! (string-join lines " | "))]))

;; Run the check at module load + schedule the surface.
(run-health-check!)
(install-first-focus-hint!)
