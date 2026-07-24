;;; plot-resize.scm — Live grow/shrink of an @image plot's canvas + re-render

(require "common.scm")
(require "string-utils.scm")
(require "cell-boundaries.scm")
(require "cursor-restore.scm")
(require "image-cache.scm")
(require "widgets.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/ext.scm")

(provide plot-grow
         plot-shrink)

(define *plot-resize-step* 2)
(define *plot-canvas-floor* 2)

;; Nearest `# @image ` marker line at or above from-line, or #false.
(define (find-image-marker-above rope total-lines from-line)
  (let loop ([i (min from-line (- total-lines 1))])
    (cond
      [(< i 0) #false]
      [(string-starts-with? (doc-get-line rope total-lines i) "# @image ") i]
      [else (loop (- i 1))])))

;; Count consecutive blank lines after marker-line up to the next non-blank line.
(define (canvas-blank-count rope total-lines marker-line)
  (let loop ([i (+ marker-line 1)] [n 0])
    (cond
      [(>= i total-lines) n]
      [(line-blank? (doc-get-line rope total-lines i)) (loop (+ i 1) (+ n 1))]
      [else n])))

(define (finish-resize! doc-id new-blanks)
  (sync-images-to-markers!)
  (restore-cursor-for! doc-id)
  (clear-cursor-restore! doc-id)
  (helix.redraw)
  (set-status!
    (string-append "plot resized to " (number->string (+ new-blanks 1)) " rows")))

(define (resize-plot-under-cursor mode)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (set-status! "plot-resize: only runs on .jl notebook files")]
    [else
     (define rope (editor->text doc-id))
     (define total-lines (text.rope-len-lines rope))
     (define cursor-line (current-line-number))
     (define marker-line (find-image-marker-above rope total-lines cursor-line))
     (cond
       [(not marker-line)
        (set-status! "plot-resize: no # @image block above the cursor")]
       [else
        (define blanks (canvas-blank-count rope total-lines marker-line))
        (define canvas-end (+ marker-line 1 blanks))
        (define max-blanks (- *plot-max-rows* 1))
        (cond
          [(eq? mode 'grow)
           (define target (min max-blanks (+ blanks *plot-resize-step*)))
           (define to-insert (- target blanks))
           (cond
             [(<= to-insert 0)
              (set-status!
                (string-append "plot-grow: already at max ("
                               (number->string (+ blanks 1)) " rows)"))]
             [else
              (save-cursor-for-restore! doc-id)
              (move-to-line-start-no-center! rope canvas-end)
              (let loop ([i 0])
                (when (< i to-insert)
                  (helix.static.insert_string "\n")
                  (loop (+ i 1))))
              (helix.static.collapse_selection)
              (helix.static.commit-changes-to-history)
              (finish-resize! doc-id target)])]
          [else
           (define target (max *plot-canvas-floor* (- blanks *plot-resize-step*)))
           (define to-delete (- blanks target))
           (cond
             [(<= to-delete 0)
              (set-status!
                (string-append "plot-shrink: already at min ("
                               (number->string (+ blanks 1)) " rows)"))]
             [else
              (save-cursor-for-restore! doc-id)
              (delete-line-range (- canvas-end to-delete) canvas-end)
              (finish-resize! doc-id target)])])])]))

;;@doc
;; Grow the @image plot block under the cursor by two canvas rows and re-render.
(define (plot-grow)
  (resize-plot-under-cursor 'grow))

;;@doc
;; Shrink the @image plot block under the cursor by two canvas rows and re-render.
(define (plot-shrink)
  (resize-plot-under-cursor 'shrink))

;; --- widget-kind registration (size: @image plot canvas; modal-less) ---

(define (discover-plot-widgets scan)
  (define total (WidgetScan-total scan))
  (define get-line (WidgetScan-get-line scan))
  (let loop ([i 0] [acc '()])
    (if (>= i total)
        (reverse acc)
        (loop (+ i 1)
              (if (string-starts-with? (get-line i) "# @image ")
                  (cons (cons i #false) acc)
                  acc)))))

(register-widget-kind! 'size "plot" ":plot-grow / :plot-shrink" discover-plot-widgets)
