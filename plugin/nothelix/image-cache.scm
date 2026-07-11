;;; image-cache.scm — Kitty inline-image registration and cache management

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "graphics.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache!
                          load-image-from-cache))

(provide cell-index->image-id
         cell-img->image-id
         plots-per-cell
         set-plots-per-cell!
         path->image-id
         count-image-markers
         sync-images-to-markers!
         sync-images-if-markers-changed!
         extract-cell-index-from-path
         extract-cell-and-img-from-path
         render-cached-images
         insert-image
         djb2-hash)

;; State

;; doc-ids already registered (dedup guard).
(define *rendered-image-docs* (hash))

;; `# @image` marker counts keyed by doc id, for cheap change detection.
(define *image-marker-counts* (hash))

;; ID and path helpers

(define *plots-per-cell* (box 32))

;;@doc
;; Configured number of image-id slots reserved per cell.
(define (plots-per-cell) (unbox *plots-per-cell*))

;;@doc
;; Override the number of image-id slots reserved per cell. Ignores anything
;; that isn't a positive integer; clamps in-range values to [1, 256] so the
;; id-space divisor in `cell-img->image-id` can never be non-positive.
(define (set-plots-per-cell! n)
  (when (and (exact-integer? n) (> n 0))
    (set-box! *plots-per-cell* (min 256 n))))

;;@doc
;; Distinct kitty image id for the (cell, image) pair, inside the plot band.
;; `img-index` is wrapped into the per-cell block so an out-of-range caller
;; still lands inside its own cell's slot range.
(define (cell-img->image-id cell-index img-index)
  (define ppc (plots-per-cell))
  (+ 1000 (modulo (+ (* cell-index ppc) (modulo img-index ppc))
                  (- 3999000 ppc))))

;;@doc
;; Derive a stable kitty image id from a cell index (legacy single-image callers).
(define (cell-index->image-id cell-index)
  (cell-img->image-id cell-index 0))

;;@doc
;; djb2 string hash, kept below 2^31 for the kitty id space.
(define (djb2-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

;;@doc
;; Derive a stable kitty image id for any image path.
(define (path->image-id rel-path)
  (define cell-and-img (extract-cell-and-img-from-path rel-path))
  (if cell-and-img
      (cell-img->image-id (car cell-and-img) (cdr cell-and-img))
      (+ 1000000 (modulo (djb2-hash rel-path) 3000000))))

;;@doc
;; Best-effort clear of the plot/@image id band [0, 4M); no-op without the fork binding.
(define *plot-image-id-limit* 4000000)
(define (clear-plot-image-band!)
  (with-handler
    (lambda (_) #f)
    (eval `(helix.static.clear-raw-content-in-range! 0 ,*plot-image-id-limit*))))

;;@doc
;; End index of the run of ASCII digits in `rel-path` starting at `start`
;; (first non-digit position, or the string length).
(define (scan-digit-run rel-path start)
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
(define (extract-cell-and-img-from-path rel-path)
  (define marker "cell-")
  (define marker-len (string-length marker))
  (let scan ([i 0])
    (cond
      [(> (+ i marker-len) (string-length rel-path)) #false]
      [(string=? (substring rel-path i (+ i marker-len)) marker)
       (define num-start (+ i marker-len))
       (define num-end (scan-digit-run rel-path num-start))
       (if (= num-end num-start)
           #false
           (let ([cell-idx (string->number (substring rel-path num-start num-end))])
             (if (and (< num-end (string-length rel-path))
                      (char=? (string-ref rel-path num-end) #\-))
                 (let* ([img-start (+ num-end 1)]
                        [img-end (scan-digit-run rel-path img-start)])
                   (if (= img-end img-start)
                       (cons cell-idx 0)
                       (cons cell-idx (string->number (substring rel-path img-start img-end)))))
                 (cons cell-idx 0))))]
      [else (scan (+ i 1))])))

;;@doc
;; Extract the integer N from a `cell-N[-M].png` path, or #false if it doesn't match.
(define (extract-cell-index-from-path rel-path)
  (define result (extract-cell-and-img-from-path rel-path))
  (and result (car result)))

;; Marker counting and sync

;;@doc
;; Count the number of `# @image ...` marker lines in the buffer.
(define (count-image-markers rope total-lines)
  (let loop ([line-idx 0] [count 0])
    (if (>= line-idx total-lines)
        count
        (let ([line (doc-get-line rope total-lines line-idx)])
          (loop (+ line-idx 1)
                (if (string-starts-with? line "# @image ")
                    (+ count 1)
                    count))))))

;;@doc
;; Re-register images for `# @image` markers still present in the buffer.
(define (sync-images-to-markers!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (and path (string-suffix? path ".jl"))
    (clear-plot-image-band!)
    (set! *rendered-image-docs* (hash-remove *rendered-image-docs* doc-id))
    (render-cached-images)))

;;@doc
;; Re-sync images only when the `# @image` marker count has changed.
(define (sync-images-if-markers-changed!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (and path (string-suffix? path ".jl"))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define current-count (count-image-markers rope total-lines))
    (define prev-count
      (if (hash-contains? *image-marker-counts* doc-id)
          (hash-get *image-marker-counts* doc-id)
          -1))
    (when (not (= current-count prev-count))
      (set! *image-marker-counts*
            (hash-insert *image-marker-counts* doc-id current-count))
      (debug-log
        (string-append "image-cache.sync: marker count "
                       (number->string prev-count) " -> "
                       (number->string current-count)
                       " -- re-registering images"))
      (sync-images-to-markers!))))

;; Cached image rendering

;;@doc
;; Scan the buffer for `# @image <path>` markers and register the cached images as RawContent.
(define (render-cached-images)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (debug-log
    (string-append "image-cache.render: entry path="
                   (if path path "<nil>")
                   " already-rendered="
                   (if (and path (hash-contains? *rendered-image-docs* doc-id)) "yes" "no")))

  (when (and path
             (string-suffix? path ".jl")
             (not (hash-contains? *rendered-image-docs* doc-id)))
    (set! *rendered-image-docs* (hash-insert *rendered-image-docs* doc-id #t))
    (clear-plot-image-band!)

    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define registered-count 0)

    (let loop ([line-idx 0])
      (when (< line-idx total-lines)
        (define line (doc-get-line rope total-lines line-idx))
        (cond
          [(string-starts-with? line "# @image ")
           (define rel-path (string-trim
                             (substring line 9 (string-length line))))
           (define image-b64 (load-image-from-cache path rel-path))
           (cond
             [(> (string-length image-b64) 0)
              (define image-id (path->image-id rel-path))
              (define image-cols *plot-cols*)
              (define max-image-rows *plot-max-rows*)
              (define available-padding
                (let count ([j (+ line-idx 1)] [n 0])
                  (cond
                    [(>= n max-image-rows) n]
                    [(>= j total-lines) n]
                    [else
                     (define next-line (doc-get-line rope total-lines j))
                     (define trimmed
                       (if (string-suffix? next-line "\n")
                           (substring next-line 0 (- (string-length next-line) 1))
                           next-line))
                     (if (= (string-length trimmed) 0)
                         (count (+ j 1) (+ n 1))
                         n)])))
              (define image-rows (min max-image-rows (+ 1 available-padding)))
              (define payload
                (with-handler
                  (lambda (_) "ERROR: placeholder-payload-failed")
                  (kitty-placeholder-payload image-b64 image-id)))
              (define placeholder-rows
                (with-handler
                  (lambda (_) "")
                  (kitty-placeholder-rows image-id image-cols image-rows)))
              (define char-pos (text.rope-line->char rope line-idx))
              (cond
                [(string-starts-with? payload "ERROR:")
                 (debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=payload-error path=" rel-path))]
                [(= (string-length placeholder-rows) 0)
                 (debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=grid-too-large"))]
                [(< image-rows 1)
                 (debug-log
                   (string-append "image-cache.render: SKIP cell="
                                  (number->string image-id)
                                  " reason=no-padding"))]
                [else
                 (debug-log
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
              (debug-log
                (string-append "image-cache.render: SKIP marker-line="
                               (number->string line-idx)
                               " reason=cache-empty"
                               " rel-path=" rel-path))
              (loop (+ line-idx 1))])]
          [else (loop (+ line-idx 1))])))

    (debug-log
      (string-append "image-cache.render: done path=" path
                     " registered=" (number->string registered-count)))))

;;@doc
;; Insert a `# @image <path>` marker plus blank canvas lines at the cursor.
(define (insert-image img-path)
  (cond
    [(or (not img-path) (equal? img-path ""))
     (set-status! "insert-image: provide an image path")]
    [else
     (define trimmed (string-trim img-path))
     (helix.static.insert_string (string-append "# @image " trimmed "\n"))
     (let loop ([i 1])
       (when (< i *plot-rows*)
         (helix.static.insert_string "\n")
         (loop (+ i 1))))
     (set-status! (string-append "Inserted @image marker: " trimmed))]))
