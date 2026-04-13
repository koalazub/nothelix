;;; execution.scm - Cell execution orchestration
;;;
;;; Orchestrates the full execution cycle: starting async kernel execution,
;;; polling for results, and delegating output insertion to output-insert.scm.
;;; Cell boundary detection, cursor restoration, and image management live
;;; in their own focused modules.

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "cell-boundaries.scm")
(require "cursor-restore.scm")
(require "image-cache.scm")
(require "output-insert.scm")
(require "kernel.scm")
(require "spinner.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

;; FFI imports for execution orchestration
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kernel-execute-cell-start
                          kernel-poll-result
                          kernel-interrupt
                          get-cell-at-line
                          get-cell-code-from-jl
                          list-jl-code-cells
                          json-get))

(provide execute-cell
         execute-all-cells
         execute-cells-above
         cancel-cell
         ;; Re-export from sub-modules so existing consumers don't break
         render-cached-images
         sync-images-to-markers!
         sync-images-if-markers-changed!
         find-cell-start-line
         find-cell-code-end
         find-output-start
         find-output-end-line
         extract-cell-code
         delete-line-range
         find-cell-marker-by-index)

;;@doc
;; Advance the spinner animation in the status line.
(define (update-spinner-frame)
  (define new-frame (spinner-next-frame))
  (set-status! (string-append new-frame " Executing cell...")))

;;@doc
;; Poll for execution result with exponential backoff (100ms -> 500ms).
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
;; Execute the code cell under the cursor (async, non-blocking).
(define (execute-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  ;; Save cursor BEFORE any buffer mutation for later restoration.
  (save-cursor-for-restore! doc-id)

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define cell-lines (extract-cell-code get-line cell-start cell-code-end))
  (define code (string-join cell-lines "\n"))

  (when (equal? (string-length code) 0)
    (set-status! "Cell is empty")
    (helix.redraw)
    (error "Cell is empty"))

  (define path (editor-document->path doc-id))
  (define lang (cond
                 [(string-contains? path ".ipynb") "julia"]
                 [(string-contains? path ".jl") "julia"]
                 [(string-contains? path ".py") "python"]
                 [else "julia"]))

  ;; Delete existing output section (+ surrounding blank-line padding).
  (define output-start (find-output-start get-line total-lines cell-code-end))
  (when output-start
    (define output-end (find-output-end-line get-line total-lines (+ output-start 1)))
    (define extended-start (expand-delete-start-backward get-line cell-start output-start))
    (define extended-end (expand-delete-end-forward get-line total-lines output-end))
    (delete-line-range extended-start extended-end))

  ;; Position cursor at last real code line; use goto_line_end_newline
  ;; (not goto_line_end) to avoid slicing the last grapheme.
  (define insert-at-line
    (find-last-non-blank-line-before get-line cell-start cell-code-end))
  (helix.goto (number->string (+ insert-at-line 1)))
  (helix.static.goto_line_end_newline)

  (define notebook-path (editor-document->path doc-id))
  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (define cell-info-json (get-cell-at-line path current-line))
      (define cell-index-str (json-get cell-info-json "cell_index"))
      (define cell-index (if (> (string-length cell-index-str) 0)
                              (string->number cell-index-str)
                              0))

      ;; Insert output header only; spinner stays in status line.
      (spinner-reset)
      (define spinner-frame (spinner-next-frame))
      (helix.static.insert_string "\n\n# ─── Output ───\n")
      (helix.static.commit-changes-to-history)
      (set-status! (string-append spinner-frame " Executing cell..."))
      (helix.redraw)

      (set! *executing-kernel-dir* kernel-dir)
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
;; Cancel/interrupt any running cell execution.
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
;; Execute all cells in the notebook from top to bottom.
;; Works on .jl converted files only.
(define (execute-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  (save-cursor-for-restore! doc-id)

  (when (string-suffix? path ".ipynb")
    (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
    (error "Not a converted file"))
  (when (not (string-suffix? path ".jl"))
    (set-status! "Error: Only .jl files supported")
    (error "Not a .jl file"))

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

  (define notebook-path path)
  (define lang "julia")
  (kernel-get-for-notebook notebook-path lang
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (set-status! (string-append "Executing " (number->string cell-count) " cells: " indices-str))
      (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))))

;;@doc
;; Execute a list of cells sequentially with async polling.
(define (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
  (if (null? remaining-indices)
      (begin
        (restore-cursor-for! doc-id)
        (clear-cursor-restore! doc-id)
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
;; Execute a single cell asynchronously, then continue with remaining cells.
(define (execute-single-cell-async doc-id notebook-path kernel-dir jl-path cell-idx cell-code cell-indices remaining-indices total-count original-line)
  (define updated-rope (editor->text doc-id))
  (define updated-total-lines (text.rope-len-lines updated-rope))
  (define cell-marker-line (find-cell-marker-by-index updated-rope updated-total-lines cell-idx))

  (if (not cell-marker-line)
      (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
      (let ()
        (define (get-line idx)
          (if (< idx updated-total-lines)
              (text.rope->string (text.rope->line updated-rope idx))
              ""))

        (define cell-code-end (find-cell-code-end get-line updated-total-lines (+ cell-marker-line 1)))

        ;; Delete existing output + surrounding blank padding.
        (define output-start (find-output-start get-line updated-total-lines cell-code-end))
        (when output-start
          (define output-end (find-output-end-line get-line updated-total-lines (+ output-start 1)))
          (define extended-start (expand-delete-start-backward get-line cell-marker-line output-start))
          (define extended-end (expand-delete-end-forward get-line updated-total-lines output-end))
          (delete-line-range extended-start extended-end))

        ;; Position at last real code line; goto_line_end_newline avoids
        ;; slicing the last grapheme.
        (define insert-at-line
          (find-last-non-blank-line-before get-line cell-marker-line cell-code-end))
        (helix.goto (number->string (+ insert-at-line 1)))
        (helix.static.goto_line_end_newline)
        (spinner-reset)
        (define spinner-frame (spinner-next-frame))
        (define executed-count (- total-count (length remaining-indices)))
        (helix.static.insert_string "\n\n# ─── Output ───\n")
        (helix.static.commit-changes-to-history)
        (set-status! (string-append spinner-frame " Executing cell " (number->string executed-count) "/" (number->string total-count) "..."))
        (helix.redraw)

        (define start-result (kernel-execute-cell-start kernel-dir cell-idx cell-code))
        (define start-status (json-get start-result "status"))

        (if (equal? start-status "started")
            (enqueue-thread-local-callback-with-delay 100
              (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)))
            (let ()
              (define err (json-get start-result "error"))
              (handle-execution-error cell-code-end err)
              (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line))))))

;;@doc
;; Handle an execution error: write error line and footer under the output header.
(define (handle-execution-error cell-code-end err)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define post-rope (editor->text doc-id))
  (define post-lines (text.rope-len-lines post-rope))

  ;; Scan forward from cell-code-end to find the output header.
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

;;@doc
;; Parse comma-separated string into list of numbers.
(define (parse-indices-string str)
  (if (or (not str) (equal? str ""))
      '()
      (map string->number (string-split str ","))))

;;@doc
;; Execute all cells from the top up to and including the current cell.
;; ONLY works on converted files (not raw .ipynb).
(define (execute-cells-above)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! "Error: No file path")
    (error "No file path"))

  ;; Save file first so Rust can read the latest content.
  (helix.write)

  (enqueue-thread-local-callback-with-delay 100
    (lambda ()
      (when (string-suffix? path ".ipynb")
        (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
        (error "Not a converted file"))

      (define cell-info-json (get-cell-at-line path current-line))
      (define err (json-get cell-info-json "error"))
      (when (> (string-length err) 0)
        (set-status! "Error: Not in a notebook file")
        (error "Not in a notebook file"))

      (define notebook-path (json-get cell-info-json "source_path"))
      (define current-cell-idx (string->number (json-get cell-info-json "cell_index")))
      (define lang "julia")

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

      (kernel-get-for-notebook notebook-path lang
        (lambda (kernel-state)
          (define kernel-dir (hash-get kernel-state 'kernel-dir))
          (set-status! (string-append "Executing cells: " indices-str))
          (execute-cell-list doc-id notebook-path kernel-dir path cell-indices cell-indices cell-count current-line))))))
