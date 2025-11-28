;;; execution.scm - Cell execution and output management

(require "string-utils.scm")
(require "kernel.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For cursor-position, set-status!
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
;; enqueue-thread-local-callback-with-delay is a global Helix function

;; Helper: Get current line number (0-indexed)
(define (current-line-number)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (text.rope-char->line rope pos))

;; FFI imports for execution functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          get-cell-at-line
                          notebook-cell-count
                          notebook-get-cell-code
                          kernel-execute-cell
                          kernel-interrupt
                          ;; Async execution (non-blocking)
                          kernel-execute-cell-start
                          kernel-poll-result
                          ;; JSON utilities (proper serde parsing)
                          json-get
                          json-get-bool))

(provide execute-cell
         execute-all-cells
         execute-cells-above
         cancel-cell
         doc-get-line
         find-cell-start-line
         find-cell-code-end
         find-output-start
         find-output-end-line
         extract-cell-code
         delete-line-range
         find-cell-marker-by-index)

;; Helper: Get line content by index
(define (doc-get-line rope total-lines line-idx)
  (if (< line-idx total-lines)
      (text.rope->string (text.rope->line rope line-idx))
      ""))

;; Helper: Find cell start (searching backwards for @cell or @markdown marker)
(define (find-cell-start-line get-line line-idx)
  (if (< line-idx 0) 0
      (let ([line (get-line line-idx)])
        (if (or (string-starts-with? line "@cell ")
                (string-starts-with? line "@markdown "))
            line-idx
            (find-cell-start-line get-line (- line-idx 1))))))

;; Helper: Find cell code end (next @cell/@markdown marker, output section, or EOF)
(define (find-cell-code-end get-line total-lines line-idx)
  (if (>= line-idx total-lines) total-lines
      (let ([line (get-line line-idx)])
        (if (or (string-starts-with? line "@cell ")
                (string-starts-with? line "@markdown ")
                (string-starts-with? line "# ═══")  ; Cell separator line
                (string-contains? line "# ─── Output"))
            line-idx
            (find-cell-code-end get-line total-lines (+ line-idx 1))))))

;; Helper: Find output section start (returns #f if not found)
(define (find-output-start get-line total-lines line-idx limit)
  (if (>= line-idx (min total-lines limit)) #f
      (let ([line (get-line line-idx)])
        (cond
          [(string-contains? line "# ─── Output ───") line-idx]
          [(or (string-starts-with? line "@cell ")
               (string-starts-with? line "@markdown ")
               (string-starts-with? line "# ═══")) #f]
          [else (find-output-start get-line total-lines (+ line-idx 1) limit)]))))

;; Helper: Find output section end
(define (find-output-end-line get-line total-lines line-idx)
  (if (>= line-idx total-lines) line-idx
      (let ([line (get-line line-idx)])
        (cond
          [(string-contains? line "# ─────────────") (+ line-idx 1)]
          [(or (string-starts-with? line "@cell ")
               (string-starts-with? line "@markdown ")
               (string-starts-with? line "# ═══")) line-idx]
          [else (find-output-end-line get-line total-lines (+ line-idx 1))]))))

;; Helper: Extract code lines from cell (skips @cell marker and separator lines)
(define (extract-cell-code get-line start end)
  (let loop ([idx (+ start 1)] [acc '()])
    (if (>= idx end)
        (reverse acc)
        (let ([line (get-line idx)])
          (if (or (string-starts-with? line "# ═══")
                  (string-starts-with? line "# ─── "))
              (loop (+ idx 1) acc)
              (loop (+ idx 1) (cons line acc)))))))

;; Helper: Delete lines from start to end (inclusive of start, exclusive of end)
(define (delete-line-range start-line end-line)
  ;; Go to start line, select to end line, delete
  (helix.goto (number->string (+ start-line 1)))
  (helix.static.goto_line_start)
  (helix.static.extend_to_line_bounds)
  (let ([lines-to-extend (- end-line start-line 1)])
    (when (> lines-to-extend 0)
      (let loop ([i 0])
        (when (< i lines-to-extend)
          (helix.static.extend_line_below)
          (loop (+ i 1))))))
  (helix.static.delete_selection)
  ;; Delete trailing newline if we're not at start of file
  (when (> start-line 0)
    (helix.static.delete_char_backward)))

;; Helper: Update cell output when execution completes
(define (update-cell-output result-json output-header-line)
  (set! *executing-kernel-dir* #f)

  ;; Re-read document state
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line idx)
    (if (< idx total-lines)
        (text.rope->string (text.rope->line rope idx))
        ""))

  ;; Find the "Running..." placeholder
  (define (find-running-marker line-idx limit)
    (cond
      [(>= line-idx limit) #f]
      [(string-contains? (get-line line-idx) "# ⏳ Running...") line-idx]
      [else (find-running-marker (+ line-idx 1) limit)]))

  (define running-line (find-running-marker output-header-line (+ output-header-line 10)))

  (when running-line
    ;; Delete the "Running..." line
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

     ;; Insert output representation
     (when (> (string-length output-repr) 0)
       (helix.static.insert_string (string-append output-repr "\n")))

     ;; Insert stderr if present
     (when (> (string-length stderr-text) 0)
       (helix.static.insert_string (string-append "# stderr: " stderr-text "\n")))

     ;; Insert footer
     (helix.static.insert_string "# ─────────────\n")

     (if has-error
         (set-status! "Cell executed with errors")
         (set-status! "✓ Cell executed"))])

  (helix.redraw))

;; Helper: Poll for execution result (called repeatedly via delayed callback)
(define (poll-for-result kernel-dir output-header-line)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     ;; Still running - poll again in 100ms (non-blocking)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-for-result kernel-dir output-header-line)))]
    [else
     ;; Done - update UI with result
     (update-cell-output result-json output-header-line)]))

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
    (void))

  ;; Detect language
  (define path (editor-document->path doc-id))
  (define lang (cond
                 [(string-contains? path ".ipynb") "julia"]
                 [(string-contains? path ".jl") "julia"]
                 [(string-contains? path ".py") "python"]
                 [else "julia"]))

  ;; Find and delete existing output section if present
  (define output-start (find-output-start get-line total-lines cell-code-end (+ cell-code-end 5)))

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
  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  ;; Get cell index for dependency tracking
  (define cell-info-json (get-cell-at-line path current-line))
  (define cell-index-str (json-get cell-info-json "cell_index"))
  (define cell-index (if cell-index-str (string->number cell-index-str) 0))

  ;; Insert output header
  (helix.static.insert_string "\n\n# ─── Output ───\n# ⏳ Running...\n")
  (set-status! "⏳ Executing cell...")
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
       (lambda () (poll-for-result kernel-dir cell-code-end)))]
    [else
     ;; Error starting execution
     (define err (let ([e (json-get start-result "error")]) (if (> (string-length e) 0) e "Unknown error")))
     (helix.static.insert_string (string-append "# ERROR: " err "\n"))
     (helix.static.insert_string "# ─────────────\n")
     (set-status! (string-append "✗ " err))
     (set! *executing-kernel-dir* #f)
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

;;; Find the line number of a cell marker with given index in a converted file.
;;; Returns the line number of the "@cell N ..." marker, or #f if not found.
(define (find-cell-marker-by-index rope total-lines cell-index)
  ;; Pattern: @cell N (where N is the cell index)
  (define code-pattern (string-append "@cell " (number->string cell-index) " "))
  (define markdown-pattern (string-append "@markdown " (number->string cell-index)))

  (define (get-line idx)
    (if (< idx total-lines)
        (text.rope->string (text.rope->line rope idx))
        ""))

  (let loop ([line-idx 0])
    (cond
      [(>= line-idx total-lines) #f]  ; Not found
      [(string-starts-with? (get-line line-idx) code-pattern) line-idx]  ; Found code cell!
      [(string-starts-with? (get-line line-idx) markdown-pattern) line-idx]  ; Found markdown cell!
      [else (loop (+ line-idx 1))])))

;;@doc
;; Execute all cells in the notebook from top to bottom
;; ONLY works on converted files (not raw .ipynb) since we need to insert outputs
(define (execute-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  ;; Only works on converted files
  (when (string-suffix? path ".ipynb")
    (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
    (error "Not a converted file"))

  ;; Get source notebook path from metadata
  (define cell-info-json (get-cell-at-line path 0))
  (define err (json-get cell-info-json "error"))
  (when err
    (set-status! "Error: Not a converted notebook file")
    (error "Not a converted notebook"))

  (define notebook-path (json-get cell-info-json "source_path"))
  (define lang "julia")  ; TODO: detect from notebook metadata

  ;; Get cell count
  (define cell-count-raw (notebook-cell-count notebook-path))
  (when (< cell-count-raw 0)
    (set-status! "Error: Failed to read notebook")
    (error "Failed to read notebook"))

  ;; Start kernel
  (define kernel-state (kernel-get-for-notebook notebook-path lang))
  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  (set-status! (string-append "Executing " (number->string cell-count-raw) " cells..."))

  ;; Save original cursor position
  (define original-line (current-line-number))

  ;; Execute each cell and insert output
  (let loop ([cell-idx 0] [executed 0])
    (when (< cell-idx cell-count-raw)
      ;; Get cell code from Rust
      (define cell-data-json (notebook-get-cell-code notebook-path cell-idx))
      (define cell-code (json-get cell-data-json "code"))
      (define cell-type (json-get cell-data-json "type"))

      ;; Only execute code cells
      (when (equal? cell-type "code")
        (when (not cell-code)
          (set-status! (string-append "Warning: Cell " (number->string cell-idx) " has no code, skipping"))
          (void))

        (when cell-code
          ;; Find this cell's marker in the converted file
          (define updated-rope (editor->text doc-id))  ; Re-read after previous insertions
          (define updated-total-lines (text.rope-len-lines updated-rope))
          (define cell-marker-line (find-cell-marker-by-index updated-rope updated-total-lines cell-idx))

          (when (not cell-marker-line)
            (set-status! (string-append "ERROR: Cell " (number->string cell-idx) " marker not found in converted file"))
            (error (string-append "Cell marker " (number->string cell-idx) " not found")))

          (when cell-marker-line
            (define (get-line idx)
              (if (< idx updated-total-lines)
                  (text.rope->string (text.rope->line updated-rope idx))
                  ""))

            ;; Find where code ends
            (define cell-code-end (find-cell-code-end get-line updated-total-lines (+ cell-marker-line 1)))

            ;; Delete existing output if present
            (define output-start (find-output-start get-line updated-total-lines cell-code-end (+ cell-code-end 5)))
            (when output-start
              (define output-end (find-output-end-line get-line updated-total-lines (+ output-start 1)))
              (delete-line-range output-start output-end))

            ;; Position cursor at end of cell code
            (helix.goto (number->string cell-code-end))
            (helix.static.goto_line_end)

            ;; Show which cell is executing
            (set-status! (string-append "⚙ Executing cell " (number->string (+ cell-idx 1)) "/" (number->string cell-count-raw) "..."))
            (helix.redraw)

            ;; Execute via kernel
            (define result-json (kernel-execute-cell kernel-dir cell-idx cell-code))
            (define err (json-get result-json "error"))
            (when err
              (set-status! err)
              (error err))

            ;; Extract output
            (define output-repr (json-get result-json "output_repr"))
            (define stdout-text (json-get result-json "stdout"))

            ;; Insert output section
            (helix.static.insert_string "\n\n# ─── Output ───\n")
            (when (> (string-length stdout-text) 0)
              (helix.static.insert_string stdout-text)
              (when (not (string-suffix? stdout-text "\n"))
                (helix.static.insert_string "\n")))
            (when (> (string-length output-repr) 0)
              (helix.static.insert_string (string-append output-repr "\n")))
            (helix.static.insert_string "# ─────────────\n")

            ;; Show progress after each cell
            (set-status! (string-append "✓ Cell " (number->string (+ cell-idx 1)) "/" (number->string cell-count-raw) " done"))
            (helix.redraw))))

      (loop (+ cell-idx 1) (+ executed 1))))

  ;; Return to original position (approximately - line numbers have changed)
  (helix.goto (number->string (+ original-line 1)))
  (set-status! (string-append "✓ Executed all " (number->string cell-count-raw) " cells")))

;;@doc
;; Execute all cells from the top up to and including the current cell
;; ONLY works on converted files (not raw .ipynb) since we need to insert outputs
(define (execute-cells-above)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  ;; Only works on converted files
  (when (string-suffix? path ".ipynb")
    (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
    (error "Not a converted file"))

  ;; Get current cell info
  (define cell-info-json (get-cell-at-line path current-line))
  (define err (json-get cell-info-json "error"))
  (when err
    (set-status! "Error: Not in a notebook file")
    (error "Not in a notebook file"))

  (define notebook-path (json-get cell-info-json "source_path"))
  (define current-cell-idx (string->number (json-get cell-info-json "cell_index")))
  (define lang "julia")  ; TODO: detect from notebook metadata

  ;; Calculate how many cells to execute (0 to current-cell-idx inclusive)
  (define cells-to-execute (+ current-cell-idx 1))

  ;; Start kernel
  (define kernel-state (kernel-get-for-notebook notebook-path lang))
  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  (set-status! (string-append "Executing " (number->string cells-to-execute) " cells up to current..."))

  ;; Save original cursor position
  (define original-line current-line)

  ;; Execute cells from 0 to current-cell-idx (inclusive)
  (let loop ([cell-idx 0] [executed 0])
    (when (<= cell-idx current-cell-idx)
      ;; Get cell code from Rust
      (define cell-data-json (notebook-get-cell-code notebook-path cell-idx))
      (define cell-code (json-get cell-data-json "code"))
      (define cell-type (json-get cell-data-json "type"))

      ;; Only execute code cells
      (when (equal? cell-type "code")
        (when (not cell-code)
          (set-status! (string-append "Warning: Cell " (number->string cell-idx) " has no code, skipping"))
          (void))

        (when cell-code
          ;; Find this cell's marker in the converted file
          (define updated-rope (editor->text doc-id))  ; Re-read after previous insertions
          (define updated-total-lines (text.rope-len-lines updated-rope))
          (define cell-marker-line (find-cell-marker-by-index updated-rope updated-total-lines cell-idx))

          (when (not cell-marker-line)
            (set-status! (string-append "ERROR: Cell " (number->string cell-idx) " marker not found in converted file"))
            (error (string-append "Cell marker " (number->string cell-idx) " not found")))

          (when cell-marker-line
            (define (get-line idx)
              (if (< idx updated-total-lines)
                  (text.rope->string (text.rope->line updated-rope idx))
                  ""))

            ;; Find where code ends
            (define cell-code-end (find-cell-code-end get-line updated-total-lines (+ cell-marker-line 1)))

            ;; Delete existing output if present
            (define output-start (find-output-start get-line updated-total-lines cell-code-end (+ cell-code-end 5)))
            (when output-start
              (define output-end (find-output-end-line get-line updated-total-lines (+ output-start 1)))
              (delete-line-range output-start output-end))

            ;; Position cursor at end of cell code
            (helix.goto (number->string cell-code-end))
            (helix.static.goto_line_end)

            ;; Show which cell is executing
            (set-status! (string-append "⚙ Executing cell " (number->string (+ cell-idx 1)) "/" (number->string cells-to-execute) "..."))
            (helix.redraw)

            ;; Execute via kernel
            (define result-json (kernel-execute-cell kernel-dir cell-idx cell-code))
            (define err (json-get result-json "error"))
            (when err
              (set-status! err)
              (error err))

            ;; Extract output
            (define output-repr (json-get result-json "output_repr"))
            (define stdout-text (json-get result-json "stdout"))

            ;; Insert output section
            (helix.static.insert_string "\n\n# ─── Output ───\n")
            (when (> (string-length stdout-text) 0)
              (helix.static.insert_string stdout-text)
              (when (not (string-suffix? stdout-text "\n"))
                (helix.static.insert_string "\n")))
            (when (> (string-length output-repr) 0)
              (helix.static.insert_string (string-append output-repr "\n")))
            (helix.static.insert_string "# ─────────────\n")

            ;; Show progress after each cell
            (set-status! (string-append "✓ Cell " (number->string (+ cell-idx 1)) "/" (number->string cells-to-execute) " done"))
            (helix.redraw))))

      (loop (+ cell-idx 1) (+ executed 1))))

  ;; Return to original position (approximately - line numbers have changed)
  (helix.goto (number->string (+ original-line 1)))
  (set-status! (string-append "✓ Executed " (number->string cells-to-execute) " cells up to current")))
