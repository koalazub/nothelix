;;; flag.scm — the `flag` widget: `name = true  # @toggle` flips a boolean. Mirrors
;;; @param's trailing-comment grammar and line targeting (the name is the
;;; assignment LHS). A flip is non-directional, so there is no modal and no
;;; bracket pair: <space>nt flips in place, and the walk names that key. The flip
;;; rewrites the literal and re-runs the owning cell through the shared apply path.

(require "string-utils.scm")
(require "common.scm")
(require "cell-boundaries.scm")
(require "execution.scm")
(require "param-tweak.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)

(provide parse-toggle-line
         toggle-flip-value
         toggle-flag)

(define (split-on-first s ch)
  (let loop ([i 0])
    (cond
      [(>= i (string-length s)) #false]
      [(char=? (string-ref s i) ch)
       (cons (substring s 0 i) (substring s (+ i 1) (string-length s)))]
      [else (loop (+ i 1))])))

(define (string-trim-right s)
  (if (string? s) (trim-end s) ""))

(define (boolean-literal? v)
  (or (equal? v "true") (equal? v "false")))

;;@doc
;; Parse `name = true|false  # @toggle` into (name value-str), or #false when the
;; line is not a well-formed @toggle (no assignment, wrong annotation, or a
;; non-boolean literal).
(define (parse-toggle-line line)
  (define halves (split-on-first line #\#))
  (and halves
       (let* ([code (car halves)]
              [comment (string-trim (cdr halves))])
         (and (string-starts-with? comment "@toggle")
              (let ([code-parts (split-on-first code #\=)])
                (and code-parts
                     (let* ([name (string-trim (car code-parts))]
                            [value-str (string-trim (cdr code-parts))])
                       (and (> (string-length name) 0)
                            (boolean-literal? value-str)
                            (list name value-str)))))))))

;;@doc
;; The opposite boolean literal: "true" -> "false", "false" -> "true".
(define (toggle-flip-value v)
  (if (equal? v "true") "false" "true"))

(define (find-toggle-target-line get-line total-lines cursor-line)
  (let loop ([i (min cursor-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(cell-marker? (string-trim (get-line i))) #false]
      [(parse-toggle-line (get-line i)) i]
      [else (loop (- i 1))])))

(define (toggle-stale-label name)
  (string-append "  ○ stale · " name " changed"))

(define (build-toggle-line name new-value-str spec-suffix)
  (string-append name " = " new-value-str spec-suffix))

;;@doc
;; Flip the boolean of the @toggle at/above the cursor, rewrite the literal, stage
;; downstream stale tags, and debounce a single cell re-run.
(define (toggle-flag)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "toggle: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define tgt (find-toggle-target-line get-line total (current-line-number)))
     (cond
       [(not tgt) (set-status! "toggle: no @toggle at or above the cursor")]
       [else
        (define line (get-line tgt))
        (define p (parse-toggle-line line))
        (define name (car p))
        (define new-str (toggle-flip-value (cadr p)))
        (define comment-half (split-on-first line #\#))
        (define spec-suffix
          (if comment-half (string-trim-right (string-append "  #" (cdr comment-half))) ""))
        (define newline-suffix (if (string-suffix? line "\n") "\n" ""))
        (define new-line-text
          (string-append (build-toggle-line name new-str spec-suffix) newline-suffix))
        (define cell-start (find-cell-start-line get-line tgt))
        (define stale-lines (scan-stale-lines get-line total cell-start (list name)))
        (apply-source-widget! doc-id tgt new-line-text stale-lines
                              (toggle-stale-label name) execute-cell)
        (set-status! (string-append name " = " new-str))])]))

;; --- widget-kind registration (flag: @toggle flip; modal-less) ---

(define (discover-toggle-widgets scan)
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (let loop ([i 0] [acc '()])
    (if (>= i total)
        (reverse acc)
        (loop (+ i 1)
              (if (parse-toggle-line (get-line i)) (cons (cons i #false) acc) acc)))))

(register-widget-kind! 'flag "toggle" "<space>nt flip" discover-toggle-widgets)
