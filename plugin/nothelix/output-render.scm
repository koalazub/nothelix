;;; output-render.scm — Deferred wrappers over the fork's output-lines
;;; virtual-line annotation, so the plugin loads on an hx without it.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix.static. "helix/static.scm"))

(provide try-set-output-lines-below!
         try-clear-output-lines-at!
         try-clear-all-output-lines!
         output-lines-ffi-available?
         try-commit-output-changes!)

(define (try-set-output-lines-below! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.set-output-lines-below! ,line-idx ',lines))))

(define (try-clear-output-lines-at! line-idx)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.clear-output-lines-at! ,line-idx))))

(define (try-clear-all-output-lines!)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-output-lines!))))

(define (output-lines-ffi-available?)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-output-lines!))
    #true))

;;@doc
;; Commit pending buffer changes as a tagged `output` revision (skipped by
;; user undo/redo) when the fork binding exists; falls back to a plain
;; commit on an hx without it, so behavior matches today exactly.
(define (try-commit-output-changes!)
  (with-handler
    (lambda (_) (helix.static.commit-changes-to-history))
    (eval '(helix.static.commit-output-changes-to-history!))))
