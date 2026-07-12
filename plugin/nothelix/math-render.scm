;;; math-render.scm - Stack big-operator limits and fraction bodies onto virtual rows above/below the source line.

(require "common.scm")
(require "string-utils.scm")
(require "json-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "conceal.scm")
(require "math-image.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          parse-math-spans))

(provide math-render-buffer
         math-render-clear)

(define (try-set-math-lines-above! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.set-math-lines-above! *helix.cx* ,line-idx ',lines)))
    #true))

(define (try-set-math-lines-below! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(begin (require-builtin helix/core/static as hs.)
                  (hs.set-math-lines-below! *helix.cx* ,line-idx ',lines)))
    #true))

(define (try-clear-all-math-lines!)
  (with-handler
    (lambda (_) #false)
    (eval '(begin (require-builtin helix/core/static as hs.)
                  (hs.clear-all-math-lines! *helix.cx*)))
    #true))

(define (math-render-ffi-available?)
  (with-handler
    (lambda (_) #false)
    (eval '(begin (require-builtin helix/core/static as hs.)
                  hs.set-math-lines-above!))
    #true))

(define (spaces n)
  (if (<= n 0) "" (make-string n #\space)))

;; Parse one TSV row from parse-math-spans into field strings, or '() on a blank line.
;; Format: KIND\tCMD\tSTART\tEND\tCOL\t{SUB|NUM}\t{SUP|DEN}
(define (parse-math-span-row row)
  (if (= (string-length row) 0)
      '()
      (map unescape-field (string-split row "\t"))))

(define (unescape-field s)
  ;; Order matters: backslash last, so a "\\t" doesn't become a literal tab.
  (let* ([step1 (string-replace-all s "\\n" "\n")]
         [step2 (string-replace-all step1 "\\t" "\t")])
    (string-replace-all step2 "\\\\" "\\")))

;; Turn one parsed-row into zero or more (line-idx above below) entries.
(define (row->entries fields line-idx)
  (cond
    [(null? fields) '()]
    [else
     (define kind (car fields))
     (define visual-col (string->number (list-ref fields 4)))
     (define padding (spaces (or visual-col 0)))
     (cond
       [(equal? kind "big_op")
        (define sub-text (list-ref fields 5))
        (define sup-text (list-ref fields 6))
        (list (list line-idx
                    (if (equal? sup-text "") #false (string-append padding sup-text))
                    (if (equal? sub-text "") #false (string-append padding sub-text))))]
       [(equal? kind "frac")
        (define num-text (list-ref fields 5))
        (define den-text (list-ref fields 6))
        (list (list line-idx
                    (string-append padding num-text)
                    (string-append padding den-text)))]
       [else '()])]))

;;@doc
;; Stage big-operator/fraction above/below rows for every comment line in the buffer.
(define (math-render-buffer)
  (cond
    [(not (math-render-ffi-available?))
     (set-box! *math-render-active* #false)
     (set-status! "math-render: hx fork FFI missing — run darwin-rebuild to enable")]
    [else (math-render-buffer-impl)]))

(define (math-render-buffer-impl)
  (try-clear-all-math-lines!)
  (set-box! *math-render-active* #true)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define display-ranges (display-math-block-ranges rope total-lines))

  (let loop ([line-idx 0])
    (when (< line-idx total-lines)
      (define line (text.rope->string (text.rope->line rope line-idx)))
      (define trimmed
        (if (string-suffix? line "\n")
            (substring line 0 (- (string-length line) 1))
            line))
      (when (and (string-starts-with? trimmed "# ")
                 (not (line-in-ranges? line-idx display-ranges)))
        (define content (substring trimmed 2 (string-length trimmed)))
        (define tsv (parse-math-spans content))
        (define rows
          (if (= (string-length tsv) 0)
              '()
              (filter
                (lambda (r) (> (string-length r) 0))
                (string-split tsv "\n"))))
        (define entries
          (apply append
                 (map (lambda (row)
                        (row->entries (parse-math-span-row row) line-idx))
                      rows)))
        (stage-merged line-idx entries))
      (loop (+ line-idx 1)))))

(define (stage-merged line-idx entries)
  (define above-lines '())
  (define below-lines '())
  (for-each
    (lambda (entry)
      (define a (cadr entry))
      (define b (caddr entry))
      (when a (set! above-lines (cons a above-lines)))
      (when b (set! below-lines (cons b below-lines))))
    entries)
  (when (not (null? above-lines))
    (try-set-math-lines-above! line-idx (reverse above-lines)))
  (when (not (null? below-lines))
    (try-set-math-lines-below! line-idx (reverse below-lines))))

;;@doc
;; Drop every math annotation on the current document.
(define (math-render-clear)
  (try-clear-all-math-lines!)
  (set-box! *math-render-active* #false))

;; Install the refresh hook so the conceal cycle restages annotations.
(set-box! *math-render-refresh-hook*
          (lambda ()
            (when (math-render-ffi-available?)
              (math-render-buffer-impl))))
