;;; conceal-state.scm - Versioned conceal overlay cache; overlays apply only when the (doc-id, char-count) fingerprint still matches.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(provide conceal-cache-update!
         conceal-cache-clear!
         conceal-cache-overlays
         conceal-cache-fingerprint
         conceal-cache-doc-id
         conceal-fingerprint-current
         conceal-fingerprint-matches?
         conceal-cache-empty?)

(define *conceal-cache*
  (hash 'overlays '()
        'doc-id #false
        'doc-len 0))

;;@doc
;; Record overlays as the current cache, tagged with the doc fingerprint.
(define (conceal-cache-update! doc-id overlays)
  (define rope (editor->text doc-id))
  (define fp-len (if rope (text.rope-len-chars rope) 0))
  (set! *conceal-cache*
        (hash 'overlays overlays
              'doc-id doc-id
              'doc-len fp-len)))

;;@doc
;; Forget any cached overlays.
(define (conceal-cache-clear!)
  (set! *conceal-cache*
        (hash 'overlays '()
              'doc-id #false
              'doc-len 0)))

;;@doc
;; The cached overlay list, or '() if empty.
(define (conceal-cache-overlays)
  (hash-get *conceal-cache* 'overlays))

;;@doc
;; #true if the cache holds no overlays.
(define (conceal-cache-empty?)
  (null? (hash-get *conceal-cache* 'overlays)))

;;@doc
;; The document id the cached overlays were computed from, or #false.
(define (conceal-cache-doc-id)
  (hash-get *conceal-cache* 'doc-id))

;;@doc
;; The (doc-id, doc-len) fingerprint the cache was tagged with.
(define (conceal-cache-fingerprint)
  (cons (hash-get *conceal-cache* 'doc-id)
        (hash-get *conceal-cache* 'doc-len)))

;;@doc
;; Compute a fingerprint for the currently-focused document.
(define (conceal-fingerprint-current)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (cons doc-id (if rope (text.rope-len-chars rope) 0)))

;;@doc
;; #true if the cached fingerprint is still valid for the given doc fingerprint.
(define (conceal-fingerprint-matches? fp)
  (and (equal? (car fp) (hash-get *conceal-cache* 'doc-id))
       (equal? (cdr fp) (hash-get *conceal-cache* 'doc-len))))
