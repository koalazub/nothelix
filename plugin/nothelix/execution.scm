;;; execution.scm - Cell execution and output management

(require "string-utils.scm")
(require "kernel.scm")
(require "graphics.scm")  ; For render-image-b64, graphics-protocol
(require "spinner.scm")   ; For spinner-next-frame, spinner-reset
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For cursor-position, set-status!
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
;; enqueue-thread-local-callback-with-delay is a global Helix function

;; Global counter for unique image IDs (incremented each time an image is rendered)
(define *image-id-counter* 1)

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
                          get-notebook-source-path
                          notebook-cell-count
                          notebook-get-cell-code
                          get-cell-code-from-jl      ;; Extract code from .jl files by @cell marker
                          list-jl-code-cells         ;; List all code cell indices from .jl file
                          kernel-execute-cell
                          kernel-interrupt
                          ;; Async execution (non-blocking)
                          kernel-execute-cell-start
                          kernel-poll-result
                          ;; JSON utilities (proper serde parsing)
                          json-get
                          json-get-bool
                          json-get-first-image
                          ;; Kitty graphics - returns raw escape sequence bytes
                          kitty-display-image-bytes
                          ;; Kitty graphics - Unicode placeholder mode (proper scrolling)
                          kitty-placeholder-image
                          ;; Base64 decode utility
                          base64-decode-to-string
                          ;; Logging
                          log-info))

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
;; Searches from line-idx until next cell marker or EOF
(define (find-output-start get-line total-lines line-idx limit)
  (if (>= line-idx total-lines) #f
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

;; Helper: Update cell output when execution completes
;; Searches for the spinner line instead of using cached positions
(define (update-cell-output result-json)
  (set! *executing-kernel-dir* #f)

  ;; Re-read document state FRESH
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line idx)
    (if (< idx total-lines)
        (text.rope->string (text.rope->line rope idx))
        ""))

  ;; Search entire document for "Executing..." placeholder
  (define (find-running-marker line-idx)
    (cond
      [(>= line-idx total-lines) #f]
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

     ;; Check for images first - if we successfully render an image, skip output_repr
     (log-info (string-append "[execution] result-json: " (truncate-string result-json 500)))
     (define image-b64 (json-get-first-image result-json))
     (log-info (string-append "[execution] image-b64 length: " (number->string (string-length image-b64))))
     (define image-rendered #f)
     
     (when (> (string-length image-b64) 0)
       (define image-id *image-id-counter*)
       (set! *image-id-counter* (+ *image-id-counter* 1))
       (when (> *image-id-counter* 16777200)
         (set! *image-id-counter* 1))
       (define image-rows 12)

       (log-info (string-append "[execution] Calling kitty-display-image-bytes with id=" (number->string image-id)))
       (define escape-seq (kitty-display-image-bytes image-b64 image-id image-rows))
       (log-info (string-append "[execution] escape-seq length: " (number->string (string-length escape-seq))))
       (log-info (string-append "[execution] escape-seq starts-with ERROR?: " (if (string-starts-with? escape-seq "ERROR:") "yes" "no")))

       (if (not (string-starts-with? escape-seq "ERROR:"))
           (begin
             (define char-idx (cursor-position))
             (log-info (string-append "[execution] Calling add-raw-content! at char-idx=" (number->string char-idx) " image-id=" (number->string image-id)))
             (helix.static.add-raw-content! escape-seq image-id image-rows char-idx)
             (log-info "[execution] add-raw-content! called successfully")
             (set! image-rendered #t))
           (begin
             (log-info (string-append "[execution] ERROR from kitty-display-image-bytes: " escape-seq))
             (helix.static.insert_string (string-append "# 📊 [Plot: " (number->string (quotient (string-length image-b64) 1024)) "KB - render failed]\n")))))

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

;; Helper: Update the spinner frame in the "Executing..." line
;; Searches for the spinner line instead of using cached positions
(define (update-spinner-frame)
  ;; Re-read document state FRESH each time
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line idx)
    (if (< idx total-lines)
        (text.rope->string (text.rope->line rope idx))
        ""))

  ;; Search entire document for "Executing..." line
  (define (find-spinner-line line-idx)
    (cond
      [(>= line-idx total-lines) #f]
      [(string-contains? (get-line line-idx) "Executing...") line-idx]
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
(define (poll-for-result kernel-dir)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     ;; Still running - update spinner and poll again in 100ms (non-blocking)
     (update-spinner-frame)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-for-result kernel-dir)))]
    [else
     ;; Done - update UI with result
     (update-cell-output result-json)]))

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
       (lambda () (poll-for-result kernel-dir)))]
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
        (define output-start (find-output-start get-line updated-total-lines cell-code-end (+ cell-code-end 5)))
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
              (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)))
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

(define (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
  (define result-json (kernel-poll-result kernel-dir))
  (define status (json-get result-json "status"))

  (cond
    [(equal? status "pending")
     (update-spinner-frame)
     (enqueue-thread-local-callback-with-delay 100
       (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)))]
    [else
     (update-cell-output result-json)
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
  (helix.run-shell-command "sleep 0.1")

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
  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  (set-status! (string-append "Executing cells: " indices-str))
  (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))
