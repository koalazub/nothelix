;;; markdown-render.scm — In-buffer markdown rendering for @markdown cells

(require "conceal.scm")
(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(#%require-dylib "libnothelix"
                 (only-in nothelix scan-markdown-overlays))

(provide markdown-render-enable!)

(define *md-cache* (box #false))

;; Deferred so the plugin loads on an older hx lacking set-style-overlays!.
(define (try-set-style-overlays! spans)
  (with-handler
    (lambda (_) #false)
    (eval `(set-style-overlays! (quote ,spans)))))

(define (line-text rope i)
  (text.rope->string (text.rope->line rope i)))

(define (next-cell-marker rope total j)
  (cond
    [(>= j total) total]
    [(cell-marker? (line-text rope j)) j]
    [else (next-cell-marker rope total (+ j 1))]))

(define (markdown-cell-ranges rope total)
  (let loop ([i 0] [acc '()])
    (if (>= i total)
        (reverse acc)
        (if (string-starts-with? (line-text rope i) "@markdown")
            (let ([end (next-cell-marker rope total (+ i 1))])
              (loop end (cons (cons (+ i 1) end) acc)))
            (loop (+ i 1) acc)))))

(define (cell-substring rope s e)
  (let loop ([i s] [acc ""])
    (if (>= i e) acc (loop (+ i 1) (string-append acc (line-text rope i))))))

(define (field lst n)
  (cond
    [(null? lst) ""]
    [(= n 0) (car lst)]
    [else (field (cdr lst) (- n 1))]))

(define (parse-md-scan tsv)
  (let loop ([lines (string-split tsv "\n")] [markers '()] [styles '()])
    (if (null? lines)
        (cons (reverse markers) (reverse styles))
        (let ([line (car lines)])
          (if (equal? line "")
              (loop (cdr lines) markers styles)
              (let ([f (string-split line "\t")])
                (cond
                  [(equal? (field f 0) "O")
                   (loop (cdr lines)
                         (cons (cons (string->number (field f 1)) (field f 2)) markers)
                         styles)]
                  [(equal? (field f 0) "S")
                   (loop (cdr lines)
                         markers
                         (cons (list (string->number (field f 1))
                                     (string->number (field f 2))
                                     (field f 3))
                               styles))]
                  [else (loop (cdr lines) markers styles)])))))))

(define (scan-all-markdown rope total)
  (let loop ([ranges (markdown-cell-ranges rope total)] [markers '()] [styles '()])
    (if (null? ranges)
        (cons markers styles)
        (let* ([r (car ranges)]
               [base (text.rope-line->char rope (car r))]
               [text (cell-substring rope (car r) (cdr r))]
               [parsed (parse-md-scan (scan-markdown-overlays text base))])
          (loop (cdr ranges)
                (append (car parsed) markers)
                (append (cdr parsed) styles))))))

(define (md-ensure-cache!)
  (define doc-id (editor->doc-id (editor-focus)))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-box! *md-cache* (list doc-id -1 '() '()))]
    [else
     (define rope (editor->text doc-id))
     (define clen (text.rope-len-chars rope))
     (define cur (unbox *md-cache*))
     (when (not (and cur (equal? (list-ref cur 0) doc-id) (equal? (list-ref cur 1) clen)))
       (define parsed (scan-all-markdown rope (text.rope-len-lines rope)))
       (set-box! *md-cache* (list doc-id clen (car parsed) (cdr parsed))))]))

(define (markdown-markers-hook)
  (md-ensure-cache!)
  (list-ref (unbox *md-cache*) 2))

(define (markdown-style-hook line-start line-end)
  (md-ensure-cache!)
  (define filtered
    (filter (lambda (sp)
              (or (<= (cadr sp) line-start) (>= (car sp) line-end)))
            (list-ref (unbox *md-cache*) 3)))
  (try-set-style-overlays! filtered))

(define (markdown-render-enable!)
  (set-box! *markdown-marker-hook* markdown-markers-hook)
  (set-box! *markdown-style-hook* markdown-style-hook))

(markdown-render-enable!)
