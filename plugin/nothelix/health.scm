;;; health.scm — In-editor health check + first-focus notification

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

;; Cached issues from the most recent check: each is (id message fix-hint).
(define *health-issues* (list))

(define *health-hint-shown?* #f)

(define (health-issues)
  *health-issues*)

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
;; Re-run the static health check, update the cache, and log the outcome.
(define (run-health-check!)
  (set! *health-issues* (parse-health-tsv (nothelix-health-check-tsv)))
  (cond
    [(null? *health-issues*)
     (log::info! "nothelix-health: all checks pass")]
    [else
     (log::warn! (string-append
                  "nothelix-health: "
                  (number->string (length *health-issues*))
                  " issue(s) detected"))
     (for-each
      (lambda (issue)
        (log::warn! (string-append
                     "nothelix-health: ["
                     (list-ref issue 0)
                     "] "
                     (list-ref issue 1)
                     " — fix: "
                     (list-ref issue 2))))
      *health-issues*)])
  *health-issues*)

(define (format-issue-line issue)
  (string-append "⚠ " (list-ref issue 1) " — " (list-ref issue 2)))

(define (focused-doc-is-notebook?)
  (define path (editor-document->path (editor->doc-id (editor-focus))))
  (and path (string-suffix? path ".jl")))

(define (surface-first-issue!)
  (when (and (not *health-hint-shown?*)
             (not (null? *health-issues*))
             (focused-doc-is-notebook?))
    (set! *health-hint-shown?* #t)
    (define first (car *health-issues*))
    (define base (format-issue-line first))
    (define msg
      (if (> (length *health-issues*) 1)
          (string-append base " (more — :nothelix-status)")
          base))
    (set-status! msg)
    (log::warn!
      (string-append "nothelix-health (surfaced to user): " msg))))

;;@doc
;; Schedule the first-issue surface after a short delay.
(define (install-first-focus-hint!)
  (enqueue-thread-local-callback-with-delay 500
    (lambda () (surface-first-issue!))))

;;@doc
;; Body of the :nothelix-status command: re-run the check and report.
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

(run-health-check!)
(install-first-focus-hint!)

(register-hook! "document-focus-gained"
  (lambda (_doc-id) (surface-first-issue!)))
