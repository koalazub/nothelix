;;; kernel-widget-apply.scm — the manipulation half of kernel-declared widgets.
;;; Reached by the ]w/[w walk, nudged through the number/choice grammar (]p/[p,
;;; ]s/[s) and the modal (<space>nc) via the fallbacks registered in widgets.scm,
;;; each acting on the kernel widget of the cell under the cursor. An apply sends
;;; kernel-set-var, persists the new value to the output store, re-renders the
;;; cell's widget row, and refreshes the provenance surfaces from the reply so
;;; dependent cells show stale badges immediately. No auto-rerun.
;;;
;;; Required after execution so it can reuse refresh-provenance-surfaces! and the
;;; shared recompose path; the leaf half (kernel-widget.scm) carries the rendering
;;; and pure maths so the composition sites never depend on this module.

(require "string-utils.scm")
(require "common.scm")
(require "cell-boundaries.scm")
(require "cell-state.scm")
(require "output-store.scm")
(require "output-render.scm")
(require "kernel-widget.scm")
(require "param-tweak.scm")
(require "execution.scm")
(require "audio.scm")
(require "kernel.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require-builtin helix/components)

(#%require-dylib "libnothelix"
                 (only-in nothelix kernel-set-var json-get))

(provide kernel-slider-nudge!
         kernel-choice-nudge!
         kernel-widget-modal!
         nudged-slider-string
         cycled-choice-value)

;; --- cell context (the cell under the cursor on a .jl notebook) ---

(define (current-cell-context)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (if (not (and path (string-suffix? path ".jl")))
      #false
      (let* ([rope (editor->text doc-id)]
             [total (text.rope-len-lines rope)]
             [get-line (lambda (i) (doc-get-line rope total i))]
             [cell-start (find-cell-start-line get-line (current-line-number))]
             [idx (marker-line-cell-index (get-line cell-start))])
        (if idx (list doc-id path idx) #false))))

(define (context-doc-id ctx) (list-ref ctx 0))
(define (context-path ctx) (list-ref ctx 1))
(define (context-idx ctx) (list-ref ctx 2))

;; --- payload maths ---

;;@doc
;; The value string a slider nudge sends to the kernel: `current` moved one `dir`
;; (±1) step over the spec's range, formatted to the step's precision. #false when
;; the spec's params are malformed.
(define (nudged-slider-string spec dir)
  (define p (parse-slider-params (widget-spec-params spec)))
  (if (not p)
      #false
      (let* ([lo (list-ref p 0)]
             [hi (list-ref p 1)]
             [step (slider-step lo hi (list-ref p 2))]
             [current (or (string->number (widget-spec-current spec)) lo)]
             [next (slider-nudge-value lo hi step current dir)]
             [dec (if (and (exact-integer? lo) (exact-integer? step)) 0 (decimals-of step))])
        (format-number next dec))))

;;@doc
;; The value string a choice cycle sends to the kernel: the option one `dir` (±1)
;; from the spec's current, wrapping around.
(define (cycled-choice-value spec dir)
  (choice-cycle-value (parse-choice-options (widget-spec-params spec))
                      (widget-spec-current spec) dir))

;; --- the shared apply path ---

(define (reply-error reply)
  (define e (json-get reply "error"))
  (if (> (string-length e) 0) e "set-var failed"))

(define (persist-and-refresh! doc-id path idx spec new-current reply)
  (define name (widget-spec-name spec))
  (define specs (widgets-blob->specs path idx))
  (define updated
    (map (lambda (s)
           (if (equal? (widget-spec-name s) name) (spec-with-current s new-current) s))
         specs))
  (store-set-widgets-blob! (cell-id idx) (serialize-widget-specs updated))
  (recompose-cell! idx -1 -1 -1)
  (refresh-cell-states-from-result! reply)
  (refresh-provenance-surfaces! doc-id path))

;; Apply a value to a kernel widget: no-op with a status when the widgets knob is
;; off or the kernel is not running; otherwise set-var, persist, and refresh.
(define (apply-kernel-value! doc-id path idx spec value-str)
  (define name (widget-spec-name spec))
  (define blocked (widget-walk-guard))
  (cond
    [blocked (set-status! blocked)]
    [(not value-str) (set-status! (string-append name ": malformed widget params"))]
    [else
     (define kdir (running-kernel-dir path))
     (cond
       [(not kdir) (set-status! "kernel not running — run the cell first")]
       [else
        (define reply (kernel-set-var kdir name value-str idx))
        (cond
          [(equal? (json-get reply "status") "ok")
           (persist-and-refresh! doc-id path idx spec value-str reply)
           (set-status! (string-append name " = " value-str))]
          [else (set-status! (string-append name ": " (reply-error reply)))])])]))

;; --- nudge fallbacks (]p/[p slider, ]s/[s choice) ---

;;@doc
;; Nudge a kernel slider on the cell under the cursor by one step in `dir`. #true
;; when a slider was found and handled (so the source nudge stops), #false to let
;; the source @param nudge report its own miss.
(define (kernel-slider-nudge! dir)
  (define ctx (current-cell-context))
  (cond
    [(not ctx) #false]
    [else
     (define spec (first-spec-of-kind (context-path ctx) (context-idx ctx) "slider"))
     (cond
       [(not spec) #false]
       [else
        (apply-kernel-value! (context-doc-id ctx) (context-path ctx) (context-idx ctx)
                             spec (nudged-slider-string spec dir))
        #true])]))

;;@doc
;; Cycle a kernel choice on the cell under the cursor by one option in `dir`.
;; #true when a choice was found and handled, #false otherwise.
(define (kernel-choice-nudge! dir)
  (define ctx (current-cell-context))
  (cond
    [(not ctx) #false]
    [else
     (define spec (first-spec-of-kind (context-path ctx) (context-idx ctx) "choice"))
     (cond
       [(not spec) #false]
       [else
        (apply-kernel-value! (context-doc-id ctx) (context-path ctx) (context-idx ctx)
                             spec (cycled-choice-value spec dir))
        #true])]))

;; --- the slider modal (h/l move, enter apply, esc leave), via the shared shell ---

(struct KernelSliderState (doc-id path idx spec lo hi step value) #:mutable)

(define (kernel-slider-move! st dir)
  (set-KernelSliderState-value! st
    (slider-nudge-value (KernelSliderState-lo st) (KernelSliderState-hi st)
                        (KernelSliderState-step st) (KernelSliderState-value st) dir)))

(define (kernel-slider-value-string st)
  (define lo (KernelSliderState-lo st))
  (define step (KernelSliderState-step st))
  (define dec (if (and (exact-integer? lo) (exact-integer? step)) 0 (decimals-of step)))
  (format-number (KernelSliderState-value st) dec))

(define (kernel-slider-apply! st)
  (apply-kernel-value! (KernelSliderState-doc-id st) (KernelSliderState-path st)
                       (KernelSliderState-idx st) (KernelSliderState-spec st)
                       (kernel-slider-value-string st)))

(define (render-kernel-slider st rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))
  (define name (widget-spec-name (KernelSliderState-spec st)))
  (define row (widget-slider-row name (KernelSliderState-lo st) (KernelSliderState-hi st)
                                 (KernelSliderState-value st) (kernel-slider-value-string st)))
  (define title (string-append "Slider  " name))
  (define footer "h/l move · enter apply · esc cancel")
  (define content-w (max (string-length title) (max (string-length row) (string-length footer))))
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

(define kernel-slider-vtable
  (hash 'render render-kernel-slider
        'move   kernel-slider-move!
        'step   (lambda (st d) (void))
        'apply  kernel-slider-apply!))

(define (open-kernel-slider-modal! doc-id path idx spec)
  (define p (parse-slider-params (widget-spec-params spec)))
  (when p
    (let* ([lo (list-ref p 0)]
           [hi (list-ref p 1)]
           [step (slider-step lo hi (list-ref p 2))]
           [value (or (string->number (widget-spec-current spec)) lo)])
      (open-widget-modal! kernel-slider-vtable
                          (KernelSliderState doc-id path idx spec lo hi step value)
                          "kernel-slider"))))

;; --- the choice modal (h/l choose, enter apply, esc leave) ---

(struct KernelChoiceState (doc-id path idx spec options index) #:mutable)

(define (kernel-choice-move! st dir)
  (set-KernelChoiceState-index! st
    (modulo (+ (KernelChoiceState-index st) dir) (length (KernelChoiceState-options st)))))

(define (kernel-choice-apply! st)
  (apply-kernel-value! (KernelChoiceState-doc-id st) (KernelChoiceState-path st)
                       (KernelChoiceState-idx st) (KernelChoiceState-spec st)
                       (list-ref (KernelChoiceState-options st) (KernelChoiceState-index st))))

(define (render-kernel-choice st rect buf)
  (define rw (area-width rect))
  (define rh (area-height rect))
  (define name (widget-spec-name (KernelChoiceState-spec st)))
  (define current (list-ref (KernelChoiceState-options st) (KernelChoiceState-index st)))
  (define row (widget-choice-row name (KernelChoiceState-options st) current))
  (define title (string-append "Choice  " name))
  (define footer "h/l choose · enter apply · esc cancel")
  (define content-w (max (string-length title) (max (string-length row) (string-length footer))))
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

(define kernel-choice-vtable
  (hash 'render render-kernel-choice
        'move   kernel-choice-move!
        'step   (lambda (st d) (void))
        'apply  kernel-choice-apply!))

(define (open-kernel-choice-modal! doc-id path idx spec)
  (define options (parse-choice-options (widget-spec-params spec)))
  (when (pair? options)
    (open-widget-modal! kernel-choice-vtable
                        (KernelChoiceState doc-id path idx spec options
                                           (option-index-of options (widget-spec-current spec)))
                        "kernel-choice")))

;;@doc
;; Open the modal for the kernel widget on the cell under the cursor: the slider
;; modal when a slider is present, else the choice modal. #true when one opened,
;; #false to let the source @select modal report its own miss.
(define (kernel-widget-modal!)
  (define ctx (current-cell-context))
  (cond
    [(not ctx) #false]
    [else
     (define slider (first-spec-of-kind (context-path ctx) (context-idx ctx) "slider"))
     (define choice (first-spec-of-kind (context-path ctx) (context-idx ctx) "choice"))
     (cond
       [slider (open-kernel-slider-modal! (context-doc-id ctx) (context-path ctx) (context-idx ctx) slider) #true]
       [choice (open-kernel-choice-modal! (context-doc-id ctx) (context-path ctx) (context-idx ctx) choice) #true]
       [else #false])]))

;; --- widget-kind registration + fallback wiring ---

(define (discover-kernel-widgets scan)
  (define path (WidgetScan-path scan))
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (if (not path)
      '()
      (let loop ([i 0] [acc '()])
        (if (>= i total)
            (reverse acc)
            (let ([line (get-line i)])
              (if (cell-marker? line)
                  (let ([idx (marker-line-cell-index line)])
                    (if (and idx (cell-has-stored-widgets? path idx))
                        (loop (+ i 1) (cons (cons i idx) acc))
                        (loop (+ i 1) acc)))
                  (loop (+ i 1) acc)))))))

(register-widget-kind! 'kernel-widget "kernel widget"
                       "]p/[p·]s/[s nudge · <space>nc modal" discover-kernel-widgets)

(register-number-nudge-fallback! kernel-slider-nudge!)
(register-choice-nudge-fallback! kernel-choice-nudge!)
(register-widget-modal-fallback! kernel-widget-modal!)
