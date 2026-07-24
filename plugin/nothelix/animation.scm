;;; animation.scm — animated media driver bridging fork events to libnothelix engines

(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require-builtin helix/core/text as text.)
(require-builtin steel/time)

(define *animation-ffi-warned?* #f)
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

;; State

;; engine_id -> hash with keys: 'char-idx 'doc-id 'height 'focused?
;; 'visible? 'manual-paused? 'status.
(define *animations* (hash))
(define *first-hint-shown?* #f)
;; True while the hx terminal window holds focus.
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

;; Public commands

;;@doc
;; Register an animation engine and start its tick loop; returns the engine id or #f on failure.
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
;; Toggle play/pause on the animation under the cursor; returns its engine id or #f.
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
;; Pause every active engine.
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

;; Tick scheduler

(define (schedule-tick eid)
  (define st (state-of eid))
  (when (animation-state-active? st)
    (animation-tick eid)
    (define status (animation-tick-status eid))
    (define delay (max 16 (animation-tick-delay-ms eid)))
    (cond
      [(= status 0)
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
       (state-update! eid (lambda (s) (hash-insert s 'status 'finished)))]
      [(< status 0)
       (state-update! eid (lambda (s) (hash-insert s 'status 'errored)))]
      ;; status 1: unchanged content, keep ticking.
      [else (void)])
    (when (and (>= status 0)
               (animation-state-active? (state-of eid)))
      (enqueue-thread-local-callback-with-delay
        delay
        (lambda () (schedule-tick eid))))))

;; Cursor / overlay lookup

;; Match engines whose anchor is within 4096 chars of the cursor.
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

;; Hooks

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

(register-hook! "terminal-focus-lost"
  (lambda ()
    (set! *terminal-focused?* #f)))

(register-hook! "terminal-focus-gained"
  (lambda ()
    (set! *terminal-focused?* #t)
    (for-each schedule-tick (hash-keys->list *animations*))))

(define (maybe-show-first-hint!)
  (when (not *first-hint-shown?*)
    (set! *first-hint-shown?* #t)
    (set-status! "animation playing — <space>p to pause")))

;; --- widget-kind registration (toggle: an animation at the cursor; modal-less) ---

(define (discover-animation-widgets scan)
  (define doc-id (WidgetScan-doc-id scan))
  (define rope (WidgetScan-rope scan))
  (define len (text.rope-len-chars rope))
  (let loop ([eids (hash-keys->list *animations*)] [acc '()])
    (if (null? eids)
        (reverse acc)
        (let ([st (state-of (car eids))])
          (define ci (and st (hash-try-get st 'char-idx)))
          (if (and st (equal? (hash-try-get st 'doc-id) doc-id) ci (< ci len))
              (loop (cdr eids) (cons (cons (text.rope-char->line rope ci) #false) acc))
              (loop (cdr eids) acc))))))

(register-widget-kind! 'toggle "animation" "<space>p toggle" discover-animation-widgets)
