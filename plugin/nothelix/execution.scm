;;; execution.scm - Cell execution and output management
;;;
;;; Orchestrates the full execution cycle: locating cell boundaries in the
;;; document, starting async kernel execution, polling for results via
;;; `enqueue-thread-local-callback-with-delay`, and inserting output
;;; (text, errors, inline images) back into the buffer.

(require "common.scm")
(require "string-utils.scm")
(require "kernel.scm")
(require "graphics.scm")
(require "spinner.scm")
(require "chart-viewer.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

;; Global counter for unique Kitty image IDs (wraps at 16M to stay in range).
(define *image-id-counter* 1)

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
                          ;; Kitty graphics - returns raw escape sequence bytes
                          kitty-display-image-bytes
                          ;; Image cache persistence
                          save-image-to-cache
                          load-image-from-cache
                          ;; Shell-free utilities
                          sleep-ms))

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
;; Deletes one line at a time to avoid confusing Helix's position tracking,
;; then commits the transaction so async callbacks see a consistent state.
(define (delete-line-range start-line end-line)
  ;; Instead of extending selection in a loop, delete lines one by one from the top
  ;; This avoids creating huge selections that confuse Helix's position tracking
  (let loop ([current-line start-line]
             [remaining (- end-line start-line)])
    (when (> remaining 0)
      ;; Always delete the line at start-line position (lines shift up after each delete)
      (helix.goto (number->string (+ start-line 1)))
      (helix.static.goto_line_start)
      (helix.static.extend_to_line_bounds)
      (helix.static.delete_selection)
      ;; Immediately collapse to avoid tracking the deleted range
      (helix.static.collapse_selection)
      (loop start-line (- remaining 1))))
  ;; Delete trailing newline if needed
  (when (> start-line 0)
    (helix.goto (number->string (+ start-line 1)))
    (helix.static.delete_char_backward))
  ;; Collapse selection to avoid stale position tracking issues
  (helix.static.collapse_selection)
  ;; CRITICAL: Commit changes to history immediately to prevent async callback crashes
  (helix.static.commit-changes-to-history))

;;@doc
;; Insert execution results into the buffer, replacing the "Executing..." spinner.
;; Handles stdout, stderr, images (via Kitty graphics), and errors.
;; `jl-path` and `cell-index` are used to persist images to the cache directory.
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

  (define (get-line idx)
    (if (< idx total-lines)
        (text.rope->string (text.rope->line rope idx))
        ""))

  (define (find-running-marker line-idx)
    (cond
      [(>= line-idx total-lines) #false]
      [(string-contains? (get-line line-idx) "Executing...") line-idx]
      [else (find-running-marker (+ line-idx 1))]))

  (define running-line (find-running-marker 0))

  (when running-line
    ;; Delete the "Executing..." line
    (helix.goto (number->string (+ running-line 1)))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection))

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

     ;; Check for images — if we render one, skip the text output_repr.
     (define image-b64 (json-get-first-image result-json))
     (define image-rendered #false)

     (when (> (string-length image-b64) 0)
       (define image-id *image-id-counter*)
       (set! *image-id-counter* (+ *image-id-counter* 1))
       (when (> *image-id-counter* 16777200)
         (set! *image-id-counter* 1))
       (define image-rows 12)
       (define escape-seq (kitty-display-image-bytes image-b64 image-id image-rows))

       (if (not (string-starts-with? escape-seq "ERROR:"))
           (begin
             ;; Persist the image to the cache directory so it survives close/reopen.
             (define cache-path (save-image-to-cache jl-path cell-index image-b64))

             ;; Insert a marker line that references the cached file.
             (if (string-starts-with? cache-path "ERROR:")
                 (helix.static.insert_string (string-append "# @image [render only]\n"))
                 (helix.static.insert_string (string-append "# @image " cache-path "\n")))

             ;; Render the image inline via RawContent.
             (define char-idx (cursor-position))
             (helix.static.add-raw-content! escape-seq image-id image-rows char-idx)
             (set! image-rendered #true))
           (helix.static.insert_string
             (string-append "# [Plot: " (number->string (quotient (string-length image-b64) 1024)) "KB - render failed]\n"))))

     ;; Insert output representation only if no image was rendered
     (when (and (not image-rendered) (> (string-length output-repr) 0))
       (helix.static.insert_string (string-append output-repr "\n")))

     ;; Insert stderr if present
     (when (> (string-length stderr-text) 0)
       (helix.static.insert_string (string-append "# stderr: " stderr-text "\n")))

     ;; Insert footer
     (helix.static.insert_string "# ─────────────\n")

     (helix.static.collapse_selection)
     (helix.static.commit-changes-to-history)

     (if has-error
         (set-status! "Cell executed with errors")
         (if image-rendered
             (set-status! "✓ Cell executed (with plot)")
             (set-status! "✓ Cell executed")))])

  (helix.redraw))

;;@doc
;; Advance the spinner animation in the "Executing..." line.
;; Re-reads document state each tick to find the current spinner position.
(define (update-spinner-frame)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (find-spinner-line line-idx)
    (cond
      [(>= line-idx total-lines) #false]
      [(string-contains? (doc-get-line rope total-lines line-idx) "Executing...") line-idx]
      [else (find-spinner-line (+ line-idx 1))]))

  (define spinner-line (find-spinner-line 0))

  (when spinner-line
    ;; Get next spinner frame
    (define new-frame (spinner-next-frame))
    ;; Replace the line with updated spinner
    (helix.goto (number->string (+ spinner-line 1)))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (helix.static.insert_string (string-append "# " new-frame " Executing...\n"))
    ;; Stay at current position
    (helix.static.collapse_selection)
    ;; CRITICAL: Commit changes to history immediately to prevent async callback crashes
    (helix.static.commit-changes-to-history)
    ;; Update status line too
    (set-status! (string-append new-frame " Executing cell..."))
    (helix.redraw)))

;; Helper: Poll for execution result (called repeatedly via delayed callback)
(define (poll-for-result kernel-dir jl-path cell-index)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-for-result kernel-dir jl-path cell-index)))]
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

  ;; Get kernel for this notebook
  (define notebook-path (editor-document->path doc-id))
  (define kernel-state (kernel-get-for-notebook notebook-path lang))

  (when (not kernel-state)
    ;; kernel-start already showed an error via set-status!
    (helix.redraw)
    (error "Kernel failed to start"))

  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  ;; Get cell index for dependency tracking
  (define cell-info-json (get-cell-at-line path current-line))
  (define cell-index-str (json-get cell-info-json "cell_index"))
  (define cell-index (if (> (string-length cell-index-str) 0)
                          (string->number cell-index-str)
                          0))

  ;; Insert output header with spinner
  (spinner-reset)  ;; Start from first frame
  (define spinner-frame (spinner-next-frame))
  (helix.static.insert_string (string-append "\n\n# ─── Output ───\n# " spinner-frame " Executing...\n"))
  ;; CRITICAL: Commit changes to history immediately
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
     ;; Execution started - begin polling for result
     ;; Uses enqueue-thread-local-callback-with-delay for non-blocking polling
      (enqueue-thread-local-callback-with-delay 100
        (lambda () (poll-for-result kernel-dir path cell-index)))]
    [else
     ;; Error starting execution
     (define err (let ([e (json-get start-result "error")]) (if (> (string-length e) 0) e "Unknown error")))

     ;; If kernel directory doesn't exist or PID missing, remove from hash
     ;; so next execution will create a fresh kernel
     (when (or (string-contains? err "does not exist")
               (string-contains? err "PID file missing"))
       (set! *kernels* (hash-remove *kernels* notebook-path)))

     (helix.static.insert_string (string-append "# ERROR: " err "\n"))
     (helix.static.insert_string "# ─────────────\n")
     (set-status! (string-append "✗ " err))
     ;; CRITICAL: Commit changes to history immediately
     (helix.static.commit-changes-to-history)
      (set! *executing-kernel-dir* #false)
      (helix.redraw)]))

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

  ;; Start kernel
  (define kernel-state (kernel-get-for-notebook notebook-path lang))

  (when (not kernel-state)
    (helix.redraw)
    (error "Kernel failed to start"))

  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  (set-status! (string-append "Executing " (number->string cell-count) " cells: " indices-str))
  (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))

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

        ;; Position cursor and insert spinner
        (helix.goto (number->string cell-code-end))
        (helix.static.goto_line_end)
        (spinner-reset)
        (define spinner-frame (spinner-next-frame))
        (define executed-count (- total-count (length remaining-indices)))
        (helix.static.insert_string (string-append "\n\n# ─── Output ───\n# " spinner-frame " Executing...\n"))
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
;; Handle execution error - delete spinner and show error
(define (handle-execution-error cell-code-end err)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define post-rope (editor->text doc-id))
  (define post-lines (text.rope-len-lines post-rope))

  (define (get-post-line idx)
    (if (< idx post-lines)
        (text.rope->string (text.rope->line post-rope idx))
        ""))

  (define (find-spin idx lim)
    (cond [(>= idx lim) #f]
          [(string-contains? (get-post-line idx) "Executing...") idx]
          [else (find-spin (+ idx 1) lim)]))

  (define spin-line (find-spin cell-code-end (+ cell-code-end 20)))
  (when spin-line
    (helix.goto (number->string (+ spin-line 1)))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection))

  (helix.static.insert_string (string-append "# ERROR: " err "\n# ─────────────\n"))
  ;; Stay at current position so output remains visible
  (helix.static.collapse_selection)
  ;; CRITICAL: Commit changes to history immediately
  (helix.static.commit-changes-to-history)
  (helix.redraw))

(define (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)))]
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
  ;; Small delay to ensure file is flushed to disk
  (sleep-ms 100)

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

  ;; Start kernel
  (define kernel-state (kernel-get-for-notebook notebook-path lang))

  (when (not kernel-state)
    (helix.redraw)
    (error "Kernel failed to start"))

  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  (set-status! (string-append "Executing cells: " indices-str))
  (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))

;;; ---------------------------------------------------------------------------
;;; Image cache rendering (for file re-open)
;;; ---------------------------------------------------------------------------

;;@doc
;; Scan the current buffer for `# @image <path>` markers and re-render
;; the cached images via RawContent.  Called on file open for .jl files.
(define (render-cached-images)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (when (and path (string-suffix? path ".jl"))
    (define rope (editor->text doc-id))
    (define total-lines (text.rope-len-lines rope))

    (let loop ([line-idx 0] [rendered 0])
      (when (< line-idx total-lines)
        (define line (doc-get-line rope total-lines line-idx))
        (if (string-starts-with? line "# @image ")
            (let ()
              (define rel-path (string-trim (substring line 9 (string-length line))))
              (define image-b64 (load-image-from-cache path rel-path))

              (if (> (string-length image-b64) 0)
                  (let ()
                    (define image-id *image-id-counter*)
                    (set! *image-id-counter* (+ *image-id-counter* 1))
                    (when (> *image-id-counter* 16777200)
                      (set! *image-id-counter* 1))
                    (define image-rows 12)
                    (define escape-seq (kitty-display-image-bytes image-b64 image-id image-rows))

                    (when (not (string-starts-with? escape-seq "ERROR:"))
                      ;; Get char position of this line for RawContent placement.
                      (define char-pos (text.rope-line->char rope line-idx))
                      (helix.static.add-raw-content! escape-seq image-id image-rows char-pos))

                    (loop (+ line-idx 1) (+ rendered 1)))
                  ;; Cache file missing — skip silently.
                  (loop (+ line-idx 1) rendered)))
            (loop (+ line-idx 1) rendered))))))
