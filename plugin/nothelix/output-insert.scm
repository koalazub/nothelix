;;; output-insert.scm — Cell output insertion into the buffer

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "cursor-restore.scm")
(require "image-cache.scm")
(require "graphics.scm")
(require "kernel.scm")
(require "spinner.scm")
(require "conceal.scm")
(require "chart-viewer.scm")
(require "cell-boundaries.scm")
(require "output-store.scm")
(require "output-render.scm")
(require "cell-state.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          json-get-many
                          json-get-all-images
                          json-get-first-image-bytes
                          json-get-animated-mime
                          json-get-plot-data
                          json-get-notes
                          json-get-text-plots
                          json-get-audio
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache!
                          format-julia-error
                          format-julia-error-with-notebook))

(require "nothelix/animation.scm")
(require "audio.scm")
(require "kernel-widget.scm")

(provide update-cell-output cell-marker-and-code-end clear-cell-output!
         take-first-n images-truncated? notes-blob->group)

;;@doc
;; Strip a trailing newline and split `text` into a list of plain lines; "" yields '().
(define (text->plain-lines text)
  (cond
    [(or (not text) (= (string-length text) 0)) '()]
    [else
     (define trimmed
       (if (string-suffix? text "\n")
           (substring text 0 (- (string-length text) 1))
           text))
     (if (= (string-length trimmed) 0) '() (string-split trimmed "\n"))]))

;;@doc
;; Locates `cell-index`'s marker line and its code-end line, or #false . #false if the marker is gone.
(define (cell-marker-and-code-end rope total-lines cell-index)
  (define cell-marker-line (find-cell-marker-by-index rope total-lines cell-index))
  (if cell-marker-line
      (cons cell-marker-line
            (find-cell-code-end (lambda (idx) (doc-get-line rope total-lines idx))
                                 total-lines (+ cell-marker-line 1)))
      (cons #false #false)))

;;@doc
;; Return the first `n` elements of `lst` (the whole list if shorter than `n`, '() if `n <= 0`).
(define (take-first-n lst n)
  (cond
    [(or (<= n 0) (null? lst)) '()]
    [else (cons (car lst) (take-first-n (cdr lst) (- n 1)))]))

;;@doc
;; #t iff `raw-count` images exceed the per-cell `cap`, i.e. some images will be dropped.
(define (images-truncated? raw-count cap)
  (> raw-count cap))

(define (notes-blob->group notes-blob)
  (if (and notes-blob
           (> (string-length notes-blob) 0)
           (not (string-starts-with? notes-blob "ERROR:")))
      (string-split notes-blob "\n")
      '()))

;;@doc
;; Clear a cell's prior output: virtual text rows at its anchor, any stale
;; `# @image` marker/blank lines left in the buffer from a previous run
;; (deleted and committed as a tagged, non-undo revision), its image id
;; band, and its stored output entry.
(define (clear-cell-output! cell-index)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define marker+code-end (cell-marker-and-code-end rope total-lines cell-index))
  (define cell-code-end (cdr marker+code-end))
  (define anchor-line (and cell-code-end (- cell-code-end 1)))
  (when anchor-line
    (try-clear-output-lines-at! anchor-line))
  (when cell-code-end
    (define (get-line idx) (doc-get-line rope total-lines idx))
    (define region-end (find-cell-region-end get-line total-lines cell-code-end))
    (when (> region-end cell-code-end)
      (delete-line-range cell-code-end region-end #false)
      (try-commit-output-changes!)))
  (define band-start (cell-img->image-id cell-index 0))
  (with-handler
    (lambda (_) #f)
    (eval `(helix.static.clear-raw-content-in-range!
             ,band-start ,(+ band-start *image-slots-per-cell*))))
  (store-clear! (cell-id cell-index)))

;;@doc
;; In-buffer fallback for an hx without the output-lines annotation: insert
;; the classic commented output block below the cell, committed through the
;; tagged path (plain commit on the same old binary). Re-execution deletes it
;; via find-cell-region-end's output-block consumption.
(define (insert-legacy-output-block! anchor-line lines)
  (when (and anchor-line (not (null? lines)))
    (helix.goto (number->string (+ anchor-line 1)))
    (helix.static.goto_line_end_newline)
    (helix.static.insert_string
      (string-append "\n# ─── Output ───\n"
                     (string-join (map (lambda (l) (string-append "# " l)) lines) "\n")
                     "\n# ─────────────\n"))
    (helix.static.collapse_selection)
    (try-commit-output-changes!)))

;;@doc
;; Insert execution results (stdout, stderr, images, errors) into the buffer under the cell's output header.
;; Text-plot styled rows (span lists, not plain strings) are folded only into what's rendered — the store keeps the plain-string rows separately.
(define (update-cell-output result-json jl-path cell-index . rest)
  (define saved-kernel-dir
    (if (and (not (null? rest)) (string? (car rest)))
        (car rest)
        *executing-kernel-dir*))
  (set! *executing-kernel-dir* #false)

  (define plot-data-str (json-get-plot-data result-json))
  (when (and (> (string-length plot-data-str) 0)
             (not (string-starts-with? plot-data-str "ERROR:")))
    (set! *last-plot-data* plot-data-str))

  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define marker+code-end (cell-marker-and-code-end rope total-lines cell-index))
  (define cell-marker-line (car marker+code-end))
  (define cell-code-end (cdr marker+code-end))
  (define anchor-line (and cell-code-end (- cell-code-end 1)))

  (define cell-code
    (if (and cell-marker-line cell-code-end)
        (string-join (extract-cell-code (lambda (idx) (doc-get-line rope total-lines idx))
                                         cell-marker-line cell-code-end)
                      "\n")
        ""))
  (define store-cell-id (cell-id cell-index))
  (define store-source-hash (cell-source-hash cell-code))

  (refresh-cell-states-from-result! result-json)
  (define (bars-for groups)
    (define rec (cell-state-for cell-index))
    (if (and rec (cell-state-nonfresh? (cell-state-record-state rec)))
        (assign-stale-bars groups)
        (assign-cycling-bars groups)))

  (define render-ok? #false)
  (define (render-body)
  (define all-fields (json-get-many result-json "error,structured_error,output_repr,stdout,stderr,has_error"))
  (define unreadable-reply? (string-starts-with? all-fields "ERROR:"))
  (define field-list (if unreadable-reply? '() (string-split all-fields "\t")))
  (define (field-at n) (if (< n (length field-list)) (list-ref field-list n) ""))
  (define err (if unreadable-reply? all-fields (field-at 0)))
  (cond
    [(> (string-length err) 0)
     (define structured (field-at 1))
     (define jl-path (editor-document->path doc-id))
     (define formatted
       (if (and jl-path (string-suffix? jl-path ".jl"))
           (format-julia-error-with-notebook (or structured "") err jl-path)
           (format-julia-error (or structured "") err)))
     (define error-rows (text->plain-lines formatted))
     (when (and anchor-line
                (not (try-set-output-lines-below! anchor-line
                       (bars-for (list error-rows)))))
       (insert-legacy-output-block! anchor-line error-rows))
     (store-put! store-cell-id store-source-hash
                 (encode-outputs+rows
                   (outputs-json-for-cell "" "" "" formatted) error-rows))
     (set-status! (string-append "✗ " err))]
    [else
     (define output-repr (field-at 2))
     (define stdout-text (field-at 3))
     (define stderr-text (field-at 4))
     (define has-error (equal? (field-at 5) "true"))
     (define text-lines
       (if (> (string-length stdout-text) 0) (text->plain-lines stdout-text) '()))

     (define text-plots-blob (json-get-text-plots result-json))
     (define text-plot-groups (decode-text-plots-blob text-plots-blob))
     (define text-plot-ready? (not (null? text-plot-groups)))
     (define text-plot-styled-groups
       (if text-plot-ready?
           (map (lambda (plot) (text-plot->styled-rows (car plot) (cdr plot)))
                text-plot-groups)
           '()))

     (define all-images-str
       (json-get-all-images result-json
                             (if (and saved-kernel-dir (string? saved-kernel-dir))
                                 saved-kernel-dir
                                 "")))
     (define raw-image-list
       (if (> (string-length all-images-str) 0) (string-split all-images-str "\n") '()))
     (define image-list (filter (lambda (s) (> (string-length s) 0)) raw-image-list))
     (define image-count (length image-list))
     (define plot-cap (plots-per-cell))
     (define truncated? (images-truncated? image-count plot-cap))

     (define images-to-place (take-first-n image-list plot-cap))

     (define image-ready #false)
     (define image-error-msg "")
     (define image-rows *plot-rows*)
     (define image-cols *plot-cols*)
     (define registered-images '())
     (define positioned? #false)

     (define (ensure-image-position!)
       (when (not positioned?)
         (set! positioned? #true)
         (when (and anchor-line (>= (+ anchor-line 1) total-lines))
           (helix.goto (number->string (+ anchor-line 1)))
           (helix.static.goto_line_end_newline)
           (helix.static.insert_string "\n")
           (set! total-lines (text.rope-len-lines (editor->text doc-id))))
         (when anchor-line
           (helix.goto (number->string (+ anchor-line 2)))
           (helix.static.goto_line_start))))

     (define (place-image! b64 img-index)
       (define img-id (cell-img->image-id cell-index img-index))
       (define payload
         (with-handler
           (lambda (_) "ERROR: placeholder-payload-failed")
           (kitty-placeholder-payload b64 img-id)))
       (define placeholder-rows
         (with-handler
           (lambda (_) "")
           (kitty-placeholder-rows img-id image-cols image-rows)))
       (cond
         [(string-starts-with? payload "ERROR:")
          (set! image-error-msg
                (string-append image-error-msg "# [Plot "
                               (number->string (+ img-index 1)) ": "
                               (number->string (quotient (string-length b64) 1024))
                               "KB - render failed]\n"))]
         [(= (string-length placeholder-rows) 0)
          (set! image-error-msg
                (string-append image-error-msg "# [Plot "
                               (number->string (+ img-index 1)) ": "
                               (number->string (quotient (string-length b64) 1024))
                               "KB - grid too large for placeholder protocol]\n"))]
         [else
          (ensure-image-position!)
          (define marker-line (current-line-number))
          (define cache-path (save-image-to-cache! jl-path cell-index img-index b64))
          (if (string-starts-with? cache-path "ERROR:")
              (helix.static.insert_string "# @image [render only]\n")
              (helix.static.insert_string (string-append "# @image " cache-path "\n")))
          (let loop ([i 1])
            (when (< i image-rows)
              (helix.static.insert_string "\n")
              (loop (+ i 1))))
          (set! image-ready #true)
          (set! registered-images
                (append registered-images (list (list img-id marker-line payload placeholder-rows))))]))

     (let loop ([entries images-to-place] [idx 0])
       (when (not (null? entries))
         (place-image! (car entries) idx)
         (loop (cdr entries) (+ idx 1))))

     (when (> (string-length image-error-msg) 0)
       (helix.static.insert_string image-error-msg))

     (when (and (not image-ready) (not text-plot-ready?) (> (string-length output-repr) 0))
       (set! text-lines (append text-lines (text->plain-lines output-repr))))

     (define filtered-stderr
       (if (> (string-length stderr-text) 0)
           (let* ([lines (string-split stderr-text "\n")]
                  [keep (filter
                          (lambda (line)
                            (define trimmed (string-trim line))
                            (not (or (= (string-length trimmed) 0)
                                     (string-contains? trimmed "Resolving package versions")
                                     (string-contains? trimmed "No packages added to or removed from")
                                     (string-contains? trimmed "No packages added or removed from")
                                     (string-contains? trimmed "Manifest No packages added")
                                     (string-contains? trimmed "Project No packages added")
                                     (and (string-contains? trimmed "Precompiling")
                                          (not (string-contains? trimmed "error")))
                                     (and (string-contains? trimmed "Progress")
                                          (not (string-contains? trimmed "error"))))))
                          lines)])
             (string-join keep "\n"))
           ""))
     (when (> (string-length (string-trim filtered-stderr)) 0)
       (set! text-lines (append text-lines (cons "stderr:" (text->plain-lines filtered-stderr)))))

     (define notes-group (notes-blob->group (json-get-notes result-json)))
     (define stored-text-lines (append notes-group text-lines))
     (define stdout-group
       (if (> (string-length stdout-text) 0) (text->plain-lines stdout-text) '()))
     (define repr-group
       (if (and (not image-ready) (not text-plot-ready?) (> (string-length output-repr) 0))
           (text->plain-lines output-repr)
           '()))
     (define stderr-group
       (if (> (string-length (string-trim filtered-stderr)) 0)
           (cons "stderr:" (text->plain-lines filtered-stderr))
           '()))
     (define audio-blob
       (let ([b (json-get-audio result-json)])
         (if (and (> (string-length b) 0) (not (string-starts-with? b "ERROR:"))) b "")))
     (define waveform-group (waveform-group-for audio-blob -1 -1 -1))
     (define widgets-blob
       (let ([b (json-get-widgets result-json)])
         (if (and (> (string-length b) 0) (not (string-starts-with? b "ERROR:"))) b "")))
     (define widget-group (widget-group-for widgets-blob))
     (define render-lines
       (bars-for
         (append (list notes-group stdout-group repr-group stderr-group)
                 text-plot-styled-groups
                 (if (null? waveform-group) '() (list waveform-group))
                 (if (null? widget-group) '() (list widget-group)))))

     (when (and anchor-line
                (not (null? render-lines))
                (not (try-set-output-lines-below! anchor-line render-lines)))
       (insert-legacy-output-block! anchor-line stored-text-lines))
     (store-put! store-cell-id store-source-hash
                 (encode-outputs+rows+text-plots+audio+widgets
                   (outputs-json-for-cell stdout-text filtered-stderr output-repr "")
                   stored-text-lines
                   text-plots-blob
                   audio-blob
                   widgets-blob))

     (define animated-mime
       (json-get-animated-mime result-json))
     (define is-animated? (> (string-length animated-mime) 0))

     (when image-ready
       (define focus (editor-focus))
       (define doc-id (editor->doc-id focus))
       (define rope (editor->text doc-id))
       (define total-lines (text.rope-len-lines rope))

       (define (register-image! entry first?)
         (define img-id (list-ref entry 0))
         (define marker-line (list-ref entry 1))
         (define payload (list-ref entry 2))
         (define placeholder-rows (list-ref entry 3))
         (define safe-line
           (cond
             [(< marker-line 0) 0]
             [(>= marker-line total-lines) (- total-lines 1)]
             [else marker-line]))
         (define char-idx (text.rope-line->char rope safe-line))
         (debug-log
           (string-append "output-insert.update-cell-output: register image cell="
                          (number->string cell-index)
                          " id=" (number->string img-id)
                          " marker-line=" (number->string safe-line)
                          " char-idx=" (number->string char-idx)
                          " total-lines=" (number->string total-lines)
                          " payload-bytes=" (number->string (string-length payload))
                          " rows-bytes=" (number->string (string-length placeholder-rows))
                          " animated-mime=" (if first? animated-mime "")))
         (with-handler
           (lambda (_) #f)
           (eval `(helix.static.clear-raw-content-in-range! ,img-id ,(+ img-id 1))))
         (define static-fallback!
           (lambda ()
             (helix.static.add-raw-content-with-placeholders!
               payload image-rows image-cols placeholder-rows char-idx)))
         (cond
           [(and first? is-animated?)
            (define raw-bytes
              (json-get-first-image-bytes result-json (or saved-kernel-dir "")))
            (define registered?
              (and (> (bytes-length raw-bytes) 0)
                   (register-animation! animated-mime raw-bytes char-idx image-rows)))
            (when (not registered?)
              (static-fallback!))]
           [else
            (static-fallback!)]))

       (let loop ([entries registered-images] [first? #true])
         (when (not (null? entries))
           (register-image! (car entries) first?)
           (loop (cdr entries) #false))))

     (helix.static.collapse_selection)
     (try-commit-output-changes!)

     (define base-status
       (cond
         [has-error "Cell executed with errors"]
         [(or image-ready text-plot-ready?) "✓ Cell executed (with plot)"]
         [else "✓ Cell executed"]))
     (set-status!
       (if truncated?
           (string-append base-status
                          " (showing " (number->string plot-cap) " of "
                          (number->string image-count) " plots — plots-per-cell cap)")
           base-status))]))

  (with-cursor-restore doc-id
    (lambda ()
      (helix.redraw)
      (when (not render-ok?)
        (set-status! "✗ Cell output render failed")))
    (lambda ()
      (render-body)
      (set! render-ok? #true)))

  (audio-auto-play-from-result! result-json cell-index)

  (schedule-reconceal 50))
