;;; output-render.scm — Deferred wrappers over the fork's output-lines
;;; virtual-line annotation, so the plugin loads on an hx without it.

(require "helix/editor.scm")
(require "helix/misc.scm")

(provide try-set-output-lines-below!
         try-clear-output-lines-at!
         try-clear-all-output-lines!
         output-lines-ffi-available?)

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
