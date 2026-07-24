;;; choice.scm — the `choice` widget: `name = value  # @select a|b|c` rewrites an
;;; assignment from a closed set. Mirrors @param's trailing-comment grammar and
;;; line targeting: the name is the assignment LHS (not repeated in the comment),
;;; the spec after @select is the pipe-delimited option set. ]s / [s cycle the
;;; value; <space>nc opens the shared modal. String literals keep their quotes,
;;; bare identifiers stay bare — inferred from the current value's shape.

(require "string-utils.scm")
(require "common.scm")
(require "cell-boundaries.scm")
(require "execution.scm")
(require "param-tweak.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require-builtin helix/components)

(provide parse-select-line
         option-index
         next-option
         select-value-string
         choice-options-row
         select-next
         select-prev
         select-choice)

(define (split-on-first s ch)
  (let loop ([i 0])
    (cond
      [(>= i (string-length s)) #false]
      [(char=? (string-ref s i) ch)
       (cons (substring s 0 i) (substring s (+ i 1) (string-length s)))]
      [else (loop (+ i 1))])))

(define (string-trim-right s)
  (if (string? s) (trim-end s) ""))

(define (value-quoted? v)
  (and (>= (string-length v) 2)
       (string-starts-with? v "\"")
       (string-suffix? v "\"")))

(define (unquote-value v)
  (substring v 1 (- (string-length v) 1)))

(define (parse-options spec)
  (filter (lambda (o) (> (string-length o) 0))
          (map string-trim (string-split spec "|"))))

;;@doc
;; Parse `name = value  # @select a|b|c` into
;; (name value-str current-token options quoted?), or #false when the line is not
;; a well-formed @select (no assignment, wrong annotation, or empty option set).
;; `current-token` is the value with quotes stripped so it compares to the bare
;; options; `quoted?` records the literal's shape for the rewrite.
(define (parse-select-line line)
  (define halves (split-on-first line #\#))
  (and halves
       (let* ([code (car halves)]
              [comment (string-trim (cdr halves))])
         (and (string-starts-with? comment "@select")
              (let* ([spec (string-trim (substring comment 7 (string-length comment)))]
                     [options (parse-options spec)]
                     [code-parts (split-on-first code #\=)])
                (and (pair? options)
                     code-parts
                     (let* ([name (string-trim (car code-parts))]
                            [value-str (string-trim (cdr code-parts))])
                       (and (> (string-length name) 0)
                            (> (string-length value-str) 0)
                            (let* ([quoted? (value-quoted? value-str)]
                                   [current (if quoted? (unquote-value value-str) value-str)])
                              (list name value-str current options quoted?))))))))))

;;@doc
;; Index of `token` within `options`, or 0 when it is not a member.
(define (option-index options token)
  (let loop ([os options] [i 0])
    (cond
      [(null? os) 0]
      [(equal? (car os) token) i]
      [else (loop (cdr os) (+ i 1))])))

;;@doc
;; The option one step (`dir` ±1) from `token` in `options`, wrapping around.
(define (next-option options token dir)
  (list-ref options (modulo (+ (option-index options token) dir) (length options))))

;;@doc
;; Render an option `token` as a source literal: quoted when the current value
;; was a string, bare when it was an identifier.
(define (select-value-string token quoted?)
  (if quoted? (string-append "\"" token "\"") token))

;;@doc
;; A one-row option list with the option at `index` bracketed as the current one.
(define (choice-options-row options index)
  (string-join
    (let loop ([os options] [i 0] [acc '()])
      (if (null? os)
          (reverse acc)
          (loop (cdr os) (+ i 1)
                (cons (if (= i index) (string-append "[" (car os) "]") (car os)) acc))))
    " "))

(define (find-select-target-line get-line total-lines cursor-line)
  (let loop ([i (min cursor-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(cell-marker? (string-trim (get-line i))) #false]
      [(parse-select-line (get-line i)) i]
      [else (loop (- i 1))])))

(define (select-stale-label name)
  (string-append "  ○ stale · " name " changed"))

(define (build-select-line name new-value-str spec-suffix)
  (string-append name " = " new-value-str spec-suffix))

(define (spec-suffix-of line)
  (define comment-half (split-on-first line #\#))
  (if comment-half (string-trim-right (string-append "  #" (cdr comment-half))) ""))

(define (apply-select! doc-id get-line total tgt line name new-str)
  (define newline-suffix (if (string-suffix? line "\n") "\n" ""))
  (define new-line-text
    (string-append (build-select-line name new-str (spec-suffix-of line)) newline-suffix))
  (define cell-start (find-cell-start-line get-line tgt))
  (define stale-lines (scan-stale-lines get-line total cell-start (list name)))
  (apply-source-widget! doc-id tgt new-line-text stale-lines
                        (select-stale-label name) execute-cell)
  (set-status! (string-append name " = " new-str)))

;; --- ]s / [s cycle ---

(define (select-cycle! dir)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "select: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define tgt (find-select-target-line get-line total (current-line-number)))
     (cond
       [(not tgt) (set-status! "select: no @select at or above the cursor")]
       [else
        (define line (get-line tgt))
        (define p (parse-select-line line))
        (define name (car p))
        (define current (list-ref p 2))
        (define options (list-ref p 3))
        (define quoted? (list-ref p 4))
        (define new-str (select-value-string (next-option options current dir) quoted?))
        (apply-select! doc-id get-line total tgt line name new-str)])]))

;;@doc
;; Cycle the @select at/above the cursor forward to the next option, rewrite the
;; assignment, stage downstream stale tags, and debounce a cell re-run.
(define (select-next) (select-cycle! 1))

;;@doc
;; Cycle the @select at/above the cursor back to the previous option, rewrite the
;; assignment, stage downstream stale tags, and debounce a cell re-run.
(define (select-prev) (select-cycle! -1))

;; --- the shared modal (h/l choose, enter apply, esc leave) ---

(struct ChoiceState (doc-id target-line name options index quoted? spec-suffix newline? cell-start)
  #:mutable)

(define (choice-move! st dir)
  (set-ChoiceState-index! st
    (modulo (+ (ChoiceState-index st) dir) (length (ChoiceState-options st)))))

(define (choice-apply! st)
  (define name (ChoiceState-name st))
  (define opt (list-ref (ChoiceState-options st) (ChoiceState-index st)))
  (define new-str (select-value-string opt (ChoiceState-quoted? st)))
  (define new-line
    (string-append (build-select-line name new-str (ChoiceState-spec-suffix st))
                   (if (ChoiceState-newline? st) "\n" "")))
  (define doc-id (ChoiceState-doc-id st))
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define get-line (lambda (i) (doc-get-line rope total i)))
  (define stale-lines
    (scan-stale-lines get-line total (ChoiceState-cell-start st) (list name)))
  (apply-source-widget! doc-id (ChoiceState-target-line st) new-line stale-lines
                        (select-stale-label name) execute-cell)
  (set-status! (string-append name " = " new-str)))

(define (render-choice state rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))
  (define title (string-append "Select  " (ChoiceState-name state)))
  (define row (choice-options-row (ChoiceState-options state) (ChoiceState-index state)))
  (define footer "h/l choose · enter apply · esc cancel")
  (define content-w
    (max (string-length title) (max (string-length row) (string-length footer))))
  (define popup-w (min (- rw 2) (+ content-w 4)))
  (define popup-h (min (- rh 2) 5))
  (define px (max 0 (quotient (- rw popup-w) 2)))
  (define py (max 0 (quotient (- rh popup-h) 2)))
  (define popup-area (area px py popup-w popup-h))
  (define bg-style (style))
  (define border-style (style-fg (style) Color/Cyan))
  (define title-style (style-with-bold (style-fg (style) Color/White)))
  (define row-style (style-fg (style) Color/Cyan))
  (define footer-style (style-fg (style) Color/Gray))
  (buffer/clear buf popup-area)
  (block/render buf popup-area (make-block bg-style border-style "all" "rounded"))
  (frame-set-string! buf (+ px 2) (+ py 1) title title-style)
  (frame-set-string! buf (+ px 2) (+ py 2) row row-style)
  (frame-set-string! buf (+ px 2) (+ py popup-h -2) footer footer-style))

(define choice-vtable
  (hash 'render render-choice
        'move   choice-move!
        'step   (lambda (st d) (void))
        'apply  choice-apply!))

;;@doc
;; Open the closed-set chooser for the @select at/above the cursor: h/l move the
;; selection, Enter rewrites the assignment through the shared apply path, Esc
;; leaves. No-ops off a .jl file or when no @select is in reach.
(define (select-choice)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "select: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define tgt (find-select-target-line get-line total (current-line-number)))
     (cond
       [(not tgt) (set-status! "select: no @select at or above the cursor")]
       [else
        (define line (get-line tgt))
        (define p (parse-select-line line))
        (define state
          (ChoiceState doc-id tgt (car p) (list-ref p 3)
                       (option-index (list-ref p 3) (list-ref p 2))
                       (list-ref p 4) (spec-suffix-of line)
                       (string-suffix? line "\n")
                       (find-cell-start-line get-line tgt)))
        (open-widget-modal! choice-vtable state "select-choice")])]))

;; --- widget-kind registration (choice: @select cycle + modal) ---

(define (discover-select-widgets scan)
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (let loop ([i 0] [acc '()])
    (if (>= i total)
        (reverse acc)
        (loop (+ i 1)
              (if (parse-select-line (get-line i)) (cons (cons i #false) acc) acc)))))

(register-widget-kind! 'choice "select" "]s/[s cycle · <space>nc menu" discover-select-widgets)
