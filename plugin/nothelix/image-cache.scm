;;; image-cache.scm - Kitty image registration and cache management
;;;
;;; Manages the lifecycle of inline images rendered via the Kitty Unicode
;;; placeholder protocol. Tracks which documents have already had their
;;; cached images registered (to avoid duplicate RawContent entries on
;;; stock Helix), provides marker-count-based change detection so the
;;; post-command hook can cheaply decide whether a re-sync is needed,
;;; and handles the actual rendering of cached images on document open.

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "graphics.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "helix/ext.scm")

;; FFI imports for image operations
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache!
                          load-image-from-cache))

(provide cell-index->image-id
         path->image-id
         maybe-clear-raw-content!
         count-image-markers
         sync-images-to-markers!
         sync-images-if-markers-changed!
         extract-cell-index-from-path
         render-cached-images
         insert-image)

;;; ============================================================================
;;; IMAGE DEDUP STATE
;;; ============================================================================

;; Set of document ids we've already re-registered cached images for.
;; Stock Helix's `add-raw-content!` unconditionally appends to a per-view
;; `Vec<RawContent>` -- it has no dedup and no `clear-raw-content!` binding
;; to flush stale entries. Running `render-cached-images` more than once
;; per document therefore accumulates duplicate registrations at drifting
;; char positions, which the terminal renderer draws as stacked images
;; (the "smeared Matrix C" artefact). We guard against that by tracking
;; which doc-ids we've already rendered and refusing to rerun for the
;; same doc. The fork's `add_or_replace_raw_content` would make this
;; guard unnecessary, but the guard is harmless on the fork and correct
;; on stock Helix, so we always use it.
;;
;; Represented as a hashmap -> boolean because Steel's core collections
;; API documents `hash`, `hash-insert`, `hash-contains?` but no
;; first-class hashset primitive.
(define *rendered-image-docs* (hash))

;; Cache of `# @image ...` marker-line counts, keyed by doc id. The
;; `sync-images-if-markers-changed!` hook compares the buffer's
;; current count against this cache on every mutation and only does
;; the expensive re-register pass when the number actually changed.
;; Typical typing leaves the count alone so the hook is O(lines) for
;; the scan and O(1) for the comparison.
(define *image-marker-counts* (hash))

;;; ============================================================================
;;; ID AND PATH HELPERS
;;; ============================================================================

;;@doc
;; Derive a stable kitty image id from a cell index. Offset by 1000 so we
;; don't collide with whatever low-id range the terminal reserves. Two
;; invocations with the same `cell-index` produce the same id, which lets
;; the idempotent `add-raw-content!` in the Helix fork replace-in-place
;; rather than accumulate duplicates across re-execution and buffer switches.
(define (cell-index->image-id cell-index)
  (+ 1000 cell-index))

;;@doc
;; djb2 string hash — used to derive a stable kitty image id for arbitrary
;; image paths that don't follow the `cell-N.png` convention. Result is
;; kept below 2^31 so it fits the 32-bit id space the protocol uses.
(define (djb2-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

;;@doc
;; Derive a stable kitty image id for any image path.
;;
;; For paths matching `.nothelix/images/cell-N.png`, returns the same id as
;; `(cell-index->image-id N)` so kernel-produced outputs keep their existing
;; id → in-place replacement behavior. For arbitrary paths (user-inserted
;; images, converter-emitted paths that don't round-trip through the kernel),
;; hashes the path into a disjoint id range (>= 2_000_000) so it can't
;; collide with the cell-index range.
(define (path->image-id rel-path)
  (define cell-idx (extract-cell-index-from-path rel-path))
  (if cell-idx
      (cell-index->image-id cell-idx)
      (+ 2000000 (modulo (djb2-hash rel-path) 100000000))))

;;@doc
;; Best-effort `clear-raw-content!` wrapper.
;;
;; The underlying Helix fork binding was added recently; stock or older
;; Helix builds don't register `helix.static.clear-raw-content!`, and
;; naming it directly from Scheme would produce a `FreeIdentifier`
;; error at parse time. We defer the reference through a quoted
;; expression passed to `eval`, so the symbol is only resolved at
;; call time, and wrap the whole thing in `with-handler` so a missing
;; binding becomes a silent no-op. When the fork is present this
;; degenerates into a direct call; when it isn't, image dedup still
;; works because every entry uses a stable id (see
;; `cell-index->image-id`). This mirrors the pattern helix/keymaps.scm
;; uses for its optional function-pointer lookups.
(define (maybe-clear-raw-content!)
  (with-handler
    (lambda (_) #f)
    (eval '(helix.static.clear-raw-content!))))

;;@doc
;; Extract the integer N from a `.nothelix/images/cell-N.png` path.
;; Returns `#false` if the path doesn't match the expected format.
(define (extract-cell-index-from-path rel-path)
  (define marker "cell-")
  (define marker-len (string-length marker))
  (let scan ([i 0])
    (cond
      [(> (+ i marker-len) (string-length rel-path)) #false]
      [(string=? (substring rel-path i (+ i marker-len)) marker)
       (define num-start (+ i marker-len))
       (let num-scan ([j num-start])
         (cond
           [(>= j (string-length rel-path))
            (if (> j num-start)
                (string->number (substring rel-path num-start j))
                #false)]
           [(and (char>=? (string-ref rel-path j) #\0)
                 (char<=? (string-ref rel-path j) #\9))
            (num-scan (+ j 1))]
           [else
            (if (> j num-start)
                (string->number (substring rel-path num-start j))
                #false)]))]
      [else (scan (+ i 1))])))

;;; ============================================================================
;;; MARKER COUNTING AND SYNC
;;; ============================================================================

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
;; Clear every RawContent entry on the focused view and then re-run
;; `render-cached-images` so only images whose `# @image` marker
;; lines still exist in the buffer end up registered. The two-step
;; "clear then re-register" pattern is how we bind an image's
;; lifetime to its marker line: a marker deleted by backspacing (or
;; any other edit) simply doesn't make it back into the RawContent
;; set on the next sync.
;;
;; We also clear the per-doc entry from `*rendered-image-docs*` so
;; `render-cached-images` doesn't short-circuit on its "already
;; rendered" guard -- that guard exists to prevent duplicate
;; registration on buffer focus changes, which is a different
;; problem than a deliberate re-sync.
(define (sync-images-to-markers!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (and path (string-suffix? path ".jl"))
    (maybe-clear-raw-content!)
    (set! *rendered-image-docs* (hash-remove *rendered-image-docs* doc-id))
    (render-cached-images)))

;;@doc
;; Cheap wrapper that only re-syncs when the `# @image` marker line
;; count has changed since the last sync. Called from
;; `post-insert-char` and `post-command` -- when you're just typing a
;; word mid-file the count check returns immediately and no image
;; re-registration happens; when you delete an `# @image` line
;; (via backspace, `xd`, `delete-selection`, etc.) the count drops
;; and we run the full sync.
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

;;; ============================================================================
;;; CACHED IMAGE RENDERING
;;; ============================================================================

;;@doc
;; Scan the current buffer for `# @image <path>` markers and re-register
;; the cached images as Helix RawContent entries.
;;
;; Called exactly once per document via the `document-opened` hook. A
;; `*rendered-image-docs*` guard short-circuits subsequent invocations
;; for the same doc-id so we never double-register -- which would cause
;; the "stacked Matrix C" smearing on stock Helix, where the underlying
;; `add_raw_content` has no built-in dedup.
;;
;; Images registered here persist on the document (keyed by ViewId) for
;; the lifetime of the document, so buffer switches do not need to
;; re-run this -- Helix retains the entries across view focus changes.
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
    ;; Mark the doc as rendered BEFORE we register anything, so that if
    ;; a reentrant call reaches this point it takes the early-return.
    (set! *rendered-image-docs* (hash-insert *rendered-image-docs* doc-id #t))

    ;; Best-effort flush on Helix fork builds that expose the binding.
    ;; On stock Helix this is a no-op; the guard above is what actually
    ;; prevents accumulation.
    (maybe-clear-raw-content!)

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
              (define max-image-rows *plot-rows*)
              ;; Count how many blank lines follow this `# @image`
              ;; line before we hit the next content. That tells us
              ;; how many real buffer rows the grid actually has to
              ;; paint on -- if the user backspaced some padding, we
              ;; render a shorter image instead of stomping on the
              ;; footer or the next cell header.
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
              ;; `image-rows` = marker line (1) + available blank
              ;; lines below. Clamped to the full grid height so
              ;; we never exceed the placeholder table's
              ;; max-dim guarantees.
              (define image-rows (min max-image-rows (+ 1 available-padding)))
              (define payload (kitty-placeholder-payload image-b64 image-id))
              (define placeholder-rows
                (kitty-placeholder-rows image-id image-cols image-rows))
              ;; Anchor at the marker line itself -- row 0 of the
              ;; image overwrites the `# @image ...` text on reopen,
              ;; matching the live-execution anchoring in
              ;; `update-cell-output`. Rows 1..image-rows-1 cover
              ;; the blank lines immediately below the marker.
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
;; Insert a `# @image <path>` marker at the current cursor position,
;; followed by `*plot-rows*` truly-blank lines that serve as the image's
;; render canvas. The path is taken verbatim — arbitrary paths work
;; (e.g. `diagrams/foo.png`, `../shared/bar.jpg`), and the FFI
;; `load-image-from-cache` resolves the path relative to the notebook
;; file's directory. A buffer-level re-sync runs via the usual
;; post-command hook, so the image paints in place on the next redraw.
(define (insert-image img-path)
  (cond
    [(or (not img-path) (equal? img-path ""))
     (set-status! "insert-image: provide an image path")]
    [else
     (define trimmed (string-trim img-path))
     (helix.static.insert_string (string-append "# @image " trimmed "\n"))
     ;; Pad with bare blank lines (not `# ` comment lines — the
     ;; placeholder canvas detector in `render-cached-images` counts
     ;; length-zero lines as blank, and `# ` would register as content
     ;; and truncate the image height.
     (let loop ([i 1])
       (when (< i *plot-rows*)
         (helix.static.insert_string "\n")
         (loop (+ i 1))))
     (set-status! (string-append "Inserted @image marker: " trimmed))]))
