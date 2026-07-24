(require "helix/editor.scm")
(require "helix/misc.scm")

(provide nothelix-load)

(define *nothelix-loaded* (box #f))

(define (load-nothelix!)
  (when (not (unbox *nothelix-loaded*))
    (eval '(require "nothelix/main.scm"))
    (set-box! *nothelix-loaded* #t)
    (set-status! "nothelix loaded")))

(define (nothelix-load)
  (load-nothelix!))

(define (focused-path)
  (editor-document->path (editor->doc-id (editor-focus))))

(define (load-if-notebook!)
  (define path (focused-path))
  (when (and path (or (ends-with? path ".jl") (ends-with? path ".ipynb")))
    (load-nothelix!)))

(register-hook! "document-focus-gained"
  (lambda (_doc-id) (load-if-notebook!)))

(enqueue-thread-local-callback-with-delay 200
  (lambda () (load-if-notebook!)))
