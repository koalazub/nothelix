;;; animation.scm - Animated media driver
;;;
;;; The libnothelix `animation` module owns engines (decoder + renderer + cache
;;; + state). This module is the conduit between fork events and those engines:
;;; it subscribes to focus / viewport / tick signals, calls
;;; `animation-tick`, and routes the rendered bytes back into Helix's
;;; `add-or-replace-animating-raw-content!` so the editor draws the next frame.
;;;
;;; Lifecycle:
;;;   register-animation!     -> insert state, schedule first tick
;;;   document-focus-lost     -> mark engines for the doc as unfocused, drop
;;;                              their next callback
;;;   document-focus-gained   -> mark engines as focused, reschedule
;;;   viewport-changed        -> recompute 'visible? for engines in that doc,
;;;                              reschedule the ones that just became visible
;;;
;;; Pause-when-offscreen / pause-when-unfocused short-circuits in
;;; `animation-state-active?`: when the predicate is false, the in-flight
;;; callback fires once, sees the gate, exits without re-arming. Idle cost
;;; is then dominated by the dedup hot-path on the libnothelix side (8 ns
;;; per tick benchmarked).

(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require-builtin steel/time)

;; Steel resolves `prefix-in` imports at module-load time against the
;; import set, not at call time. So a bare `helix.static.add-or-
;; replace-animating-raw-content!` reference here fails with
;; FreeIdentifier on any binary where the symbol isn't already in the
;; helix.static. namespace at load — not just stale binaries, but ALSO
;; rebuilt binaries where the static-command registration timing means
;; the symbol may not be in the import set yet when this module is
;; first compiled. `eval` defers the lookup to call time so we get the
;; symbol when it actually exists, and `with-handler` turns missing-
;; symbol errors into silent no-ops (which the doctor / `:nothelix-
;; status` then flag for the user). `set-status!` keeps the failure
;; visible on the first occurrence per session so silent-no-op doesn't
;; mask a genuine stale-binary case.
(define *animation-ffi-warned?* #f)
;; Rust signature is (cx, view_id, char_idx, id, payload, height, is_animating);
;; cx is auto-injected by Steel, leaving 6 user-visible args in that exact order.
(define (try-add-or-replace-animating-raw-content! view-id char-idx id bytes height is-anim?)
  (with-handler
    (lambda (err)
      (when (not *animation-ffi-warned?*)
        (set! *animation-ffi-warned?* #t)
        (set-status! "nothelix: animation FFI unavailable — run :nothelix-status"))
      #false)
    (eval `(helix.static.add-or-replace-animating-raw-content!
             ,view-id ,char-idx ,id ,bytes ,height ,is-anim?))))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          animation-register
                          animation-tick
                          animation-tick-bytes
                          animation-tick-status
                          animation-tick-height
                          animation-tick-delay-ms
                          animation-tick-frame-index
                          animation-set-pause
                          animation-drop
                          json-get-animated-mime))

(provide register-animation!
         animation-toggle-at-cursor
         animation-pause-all
         animation-resume-all
         animation-engine-count)

;;; ---------------------------------------------------------------------------
;;; State
;;; ---------------------------------------------------------------------------

;; Map engine_id (int) -> state hash with keys:
;;   'char-idx        char index where the overlay is anchored (int)
;;   'doc-id          owning document id
;;   'height          terminal rows the overlay occupies
;;   'focused?        is the doc currently focused?
;;   'visible?        is the cell within the current viewport?
;;   'manual-paused?  user pressed <space>p
;;   'status          'playing 'finished 'errored
;;
;; We never assume the hash exists in any branch; every hash-ref carries a
;; default and every mutation rebinds *animations* via set!.
(define *animations* (hash))
(define *first-hint-shown?* #f)
;; True while the hx terminal window holds focus. Flipped by the
;; `terminal-focus-{gained,lost}` hooks below; AND'd into the tick
;; gate so animations don't burn CPU when the user has alt-tabbed.
(define *terminal-focused?* #t)

(define (animation-engine-count)
  (length (hash-keys->list *animations*)))

(define (now-ms)
  (current-milliseconds))

(define (state-of eid)
  (hash-try-get *animations* eid))

(define (state-update! eid mutator)
  (define st (state-of eid))
  (when st
    (set! *animations* (hash-insert *animations* eid (mutator st)))))

(define (state-set! eid st)
  (set! *animations* (hash-insert *animations* eid st)))

(define (state-remove! eid)
  (set! *animations* (hash-remove *animations* eid)))

(define (animation-state-active? st)
  (and st
       *terminal-focused?*
       (hash-try-get st 'focused?)
       (hash-try-get st 'visible?)
       (not (hash-try-get st 'manual-paused?))
       (eq? (hash-try-get st 'status) 'playing)))

;;; ---------------------------------------------------------------------------
;;; Public commands
;;; ---------------------------------------------------------------------------

;;@doc
;; Register an animation engine and start its tick loop.
;; mime: MIME string (e.g. "image/gif")
;; bytes: bytevector / list of bytes containing the source
;; char-idx: anchor position in the document (int)
;; height: rows the overlay occupies (int)
;; Returns the engine id on success, or #f when libnothelix refused
;; (unknown MIME, malformed bytes, lock failure).
(define (register-animation! mime bytes char-idx height)
  (define result (animation-register mime bytes))
  (define id (if (number? result) result -999))
  (cond
    [(<= id 0) #f]
    [else
     (define st
       (hash 'char-idx char-idx
             'doc-id (editor->doc-id (editor-focus))
             'height height
             'focused? #t
             'visible? #t
             'manual-paused? #f
             'status 'playing))
     (state-set! id st)
     (maybe-show-first-hint!)
     (schedule-tick id)
     id]))

;;@doc
;; Toggle play/pause on the engine whose anchor is at the current cursor line.
;; Returns the engine-id toggled, or #f if no animation under cursor.
(define (animation-toggle-at-cursor)
  (define eid (find-engine-near-cursor))
  (cond
    [(not eid) #f]
    [else
     (define st (state-of eid))
     (define already-paused? (hash-try-get st 'manual-paused?))
     (define new-paused? (not already-paused?))
     (animation-set-pause eid new-paused?)
     (state-update! eid
       (lambda (s) (hash-insert s 'manual-paused? new-paused?)))
     (when (not new-paused?)
       (schedule-tick eid))
     (helix.redraw)
     eid]))

;;@doc
;; Pause every active engine (used by :command palette).
(define (animation-pause-all)
  (for-each
    (lambda (eid)
      (animation-set-pause eid #t)
      (state-update! eid (lambda (s) (hash-insert s 'manual-paused? #t))))
    (hash-keys->list *animations*)))

;;@doc
;; Resume every engine the user explicitly paused.
(define (animation-resume-all)
  (for-each
    (lambda (eid)
      (animation-set-pause eid #f)
      (state-update! eid (lambda (s) (hash-insert s 'manual-paused? #f)))
      (schedule-tick eid))
    (hash-keys->list *animations*)))

;;; ---------------------------------------------------------------------------
;;; Tick scheduler
;;; ---------------------------------------------------------------------------

;; Mean per-tick work measured at 2.6 µs for kitty-replay; the
;; enqueue-thread-local-callback-with-delay primitive is the standard nothelix
;; pattern for self-rescheduling (see execution.scm). When the gate flips
;; false the callback exits without re-arming, so idle cost is one dispatch's
;; worth of work — no busy loop.
(define (schedule-tick eid)
  (define st (state-of eid))
  (when (animation-state-active? st)
    (animation-tick eid) ; advances engine + populates last-tick-* metadata
    (define status (animation-tick-status eid))
    (define delay (max 16 (animation-tick-delay-ms eid)))
    (cond
      [(= status 0)
       ;; Frame produced — pull bytes, register raw content, request redraw.
       (define bytes (animation-tick-bytes eid))
       (when (> (bytes-length bytes) 0)
         (define char-idx (hash-try-get st 'char-idx))
         (define height (animation-tick-height eid))
         (try-add-or-replace-animating-raw-content!
           (editor-focus)
           char-idx
           eid
           bytes
           height
           #t)
         (helix.redraw))]
      [(= status 2)
       ;; Finished — mark and stop.
       (state-update! eid (lambda (s) (hash-insert s 'status 'finished)))]
      [(< status 0)
       (state-update! eid (lambda (s) (hash-insert s 'status 'errored)))]
      ;; status == 1: same content, no transmit, but keep ticking.
      [else (void)])
    (when (and (>= status 0)
               (animation-state-active? (state-of eid)))
      (enqueue-thread-local-callback-with-delay
        delay
        (lambda () (schedule-tick eid))))))

;;; ---------------------------------------------------------------------------
;;; Cursor / overlay lookup
;;; ---------------------------------------------------------------------------

;; Cheap match: cursor's char index falls within `[char-idx, char-idx + 4096)`
;; of an engine's anchor. The plugin doesn't currently track per-overlay
;; height in characters, so we use a generous window — false positives are
;; harmless (toggle only affects engines that actually exist).
(define (find-engine-near-cursor)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define cursor (cursor-position))
  (define candidates
    (filter
      (lambda (eid)
        (define st (state-of eid))
        (and (equal? (hash-try-get st 'doc-id) doc-id)
             (let ([anchor (hash-try-get st 'char-idx)])
               (and (>= cursor anchor)
                    (< cursor (+ anchor 4096))))))
      (hash-keys->list *animations*)))
  (if (null? candidates) #f (car candidates)))

;;; ---------------------------------------------------------------------------
;;; Hooks
;;; ---------------------------------------------------------------------------

;; Fork-only events `document-focus-{lost,gained}` and
;; `viewport-changed`. On a stale `hx` these throw `Unknown event
;; type` at plugin-load; the doctor and `:nothelix-status` warn the
;; user before they get here.
(register-hook! "document-focus-lost"
  (lambda (doc-id)
    (for-each
      (lambda (eid)
        (define st (state-of eid))
        (when (equal? (hash-try-get st 'doc-id) doc-id)
          (state-update! eid (lambda (s) (hash-insert s 'focused? #f)))))
      (hash-keys->list *animations*))))

(register-hook! "document-focus-gained"
  (lambda (doc-id)
    (for-each
      (lambda (eid)
        (define st (state-of eid))
        (when (equal? (hash-try-get st 'doc-id) doc-id)
          (state-update! eid (lambda (s) (hash-insert s 'focused? #t)))
          (schedule-tick eid)))
      (hash-keys->list *animations*))))

(register-hook! "viewport-changed"
  (lambda (_view-id doc-id anchor height)
    (define visible-end (+ anchor (* (max 1 height) 200))) ; ~200 chars/row heuristic
    (for-each
      (lambda (eid)
        (define st (state-of eid))
        (when (equal? (hash-try-get st 'doc-id) doc-id)
          (define cell-anchor (hash-try-get st 'char-idx))
          (define newly-visible?
            (and (>= cell-anchor anchor) (< cell-anchor visible-end)))
          (define was-visible? (hash-try-get st 'visible?))
          (state-update! eid
            (lambda (s) (hash-insert s 'visible? newly-visible?)))
          (when (and newly-visible? (not was-visible?))
            (schedule-tick eid))))
      (hash-keys->list *animations*))))

;; Whole-window focus. Without this, alt-tabbing to a browser leaves
;; `'focused?` true on the active doc and animations keep ticking off-
;; screen. On focus loss we flip the global gate so every engine's
;; `animation-state-active?` returns false; on regain we re-arm by
;; calling `schedule-tick` on each engine (the gate decides whether
;; the tick actually runs — engines whose doc is now backgrounded or
;; whose cell isn't visible stay paused).
(register-hook! "terminal-focus-lost"
  (lambda ()
    (set! *terminal-focused?* #f)))

(register-hook! "terminal-focus-gained"
  (lambda ()
    (set! *terminal-focused?* #t)
    (for-each schedule-tick (hash-keys->list *animations*))))

;;; ---------------------------------------------------------------------------
;;; Discoverability
;;; ---------------------------------------------------------------------------

(define (maybe-show-first-hint!)
  (when (not *first-hint-shown?*)
    (set! *first-hint-shown?* #t)
    (set-status! "animation playing — <space>p to pause")))
