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
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          json-get-many
                          json-get-first-image
                          json-get-first-image-with-dir
                          json-get-first-image-bytes
                          json-get-animated-mime
                          json-get-plot-data
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache!
                          format-julia-error
                          format-julia-error-with-notebook))

(require "nothelix/animation.scm")

(provide update-cell-output cell-marker-and-code-end clear-cell-output!)

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
;; Clear a cell's prior output: virtual text rows at its anchor, its image id
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
  (define image-id (cell-index->image-id cell-index))
  (with-handler
    (lambda (_) #f)
    (eval `(helix.static.clear-raw-content-in-range! ,image-id ,(+ image-id 1))))
  (store-clear! (cell-id cell-index)))

;;@doc
;; Insert execution results (stdout, stderr, images, errors) into the buffer under the cell's output header.
(define (update-cell-output result-json jl-path cell-index . rest)
  (define saved-kernel-dir
    (if (and (not (null? rest)) (string? (car rest)))
        (car rest)
        *executing-kernel-dir*))
  (set! *executing-kernel-dir* #false)

  (define plot-data-str (json-get-plot-data result-json))
  (when (> (string-length plot-data-str) 0)
    (set! *last-plot-data* plot-data-str))

  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define marker+code-end (cell-marker-and-code-end rope total-lines cell-index))
  (define cell-marker-line (car marker+code-end))
  (define cell-code-end (cdr marker+code-end))
  (define anchor-line (and cell-code-end (- cell-code-end 1)))

  (when (and anchor-line (>= (+ anchor-line 1) total-lines))
    (helix.goto (number->string (+ anchor-line 1)))
    (helix.static.goto_line_end_newline)
    (helix.static.insert_string "\n")
    (set! total-lines (text.rope-len-lines (editor->text doc-id))))

  (when anchor-line
    (helix.goto (number->string (+ anchor-line 2)))
    (helix.static.goto_line_start))

  (define cell-code
    (if (and cell-marker-line cell-code-end)
        (string-join (extract-cell-code (lambda (idx) (doc-get-line rope total-lines idx))
                                         cell-marker-line cell-code-end)
                      "\n")
        ""))
  (define store-cell-id (cell-id cell-index))
  (define store-source-hash (cell-source-hash cell-code))

  (define all-fields (json-get-many result-json "error,structured_error,output_repr,stdout,stderr,has_error"))
  (define field-list (string-split all-fields "\t"))
  (define (field-at n) (if (< n (length field-list)) (list-ref field-list n) ""))
  (define err (field-at 0))
  (cond
    [(> (string-length err) 0)
     (define structured (field-at 1))
     (define jl-path (editor-document->path doc-id))
     (define formatted
       (if (and jl-path (string-suffix? jl-path ".jl"))
           (format-julia-error-with-notebook (or structured "") err jl-path)
           (format-julia-error (or structured "") err)))
     (define error-rows (text->plain-lines formatted))
     (when anchor-line
       (try-set-output-lines-below! anchor-line error-rows))
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

     (define image-b64
       (if (and saved-kernel-dir (string? saved-kernel-dir))
           (json-get-first-image-with-dir result-json saved-kernel-dir)
           (json-get-first-image result-json)))
     (define image-ready #false)
     (define image-error-msg "")
     (define image-id 0)
     (define image-rows *plot-rows*)
     (define image-cols *plot-cols*)
     (define image-payload "")
     (define image-placeholder-rows "")

     (when (> (string-length image-b64) 0)
       (set! image-id (cell-index->image-id cell-index))
       (set! image-payload (kitty-placeholder-payload image-b64 image-id))
       (set! image-placeholder-rows
             (kitty-placeholder-rows image-id image-cols image-rows))
       (cond
         [(string-starts-with? image-payload "ERROR:")
          (set! image-error-msg
                (string-append "# [Plot: "
                               (number->string (quotient (string-length image-b64) 1024))
                               "KB - render failed]\n"))]
         [(= (string-length image-placeholder-rows) 0)
          (set! image-error-msg
                (string-append "# [Plot: "
                               (number->string (quotient (string-length image-b64) 1024))
                               "KB - grid too large for placeholder protocol]\n"))]
         [else
          (set! image-ready #true)]))

     (define image-marker-line -1)
     (when image-ready
       (set! image-marker-line (current-line-number))
       (define cache-path (save-image-to-cache! jl-path cell-index image-b64))
       (if (string-starts-with? cache-path "ERROR:")
           (helix.static.insert_string "# @image [render only]\n")
           (helix.static.insert_string (string-append "# @image " cache-path "\n")))
       (let loop ([i 1])
         (when (< i image-rows)
           (helix.static.insert_string "\n")
           (loop (+ i 1)))))

     (when (> (string-length image-error-msg) 0)
       (helix.static.insert_string image-error-msg))

     (when (and (not image-ready) (> (string-length output-repr) 0))
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

     (when anchor-line
       (try-set-output-lines-below! anchor-line text-lines))
     (store-put! store-cell-id store-source-hash
                 (encode-outputs+rows
                   (outputs-json-for-cell stdout-text filtered-stderr output-repr "")
                   text-lines))

     (define animated-mime
       (json-get-animated-mime result-json))
     (define is-animated? (> (string-length animated-mime) 0))

     (when image-ready
       (define focus (editor-focus))
       (define doc-id (editor->doc-id focus))
       (define rope (editor->text doc-id))
       (define total-lines (text.rope-len-lines rope))
       (define safe-line
         (cond
           [(< image-marker-line 0) 0]
           [(>= image-marker-line total-lines) (- total-lines 1)]
           [else image-marker-line]))
       (define char-idx (text.rope-line->char rope safe-line))
       (debug-log
         (string-append "output-insert.update-cell-output: register image cell="
                        (number->string cell-index)
                        " id=" (number->string image-id)
                        " marker-line=" (number->string safe-line)
                        " char-idx=" (number->string char-idx)
                        " total-lines=" (number->string total-lines)
                        " payload-bytes=" (number->string (string-length image-payload))
                        " rows-bytes=" (number->string (string-length image-placeholder-rows))
                        " animated-mime=" animated-mime))
       (with-handler
         (lambda (_) #f)
         (eval `(helix.static.clear-raw-content-in-range! ,image-id ,(+ image-id 1))))
       (define static-fallback!
         (lambda ()
           (helix.static.add-raw-content-with-placeholders!
             image-payload image-rows image-cols image-placeholder-rows char-idx)))
       (cond
         [is-animated?
          (define raw-bytes
            (json-get-first-image-bytes result-json (or saved-kernel-dir "")))
          (define registered?
            (and (> (bytes-length raw-bytes) 0)
                 (register-animation! animated-mime raw-bytes char-idx image-rows)))
          (when (not registered?)
            (static-fallback!))]
         [else
          (static-fallback!)]))

     (helix.static.collapse_selection)
     (helix.static.commit-changes-to-history)

     (if has-error
         (set-status! "Cell executed with errors")
         (if image-ready
             (set-status! "✓ Cell executed (with plot)")
             (set-status! "✓ Cell executed")))])

  (restore-cursor-for! doc-id)

  (helix.redraw)

  (schedule-reconceal 50))
