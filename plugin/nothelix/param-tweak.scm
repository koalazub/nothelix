;;; param-tweak.scm — declare a numeric @param in a cell, nudge it, re-render.

(require "string-utils.scm")
(require "common.scm")
(require "cell-boundaries.scm")
(require "execution.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/components.scm")
(require-builtin helix/core/text as text.)

(provide parse-param-line
         format-number
         nudge-param-value
         decimals-of
         find-param-target-line
         collect-assigned-names
         token-references?
         scan-stale-lines
         param-track-position
         param-track-string
         param-up
         param-down)

(define (split-on-first s ch)
  (let loop ([i 0])
    (cond
      [(>= i (string-length s)) #false]
      [(char=? (string-ref s i) ch)
       (cons (substring s 0 i) (substring s (+ i 1) (string-length s)))]
      [else (loop (+ i 1))])))

(define (literal-kind value-str)
  (if (string-contains? value-str ".") 'float 'int))

(define (tokens-of s)
  (filter (lambda (t) (> (string-length t) 0))
          (string-split (string-replace-all (string-trim s) "\t" " ") " ")))

(define (parse-range tok)
  (define parts (split-on-first tok #\:))
  (and parts
       (let ([lo (string->number (string-trim (car parts)))]
             [hi (string->number (string-trim (cdr parts)))])
         (and lo hi (cons lo hi)))))

(define (default-step lo hi kind)
  (if (eq? kind 'int) 1 (/ (- hi lo) 100)))

(define (parse-param-line line)
  (define halves (split-on-first line #\#))
  (and halves
       (let* ([code (car halves)]
              [comment (string-trim (cdr halves))])
         (and (string-starts-with? comment "@param")
              (let* ([spec (string-trim (substring comment 6 (string-length comment)))]
                     [code-parts (split-on-first code #\=)])
                (and code-parts
                     (let* ([name (string-trim (car code-parts))]
                            [value-str (string-trim (cdr code-parts))]
                            [toks (tokens-of spec)]
                            [rng (and (pair? toks) (parse-range (car toks)))])
                       (and rng
                            (> (string-length name) 0)
                            (string->number value-str)
                            (let* ([lo (car rng)]
                                   [hi (cdr rng)]
                                   [kind (literal-kind value-str)]
                                   [step (parse-step toks lo hi kind)])
                              (list name value-str lo hi step kind))))))))))

(define (parse-step toks lo hi kind)
  (let loop ([ts toks])
    (cond
      [(null? ts) (default-step lo hi kind)]
      [(and (equal? (car ts) "step") (pair? (cdr ts)))
       (or (string->number (cadr ts)) (default-step lo hi kind))]
      [else (loop (cdr ts))])))

(define *decimals-of-max* 12)
(define *decimals-of-epsilon* 1e-6)

(define (decimals-of step)
  (define mag (abs step))
  (if (>= mag 1)
      0
      (let loop ([n mag] [count 0])
        (if (or (>= count *decimals-of-max*)
                (< (abs (- n (round n))) *decimals-of-epsilon*))
            count
            (loop (* n 10) (+ count 1))))))

(define (format-number n decimals)
  (if (<= decimals 0)
      (number->string (inexact->exact (round n)))
      (let* ([scale (expt 10 decimals)]
             [scaled (inexact->exact (round (* n scale)))]
             [neg (< scaled 0)]
             [mag (abs scaled)]
             [int-part (quotient mag scale)]
             [frac-part (remainder mag scale)]
             [frac-str (number->string frac-part)]
             [padded (string-append
                       (make-string (- decimals (string-length frac-str)) #\0)
                       frac-str)])
        (string-append (if neg "-" "")
                       (number->string int-part) "." padded))))

(define (nudge-param-value current lo hi step dir)
  (define steps (round (/ (- current lo) step)))
  (define next (+ steps dir))
  (define max-steps (floor (/ (- hi lo) step)))
  (define clamped (max 0 (min next max-steps)))
  (define raw (+ lo (* clamped step)))
  (if (and (integer? lo) (integer? step)) (inexact->exact (round raw)) raw))

(define (find-param-target-line get-line total-lines cursor-line)
  (let loop ([i (min cursor-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(cell-marker? (string-trim (get-line i))) #false]
      [(parse-param-line (get-line i)) i]
      [else (loop (- i 1))])))

(define (collect-assigned-names get-line cell-start cell-end)
  (let loop ([i cell-start] [acc '()])
    (if (>= i cell-end)
        (reverse acc)
        (let ([p (parse-param-line (get-line i))])
          (loop (+ i 1) (if p (cons (car p) acc) acc))))))

(define *ident-chars* "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_!")

(define (ident-char? c)
  (or (string-contains? *ident-chars* (string c))
      (>= (char->integer c) 128)))

(define (token-references? code-line name)
  (define nlen (string-length name))
  (define llen (string-length code-line))
  (let loop ([i 0])
    (cond
      [(> (+ i nlen) llen) #false]
      [(and (equal? (substring code-line i (+ i nlen)) name)
            (or (= i 0) (not (ident-char? (string-ref code-line (- i 1)))))
            (or (= (+ i nlen) llen) (not (ident-char? (string-ref code-line (+ i nlen))))))
       #true]
      [else (loop (+ i 1))])))

(define (any-name-referenced? code-line names)
  (cond
    [(null? names) #false]
    [(token-references? code-line (car names)) #true]
    [else (any-name-referenced? code-line (cdr names))]))

(define (scan-stale-lines get-line total-lines from-line names)
  (let loop ([i (+ from-line 1)] [current-marker #false] [hit #false] [acc '()])
    (cond
      [(>= i total-lines)
       (reverse (if (and current-marker hit) (cons current-marker acc) acc))]
      [(cell-marker? (string-trim (get-line i)))
       (loop (+ i 1) i #false
             (if (and current-marker hit) (cons current-marker acc) acc))]
      [(and current-marker (any-name-referenced? (get-line i) names))
       (loop (+ i 1) current-marker #true acc)]
      [else (loop (+ i 1) current-marker hit acc)])))

;; Buffer rewrite — the target line's full-text replacement is the shared
;; source-widget apply path (widgets.scm's rewrite-line-literal!).

(define (build-param-line name new-value-str spec-suffix)
  (string-append name " = " new-value-str spec-suffix))

;; Slider track — a one-row gauge painted above the param line while the cursor
;; is in its cell or the walk lands on it. Built from small pieces, never a
;; per-char loop over a long string.

(define *param-track-min-width* 6)
(define *param-track-max-width* 48)
(define *param-track-width* 24)
(define *param-track-fill* "─")
(define *param-track-marker* "●")

(define (clamp-track-width w)
  (max *param-track-min-width* (min *param-track-max-width* w)))

(define (hbar n)
  (let loop ([i 0] [acc ""])
    (if (>= i n) acc (loop (+ i 1) (string-append acc *param-track-fill*)))))

;;@doc
;; Column (0-based, in [0, width-1]) of the marker for `value` on a `width`-wide
;; track over [lo, hi]: 0 at lo, width-1 at hi, proportional between. A degenerate
;; range or width clamps to a single valid column.
(define (param-track-position lo hi value width)
  (define w (max 1 width))
  (if (<= hi lo)
      0
      (let* ([frac (exact->inexact (/ (- value lo) (- hi lo)))]
             [clamped (max 0.0 (min 1.0 frac))])
        (inexact->exact (round (* clamped (- w 1)))))))

;;@doc
;; A one-row slider track string: a bracketed gauge with the marker at `value`'s
;; position over [lo, hi], the literal, and the self-teaching key suffix. `width`
;; is clamped to the track's bounds.
(define (param-track-string lo hi value width value-str)
  (define w (clamp-track-width width))
  (define pos (param-track-position lo hi value w))
  (string-append "  [" (hbar pos) *param-track-marker* (hbar (max 0 (- (- w 1) pos)))
                 "] " value-str "  ]p/[p"))

(define (render-param-track-at! doc-id get-line total tgt line lo hi value value-str)
  (define cell-start (find-cell-start-line get-line tgt))
  (define cell-end (find-cell-code-end get-line total (+ cell-start 1)))
  (set-widget-track! tgt (param-track-string lo hi value *param-track-width* value-str)
                     cell-start cell-end))

(define (param-track-on-arrive scan anchor-line)
  (define get-line (WidgetScan-get-line scan))
  (define total (WidgetScan-total scan))
  (define line (get-line anchor-line))
  (define p (parse-param-line line))
  (when p
    (render-param-track-at! (WidgetScan-doc-id scan) get-line total anchor-line line
                            (list-ref p 2) (list-ref p 3) (string->number (cadr p)) (cadr p))))

;; Active-param statusline readout

(define (param-readout-style) (theme-scope-ref "ui.statusline"))

(define (param-readout-element view-id focused)
  (if (not focused)
      '()
      (let* ([doc-id (editor->doc-id view-id)]
             [path (and doc-id (editor-document->path doc-id))])
        (if (not (and path (string-suffix? path ".jl")))
            '()
            (let* ([rope (editor->text doc-id)]
                   [total (text.rope-len-lines rope)]
                   [cl (current-line-number)]
                   [tgt (find-param-target-line
                          (lambda (i) (doc-get-line rope total i)) total cl)])
              (if (not tgt)
                  '()
                  (let ([p (parse-param-line (doc-get-line rope total tgt))])
                    (if (not p)
                        '()
                        (list (span (string-append
                                      " " (car p) "=" (cadr p)
                                      " [" (number->string (list-ref p 2))
                                      ":" (number->string (list-ref p 3)) "] ")
                                    (param-readout-style)))))))))))

(push-status-element! 'right (status-element param-readout-element))

;; Stale-line label for the downstream cells a nudge invalidates.

(define (param-stale-label names)
  (string-append "  ○ stale · " (string-join names ", ") " changed"))

;; :param-up / :param-down commands

(define (string-trim-right s)
  (if (string? s) (trim-end s) ""))

(define (nudge-param! dir)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "param: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define cl (current-line-number))
     (define tgt (find-param-target-line get-line total cl))
     (cond
       [(not tgt) (set-status! "param: no @param at or above the cursor")]
       [else
        (define line (get-line tgt))
        (define p (parse-param-line line))
        (define name (car p))
        (define cur (string->number (cadr p)))
        (define lo (list-ref p 2))
        (define hi (list-ref p 3))
        (define step (list-ref p 4))
        (define kind (list-ref p 5))
        (define next (nudge-param-value cur lo hi step dir))
        (define dec (if (eq? kind 'int) 0 (decimals-of step)))
        (define new-str (format-number next dec))
        (define comment-half (split-on-first line #\#))
        (define spec-suffix (if comment-half (string-append "  #" (cdr comment-half)) ""))
        (define newline-suffix (if (string-suffix? line "\n") "\n" ""))
        (define new-line-text
          (string-append (build-param-line name new-str (string-trim-right spec-suffix)) newline-suffix))
        (define cell-start (find-cell-start-line get-line tgt))
        (define cell-end (find-cell-code-end get-line total (+ cell-start 1)))
        (define names (collect-assigned-names get-line cell-start cell-end))
        (define stale-lines (scan-stale-lines get-line total cell-start names))
        (apply-source-widget! doc-id tgt new-line-text stale-lines
                              (param-stale-label names) execute-cell)
        (set-widget-track! tgt (param-track-string lo hi next *param-track-width* new-str)
                           cell-start cell-end)
        (set-status! (string-append name " = " new-str))])]))

;;@doc
;; Increase the @param at/above the cursor by one step, rewrite the literal,
;; stage downstream stale tags, and debounce a single cell re-run.
(define (param-up) (nudge-param! 1))

;;@doc
;; Decrease the @param at/above the cursor by one step, rewrite the literal,
;; stage downstream stale tags, and debounce a single cell re-run.
(define (param-down) (nudge-param! -1))

;; --- widget-kind registration (number: @param nudge) ---

(define (discover-param-widgets scan)
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (let loop ([i 0] [acc '()])
    (if (>= i total)
        (reverse acc)
        (loop (+ i 1)
              (if (parse-param-line (get-line i)) (cons (cons i #false) acc) acc)))))

(register-widget-kind! 'number "param" "]p/[p nudge" discover-param-widgets)
(register-widget-arrive! 'number param-track-on-arrive)
