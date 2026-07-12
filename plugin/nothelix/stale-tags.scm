;;; stale-tags.scm — Feature-probed wrappers over the fork's stale-tag
;;; virtual-row annotation (Phase A, fork-only, no libnothelix FFI change).
;;; Leaf module: requires none of param-tweak.scm/execution.scm, so both can
;;; depend on it without a require cycle.

(require "helix/editor.scm")
(require "helix/misc.scm")

(provide try-set-stale-tag!
         clear-stale-tag-for-line!
         set-stale-tags-for-lines!)

(define (try-set-stale-tag! line-idx text)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.set-stale-tags-below! *helix.cx* ,line-idx ',(list text))))
    #true))

(define (clear-stale-tag-for-line! line-idx)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.clear-stale-tags! *helix.cx* ,line-idx)))
    #true))

(define (set-stale-tags-for-lines! stale-lines label)
  (for-each (lambda (ln) (try-set-stale-tag! ln label)) stale-lines))
