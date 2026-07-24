;;; cell-state.scm — per-cell freshness classification cache + presentation.
;;; Leaf module: parses the kernel's cell_states surface, caches the latest
;;; classification, and maps a state to its picker glyph / header label.

(require "string-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix json-get-cell-states))

(provide refresh-cell-states-from-result!
         parse-cell-states
         set-cell-states!
         clear-cell-states!
         cell-state-for
         cell-state-record-state
         cell-state-record-inputs
         cell-state-record-duration
         cell-duration-for
         cell-glyph-for
         cell-state-glyph
         cell-state-nonfresh?
         cell-state-label
         cell-state-tag-text
         adorn-tag-with-audio
         apply-edited-overrides!
         set-session-baseline!
         session-baseline-for
         marker-line-cell-index
         input-freshness-word
         set-running-cell!
         clear-running-cell!
         cell-running?
         set-audio-playing-cell!
         clear-audio-playing-cell!
         audio-playing-cell?
         set-widget-cells!
         clear-widget-cells!
         cell-has-widget?)

(define *cell-states* (box (hash)))
(define *running-cell* (box #false))
(define *audio-playing-cell* (box #false))
(define *widget-cells* (box (hash)))
(define *session-baselines* (box (hash)))

(define (set-session-baseline! idx hash-str)
  (set-box! *session-baselines*
            (hash-insert (unbox *session-baselines*) idx hash-str)))

(define (session-baseline-for idx)
  (hash-try-get (unbox *session-baselines*) idx))

(define (cell-state-record-state rec) (car rec))
(define (cell-state-record-inputs rec) (cadr rec))
(define (cell-state-record-duration rec)
  (if (>= (length rec) 3) (list-ref rec 2) #false))

(define (parse-state-inputs blob)
  (if (equal? blob "")
      '()
      (filter (lambda (x) x)
              (map (lambda (part)
                     (define fields (string-split part ","))
                     (if (>= (length fields) 3)
                         (list (car fields)
                               (string->number (cadr fields))
                               (list-ref fields 2))
                         #false))
                   (string-split blob ";")))))

(define (parse-cell-states blob)
  (if (or (not blob) (equal? blob "") (string-starts-with? blob "ERROR:"))
      (hash)
      (let loop ([lines (string-split blob "\n")] [acc (hash)])
        (if (null? lines)
            acc
            (let ([parts (string-split (car lines) "\t")])
              (if (< (length parts) 2)
                  (loop (cdr lines) acc)
                  (let ([idx (string->number (car parts))]
                        [state (cadr parts)]
                        [inputs-blob (if (>= (length parts) 3) (list-ref parts 2) "")]
                        [dur-blob (if (>= (length parts) 4) (list-ref parts 3) "")])
                    (loop (cdr lines)
                          (if idx
                              (hash-insert acc idx
                                (list state
                                      (parse-state-inputs inputs-blob)
                                      (if (equal? dur-blob "") #false (string->number dur-blob))))
                              acc)))))))))

(define (set-cell-states! h) (set-box! *cell-states* h))

(define (clear-cell-states!) (set-box! *cell-states* (hash)))

(define (refresh-cell-states-from-result! result-json)
  (define h (parse-cell-states (json-get-cell-states result-json)))
  (set-cell-states! h)
  h)

(define (cell-state-for idx)
  (hash-try-get (unbox *cell-states*) idx))

(define (apply-edited-overrides! idxs)
  (let loop ([xs idxs] [h (unbox *cell-states*)])
    (if (null? xs)
        (set-box! *cell-states* h)
        (let* ([idx (car xs)]
               [prev (hash-try-get h idx)]
               [inputs (if prev (cell-state-record-inputs prev) '())]
               [duration (if prev (cell-state-record-duration prev) #false)])
          (loop (cdr xs) (hash-insert h idx (list "edited-since-run" inputs duration)))))))

(define (cell-duration-for idx)
  (define rec (cell-state-for idx))
  (if rec (cell-state-record-duration rec) #false))

(define (set-running-cell! idx) (set-box! *running-cell* idx))
(define (clear-running-cell!) (set-box! *running-cell* #false))
(define (cell-running? idx)
  (define r (unbox *running-cell*))
  (and r (equal? r idx)))

(define (set-audio-playing-cell! idx) (set-box! *audio-playing-cell* idx))
(define (clear-audio-playing-cell!) (set-box! *audio-playing-cell* #false))
(define (audio-playing-cell? idx)
  (define a (unbox *audio-playing-cell*))
  (and a (equal? a idx)))

(define (set-widget-cells! idxs)
  (let loop ([xs idxs] [h (hash)])
    (if (null? xs)
        (set-box! *widget-cells* h)
        (loop (cdr xs) (hash-insert h (car xs) #true)))))

(define (clear-widget-cells!) (set-box! *widget-cells* (hash)))

(define (cell-has-widget? idx)
  (if (hash-try-get (unbox *widget-cells*) idx) #true #false))

(define (cell-state-nonfresh? state)
  (not (or (equal? state "fresh") (equal? state ""))))

(define (cell-state-glyph state)
  (cond
    [(equal? state "out-of-order") "↕"]
    [(equal? state "stale-input") "○"]
    [(equal? state "orphan-input") "∅"]
    [(equal? state "edited-since-run") "✎"]
    [else ""]))

(define (cell-glyph-for idx)
  (define rec (cell-state-for idx))
  (if rec (cell-state-glyph (cell-state-record-state rec)) ""))

(define (first-input-with-rel inputs rel)
  (cond
    [(null? inputs) #false]
    [(equal? (list-ref (car inputs) 2) rel) (car inputs)]
    [else (first-input-with-rel (cdr inputs) rel)]))

(define (input-name inp) (car inp))
(define (input-writer inp) (cadr inp))

(define (cell-state-label state inputs)
  (cond
    [(equal? state "edited-since-run") "edited"]
    [(equal? state "out-of-order")
     (let ([inp (first-input-with-rel inputs "below")])
       (if inp
           (string-append "uses " (input-name inp) " from cell "
                          (number->string (input-writer inp)) ", below")
           "reads a cell below"))]
    [(equal? state "stale-input")
     (let ([inp (first-input-with-rel inputs "stale")])
       (if inp
           (string-append "input " (input-name inp) " changed in cell "
                          (number->string (input-writer inp)))
           "an input changed"))]
    [(equal? state "orphan-input")
     (let ([inp (first-input-with-rel inputs "orphan")])
       (if inp
           (string-append (input-name inp) " has no defining cell")
           "an input has no defining cell"))]
    [else ""]))

(define (cell-state-tag-text state inputs)
  (string-append "  " (cell-state-glyph state) " " (cell-state-label state inputs)))

;;@doc
;; Prefix a marker tag with the ♪ audio badge when the cell has a playable
;; clip. An empty tag becomes the badge alone; no audio leaves the tag as is.
(define (adorn-tag-with-audio tag has-audio?)
  (cond
    [(not has-audio?) tag]
    [(equal? tag "") "  ♪"]
    [else (string-append "  ♪" (substring tag 1 (string-length tag)))]))

(define (input-freshness-word rel)
  (cond
    [(equal? rel "below") "out of order"]
    [(equal? rel "stale") "stale"]
    [(equal? rel "orphan") "no defining cell"]
    [(equal? rel "fresh") "fresh"]
    [else rel]))

(define (marker-line-cell-index line)
  (define parts (string-split (string-trim line) " "))
  (if (>= (length parts) 2) (string->number (cadr parts)) #false))
