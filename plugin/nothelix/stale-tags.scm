;;; stale-tags.scm — Feature-probed wrappers over the fork's stale-tag
;;; virtual-row annotation (Phase A, fork-only, no libnothelix FFI change).
;;; Leaf module: requires none of param-tweak.scm/execution.scm, so both can
;;; depend on it without a require cycle.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix.static. "helix/static.scm"))

(provide try-set-stale-tag!
         clear-stale-tag-for-line!
         set-stale-tags-for-lines!)

(define (try-set-stale-tag! line-idx text)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.set-stale-tags-below! ,line-idx ',(list text)))))

(define (clear-stale-tag-for-line! line-idx)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.clear-stale-tags! ,line-idx))))

(define (set-stale-tags-for-lines! stale-lines label)
  (for-each (lambda (ln) (try-set-stale-tag! ln label)) stale-lines))
