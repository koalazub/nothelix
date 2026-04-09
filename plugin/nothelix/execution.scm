;;; execution.scm - Cell execution and output management
;;;
;;; Orchestrates the full execution cycle: locating cell boundaries in the
;;; document, starting async kernel execution, polling for results via
;;; `enqueue-thread-local-callback-with-delay`, and inserting output
;;; (text, errors, inline images) back into the buffer.

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "kernel.scm")
(require "graphics.scm")
(require "spinner.scm")
(require "chart-viewer.scm")
(require "conceal.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/ext.scm")


;; Set of document ids we've already re-registered cached images for.
;; Stock Helix's `add-raw-content!` unconditionally appends to a per-view
;; `Vec<RawContent>` — it has no dedup and no `clear-raw-content!` binding
;; to flush stale entries. Running `render-cached-images` more than once
;; per document therefore accumulates duplicate registrations at drifting
;; char positions, which the terminal renderer draws as stacked images
;; (the "smeared Matrix C" artefact). We guard against that by tracking
;; which doc-ids we've already rendered and refusing to rerun for the
;; same doc. The fork's `add_or_replace_raw_content` would make this
;; guard unnecessary, but the guard is harmless on the fork and correct
;; on stock Helix, so we always use it.
;;
;; Represented as a hashmap → boolean because Steel's core collections
;; API documents `hash`, `hash-insert`, `hash-contains?` but no
;; first-class hashset primitive.
(define *rendered-image-docs* (hash))

;; FFI imports for execution functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          get-cell-at-line
                          get-cell-code-from-jl      ;; Extract code from .jl files by @cell marker
                          list-jl-code-cells         ;; List all code cell indices from .jl file
                          kernel-interrupt
                          ;; Async execution (non-blocking)
                          kernel-execute-cell-start
                          kernel-poll-result
                          ;; JSON utilities (proper serde parsing)
                          json-get
                          json-get-bool
                          json-get-first-image
                          json-get-plot-data
                          ;; Kitty Unicode placeholder protocol (virtual placement):
                          ;; transmit the image once under a stable id, then write
                          ;; a grid of placeholder cells into the text buffer. The
                          ;; old direct-placement path (`kitty-display-image-bytes`)
                          ;; pins pixels to absolute terminal cells and smears on
                          ;; scroll — placeholders move with the buffer instead.
                          kitty-placeholder-payload
                          kitty-placeholder-rows
                          kitty-placeholder-max-dim
                          ;; Image cache persistence
                          save-image-to-cache
                          load-image-from-cache
))

(provide execute-cell
         execute-all-cells
         execute-cells-above
         cancel-cell
         render-cached-images
         find-cell-start-line
         find-cell-code-end
         find-output-start
         find-output-end-line
         extract-cell-code
         delete-line-range
         find-cell-marker-by-index)

;;@doc
;; Find the cell start line by searching backwards for an @cell or @markdown marker.
;; Returns 0 if no marker is found above `line-idx`.
(define (find-cell-start-line get-line line-idx)
  (if (< line-idx 0) 0
      (let ([line (get-line line-idx)])
        (if (cell-marker? line)
            line-idx
            (find-cell-start-line get-line (- line-idx 1))))))

;;@doc
;; Find where cell code ends: the next marker, output section header, or EOF.
(define (find-cell-code-end get-line total-lines line-idx)
  (if (>= line-idx total-lines) total-lines
      (let ([line (get-line line-idx)])
        (if (or (cell-marker? line)
                (string-starts-with? line "# ═══")
                (string-contains? line "# ─── Output"))
            line-idx
            (find-cell-code-end get-line total-lines (+ line-idx 1))))))

;;@doc
;; Find the "# --- Output ---" header line starting from `line-idx`.
;; Returns #false if no output section exists before the next cell marker or EOF.
(define (find-output-start get-line total-lines line-idx)
  (if (>= line-idx total-lines) #false
      (let ([line (get-line line-idx)])
        (cond
          [(string-contains? line "# ─── Output ───") line-idx]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) #false]
          [else (find-output-start get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Find the end of an output section (the "# -----" footer, or next marker).
(define (find-output-end-line get-line total-lines line-idx)
  (if (>= line-idx total-lines) line-idx
      (let ([line (get-line line-idx)])
        (cond
          [(string-contains? line "# ─────────────") (+ line-idx 1)]
          [(or (cell-marker? line)
               (string-starts-with? line "# ═══")) line-idx]
          [else (find-output-end-line get-line total-lines (+ line-idx 1))]))))

;;@doc
;; Extract code lines from a cell, skipping the @cell marker and separator lines.
(define (extract-cell-code get-line start end)
  (let loop ([idx (+ start 1)] [acc '()])
    (if (>= idx end)
        (reverse acc)
        (let ([line (get-line idx)])
          (if (or (string-starts-with? line "# ═══")
                  (string-starts-with? line "# ─── "))
              (loop (+ idx 1) acc)
              (loop (+ idx 1) (cons line acc)))))))

;;@doc
;; Delete lines from `start-line` to `end-line` (start inclusive, end exclusive).
;;
;; Uses a single range-based selection and a single delete_selection call,
;; so the Helix command count is O(1) regardless of the number of lines.
;; Previous implementation did `goto + extend + delete` per line, which
;; dominated the cost of re-executing a cell with a large output section.
(define (delete-line-range start-line end-line)
  (when (> end-line start-line)
    (define focus (editor-focus))
    (define doc-id (editor->doc-id focus))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))
    (define doc-char-len (text.rope-len-chars rope))

    ;; Clamp to valid bounds so we never ask for a line past EOF.
    (define clamped-start (max 0 (min start-line total-lines)))
    (define clamped-end (max clamped-start (min end-line total-lines)))

    ;; Convert line numbers to char offsets. Leading position is start of
    ;; `start-line`; trailing position is start of `end-line` (which is the
    ;; char after the last newline of line `end-line - 1`). If `end-line`
    ;; is past EOF, snap to the end of the rope.
    (define start-char (text.rope-line->char rope clamped-start))
    (define end-char
      (cond
        [(< clamped-end total-lines) (text.rope-line->char rope clamped-end)]
        [else doc-char-len]))

    (when (< start-char end-char)
      (define r (helix.static.range start-char end-char))
      (define sel (helix.static.range->selection r))
      (helix.static.set-current-selection-object! sel)
      (helix.static.delete_selection)
      (helix.static.collapse_selection)
      (helix.static.commit-changes-to-history))))

;;@doc
;; Derive a stable kitty image id from a cell index. Offset by 1000 so we
;; don't collide with whatever low-id range the terminal reserves. Two
;; invocations with the same `cell-index` produce the same id, which lets
;; the idempotent `add-raw-content!` in the Helix fork replace-in-place
;; rather than accumulate duplicates across re-execution and buffer switches.
(define (cell-index->image-id cell-index)
  (+ 1000 cell-index))

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

;;@doc
;; Locate the "# ─── Output ───" header line for a specific cell.
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
;; This function locates the cell's `# ─── Output ───` header and
;; inserts the result content right below it.
(define (update-cell-output result-json jl-path cell-index)
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
  ;; execute-cell and the result arriving), fall through silently —
  ;; the inserts will still land at whatever the cursor's current
  ;; position is, which is the best we can do.
  (define header-line (find-output-header-for-cell rope total-lines cell-index))
  (when header-line
    (helix.goto (number->string (+ header-line 2)))
    (helix.static.goto_line_start))

  ;; Rust kernel_poll_result flattens the response:
  ;; {"status": "ok", "stdout": "...", "output_repr": "...", ...}
  ;; Using Rust FFI json-get for proper serde parsing
  (define err (json-get result-json "error"))
  (cond
    [(> (string-length err) 0)
     (helix.static.insert_string (string-append "# ERROR: " err "\n"))
     (helix.static.insert_string "# ─────────────\n")
     (helix.static.collapse_selection)
     (helix.static.commit-changes-to-history)
     (set-status! (string-append "✗ " err))]
    [else
     (define output-repr (json-get result-json "output_repr"))
     (define stdout-text (json-get result-json "stdout"))
     (define stderr-text (json-get result-json "stderr"))
     (define has-error (equal? (json-get-bool result-json "has_error") "true"))

     ;; Insert stdout if present
     (when (> (string-length stdout-text) 0)
       (helix.static.insert_string stdout-text)
       (when (not (string-suffix? stdout-text "\n"))
         (helix.static.insert_string "\n")))

     ;; Prepare the image payload BEFORE any text insertion so we know
     ;; up front whether we'll render inline or fall through to a text
     ;; `output_repr`. The actual `add-raw-content-with-placeholders!`
     ;; call is deferred to the very end of this function, after all
     ;; text has been inserted — registering earlier lets Helix's
     ;; `apply_impl` remap the raw_content through each subsequent
     ;; insert transaction (Assoc::After), which accumulates drift and
     ;; pushes the image line past the footer and beyond. Deferring
     ;; keeps the anchor at exactly the line we want.
     (define image-b64 (json-get-first-image result-json))
     (define image-ready #false)
     (define image-error-msg "")
     (define image-id 0)
     (define image-rows 12)
     (define image-cols 40)
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

     ;; If the image will render, emit its cache marker now so the
     ;; marker line exists in the buffer. The marker comes before the
     ;; footer so the visual order is: [marker] [footer] [plot grid].
     (when image-ready
       (define cache-path (save-image-to-cache jl-path cell-index image-b64))
       (if (string-starts-with? cache-path "ERROR:")
           (helix.static.insert_string "# @image [render only]\n")
           (helix.static.insert_string (string-append "# @image " cache-path "\n"))))

     ;; If the payload was malformed, surface the error text in place
     ;; of the image.
     (when (> (string-length image-error-msg) 0)
       (helix.static.insert_string image-error-msg))

     ;; Insert output representation only if no image was rendered
     (when (and (not image-ready) (> (string-length output-repr) 0))
       (helix.static.insert_string (string-append output-repr "\n")))

     ;; Insert stderr if present
     (when (> (string-length stderr-text) 0)
       (helix.static.insert_string (string-append "# stderr: " stderr-text "\n")))

     ;; Insert footer — the cursor lands on the line AFTER the footer
     ;; (because insert_string just appended a newline), which is the
     ;; stable anchor point we want for the placeholder grid.
     (helix.static.insert_string "# ─────────────\n")

     ;; Register the raw_content LAST, after every text insert is done,
     ;; so no further transaction reshuffles its char_idx. The cursor
     ;; sits on the line immediately after the footer, so that line's
     ;; char position is exactly where we want the placeholder grid's
     ;; row 0 to start drawing.
     (when image-ready
       (define focus (editor-focus))
       (define doc-id (editor->doc-id focus))
       (define rope (editor->text doc-id))
       (define total-lines (text.rope-len-lines rope))
       ;; `current-line-number` reads the cursor's line after the
       ;; footer insert. If that's beyond the rope's last line (can
       ;; happen if the cell is the last thing in the file and there's
       ;; no trailing newline), clamp to the last valid line so
       ;; `text.rope-line->char` doesn't panic.
       (define anchor-line (current-line-number))
       (define safe-line
         (cond
           [(< anchor-line 0) 0]
           [(>= anchor-line total-lines) (- total-lines 1)]
           [else anchor-line]))
       (define char-idx (text.rope-line->char rope safe-line))
       (debug-log
         (string-append "execution.update-cell-output: register image cell="
                        (number->string cell-index)
                        " id=" (number->string image-id)
                        " anchor-line=" (number->string safe-line)
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

;;@doc
;; Advance the spinner animation.
;;
;; The spinner only ever touches the status line — `set-status!`
;; redraws without producing a buffer transaction, so the undo chain
;; stays clean. An earlier implementation rewrote a `# Executing…`
;; line in the buffer on every polling tick, which invalidated the
;; conceal overlay cache, cluttered undo history, and turned every
;; tick into an O(line) goto + extend + delete + insert. The buffer
;; no longer carries any spinner text at all — `execute-cell` inserts
;; the `# ─── Output ───` header and nothing else until the result
;; arrives.
(define (update-spinner-frame)
  (define new-frame (spinner-next-frame))
  (set-status! (string-append new-frame " Executing cell...")))

;; Helper: Poll for execution result with exponential backoff.
;; Starts at 100ms, grows to 500ms max.
(define (poll-for-result kernel-dir jl-path cell-index)
  (poll-for-result-with-delay kernel-dir jl-path cell-index 100))

(define (poll-for-result-with-delay kernel-dir jl-path cell-index delay-ms)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (define next-delay (min 500 (+ delay-ms 50)))
     (enqueue-thread-local-callback-with-delay next-delay
       (lambda () (poll-for-result-with-delay kernel-dir jl-path cell-index next-delay)))]
    [else
     (update-cell-output result-json jl-path cell-index)]))

;;@doc
;; Execute the code cell under the cursor (async, non-blocking)
(define (execute-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line idx) (doc-get-line rope total-lines idx))

  ;; Find cell boundaries
  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))

  ;; Extract code
  (define cell-lines (extract-cell-code get-line cell-start cell-code-end))
  (define code (string-join cell-lines "\n"))

  (when (equal? (string-length code) 0)
    (set-status! "Cell is empty")
    (helix.redraw)
    (error "Cell is empty"))

  ;; Detect language
  (define path (editor-document->path doc-id))
  (define lang (cond
                 [(string-contains? path ".ipynb") "julia"]
                 [(string-contains? path ".jl") "julia"]
                 [(string-contains? path ".py") "python"]
                 [else "julia"]))

  ;; Find and delete existing output section if present
  (define output-start (find-output-start get-line total-lines cell-code-end))

  (when output-start
    (define output-end (find-output-end-line get-line total-lines (+ output-start 1)))
    (delete-line-range output-start output-end))

  ;; Position cursor at end of last code line
  (define insert-at-line (- cell-code-end 1))
  (helix.goto (number->string (+ insert-at-line 1)))
  (helix.static.goto_line_end)

  ;; Get kernel for this notebook (async — may need to start one)
  (define notebook-path (editor-document->path doc-id))

  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))

      ;; Get cell index for dependency tracking
      (define cell-info-json (get-cell-at-line path current-line))
      (define cell-index-str (json-get cell-info-json "cell_index"))
      (define cell-index (if (> (string-length cell-index-str) 0)
                              (string->number cell-index-str)
                              0))

      ;; Insert *only* the output header into the buffer. The
      ;; spinner animates in the status line — nothing goes into the
      ;; buffer (and therefore the undo history) until the actual
      ;; result arrives in `update-cell-output`. This keeps the undo
      ;; chain free of spinner-frame noise and avoids stranding
      ;; "Executing…" text if execution is interrupted.
      (spinner-reset)
      (define spinner-frame (spinner-next-frame))
      (helix.static.insert_string "\n\n# ─── Output ───\n")
      (helix.static.commit-changes-to-history)
      (set-status! (string-append spinner-frame " Executing cell..."))
      (helix.redraw)

      ;; Track executing kernel for cancellation
      (set! *executing-kernel-dir* kernel-dir)

      ;; Start execution (non-blocking Rust FFI call)
      (define start-result (kernel-execute-cell-start kernel-dir cell-index code))
      (define start-status (json-get start-result "status"))

      (cond
        [(equal? start-status "started")
          (enqueue-thread-local-callback-with-delay 100
            (lambda () (poll-for-result kernel-dir path cell-index)))]
        [else
         (define err (let ([e (json-get start-result "error")]) (if (> (string-length e) 0) e "Unknown error")))
         (when (or (string-contains? err "does not exist")
                   (string-contains? err "PID file missing"))
           (set! *kernels* (hash-remove *kernels* notebook-path)))
         (helix.static.insert_string (string-append "# ERROR: " err "\n"))
         (helix.static.insert_string "# ─────────────\n")
         (set-status! (string-append "✗ " err))
         (helix.static.commit-changes-to-history)
         (set! *executing-kernel-dir* #false)
         (helix.redraw)]))))

;;@doc
;; Cancel/interrupt any running cell execution
(define (cancel-cell)
  (cond
    [(not *executing-kernel-dir*)
     (set-status! "No cell execution in progress")]
    [else
     (define result (kernel-interrupt *executing-kernel-dir*))
     (if (string-starts-with? result "ERROR:")
         (set-status! result)
         (begin
           (set-status! "Cell execution interrupted")
           (set! *executing-kernel-dir* #f)))]))

;;@doc
;; Find the line number of a cell marker with given index in a converted file.
;; Returns the line number of the "@cell N ..." or "@markdown N" marker, or
;; #false if not found.
(define (find-cell-marker-by-index rope total-lines cell-index)
  (define code-pattern (string-append "@cell " (number->string cell-index) " "))
  (define md-pattern (string-append "@markdown " (number->string cell-index)))

  (let loop ([line-idx 0])
    (cond
      [(>= line-idx total-lines) #false]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) code-pattern) line-idx]
      [(string-starts-with? (doc-get-line rope total-lines line-idx) md-pattern) line-idx]
      [else (loop (+ line-idx 1))])))

;;@doc
;; Execute all cells in the notebook from top to bottom
;; Works on .jl converted files - uses list-jl-code-cells to find all code cells
(define (execute-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  ;; Only works on .jl files (converted notebooks)
  (when (string-suffix? path ".ipynb")
    (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
    (error "Not a converted file"))

  (when (not (string-suffix? path ".jl"))
    (set-status! "Error: Only .jl files supported")
    (error "Not a .jl file"))

  ;; Get list of ALL code cell indices from Rust parser (pass large number for no limit)
  (define cells-json (list-jl-code-cells path 999999))
  (define cells-err (json-get cells-json "error"))
  (when (> (string-length cells-err) 0)
    (define safe-err (sanitise-error-message cells-err))
    (set-status! (string-append "✗ " safe-err))
    (error safe-err))

  (define indices-str (json-get cells-json "indices"))
  (define cell-indices (parse-indices-string indices-str))
  (define cell-count (length cell-indices))

  (when (equal? cell-count 0)
    (set-status! "No code cells found")
    (error "No code cells"))

  ;; Use the .jl file path as the notebook path for kernel management
  (define notebook-path path)
  (define lang "julia")

  ;; Start kernel (async)
  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (set-status! (string-append "Executing " (number->string cell-count) " cells: " indices-str))
      (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))))

;;@doc
;; Helper: Execute a list of cells sequentially with async execution and animated spinner.
;; Uses Rust to parse .jl files for cell indices - no Steel iteration over sparse indices.
;; cell-indices: list of cell indices to execute (from Rust parser)
;; remaining-indices: rest of the list to process
(define (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
  (if (null? remaining-indices)
      (begin
        (helix.goto (number->string (+ original-line 1)))
        (helix.static.collapse_selection)
        (set-status! (string-append "✓ Executed " (number->string total-count) " cells")))
      (let ([current-idx (car remaining-indices)]
            [rest-indices (cdr remaining-indices)])
        (define cell-data-json (get-cell-code-from-jl jl-path current-idx))
        (define cell-code (json-get cell-data-json "code"))
        (define cell-error (json-get cell-data-json "error"))
        (define code-len (if cell-code (string-length cell-code) 0))

        (set-status! (string-append "→ Cell " (number->string current-idx) " (" (number->string code-len) " bytes)"))
        (helix.redraw)

        (cond
          [(> (string-length cell-error) 0)
           (set-status! (string-append "⚠ Cell " (number->string current-idx) " error: " cell-error))
           (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices rest-indices total-count original-line)]
          [(or (not cell-code) (equal? code-len 0))
           (set-status! (string-append "⚠ Cell " (number->string current-idx) " empty"))
           (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices rest-indices total-count original-line)]
          [else
           (execute-single-cell-async doc-id notebook-path kernel-dir jl-path current-idx cell-code cell-indices rest-indices total-count original-line)]))))

;;@doc
;; Execute a single cell asynchronously, then continue with remaining cells
(define (execute-single-cell-async doc-id notebook-path kernel-dir jl-path cell-idx cell-code cell-indices remaining-indices total-count original-line)
  ;; Find this cell's marker in the document
  (define updated-rope (editor->text doc-id))
  (define updated-total-lines (text.rope-len-lines updated-rope))
  (define cell-marker-line (find-cell-marker-by-index updated-rope updated-total-lines cell-idx))

  (if (not cell-marker-line)
      (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
      ;; Found cell marker - execute it
      (let ()
        (define (get-line idx)
          (if (< idx updated-total-lines)
              (text.rope->string (text.rope->line updated-rope idx))
              ""))

        ;; Find where code ends
        (define cell-code-end (find-cell-code-end get-line updated-total-lines (+ cell-marker-line 1)))

        ;; Delete existing output if present
        (define output-start (find-output-start get-line updated-total-lines cell-code-end))
        (when output-start
          (define output-end (find-output-end-line get-line updated-total-lines (+ output-start 1)))
          (delete-line-range output-start output-end))

        ;; Position cursor and insert the output header only. The
        ;; spinner lives in the status line so we don't churn the
        ;; undo history with transient spinner-frame text (see
        ;; execute-cell for the same reasoning).
        (helix.goto (number->string cell-code-end))
        (helix.static.goto_line_end)
        (spinner-reset)
        (define spinner-frame (spinner-next-frame))
        (define executed-count (- total-count (length remaining-indices)))
        (helix.static.insert_string "\n\n# ─── Output ───\n")
        ;; CRITICAL: Commit changes to history immediately
        (helix.static.commit-changes-to-history)
        (set-status! (string-append spinner-frame " Executing cell " (number->string executed-count) "/" (number->string total-count) "..."))
        (helix.redraw)

        ;; Start async execution
        (define start-result (kernel-execute-cell-start kernel-dir cell-idx cell-code))
        (define start-status (json-get start-result "status"))

        (if (equal? start-status "started")
            ;; Started successfully - begin polling
            (enqueue-thread-local-callback-with-delay 100
              (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)))
            ;; Error starting - show error and continue
            (let ()
              (define err (json-get start-result "error"))
              (handle-execution-error cell-code-end err)
              (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line))))))

;;@doc
;; Handle an execution error by writing the error line and footer
;; under the cell's output header. With the spinner no longer in the
;; buffer, there's nothing to find-and-delete first — we just
;; position ourselves after the header and append.
(define (handle-execution-error cell-code-end err)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define post-rope (editor->text doc-id))
  (define post-lines (text.rope-len-lines post-rope))

  ;; `cell-code-end` is the line index where the cell's code ended
  ;; before we inserted the output header. The header sits one or
  ;; two lines below that, so scan forward a bounded window to find
  ;; it. Falling through silently (no goto) is safe: the error
  ;; message will still land wherever the cursor is.
  (let scan ([idx cell-code-end] [lim (+ cell-code-end 20)])
    (cond
      [(or (>= idx lim) (>= idx post-lines)) #false]
      [(string-contains?
         (text.rope->string (text.rope->line post-rope idx))
         "─── Output ───")
       (helix.goto (number->string (+ idx 2)))
       (helix.static.goto_line_start)]
      [else (scan (+ idx 1) lim)]))

  (helix.static.insert_string (string-append "# ERROR: " err "\n# ─────────────\n"))
  (helix.static.collapse_selection)
  (helix.static.commit-changes-to-history)
  (helix.redraw))

(define (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)
  (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line 100))

(define (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line delay-ms)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (define next-delay (min 500 (+ delay-ms 50)))
     (enqueue-thread-local-callback-with-delay next-delay
       (lambda () (poll-cell-list-result-with-delay doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line next-delay)))]
    [else
     (update-cell-output result-json jl-path cell-idx)
     (enqueue-thread-local-callback-with-delay 10
       (lambda () (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)))]))

;; Helper: Parse comma-separated string into list of numbers
(define (parse-indices-string str)
  (if (or (not str) (equal? str ""))
      '()
      (map string->number (string-split str ","))))

;; Execute all cells from the top up to and including the current cell
;; ONLY works on converted files (not raw .ipynb) since we need to insert outputs
(define (execute-cells-above)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  ;; IMPORTANT: Save file first so Rust can read the latest content
  (helix.write)

  (enqueue-thread-local-callback-with-delay 100
    (lambda ()
      ;; Only works on converted files
      (when (string-suffix? path ".ipynb")
        (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
        (error "Not a converted file"))

      ;; Get current cell info from Rust
      (define cell-info-json (get-cell-at-line path current-line))
      (define err (json-get cell-info-json "error"))
      (when (> (string-length err) 0)
        (set-status! "Error: Not in a notebook file")
        (error "Not in a notebook file"))

      (define notebook-path (json-get cell-info-json "source_path"))
      (define current-cell-idx (string->number (json-get cell-info-json "cell_index")))
      (define lang "julia")  ; TODO: detect from notebook metadata

      ;; Get list of code cell indices from Rust parser (up to current cell)
      (define cells-json (list-jl-code-cells path current-cell-idx))
      (define cells-err (json-get cells-json "error"))
      (when (> (string-length cells-err) 0)
        (define safe-err (sanitise-error-message cells-err))
        (set-status! (string-append "✗ " safe-err))
        (error safe-err))

      (define indices-str (json-get cells-json "indices"))
      (define cell-indices (parse-indices-string indices-str))
      (define cell-count (length cell-indices))

      (when (equal? cell-count 0)
        (set-status! "No code cells to execute")
        (error "No code cells"))

      ;; Start kernel (async)
      (kernel-get-for-notebook notebook-path lang
        (lambda (kernel-state)
          (define kernel-dir (hash-get kernel-state 'kernel-dir))
          (set-status! (string-append "Executing cells: " indices-str))
          (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))))))

;;; ---------------------------------------------------------------------------
;;; Image cache rendering (for file re-open)
;;; ---------------------------------------------------------------------------

;;@doc
;; Scan the current buffer for `# @image <path>` markers and re-register
;; the cached images as Helix RawContent entries.
;;
;; Called exactly once per document via the `document-opened` hook. A
;; `*rendered-image-docs*` guard short-circuits subsequent invocations
;; for the same doc-id so we never double-register — which would cause
;; the "stacked Matrix C" smearing on stock Helix, where the underlying
;; `add_raw_content` has no built-in dedup.
;;
;; Images registered here persist on the document (keyed by ViewId) for
;; the lifetime of the document, so buffer switches do not need to
;; re-run this — Helix retains the entries across view focus changes.
(define (render-cached-images)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (debug-log
    (string-append "execution.render-cached-images: entry path="
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
           (define cell-index (extract-cell-index-from-path rel-path))
           (define image-b64 (load-image-from-cache path rel-path))
           (cond
             [(and cell-index (> (string-length image-b64) 0))
              (define image-id (cell-index->image-id cell-index))
              (define image-rows 12)
              (define image-cols 40)
              (define payload (kitty-placeholder-payload image-b64 image-id))
              (define placeholder-rows (kitty-placeholder-rows image-id image-cols image-rows))
              ;; Anchor to the line AFTER the marker, not the marker
              ;; line itself — matches the live-execution anchoring in
              ;; update-cell-output. `line-idx + 1` resolved via the
              ;; rope gives us the char position where the placeholder
              ;; grid should start drawing.
              (define target-line
                (if (< (+ line-idx 1) total-lines)
                    (+ line-idx 1)
                    line-idx))
              (define char-pos (text.rope-line->char rope target-line))
              (cond
                [(string-starts-with? payload "ERROR:")
                 (debug-log
                   (string-append "execution.render-cached-images: SKIP cell="
                                  (number->string cell-index)
                                  " reason=payload-error path=" rel-path))]
                [(= (string-length placeholder-rows) 0)
                 (debug-log
                   (string-append "execution.render-cached-images: SKIP cell="
                                  (number->string cell-index)
                                  " reason=grid-too-large"))]
                [else
                 (debug-log
                   (string-append "execution.render-cached-images: REGISTER cell="
                                  (number->string cell-index)
                                  " id=" (number->string image-id)
                                  " marker-line=" (number->string line-idx)
                                  " target-line=" (number->string target-line)
                                  " char-pos=" (number->string char-pos)
                                  " payload-bytes=" (number->string (string-length payload))
                                  " rows-bytes=" (number->string (string-length placeholder-rows))))
                 (helix.static.add-raw-content-with-placeholders!
                   payload image-rows image-cols placeholder-rows char-pos)
                 (set! registered-count (+ registered-count 1))])
              (loop (+ line-idx 1))]
             [else
              (debug-log
                (string-append "execution.render-cached-images: SKIP marker-line="
                               (number->string line-idx)
                               " reason=" (if cell-index "cache-empty" "no-cell-index")
                               " rel-path=" rel-path))
              (loop (+ line-idx 1))])]
          [else (loop (+ line-idx 1))])))

    (debug-log
      (string-append "execution.render-cached-images: done path=" path
                     " registered=" (number->string registered-count)))))

