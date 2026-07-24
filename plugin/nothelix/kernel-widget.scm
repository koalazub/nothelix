;;; kernel-widget.scm — the leaf half of kernel-declared widgets. A cell's run
;;; can emit widget specs (nothelix_slider / nothelix_choice); this module parses
;;; them, renders one virtual output row per spec, and joins the output-row
;;; composition path the same way the waveform group does. It carries no
;;; manipulation or kernel IPC — that lives in kernel-widget-apply.scm, required
;;; after execution — so this stays a dependency-free leaf the composition sites
;;; (output-insert, execution, audio) can build on without a cycle.

(require "string-utils.scm")
(require "output-store.scm")
(require "project-config.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix json-get-widgets))

(provide parse-widget-specs
         serialize-widget-specs
         widget-spec-kind
         widget-spec-name
         widget-spec-params
         widget-spec-current
         spec-with-current
         parse-slider-params
         parse-choice-options
         slider-step
         slider-nudge-value
         choice-cycle-value
         option-index-of
         widget-track-position
         widget-slider-row
         widget-choice-row
         widget-row-for-spec
         widget-group-for
         widgets-blob->specs
         first-spec-of-kind
         cell-has-stored-widgets?
         json-get-widgets)

;; --- spec parse / serialize (kind name params current) ---

(define (widget-spec-kind spec) (list-ref spec 0))
(define (widget-spec-name spec) (list-ref spec 1))
(define (widget-spec-params spec) (list-ref spec 2))
(define (widget-spec-current spec) (list-ref spec 3))

(define (spec-with-current spec new-current)
  (list (widget-spec-kind spec) (widget-spec-name spec)
        (widget-spec-params spec) new-current))

(define (parse-one-widget-spec line)
  (cond
    [(equal? (string-trim line) "") #false]
    [else
     (define fields (string-split line "\t"))
     (if (and (>= (length fields) 4)
              (> (string-length (list-ref fields 0)) 0)
              (> (string-length (list-ref fields 1)) 0))
         (list (list-ref fields 0) (list-ref fields 1)
               (list-ref fields 2) (list-ref fields 3))
         #false)]))

;;@doc
;; Decode a `json-get-widgets` / stored widgets blob ("kind\tname\tparams\tcurrent"
;; lines) into a list of specs — '() for "" or #false. Malformed lines (fewer than
;; four fields, or an empty kind/name) are dropped rather than raising.
(define (parse-widget-specs blob)
  (if (or (not blob) (equal? blob ""))
      '()
      (filter (lambda (x) x)
              (map parse-one-widget-spec (string-split blob "\n")))))

(define (serialize-one-widget-spec spec)
  (string-join (list (widget-spec-kind spec) (widget-spec-name spec)
                     (widget-spec-params spec) (widget-spec-current spec))
               "\t"))

;;@doc
;; Serialize a list of specs back into a widgets blob byte-compatible with
;; `parse-widget-specs` and the store codec's widgets section.
(define (serialize-widget-specs specs)
  (string-join (map serialize-one-widget-spec specs) "\n"))

;; --- params grammar (slider "lo:hi:step", choice "a|b|c") ---

(define (parse-slider-params params)
  (define parts (string-split params ":"))
  (if (>= (length parts) 3)
      (list (or (string->number (string-trim (list-ref parts 0))) 0)
            (or (string->number (string-trim (list-ref parts 1))) 0)
            (or (string->number (string-trim (list-ref parts 2))) 0))
      #false))

(define (parse-choice-options params)
  (filter (lambda (o) (> (string-length o) 0))
          (map string-trim (string-split params "|"))))

;;@doc
;; The effective slider step: the declared step when positive, else 1 for an
;; integer range and (hi-lo)/100 for a fractional one (matching @param).
(define (slider-step lo hi step)
  (cond
    [(and (number? step) (> step 0)) step]
    [(and (exact-integer? lo) (exact-integer? hi)) 1]
    [else (/ (- hi lo) 100)]))

;; --- pure nudge value maths ---

;;@doc
;; The slider value one `dir` (±1) step from `current`, snapped to the step grid
;; over [lo, hi] and clamped to the range. Integer ranges stay integer.
(define (slider-nudge-value lo hi step current dir)
  (define steps (round (/ (- current lo) step)))
  (define next (+ steps dir))
  (define max-steps (floor (/ (- hi lo) step)))
  (define clamped (max 0 (min next max-steps)))
  (define raw (+ lo (* clamped step)))
  (if (and (exact-integer? lo) (exact-integer? step)) (inexact->exact (round raw)) raw))

;;@doc
;; Index of `token` within `options`, or 0 when it is not a member.
(define (option-index-of options token)
  (let loop ([os options] [i 0])
    (cond
      [(null? os) 0]
      [(equal? (car os) token) i]
      [else (loop (cdr os) (+ i 1))])))

;;@doc
;; The choice option one `dir` (±1) from `current` in `options`, wrapping around.
(define (choice-cycle-value options current dir)
  (if (null? options)
      current
      (list-ref options (modulo (+ (option-index-of options current) dir) (length options)))))

;; --- row rendering (one virtual row per spec) ---

(define *widget-track-width* 20)
(define *widget-track-fill* "─")
(define *widget-track-marker* "●")
(define *widget-marker* "⊞")

(define (repeat-str s n)
  (let loop ([i 0] [acc ""]) (if (>= i n) acc (loop (+ i 1) (string-append acc s)))))

;;@doc
;; Column (0-based, in [0, width-1]) of the marker for `value` on a `width`-wide
;; track over [lo, hi]: 0 at lo, width-1 at hi, proportional between. A degenerate
;; range or width clamps to a single valid column.
(define (widget-track-position lo hi value width)
  (define w (max 1 width))
  (if (<= hi lo)
      0
      (let* ([frac (exact->inexact (/ (- value lo) (- hi lo)))]
             [clamped (max 0.0 (min 1.0 frac))])
        (inexact->exact (round (* clamped (- w 1)))))))

;;@doc
;; A one-row slider surface: the ⊞ marker, the name, a bracketed gauge with the
;; marker at `value`'s position over [lo, hi], the current literal, and the
;; self-teaching key suffix.
(define (widget-slider-row name lo hi value value-str)
  (define w *widget-track-width*)
  (define pos (widget-track-position lo hi value w))
  (string-append *widget-marker* " " name " ["
                 (repeat-str *widget-track-fill* pos)
                 *widget-track-marker*
                 (repeat-str *widget-track-fill* (max 0 (- (- w 1) pos)))
                 "] " value-str " · ]p/[p"))

;;@doc
;; A one-row choice surface: the ⊞ marker, the name, the option set with the
;; current one bracketed, and the self-teaching key suffix.
(define (widget-choice-row name options current)
  (string-append *widget-marker* " " name "  "
                 (string-join
                   (map (lambda (o) (if (equal? o current) (string-append "[" o "]") o)) options)
                   " ")
                 " · ]s/[s"))

(define (widget-slider-row-for spec)
  (define p (parse-slider-params (widget-spec-params spec)))
  (if (not p)
      #false
      (let ([lo (list-ref p 0)]
            [hi (list-ref p 1)]
            [current-str (widget-spec-current spec)])
        (widget-slider-row (widget-spec-name spec) lo hi
                           (or (string->number current-str) lo) current-str))))

(define (widget-choice-row-for spec)
  (define options (parse-choice-options (widget-spec-params spec)))
  (if (null? options)
      #false
      (widget-choice-row (widget-spec-name spec) options (widget-spec-current spec))))

;;@doc
;; The one virtual row a spec renders as, or #false for an unknown/malformed spec.
(define (widget-row-for-spec spec)
  (cond
    [(equal? (widget-spec-kind spec) "slider") (widget-slider-row-for spec)]
    [(equal? (widget-spec-kind spec) "choice") (widget-choice-row-for spec)]
    [else #false]))

;;@doc
;; Build a cell's kernel-widget output group from a widgets blob: one virtual row
;; per spec, joined into composition like the waveform group. '() when the blob is
;; empty, or when the widgets knob is off (the knob gates this surface).
(define (widget-group-for blob)
  (if (not (widgets-enabled?))
      '()
      (filter (lambda (r) r) (map widget-row-for-spec (parse-widget-specs blob)))))

;; --- store-backed helpers ---

;;@doc
;; The specs stored for cell `idx` under `path`, hash-gated so an edited-but-not-
;; rerun cell keeps its widgets, as a list of specs ('() when none).
(define (widgets-blob->specs path idx)
  (define raw (store-get-for path (cell-id idx)))
  (parse-widget-specs (decode-stored-widgets-blob raw (stored-source-hash raw))))

;;@doc
;; The first stored spec of `kind` on cell `idx` under `path`, or #false.
(define (first-spec-of-kind path idx kind)
  (let loop ([specs (widgets-blob->specs path idx)])
    (cond
      [(null? specs) #false]
      [(equal? (widget-spec-kind (car specs)) kind) (car specs)]
      [else (loop (cdr specs))])))

;;@doc
;; Whether cell `idx` under `path` has any stored kernel widget, judged against
;; the stored hash so an edited-but-not-rerun cell keeps its widgets.
(define (cell-has-stored-widgets? path idx)
  (not (null? (widgets-blob->specs path idx))))
