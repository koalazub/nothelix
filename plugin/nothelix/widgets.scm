;;; widgets.scm — the shared widget contract: a per-document registry, the
;;; ]w / [w walk, the generic h/l/j/k modal shell, and the debounced source
;;; re-run effect. Feature modules (param-tweak, audio, plot-resize, animation)
;;; register their kinds and vtables here; this module requires none of them,
;;; so registration always flows leaf -> shared and never cycles.

(require "common.scm")
(require "string-utils.scm")
(require "stale-tags.scm")
(require "project-config.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require-builtin helix/components)

(provide register-widget-kind!
         register-widget-arrive!
         widget-walk-next
         widget-walk-prev
         open-widget-modal!
         apply-source-widget!
         schedule-source-widget-rerun!
         rewrite-line-literal!
         bump-widget-rerun-generation!
         widget-rerun-current?
         walk-line-order
         next-walk-line
         prev-walk-line
         dispatch-modal-action
         widget-walk-guard
         widgets-disabled-status
         set-widget-track!
         clear-active-widget-track!
         widget-track-maybe-clear-on-cursor!
         WidgetScan
         WidgetScan-doc-id
         WidgetScan-path
         WidgetScan-rope
         WidgetScan-total
         WidgetScan-get-line)

;; --- kind registry ---
;;
;; A widget kind is registered once by its feature module with a discovery
;; procedure. Discovery takes a WidgetScan and returns a list of
;; (anchor-line . cell-index-or-false) pairs for the current document. Source
;; kinds discover by scanning their annotation lines; output kinds discover by
;; querying their subsystem (audio checks cell-has-stored-audio?). The registry
;; itself holds no per-document state — it is rebuilt on demand at walk time,
;; so a disabled or unused widget surface costs nothing per refresh.

(struct WidgetKind (kind title walk-hint discover))
(struct Widget (kind title anchor-line cell-index walk-hint))
(struct WidgetScan (doc-id path rope total get-line))

(define *widget-kinds* (box '()))

;;@doc
;; Register a widget kind: its symbol, a human title, the self-teaching key
;; hint shown on the walk, and a discovery procedure (WidgetScan -> list of
;; (anchor-line . cell-index)).
(define (register-widget-kind! kind title walk-hint discover)
  (set-box! *widget-kinds*
            (append (unbox *widget-kinds*)
                    (list (WidgetKind kind title walk-hint discover)))))

(define (collect-document-widgets scan)
  (apply append
    (map (lambda (wk)
           (map (lambda (a)
                  (Widget (WidgetKind-kind wk)
                          (WidgetKind-title wk)
                          (car a)
                          (cdr a)
                          (WidgetKind-walk-hint wk)))
                ((WidgetKind-discover wk) scan)))
         (unbox *widget-kinds*))))

;; --- arrival hooks (per-kind on-land) + the slider track surface ---
;;
;; A kind may register an `arrive` closure (WidgetScan -> anchor-line -> void)
;; run when the walk lands on one of its widgets, so a leaf feature can paint an
;; on-demand surface without the shared module depending on it. The number kind
;; uses this to draw its slider track above the param line. Registration flows
;; leaf -> shared exactly like discovery does.

(define *widget-arrivals* (box (hash)))

;;@doc
;; Register a per-kind arrival closure, run by the walk when it lands on a
;; widget of `kind`. `proc` takes the WidgetScan and the anchor line.
(define (register-widget-arrive! kind proc)
  (set-box! *widget-arrivals* (hash-insert (unbox *widget-arrivals*) kind proc)))

(define (widget-arrive-for kind)
  (hash-try-get (unbox *widget-arrivals*) kind))

;; The single active slider track: at most one exists at a time, so nudging or
;; walking to another param moves the track rather than littering the file with
;; one per param. `*active-widget-track-range*` is the (cell-start . cell-end)
;; the track belongs to, used to clear it when the cursor leaves that cell.
(define *active-widget-track-line* (box #false))
(define *active-widget-track-range* (box #false))

;;@doc
;; Clear the one active slider track, if any, and forget its anchor.
(define (clear-active-widget-track!)
  (define ln (unbox *active-widget-track-line*))
  (when ln
    (clear-stale-tag-for-line! ln)
    (set-box! *active-widget-track-line* #false)
    (set-box! *active-widget-track-range* #false)))

;;@doc
;; Paint `text` as a one-row track above `line` (mid-cell, via the stale-tag
;; above surface), replacing any prior active track, and remember the owning
;; cell range `[cell-start, cell-end)`. A no-op when the widgets knob is off.
(define (set-widget-track! line text cell-start cell-end)
  (when (widgets-enabled?)
    (clear-active-widget-track!)
    (try-set-stale-tag-above! line text)
    (set-box! *active-widget-track-line* line)
    (set-box! *active-widget-track-range* (cons cell-start cell-end))))

;;@doc
;; Riding the existing selection hook: clear the active track once the cursor
;; leaves its cell. O(1) — a bare box check when no track is showing.
(define (widget-track-maybe-clear-on-cursor!)
  (define r (unbox *active-widget-track-range*))
  (when r
    (define cl (current-line-number))
    (when (or (< cl (car r)) (>= cl (cdr r)))
      (clear-active-widget-track!))))

;; --- walk ordering (pure) ---

;;@doc
;; Sort a list of anchor lines ascending and drop duplicates, so the walk
;; visits each line once in buffer order.
(define (walk-line-order lines)
  (let loop ([xs (sort lines <)] [acc '()])
    (cond
      [(null? xs) (reverse acc)]
      [(and (pair? acc) (= (car xs) (car acc))) (loop (cdr xs) acc)]
      [else (loop (cdr xs) (cons (car xs) acc))])))

;;@doc
;; The next anchor strictly below `cursor`, wrapping to the first; #false when
;; there are no anchors.
(define (next-walk-line lines cursor)
  (cond
    [(null? lines) #false]
    [else
     (let loop ([xs lines])
       (cond
         [(null? xs) (car lines)]
         [(> (car xs) cursor) (car xs)]
         [else (loop (cdr xs))]))]))

;;@doc
;; The previous anchor strictly above `cursor`, wrapping to the last; #false
;; when there are no anchors.
(define (prev-walk-line lines cursor)
  (cond
    [(null? lines) #false]
    [else
     (let loop ([xs lines] [best #false])
       (cond
         [(null? xs) (if best best (last-of lines))]
         [(< (car xs) cursor) (loop (cdr xs) (car xs))]
         [else (if best best (last-of lines))]))]))

(define (last-of xs)
  (if (null? (cdr xs)) (car xs) (last-of (cdr xs))))

(define (widgets-by-line widgets)
  (define sorted
    (sort widgets (lambda (a b) (< (Widget-anchor-line a) (Widget-anchor-line b)))))
  (let loop ([xs sorted] [seen '()] [acc '()])
    (cond
      [(null? xs) (reverse acc)]
      [(member (Widget-anchor-line (car xs)) seen) (loop (cdr xs) seen acc)]
      [else (loop (cdr xs)
                  (cons (Widget-anchor-line (car xs)) seen)
                  (cons (cons (Widget-anchor-line (car xs)) (car xs)) acc))])))

(define (widget-at by-line line)
  (let loop ([xs by-line])
    (cond
      [(null? xs) #false]
      [(= (car (car xs)) line) (cdr (car xs))]
      [else (loop (cdr xs))])))

(define (widget-walk-status w line)
  (string-append (Widget-title w) " widget at line "
                 (number->string (+ line 1))
                 " · " (Widget-walk-hint w)))

;; --- config gate ---

(define *widgets-disabled-status* "widgets are disabled in .nothelix.conf")
(define *widgets-empty-status* "no widgets in this notebook")

;;@doc
;; The status shown when the `widgets` knob is off.
(define (widgets-disabled-status) *widgets-disabled-status*)

;;@doc
;; #false when the widget knob is on (walk/modal may proceed); otherwise the
;; disabled status string. Callers short-circuit on a non-#false result before
;; any registry scan, so the disabled path is one flag check and no buffer walk.
(define (widget-walk-guard)
  (if (widgets-enabled?) #false *widgets-disabled-status*))

;; --- the walk (]w / [w) ---

(define (widget-walk! direction)
  (define blocked (widget-walk-guard))
  (cond
    [blocked (set-status! blocked)]
    [else
     (define focus (editor-focus))
     (define doc-id (editor->doc-id focus))
     (define path (editor-document->path doc-id))
     (define rope (editor->text doc-id))
     (define total (text.rope-len-lines rope))
     (define get-line (lambda (i) (doc-get-line rope total i)))
     (define scan (WidgetScan doc-id path rope total get-line))
     (define widgets (collect-document-widgets scan))
     (cond
       [(null? widgets) (set-status! *widgets-empty-status*)]
       [else
        (define by-line (widgets-by-line widgets))
        (define lines (map car by-line))
        (define cursor (current-line-number))
        (define target
          (if (> direction 0) (next-walk-line lines cursor) (prev-walk-line lines cursor)))
        (define w (widget-at by-line target))
        (helix.goto (number->string (+ target 1)))
        (clear-active-widget-track!)
        (let ([arrive (widget-arrive-for (Widget-kind w))])
          (when arrive (arrive scan target)))
        (set-status! (widget-walk-status w target))])]))

;;@doc
;; Jump to the next widget anchor below the cursor, wrapping to the top, and
;; name the widget's kind and keys in the status line.
(define (widget-walk-next) (widget-walk! 1))

;;@doc
;; Jump to the previous widget anchor above the cursor, wrapping to the bottom,
;; and name the widget's kind and keys in the status line.
(define (widget-walk-prev) (widget-walk! -1))

;; --- modal shell (h/l value, j/k granularity, Enter apply, Esc leave) ---
;;
;; The generic component. A kind supplies a vtable — a hash of 'render / 'move /
;; 'step / 'apply closures over its own state — and this shell owns the event
;; grammar and the component plumbing. The scrub modal is this shell plus the
;; audio vtable; its render (footer, bracket, sweep) stays kind-specific so its
;; surface is pixel-identical to the hand-rolled modal it replaces.

(struct ModalShell (vtable state))

;;@doc
;; Map a modal action symbol to its vtable effect and return 'close or 'consume.
;; 'apply runs the vtable's apply then closes; 'close leaves without applying;
;; 'right/'left drive 'move by ±1; 'down/'up drive 'step by ±1. Pure of key
;; events so the dispatch is testable on its own.
(define (dispatch-modal-action vtable state action)
  (cond
    [(eq? action 'close) 'close]
    [(eq? action 'apply) ((hash-try-get vtable 'apply) state) 'close]
    [(eq? action 'right) ((hash-try-get vtable 'move) state 1) 'consume]
    [(eq? action 'left) ((hash-try-get vtable 'move) state -1) 'consume]
    [(eq? action 'down) ((hash-try-get vtable 'step) state 1) 'consume]
    [(eq? action 'up) ((hash-try-get vtable 'step) state -1) 'consume]
    [else 'consume]))

(define (key-event->modal-action event)
  (define char (key-event-char event))
  (cond
    [(or (key-event-escape? event) (eqv? char #\q)) 'close]
    [(key-event-enter? event) 'apply]
    [(or (eqv? char #\l) (key-event-right? event)) 'right]
    [(or (eqv? char #\h) (key-event-left? event)) 'left]
    [(or (eqv? char #\j) (key-event-down? event)) 'down]
    [(or (eqv? char #\k) (key-event-up? event)) 'up]
    [else 'noop]))

(define (handle-modal-event shell event)
  (define result
    (dispatch-modal-action (ModalShell-vtable shell)
                           (ModalShell-state shell)
                           (key-event->modal-action event)))
  (if (eq? result 'close) event-result/close event-result/consume))

(define (render-modal shell rect buf)
  ((hash-try-get (ModalShell-vtable shell) 'render) (ModalShell-state shell) rect buf))

;;@doc
;; Open the shared modal for a kind: `vtable` supplies 'render/'move/'step/'apply
;; over `state`, `name` is the component name. No-ops with the disabled status
;; when the `widgets` knob is off.
(define (open-widget-modal! vtable state name)
  (define blocked (widget-walk-guard))
  (if blocked
      (set-status! blocked)
      (push-component!
        (overlaid
          (new-component! name (ModalShell vtable state) render-modal
            (hash "handle_event" handle-modal-event))))))

;; --- debounced source-widget re-run effect ---
;;
;; Lifted from param-tweak: a source widget rewrites its literal, stages
;; downstream stale tags, and coalesces repeated nudges into a single cell
;; re-run after 150ms of quiet via a generation counter. execute-cell is
;; injected by the caller (param-tweak) rather than required here, since
;; execution.scm sits above this module in the graph.

(define *widget-rerun-generation* (box 0))

;;@doc
;; Bump and return the current re-run generation. Each scheduled re-run captures
;; its generation; a later bump invalidates it, so only the last of a burst runs.
(define (bump-widget-rerun-generation!)
  (define g (+ 1 (unbox *widget-rerun-generation*)))
  (set-box! *widget-rerun-generation* g)
  g)

;;@doc
;; #true iff `g` is still the latest re-run generation.
(define (widget-rerun-current? g)
  (= g (unbox *widget-rerun-generation*)))

;;@doc
;; Debounce a single re-run: bump the generation, then after 150ms fire
;; `rerun-thunk` only if no later nudge superseded it.
(define (schedule-source-widget-rerun! rerun-thunk)
  (define gen (bump-widget-rerun-generation!))
  (enqueue-thread-local-callback-with-delay 150
    (lambda ()
      (when (widget-rerun-current? gen)
        (rerun-thunk)))))

;;@doc
;; Replace the full text of line `line-idx` in document `doc-id` with
;; `new-line-text` as one committed, undo-visible edit.
(define (rewrite-line-literal! doc-id line-idx new-line-text)
  (define rope (editor->text doc-id))
  (define total (text.rope-len-lines rope))
  (define start (text.rope-line->char rope line-idx))
  (define end
    (if (< (+ line-idx 1) total)
        (text.rope-line->char rope (+ line-idx 1))
        (text.rope-len-chars rope)))
  (define sel (helix.static.range->selection (helix.static.range start end)))
  (helix.static.set-current-selection-object! sel)
  (helix.static.replace-selection-with new-line-text)
  (helix.static.collapse_selection)
  (helix.static.commit-changes-to-history))

;;@doc
;; The shared source-widget apply path: rewrite the target line in place, stage
;; the given stale tags (provenance flows exactly as a hand edit would), and
;; debounce a single re-run of the owning cell via `rerun-thunk`.
(define (apply-source-widget! doc-id target-line new-line-text stale-lines stale-label rerun-thunk)
  (rewrite-line-literal! doc-id target-line new-line-text)
  (when (pair? stale-lines)
    (set-stale-tags-for-lines! stale-lines stale-label))
  (schedule-source-widget-rerun! rerun-thunk))
