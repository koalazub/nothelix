;;; table-image.scm - Render markdown pipe tables as inline Typst images.

(require "string-utils.scm")
(require "debug.scm")
(require "conceal.scm")
(require "math-image.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          render-table-to-svg
                          start-render-table-batch
                          poll-render-batch))

(provide render-all-tables
         set-table-image-font-pt!)

;; Tables own the 4M band (see math-image.scm bands note).
(define *table-image-id-base* 4000000)
(define *table-image-id-limit* (+ *table-image-id-base* 4000000))
(define *table-render-mask-width* (box 240))

(define (try-clear-table-image-band!)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.clear-raw-content-in-range!
             ,*table-image-id-base* ,*table-image-id-limit*))))

;; Bumped per render; a stale poll skips placement if a newer render started.
(define *table-render-generation* (box 0))
(define *table-poll-interval-ms* 60)
(define *table-poll-max-attempts* 400)
(define *table-batch-sep* (make-string 1 (integer->char 30)))

(define (bump-table-render-generation!)
  (set-box! *table-render-generation* (+ 1 (unbox *table-render-generation*)))
  (unbox *table-render-generation*))

(define *table-image-font-pt* (box 13))
;; Display-config setter — project-config.scm applies this from .nothelix.scm.
(define (set-table-image-font-pt! n) (set-box! *table-image-font-pt* n))

;; Block detection
(define (comment-body rope total idx)
  (and (>= idx 0) (< idx total)
       (let* ([s (text.rope->string (text.rope->line rope idx))]
              [t (if (string-suffix? s "\n")
                     (substring s 0 (- (string-length s) 1))
                     s)])
         (and (string-starts-with? t "# ")
              (substring t 2 (string-length t))))))

(define (list-all? pred lst)
  (cond
    [(null? lst) #true]
    [(pred (car lst)) (list-all? pred (cdr lst))]
    [else #false]))

;; #true when body is the |:--|--:| separator rule.
(define (separator-body? body)
  (and (string-contains? body "|")
       (let ([cells (filter (lambda (c) (> (string-length (string-trim c)) 0))
                            (string-split body "|"))])
         (and (not (null? cells))
              (list-all?
                (lambda (cell)
                  (let ([t (string-trim cell)])
                    (and (> (string-length t) 0)
                         (string-contains? t "-")
                         (list-all? (lambda (ch) (or (char=? ch #\-) (char=? ch #\:)))
                                    (string->list t)))))
                cells)))))

;; (anchor span block-text) for each markdown pipe table in the buffer.
(define (collect-table-jobs rope total)
  (let loop ([idx 0] [acc '()])
    (if (>= idx total)
        (reverse acc)
        (let ([body (comment-body rope total idx)])
          (if (and body (string-contains? body "|"))
              (let scan ([end (+ idx 1)] [lines (list body)] [saw-sep (separator-body? body)])
                (let ([nb (comment-body rope total end)])
                  (if (and nb (string-contains? nb "|"))
                      (scan (+ end 1) (cons nb lines) (or saw-sep (separator-body? nb)))
                      (if (and saw-sep (> (length lines) 1))
                          (loop end (cons (list idx (length lines)
                                                (string-join (reverse lines) "\n"))
                                          acc))
                          (loop end acc)))))
              (loop (+ idx 1) acc))))))

(define (table-job-anchor job) (car job))
(define (table-job-span job) (cadr job))
(define (table-job-text job) (caddr job))

;; Rendering + placement

;; djb2; same algorithm as math-image.scm.
(define (table-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

(define (table-image-id anchor block)
  (+ *table-image-id-base*
     (modulo (+ (table-hash (number->string anchor)) (table-hash block))
             4000000)))

(define (call-render-table block font-pt color)
  (if (math-image-test-mode?)
      (math-image-mock-result)
      (render-table-to-svg block font-pt color)))

(define (table-ceil-div a b) (if (<= b 0) a (quotient (+ a (max 0 (- b 1))) b)))

(define (table-line-visible-length rope li total)
  (if (< li total)
      (let ([s (text.rope->string (text.rope->line rope li))])
        (if (string-suffix? s "\n") (- (string-length s) 1) (string-length s)))
      0))

;; Screen rows a table run occupies once long cells soft-wrap at wrap-width, so
;; the image's coverage matches the wrapped source extent instead of the raw
;; document-line count (which leaves later rows leaking as `# | …` source).
(define (table-visual-span rope anchor doc-span wrap-width)
  (let ([total (text.rope-len-lines rope)])
    (let loop ([i 0] [acc 0])
      (if (>= i doc-span)
          (max 1 acc)
          (loop (+ i 1)
                (+ acc (max 1 (table-ceil-div
                                (table-line-visible-length rope (+ anchor i) total)
                                wrap-width))))))))

(define (place-table-job rope job result-json)
  (place-svg-image-at-line!
    result-json
    (table-image-id (table-job-anchor job) (table-job-text job))
    rope
    (table-job-anchor job)
    (table-visual-span rope (table-job-anchor job) (table-job-span job)
                       (unbox *table-render-mask-width*))
    "table-image"
    #false
    (unbox *table-render-mask-width*)))

(define (table-count-phrase n)
  (string-append (number->string n) " table" (if (= n 1) "" "s")))

;; Compile on a plain Rust thread (invisible to Steel GC, so no freeze); poll on main.
(define (render-all-tables-async jobs doc-id)
  (define color (effective-math-text-color))
  (define font-pt (unbox *table-image-font-pt*))
  (define blob (string-join (map table-job-text jobs) *table-batch-sep*))
  (define my-gen (bump-table-render-generation!))
  (set-box! *table-render-mask-width* (math-image-mask-width))
  (set-status! (string-append "table-image: rendering " (table-count-phrase (length jobs)) "…"))
  (define job-id (start-render-table-batch blob font-pt color))
  (poll-table-batch! job-id jobs doc-id my-gen 0))

(define (poll-table-batch! job-id jobs doc-id my-gen attempts)
  (when (= my-gen (unbox *table-render-generation*))
    (define reply (poll-render-batch job-id))
    (cond
      [(string=? reply "PENDING")
       (if (< attempts *table-poll-max-attempts*)
           (enqueue-thread-local-callback-with-delay *table-poll-interval-ms*
             (lambda () (poll-table-batch! job-id jobs doc-id my-gen (+ attempts 1))))
           (set-status! "table-image: render timed out"))]
      [(string-starts-with? reply "ERROR:")
       (set-status! (string-append "table-image: " reply))]
      [else
       (define results (string-split reply *table-batch-sep*))
       (define rope (editor->text doc-id))
       (try-clear-table-image-band!)
       (let place ([js jobs] [rs results] [placed 0])
         (if (or (null? js) (null? rs))
             (set-status! (string-append "table-image: rendered " (table-count-phrase placed)))
             (place (cdr js) (cdr rs)
                    (if (place-table-job rope (car js) (car rs))
                        (+ placed 1)
                        placed))))])))

;;@doc
;; Render every markdown pipe table in the buffer as an inline Typst image.
(define (render-all-tables)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (file-has-conceal-extension? path)
    (define rope (editor->text doc-id))
    (define total (text.rope-len-lines rope))
    (define jobs (collect-table-jobs rope total))
    (cond
      [(null? jobs)
       (bump-table-render-generation!)
       (try-clear-table-image-band!)]
      [(math-image-test-mode?)
       (bump-table-render-generation!)
       (try-clear-table-image-band!)
       (for-each
         (lambda (job)
           (place-table-job rope job
                            (call-render-table (table-job-text job) (unbox *table-image-font-pt*)
                                               (effective-math-text-color))))
         jobs)]
      [else
       (render-all-tables-async jobs doc-id)])))
