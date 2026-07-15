;;; execution.scm — Cell execution orchestration

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "cell-boundaries.scm")
(require "cursor-restore.scm")
(require "resume.scm")
(require "image-cache.scm")
(require "output-insert.scm")
(require "output-store.scm")
(require "output-render.scm")
(require "project-config.scm")
(require "kernel.scm")
(require "spinner.scm")
(require "stale-tags.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

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
         restore-cell-outputs-on-open!
         render-cached-images
         sync-images-to-markers!
         sync-images-if-markers-changed!
         find-cell-start-line
         find-cell-code-end
         extract-cell-code
         find-cell-marker-by-index)

;;@doc
;; Render a kernel-start failure as virtual error rows at the cell's anchor
;; and persist it to the output store, instead of writing text into the buffer.
(define (render-cell-error! anchor-line store-cell-id store-source-hash err)
  (define error-rows (list (string-append "# ERROR: " err)))
  (when anchor-line
    (try-set-output-lines-below! anchor-line error-rows))
  (store-put! store-cell-id store-source-hash
              (encode-outputs+rows
                (outputs-json-for-cell "" "" "" err) error-rows))
  (set-status! (string-append "✗ " err)))

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
     (update-cell-output result-json jl-path cell-index kernel-dir)]))

;; Shared preflight: saved .jl notebook, cursor saved, kernel keyed by path.
(define (with-saved-notebook command-name on-ready)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define current-line (current-line-number))

  (when (not path)
    (set-status! (string-append command-name ": no file path"))
    (error "No file path"))
  (when (string-suffix? path ".ipynb")
    (set-status! "Error: Use :convert-notebook first. Cannot insert outputs into .ipynb JSON")
    (error "Not a converted file"))
  (when (not (string-suffix? path ".jl"))
    (set-status! (string-append command-name ": only .jl notebook files are supported"))
    (error "Not a .jl file"))

  (save-cursor-for-restore! doc-id)
  (helix.write)
  (enqueue-thread-local-callback-with-delay 100
    (lambda () (on-ready doc-id path current-line))))

;;@doc
;; Execute the code cell under the cursor (async, non-blocking).
(define (execute-cell)
  (save-resume-position!)
  (with-saved-notebook ":execute-cell" execute-cell-under-cursor))

(define (execute-cell-under-cursor doc-id path current-line)
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (clear-stale-tag-for-line! cell-start)

  (when (not (string-starts-with? (string-trim (get-line cell-start)) "@cell"))
    (set-status! "Not a code cell — @markdown/@raw/@typst cells are not executed")
    (helix.redraw)
    (error "not a code cell"))

  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define cell-lines (extract-cell-code get-line cell-start cell-code-end))
  (define code (string-join cell-lines "\n"))

  (when (equal? (string-length code) 0)
    (set-status! "Cell is empty")
    (helix.redraw)
    (error "Cell is empty"))

  (define cell-info-json (get-cell-at-line path current-line))
  (define cell-index-str (json-get cell-info-json "cell_index"))
  (define cell-index (if (> (string-length cell-index-str) 0)
                          (string->number cell-index-str)
                          0))
  (clear-cell-output! cell-index)

  (define insert-at-line
    (find-last-non-blank-line-before get-line cell-start cell-code-end))
  (helix.goto (number->string (+ insert-at-line 1)))
  (helix.static.goto_line_end_newline)

  (kernel-get-for-notebook path "julia"
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))

      (spinner-reset)
      (define spinner-frame (spinner-next-frame))
      (set-status! (string-append spinner-frame " Executing cell..."))
      (helix.redraw)

      (set! *executing-kernel-dir* kernel-dir)
      (define start-result (kernel-execute-cell-start kernel-dir cell-index code (plot-mode)))
      (define start-status (json-get start-result "status"))

      (cond
        [(equal? start-status "started")
          (enqueue-thread-local-callback-with-delay 100
            (lambda () (poll-for-result kernel-dir path cell-index)))]
        [else
         (define err (let ([e (json-get start-result "error")]) (if (> (string-length e) 0) e "Unknown error")))
         (when (or (string-contains? err "does not exist")
                   (string-contains? err "PID file missing"))
           (set! *kernels* (hash-remove *kernels* path)))
         (render-cell-error! (- cell-code-end 1) (cell-id cell-index) (cell-source-hash code) err)
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
;; Execute all cells in the notebook top-to-bottom (.jl converted files only).
(define (execute-all-cells)
  (with-saved-notebook ":execute-all-cells"
    (lambda (doc-id path current-line)
      (execute-cells-up-to doc-id path current-line 999999))))

;; Run every code cell with index ≤ `limit-idx`, in file order.
(define (execute-cells-up-to doc-id path current-line limit-idx)
  (define cells-json (list-jl-code-cells path limit-idx))
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

  (kernel-get-for-notebook path "julia"
    (lambda (kernel-state)
      (define kernel-dir (hash-get kernel-state 'kernel-dir))
      (set-status! (string-append "Executing " (number->string cell-count) " cells: " indices-str))
      (execute-cell-list doc-id path kernel-dir path cell-indices cell-indices cell-count current-line))))

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
  (when cell-marker-line (clear-stale-tag-for-line! cell-marker-line))

  (if (not cell-marker-line)
      (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)
      (let ()
        (define (get-line idx)
          (if (< idx updated-total-lines)
              (text.rope->string (text.rope->line updated-rope idx))
              ""))

        (define cell-code-end (find-cell-code-end get-line updated-total-lines (+ cell-marker-line 1)))

        (clear-cell-output! cell-idx)

        (define insert-at-line
          (find-last-non-blank-line-before get-line cell-marker-line cell-code-end))
        (move-to-line-start-no-center! updated-rope insert-at-line)
        (helix.static.goto_line_end_newline)
        (spinner-reset)
        (define spinner-frame (spinner-next-frame))
        (define executed-count (- total-count (length remaining-indices)))
        (set-status! (string-append spinner-frame " Executing cell " (number->string executed-count) "/" (number->string total-count) "..."))
        (helix.redraw)

        (set! *executing-kernel-dir* kernel-dir)
        (define start-result (kernel-execute-cell-start kernel-dir cell-idx cell-code (plot-mode)))
        (define start-status (json-get start-result "status"))

        (if (equal? start-status "started")
            (enqueue-thread-local-callback-with-delay 100
              (lambda () (poll-cell-list-result doc-id notebook-path kernel-dir jl-path cell-idx cell-indices remaining-indices total-count original-line)))
            (let ()
              (define err (json-get start-result "error"))
              (set! *executing-kernel-dir* #false)
              (handle-execution-error cell-code-end err cell-idx cell-code)
              (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line))))))

;;@doc
;; Handle an execution error: render error rows at the cell's anchor and store them.
(define (handle-execution-error cell-code-end err cell-idx cell-code)
  (render-cell-error! (- cell-code-end 1) (cell-id cell-idx) (cell-source-hash cell-code) err)
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
     (update-cell-output result-json jl-path cell-idx kernel-dir)
     (restore-cursor-for! doc-id)
     (enqueue-thread-local-callback-with-delay 10
       (lambda () (execute-cell-list doc-id notebook-path kernel-dir jl-path cell-indices remaining-indices total-count original-line)))]))

;;@doc
;; Parse comma-separated string into list of numbers.
(define (parse-indices-string str)
  (if (or (not str) (equal? str ""))
      '()
      (filter (lambda (n) (and n (number? n)))
              (map string->number (string-split str ",")))))

;;@doc
;; Execute all cells from the top up to and including the current cell (.jl converted files only).
(define (execute-cells-above)
  (with-saved-notebook ":execute-cells-above"
    (lambda (doc-id path current-line)
      (define cell-info-json (get-cell-at-line path current-line))
      (define err (json-get cell-info-json "error"))
      (when (> (string-length err) 0)
        (set-status! "Error: Not in a notebook file")
        (error "Not in a notebook file"))

      (define current-cell-idx (string->number (json-get cell-info-json "cell_index")))
      (execute-cells-up-to doc-id path current-line current-cell-idx))))

;;@doc
;; Strip stale legacy `# ─── Output ───` blocks (written as buffer text by an
;; older binary's in-buffer fallback) so the virtual-row anchor lands on the
;; true end of each cell's code. Ranges are collected from one rope snapshot
;; and deleted bottom-up (highest line first): deleting a higher block never
;; shifts a lower block's line numbers, so every collected range stays valid
;; without a re-scan. The deletions are committed through the tagged,
;; non-undo path so they never pollute the user's undo history, and no commit
;; fires when there is nothing to strip (idempotent — a second open is a
;; no-op).
(define (strip-legacy-output-blocks! doc-id cell-indices)
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))
  (define ranges
    (filter (lambda (r) r)
            (map (lambda (cell-idx)
                   (define marker-line (find-cell-marker-by-index rope total-lines cell-idx))
                   (and marker-line
                        (legacy-output-block-range get-line total-lines (+ marker-line 1))))
                 cell-indices)))
  (define bottom-up (sort ranges (lambda (a b) (> (car a) (car b)))))
  (unless (null? bottom-up)
    (for-each (lambda (r) (delete-line-range (car r) (cdr r) #false)) bottom-up)
    (try-commit-output-changes!)))

;;@doc
;; Re-render every cell's output from the output store on document open,
;; skipping cells whose stored hash no longer matches their current source
;; (stale — edited since last run).
(define (restore-cell-outputs-on-open! doc-id path)
  (when (and path (string-suffix? path ".jl"))
    (define cells-json (list-jl-code-cells path 999999))
    (define cells-err (json-get cells-json "error"))
    (when (equal? (string-length cells-err) 0)
      (define indices-str (json-get cells-json "indices"))
      (define cell-indices (parse-indices-string indices-str))
      (strip-legacy-output-blocks! doc-id cell-indices)
      (define rope (editor->text doc-id))
      (define total-lines (text.rope-len-lines rope))
      (define (get-line idx) (doc-get-line rope total-lines idx))

      (try-clear-all-output-lines!)

      (for-each
        (lambda (cell-idx)
          (define marker-line (find-cell-marker-by-index rope total-lines cell-idx))
          (when marker-line
            (define code-end (find-cell-code-end get-line total-lines (+ marker-line 1)))
            (define anchor-line (- code-end 1))
            (define code (string-join (extract-cell-code get-line marker-line code-end) "\n"))
            (define stored (store-get-for path (cell-id cell-idx)))
            (define hash (cell-source-hash code))
            (define rows (decode-stored-rows stored hash))
            (define text-plot-groups
              (map (lambda (plot) (text-plot->styled-rows (car plot) (cdr plot)))
                   (decode-text-plots-blob (or (decode-stored-text-plots-blob stored hash) ""))))
            (when (or (list? rows) (not (null? text-plot-groups)))
              (try-set-output-lines-below! anchor-line
                (assign-cycling-bars
                  (cons (if (list? rows) rows '()) text-plot-groups))))))
        cell-indices))))
