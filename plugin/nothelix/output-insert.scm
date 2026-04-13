;;; output-insert.scm - Cell output insertion into the buffer
;;;
;;; Handles the insertion of execution results (stdout, stderr, images,
;;; errors) into the buffer under a cell's output header. The heavy
;;; lifting lives in `update-cell-output`, which coordinates text
;;; insertion, Kitty placeholder image registration, cursor restoration,
;;; and concealment refresh.

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
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/ext.scm")

;; FFI imports for output processing
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          json-get-many
                          json-get-first-image
                          json-get-first-image-with-dir
                          json-get-plot-data
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          save-image-to-cache
                          format-julia-error))

(provide update-cell-output
         commentify
         find-output-header-for-cell)

;;; ============================================================================
;;; TEXT COMMENTIFICATION
;;; ============================================================================

;;@doc
;; Prefix every line of `text` with `# ` so it's a safe Julia comment
;; when inserted into a notebook buffer. Preserves the original line
;; structure (including empty lines) and normalises the trailing
;; newline so there's exactly one.
;;
;; This is the defense against raw stdout / stderr / output_repr
;; leaking out of the output section and being picked up as real
;; Julia code by:
;;
;;   * The Julia LSP's StaticLint, which parses the whole buffer and
;;     throws "extra tokens after end of expression" all over the
;;     output block when it finds non-comment text there.
;;   * The next `:execute-cell` extraction, which walks line-by-line
;;     between cell markers and forwards anything that isn't itself
;;     a marker / separator -- raw matrix output with lines like
;;     `1.0  1.0  0.0 ...` would be forwarded into the kernel as code
;;     and usually destroys the cell's next execution.
(define (commentify text)
  (cond
    [(= (string-length text) 0) ""]
    [else
     (define trimmed
       (if (string-suffix? text "\n")
           (substring text 0 (- (string-length text) 1))
           text))
     (string-append
       (string-join
         (map (lambda (line) (string-append "# " line))
              (string-split trimmed "\n"))
         "\n")
       "\n")]))

;;; ============================================================================
;;; OUTPUT HEADER LOOKUP
;;; ============================================================================

;;@doc
;; Locate the "# --- Output ---" header line for a specific cell.
;; Scans forward from the cell's `@cell N` marker and returns the
;; 0-indexed line index of the header, or `#false` if not found
;; before the next cell boundary / EOF. Robust across documents with
;; many cells because the search is bounded by the cell's span.
(define (find-output-header-for-cell rope total-lines cell-index)
  (define cell-line (find-cell-marker-by-index rope total-lines cell-index))
  (cond
    [(not cell-line) #false]
    [else
     (let scan ([idx (+ cell-line 1)])
       (cond
         [(>= idx total-lines) #false]
         [(cell-marker-line? rope total-lines idx) #false]
         [(string-contains? (doc-get-line rope total-lines idx) "─── Output ───") idx]
         [else (scan (+ idx 1))]))]))

;;; ============================================================================
;;; CELL OUTPUT INSERTION
;;; ============================================================================

;;@doc
;; Insert execution results into the buffer under the cell's output
;; header. Handles stdout, stderr, images (via Kitty placeholder
;; protocol), and errors. `jl-path` and `cell-index` are used to
;; persist images to the cache directory.
;;
;; Important: the spinner no longer appears anywhere in the buffer.
;; Execution status animates in the status line only, so Helix's undo
;; history doesn't accumulate spinner-frame commits and the buffer
;; stays clean between "start execution" and "result available".
;; This function locates the cell's `# --- Output ---` header and
;; inserts the result content right below it.
(define (update-cell-output result-json jl-path cell-index . rest)
  ;; kernel-dir is passed explicitly by callers that have it.
  ;; Falls back to the global *executing-kernel-dir* for compat.
  (define saved-kernel-dir
    (if (and (not (null? rest)) (string? (car rest)))
        (car rest)
        *executing-kernel-dir*))
  (set! *executing-kernel-dir* #false)

  ;; Stash raw plot data for the interactive chart viewer (:view-plot).
  (define plot-data-str (json-get-plot-data result-json))
  (when (> (string-length plot-data-str) 0)
    (set! *last-plot-data* plot-data-str))

  ;; Re-read document state fresh.
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  ;; Navigate to the line immediately after this cell's output header,
  ;; which is where all the inserted content below will land. If we
  ;; can't find a header (e.g. the user deleted it manually between
  ;; execute-cell and the result arriving), fall through silently --
  ;; the inserts will still land at whatever the cursor's current
  ;; position is, which is the best we can do.
  (define header-line (find-output-header-for-cell rope total-lines cell-index))
  (when header-line
    (helix.goto (number->string (+ header-line 2)))
    (helix.static.goto_line_start))

  ;; Rust kernel_poll_result flattens the response:
  ;; {"status": "ok", "stdout": "...", "output_repr": "...", ...}
  ;; Parse all fields once via json-get-many (single serde parse)
  (define all-fields (json-get-many result-json "error,structured_error,output_repr,stdout,stderr,has_error"))
  (define field-list (string-split all-fields "\t"))
  (define (field-at n) (if (< n (length field-list)) (list-ref field-list n) ""))
  (define err (field-at 0))
  (cond
    [(> (string-length err) 0)
     ;; Try the Rust formatter for a guided error message.
     ;; Falls back to the raw error if structured_error is absent.
     (define structured (field-at 1))
     (define formatted (format-julia-error (or structured "") err))
     (helix.static.insert_string (commentify formatted))
     (helix.static.insert_string "# ─────────────\n")
     (helix.static.collapse_selection)
     (helix.static.commit-changes-to-history)
     (set-status! (string-append "✗ " err))]
    [else
     (define output-repr (field-at 2))
     (define stdout-text (field-at 3))
     (define stderr-text (field-at 4))
     (define has-error (equal? (field-at 5) "true"))

     ;; Insert stdout if present, line-commented so it can't pollute
     ;; the buffer with raw Julia tokens. A `display(A)` call that
     ;; prints a multi-line matrix would previously land in the
     ;; buffer as `8x8 Matrix{Float64}:\n 1.0 1.0 ...` -- the LSP
     ;; parses those continuation lines as code and complains, and
     ;; the next `:execute-cell` re-runs with that text as part of
     ;; the cell body, which blows up.
     (when (> (string-length stdout-text) 0)
       (helix.static.insert_string (commentify stdout-text)))

     ;; Prepare the image payload BEFORE any text insertion so we know
     ;; up front whether we'll render inline or fall through to a text
     ;; `output_repr`. The actual `add-raw-content-with-placeholders!`
     ;; call is deferred to the very end of this function, after all
     ;; text has been inserted -- registering earlier lets Helix's
     ;; `apply_impl` remap the raw_content through each subsequent
     ;; insert transaction (Assoc::After), which accumulates drift and
     ;; pushes the image line past the footer and beyond. Deferring
     ;; keeps the anchor at exactly the line we want.
     ;; Resolve image data. When kernel-dir is available, use the sidecar
     ;; path (reads raw PNG from file, base64-encodes in Rust). Otherwise
     ;; fall back to extracting base64 directly from the JSON.
     ;; NOTE: Phase 3 bytes path (kitty-placeholder-payload-bytes) is
     ;; deferred until the Steel ByteVector FFI patch is deployed upstream.
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

     ;; If the image will render, emit its cache marker and then
     ;; pad the buffer with `image-rows - 1` empty lines so the
     ;; placeholder grid has real buffer space to land on. The
     ;; marker line itself is row 0 of the image; the padding
     ;; lines are rows 1 through `image-rows - 1`.
     ;;
     ;; The padding is essential with the helix fork that no
     ;; longer reserves phantom visual rows for raw_content -- if
     ;; we don't provision real lines here, the grid's placeholder
     ;; cells on rows 1+ would get overwritten by whatever
     ;; follows (the footer, the next cell marker, etc).
     ;;
     ;; We capture `image-marker-line` BEFORE inserting anything
     ;; so the anchor computation at the end doesn't have to walk
     ;; the insert count backward from the current cursor position.
     (define image-marker-line -1)
     (when image-ready
       (set! image-marker-line (current-line-number))
       (define cache-path (save-image-to-cache jl-path cell-index image-b64))
       (if (string-starts-with? cache-path "ERROR:")
           (helix.static.insert_string "# @image [render only]\n")
           (helix.static.insert_string (string-append "# @image " cache-path "\n")))
       ;; Pad with blank lines for the image body.
       (let loop ([i 1])
         (when (< i image-rows)
           (helix.static.insert_string "\n")
           (loop (+ i 1)))))

     ;; If the payload was malformed, surface the error text in place
     ;; of the image.
     (when (> (string-length image-error-msg) 0)
       (helix.static.insert_string image-error-msg))

     ;; Insert output representation only if no image was rendered.
     ;; Commentified for the same reason as stdout.
     (when (and (not image-ready) (> (string-length output-repr) 0))
       (helix.static.insert_string (commentify output-repr)))

     ;; Insert stderr if present, also line-commented. Filter out
     ;; purely informational Pkg noise ("Resolving package versions",
     ;; "No packages added or removed", "Precompiling") that clutters
     ;; the output when the user has `using Pkg; Pkg.add(...)` in the
     ;; same cell as their real code. Only show stderr when it carries
     ;; real content -- actual errors, warnings, or status changes.
     (when (> (string-length stderr-text) 0)
       (define filtered-stderr
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
           (string-join keep "\n")))
       (when (> (string-length (string-trim filtered-stderr)) 0)
         (helix.static.insert_string "# stderr:\n")
         (helix.static.insert_string (commentify filtered-stderr))))

     ;; Insert footer -- the cursor lands on the line AFTER the footer
     ;; (because insert_string just appended a newline), which is the
     ;; stable anchor point we want for the placeholder grid.
     (helix.static.insert_string "# ─────────────\n")

     ;; Register the raw_content LAST, after every text insert is
     ;; done, so no further transaction reshuffles its char_idx.
     ;; The anchor is the `# @image` marker line captured before
     ;; we started inserting -- that's row 0 of the image. The
     ;; `image-rows - 1` blank lines we padded below the marker
     ;; give the grid real buffer space to paint on, and with the
     ;; helix fork's deferred raw_content draw the placeholder
     ;; cells overwrite whatever the text pass drew on those rows.
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
                        " rows-bytes=" (number->string (string-length image-placeholder-rows))))
       (helix.static.add-raw-content-with-placeholders!
         image-payload image-rows image-cols image-placeholder-rows char-idx))

     (helix.static.collapse_selection)
     (helix.static.commit-changes-to-history)

     (if has-error
         (set-status! "Cell executed with errors")
         (if image-ready
             (set-status! "✓ Cell executed (with plot)")
             (set-status! "✓ Cell executed")))])

  ;; Restore the cursor to wherever it was when the user pressed
  ;; :execute-cell. Every insert above shifted the cursor along with
  ;; the text it was typing into; now that we're done, the user
  ;; shouldn't be parked at the bottom of the output block -- they
  ;; should be right back on the cell code they just ran. `save-cursor-
  ;; for-restore!` is called at the top of execute-cell (and
  ;; execute-cell-list's start) so the snapshot exists for us here.
  (restore-cursor-for! doc-id)

  (helix.redraw)

  ;; NOTE: we deliberately do NOT call `render-cached-images` here. The
  ;; fresh image was already registered via the inline `add-raw-content!`
  ;; above; calling `render-cached-images` would walk every other marker
  ;; in the buffer and re-register all of them, which (on stock Helix,
  ;; where `add-raw-content!` has no dedup) causes stacked duplicates of
  ;; every previously-rendered image. `render-cached-images` is gated on
  ;; a per-doc guard and runs once at `document-opened`; that's enough.

  ;; Rebuild concealment. The output injection above shifted char offsets
  ;; for every byte after the output insertion point, so any cached overlays
  ;; are now lies. Scheduling (rather than calling directly) lets the
  ;; debounce collapse rapid-fire per-cell completions when executing many
  ;; cells back-to-back.
  (schedule-reconceal 50))
