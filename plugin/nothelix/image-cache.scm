;;; image-cache.scm — Kitty inline-image registration and cache management

(require "common.scm")
(require "conceal.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "graphics.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/components.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache!
                          load-image-from-cache
                          clipboard-paste-image
                          cached-image-dims
                          image-dims-b64
                          fit-image-grid
                          djb2-hash))

(provide %cell-index->image-id
         %cell-img->image-id
         %*image-slots-per-cell*
         %plots-per-cell
         %set-plots-per-cell!
         %path->image-id
         %count-image-markers
         %sync-images-to-markers!
         %sync-images-if-markers-changed!
         %extract-cell-index-from-path
         %extract-cell-and-img-from-path
         %render-cached-images
         insert-image
         %insert-image-action
         paste-image
         %paste-image-eligible?
         %paste-image-result->plan
         %parse-dims-wxh
         %parse-grid-cols-rows
         %clamp-plot-grid
         %viewport-plot-box
         %fit-image-to-viewport
         %fitted-grid-for-b64
         %fitted-grid-for-path
         %pad-placeholder-rows
         djb2-hash)

;; State

;; doc-ids already registered (dedup guard).
(define %*rendered-image-docs* (hash))

;; `# @image` marker counts keyed by doc id, for cheap change detection.
(define %*image-marker-counts* (hash))

;;@doc
;; Split a "WxH" dimension reply into (width . height), or #false when the
;; reply is an ERROR or carries no parseable pair.
(define (%parse-dims-wxh reply)
  (%parse-sep-pair reply #\x))

;;@doc
;; Split a "cols,rows" grid reply into (cols . rows), or #false when the
;; reply is an ERROR or carries no parseable pair.
(define (%parse-grid-cols-rows reply)
  (%parse-sep-pair reply #\,))

(define (%parse-sep-pair reply sep)
  (if (or (not (string? reply)) (%string-starts-with? reply "ERROR:"))
      #false
      (let scan ([i 0])
        (cond
          [(>= i (string-length reply)) #false]
          [(char=? (string-ref reply i) sep)
           (let ([left (string->number (substring reply 0 i))]
                 [right (string->number
                          (substring reply (+ i 1) (string-length reply)))])
             (if (and left right (> left 0) (> right 0))
                 (cons left right)
                 #false))]
          [else (scan (+ i 1))]))))

;;@doc
;; Clamp a (cols . rows) grid into the viewport box: columns never exceed
;; the text width minus its margin, rows never exceed the row budget.
(define (%clamp-plot-grid grid viewport-cols max-rows)
  (define avail-cols (max 1 (- viewport-cols 4)))
  (cons (max 1 (min (car grid) avail-cols))
        (max 1 (min (cdr grid) (max 1 max-rows)))))

;;@doc
;; Current text-area size as (viewport-cols . max-plot-rows): the focused
;; buffer width with the 80-column fallback, and the plot row cap bounded
;; by the visible height so a fitted image cannot run past the monitor.
(define (%viewport-plot-box)
  (define area (editor-focused-buffer-area))
  (if area
      (cons (max 1 (area-width area))
            (min %*plot-max-rows* (max 4 (- (area-height area) 4))))
      (cons 80 %*plot-max-rows*)))

;;@doc
;; Fit a WxH image into the viewport box, preserving aspect without ever
;; upscaling: falls back to the configured plot size when the FFI reply
;; is unusable, then clamps into the box.
(define (%fit-image-to-viewport img-w img-h viewport-cols max-rows)
  (define fitted
    (with-handler
      (lambda (_) #false)
      (%parse-grid-cols-rows
        (fit-image-grid img-w img-h viewport-cols max-rows "2.0"))))
  (%clamp-plot-grid (if fitted fitted (cons %*plot-cols* %*plot-rows*))
                    viewport-cols max-rows))

;;@doc
;; Fitted (cols . rows) for base64 image data, or the configured plot size
;; clamped into the viewport box when dimensions are unavailable.
(define (%fitted-grid-for-b64 image-b64 viewport-cols max-rows)
  (define dims
    (with-handler
      (lambda (_) #false)
      (%parse-dims-wxh (image-dims-b64 image-b64))))
  (if dims
      (%fit-image-to-viewport (car dims) (cdr dims) viewport-cols max-rows)
      (%clamp-plot-grid (cons %*plot-cols* %*plot-rows*) viewport-cols max-rows)))

;;@doc
;; Fitted (cols . rows) for a cached sidecar image, or the configured plot
;; size clamped into the viewport box when dimensions are unavailable.
(define (%fitted-grid-for-path jl-path rel-path viewport-cols max-rows)
  (define dims
    (with-handler
      (lambda (_) #false)
      (%parse-dims-wxh (cached-image-dims jl-path rel-path))))
  (if dims
      (%fit-image-to-viewport (car dims) (cdr dims) viewport-cols max-rows)
      (%clamp-plot-grid (cons %*plot-cols* %*plot-rows*) viewport-cols max-rows)))

;;@doc
;; Pad every placeholder row out to the text width with spaces so the
;; `# @image` marker line underneath cannot bleed through past a grid
;; narrower than the marker text.
(define (%pad-placeholder-rows placeholder-rows image-cols viewport-cols)
  (define pad (- viewport-cols image-cols))
  (if (<= pad 0)
      placeholder-rows
      (let ([padding (make-string pad #\space)])
        (%string-join
          (map (lambda (row) (string-append row padding))
               (%string-split placeholder-rows "\n"))
          "\n"))))

;; ID and path helpers

(define %*plots-per-cell* (box 32))

;;@doc
;; Configured cap on how many images MG3 renders per cell. Purely a render
;; cap — it has no bearing on kitty image-id spacing, which is fixed (see
;; `*image-slots-per-cell*`) so it can't drift out from under an already
;; rendered cell if this value changes mid-session.
(define (%plots-per-cell) (unbox %*plots-per-cell*))

;;@doc
;; Override the render cap. Ignores anything that isn't a positive integer;
;; clamps in-range values to [1, 256].
(define (%set-plots-per-cell! n)
  (when (and (exact-integer? n) (> n 0))
    (set-box! %*plots-per-cell* (min 256 n))))

;;@doc
;; Fixed number of kitty image-id slots reserved per cell — independent of
;; the mutable `plots-per-cell` render cap so that an insert and a later
;; clear of the same cell always agree on its id band, even if
;; `plots-per-cell` changed in between (e.g. a project reopen mid-session).
(define %*image-slots-per-cell* 256)

;;@doc
;; Distinct kitty image id for the (cell, image) pair, spaced by the fixed
;; `*image-slots-per-cell*`. `img-index` is wrapped into the per-cell block
;; so an out-of-range caller still lands inside its own cell's slot range.
(define (%cell-img->image-id cell-index img-index)
  (+ 1000 (modulo (+ (* cell-index %*image-slots-per-cell*)
                      (modulo img-index %*image-slots-per-cell*))
                  (- 3999000 %*image-slots-per-cell*))))

;;@doc
;; Derive a stable kitty image id from a cell index (legacy single-image callers).
(define (%cell-index->image-id cell-index)
  (%cell-img->image-id cell-index 0))

;;@doc
;; Derive a stable kitty image id for any image path.
(define (%path->image-id rel-path)
  (define cell-and-img (%extract-cell-and-img-from-path rel-path))
  (if cell-and-img
      (%cell-img->image-id (car cell-and-img) (cdr cell-and-img))
      (+ 1000000 (modulo (djb2-hash rel-path) 3000000))))

;;@doc
;; Best-effort clear of the plot/@image id band [0, 4M); no-op without the fork binding.
(define %*plot-image-id-limit* 4000000)
(define (%clear-plot-image-band!)
  (with-handler
    (lambda (_) #f)
    (eval `(helix.static.clear-raw-content-in-range! 0 ,%*plot-image-id-limit*))))

;;@doc
;; End index of the run of ASCII digits in `rel-path` starting at `start`
;; (first non-digit position, or the string length).
(define (%scan-digit-run rel-path start)
  (let loop ([j start])
    (cond
      [(>= j (string-length rel-path)) j]
      [(and (char>=? (string-ref rel-path j) #\0)
            (char<=? (string-ref rel-path j) #\9))
       (loop (+ j 1))]
      [else j])))

;;@doc
;; Extract (cell-index . img-index) from a `cell-<idx>-<img>` or legacy
;; `cell-<idx>` path segment, or #false if it doesn't match. A legacy path
;; with no img segment resolves to img-index 0.
(define (%extract-cell-and-img-from-path rel-path)
  (define marker "cell-")
  (define marker-len (string-length marker))
  (let scan ([i 0])
    (cond
      [(> (+ i marker-len) (string-length rel-path)) #false]
      [(string=? (substring rel-path i (+ i marker-len)) marker)
       (define num-start (+ i marker-len))
       (define num-end (%scan-digit-run rel-path num-start))
       (if (= num-end num-start)
           #false
           (let ([cell-idx (string->number (substring rel-path num-start num-end))])
             (if (and (< num-end (string-length rel-path))
                      (char=? (string-ref rel-path num-end) #\-))
                 (let* ([img-start (+ num-end 1)]
                        [img-end (%scan-digit-run rel-path img-start)])
                   (if (= img-end img-start)
                       (cons cell-idx 0)
                       (cons cell-idx (string->number (substring rel-path img-start img-end)))))
                 (cons cell-idx 0))))]
      [else (scan (+ i 1))])))

;;@doc
;; Extract the integer N from a `cell-N[-M].png` path, or #false if it doesn't match.
(define (%extract-cell-index-from-path rel-path)
  (define result (%extract-cell-and-img-from-path rel-path))
  (and result (car result)))

;; Marker counting and sync

;;@doc
;; Count the number of `# @image ...` marker lines in the buffer.
(define (%count-image-markers %rope %total-lines)
  (let loop ([line-idx 0] [count 0])
    (if (>= line-idx %total-lines)
        count
        (let ([line (%doc-get-line %rope %total-lines line-idx)])
          (loop (+ line-idx 1)
                (if (%string-starts-with? line "# @image ")
                    (+ count 1)
                    count))))))

;;@doc
;; Re-register images for `# @image` markers still present in the buffer.
(define (%sync-images-to-markers!)
  (define %focus (editor-focus))
  (define %doc-id (editor->doc-id %focus))
  (define %path (editor-document->path %doc-id))
  (when (and %path (%string-suffix? %path ".jl"))
    (%clear-plot-image-band!)
    (set! %*rendered-image-docs* (hash-remove %*rendered-image-docs* %doc-id))
    (%render-cached-images)))

;;@doc
;; Re-sync images only when the `# @image` marker count has changed.
(define (%sync-images-if-markers-changed!)
  (define %focus (editor-focus))
  (define %doc-id (editor->doc-id %focus))
  (define %path (editor-document->path %doc-id))
  (when (and %path (%string-suffix? %path ".jl"))
    (define %rope (editor->text %doc-id))
    (define %total-lines (text.rope-len-lines %rope))
    (define current-count (%count-image-markers %rope %total-lines))
    (define prev-count
      (if (hash-contains? %*image-marker-counts* %doc-id)
          (hash-get %*image-marker-counts* %doc-id)
          -1))
    (when (not (= current-count prev-count))
      (set! %*image-marker-counts*
            (hash-insert %*image-marker-counts* %doc-id current-count))
      (%debug-log
        (string-append "image-cache.sync: marker count "
                       (number->string prev-count) " -> "
                       (number->string current-count)
                       " -- re-registering images"))
      (%sync-images-to-markers!))))

;; Cached image rendering

;;@doc
;; Scan the buffer for `# @image <path>` markers and register the cached images as RawContent.
(define (%render-cached-images)
  (define %focus (editor-focus))
  (define %doc-id (editor->doc-id %focus))
  (define %path (editor-document->path %doc-id))

  (%debug-log
    (string-append "image-cache.render: entry path="
                   (if %path %path "<nil>")
                   " already-rendered="
                   (if (and %path (hash-contains? %*rendered-image-docs* %doc-id)) "yes" "no")))

  (when (and %path
             (%string-suffix? %path ".jl")
             (not (hash-contains? %*rendered-image-docs* %doc-id)))
    (set! %*rendered-image-docs* (hash-insert %*rendered-image-docs* %doc-id #t))
    (%clear-plot-image-band!)

    (define %rope (editor->text %doc-id))
    (define %total-lines (text.rope-len-lines %rope))
    (define registered-count 0)
    (define missing-count 0)

    (let loop ([line-idx 0])
      (when (< line-idx %total-lines)
        (define line (%doc-get-line %rope %total-lines line-idx))
        (cond
          [(%string-starts-with? line "# @image ")
           (define rel-path (%string-trim
                             (substring line 9 (string-length line))))
           (define image-b64 (load-image-from-cache %path rel-path))
           (cond
             [(> (string-length image-b64) 0)
              (define image-id (%path->image-id rel-path))
              (define viewport-box (%viewport-plot-box))
              (define viewport-cols (car viewport-box))
              (define max-image-rows (cdr viewport-box))
              (define fitted
                (%fitted-grid-for-path %path rel-path viewport-cols max-image-rows))
              (define image-cols (car fitted))
              (define available-padding
                (let count ([j (+ line-idx 1)] [n 0])
                  (cond
                    [(>= n max-image-rows) n]
                    [(>= j %total-lines) n]
                    [else
                     (define next-line (%doc-get-line %rope %total-lines j))
                     (define trimmed
                       (if (%string-suffix? next-line "\n")
                           (substring next-line 0 (- (string-length next-line) 1))
                           next-line))
                     (if (= (string-length trimmed) 0)
                         (count (+ j 1) (+ n 1))
                         n)])))
              (define image-rows (min (cdr fitted) max-image-rows (+ 1 available-padding)))
              (define payload
                (with-handler
                  (lambda (_) "ERROR: placeholder-payload-failed")
                  (kitty-placeholder-payload image-b64 image-id)))
              (define raw-placeholder-rows
                (with-handler
                  (lambda (_) "")
                  (kitty-placeholder-rows image-id image-cols image-rows)))
              (define placeholder-rows
                (%pad-placeholder-rows raw-placeholder-rows image-cols viewport-cols))
              (define char-pos (text.rope-line->char %rope line-idx))
              (cond
                [(%string-starts-with? payload "ERROR:")
                 (%debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=payload-error path=" rel-path))]
                [(= (string-length placeholder-rows) 0)
                 (%debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=grid-too-large"))]
                [(< image-rows 1)
                 (%debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=no-padding"))]
                [else
                 (%debug-log
                   (string-append "image-cache.render: REGISTER cell="
                                  (number->string image-id)
                                  " id=" (number->string image-id)
                                  " marker-line=" (number->string line-idx)
                                  " rows=" (number->string image-rows)
                                  " padding=" (number->string available-padding)
                                  " char-pos=" (number->string char-pos)
                                  " payload-bytes=" (number->string (string-length payload))
                                  " rows-bytes=" (number->string (string-length placeholder-rows))))
                 (helix.static.add-raw-content-with-placeholders!
                   payload image-rows image-cols placeholder-rows char-pos)
                 (set! registered-count (+ registered-count 1))])
              (loop (+ line-idx 1))]
             [else
              (%debug-log
                (string-append "image-cache.render: SKIP marker-line="
                               (number->string line-idx)
                               " reason=cache-empty"
                               " rel-path=" rel-path))
              (set! missing-count (+ missing-count 1))
              (loop (+ line-idx 1))])]
          [else (loop (+ line-idx 1))])))

    (%debug-log
      (string-append "image-cache.render: done path=" %path
                     " registered=" (number->string registered-count)
                     " missing=" (number->string missing-count)))
    (when (> missing-count 0)
      (set-status!
        (string-append (number->string missing-count)
                       " # @image file(s) could not be read"
                       " — re-paste the image or re-run the cell")))))

;;@doc
;; Insert a `# @image <path>` marker plus blank canvas lines at the cursor,
;; then re-register images and reconceal so the marker renders immediately.
;; An explicit row count sizes the canvas; otherwise the image is fitted to
;; the viewport so the canvas matches what the render registers.
(define (%insert-image-marker! rel-path . rest)
  (define canvas-rows
    (if (and (not (null? rest)) (number? (car rest)) (> (car rest) 0))
        (car rest)
        (with-handler
          (lambda (_) %*plot-rows*)
          (let ([box (%viewport-plot-box)])
            (cdr (%fitted-grid-for-path
                   (editor-document->path (editor->doc-id (editor-focus)))
                   rel-path (car box) (cdr box)))))))
  (helix.static.insert_string (string-append "# @image " rel-path "\n"))
  (let loop ([i 1])
    (when (< i canvas-rows)
      (helix.static.insert_string "\n")
      (loop (+ i 1))))
  (%sync-images-if-markers-changed!)
  (%schedule-reconceal 50)
  (set-status! (string-append "Inserted @image marker: " rel-path)))

;;@doc
;; Resolve what a bare `insert-image` invocation should do from its command
;; arguments: the trimmed path to insert, the symbol 'prompt when it was
;; called with no arguments at all, or #false for an explicitly empty path.
(define (%insert-image-action args)
  (cond
    [(and (not (null? args))
          (string? (car args))
          (> (string-length (%string-trim (car args))) 0))
     (%string-trim (car args))]
    [(null? args) 'prompt]
    [else #false]))

;;@doc
;; Insert a `# @image <path>` marker plus blank canvas at the cursor. Given a
;; path argument it inserts directly; called with none it prompts for the
;; path instead of failing.
(define (insert-image . args)
  (define action (%insert-image-action args))
  (cond
    [(string? action) (%insert-image-marker! action)]
    [(and (eq? action 'prompt) (not (%nothelix-test-mode?)))
     (push-component! (prompt "image path" %insert-image-from-prompt))]
    [else (set-status! "insert-image: provide an image path")]))

;;@doc
;; Prompt callback: insert the entered path, or explain when it was left blank.
(define (%insert-image-from-prompt raw-path)
  (define trimmed
    (if (string? raw-path) (%string-trim raw-path) ""))
  (if (> (string-length trimmed) 0)
      (%insert-image-marker! trimmed)
      (set-status! "insert-image: provide an image path")))

;;@doc
;; Report whether paste-image may run its clipboard read for the focused
;; document: test mode must be off and the focus must be a .jl notebook.
(define (%paste-image-eligible? %path test-mode?)
  (and (not test-mode?)
       (string? %path)
       (%string-suffix? %path ".jl")))

;;@doc
;; Interpret a clipboard-paste-image FFI reply: `(cons 'insert rel-path)` on
;; success, `(cons 'status message)` when the reply carries an ERROR.
(define (%paste-image-result->plan ffi-result)
  (if (%string-starts-with? ffi-result "ERROR:")
      (cons '%status (string-append "paste-image: " ffi-result))
      (cons 'insert ffi-result)))

;;@doc
;; Paste the image on the system clipboard into the notebook: save it under
;; the notebook's `.nothelix/images/` sidecar and drop a `# @image` marker
;; plus blank canvas at the cursor so it renders inline.
(define (paste-image)
  (define %path
    (with-handler
      (lambda (_) #false)
      (editor-document->path (editor->doc-id (editor-focus)))))
  (cond
    [(not (%paste-image-eligible? %path (%nothelix-test-mode?)))
     (set-status!
       (if (%nothelix-test-mode?)
           "paste-image: skipped in test mode"
           "paste-image: only runs on .jl notebook files"))]
    [else
     (define plan (%paste-image-result->plan (clipboard-paste-image %path)))
     (if (eq? (car plan) 'insert)
         (%insert-image-marker! (cdr plan))
         (set-status! (cdr plan)))]))
