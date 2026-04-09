;;; conceal-state.scm - Versioned conceal overlay state
;;;
;;; The conceal overlay cache stores char offsets that are only valid for a
;;; specific document snapshot. Any mutation (typing, execute-cell inserting
;;; output, sync-to-ipynb reloading) shifts those offsets and the cached
;;; overlays become lies. Applying stale overlays replaces random characters
;;; in the buffer — this is the "numerically" → "numeially" bug.
;;;
;;; This module owns the conceal cache and enforces a single rule:
;;;   overlays are only ever applied when their recorded document fingerprint
;;;   matches the current document's fingerprint.
;;;
;;; The fingerprint is (doc-id, char-count). Char count changes on every
;;; insertion or deletion, which is the only way math regions can move.
;;; Within-line replacement doesn't move math region boundaries, so char
;;; count is a sufficient proxy for "did the overlays stay valid?".

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

;; Internal cache record. We use a mutable hash rather than a struct so the
;; module stays small and we don't pay for a new struct definition per update.
(define *conceal-cache*
  (hash 'overlays '()
        'doc-id #false
        'doc-len 0))

;;@doc
;; Record a freshly-computed overlay list as the current cache, tagged with
;; the current document's fingerprint.
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
