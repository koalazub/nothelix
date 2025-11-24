;; Helix commands - functions exported here become typed commands
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/keymaps.scm")
(require "helix/ext.scm")
(require-builtin helix/core/static)
(require-builtin helix/core/text as text.)
(require-builtin helix/core/keymaps as helix.keymaps.)
(require (prefix-in helix. "helix/commands.scm"))
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in kernel. "kernel-manager.scm"))
(require "helix/components.scm")

(provide execute-cell test-apis next-cell previous-cell test-thread cell-picker)

;;@doc
;; Test Steel APIs (get current line, selection, document text)
(define (test-apis)
  (define line-num (get-current-line-number *helix.cx*))
  (define selection (current-selection->string *helix.cx*))
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))

  (displayln "=== API Test Results ===")
  (displayln (string-append "Current line: " (number->string line-num)))
  (displayln (string-append "Selection length: " (number->string (string-length selection))))
  (displayln (string-append "Document length: " (number->string (string-length text))))
  (displayln "=== Test Complete ==="))

;;@doc
;; Test if enqueue-thread-local-callback works
(define (test-thread)
  (helix.run-shell-command "echo 'Before callback' > /tmp/thread-test.log")
  (set-status! "Testing callback...")

  (enqueue-thread-local-callback
    (lambda ()
      (helix.run-shell-command "echo 'In callback' >> /tmp/thread-test.log")
      (helix.run-shell-command "date >> /tmp/thread-test.log")
      (set-status! "Callback executed!")))

  (helix.run-shell-command "echo 'After enqueue' >> /tmp/thread-test.log")
  (set-status! "Callback enqueued"))

;;@doc Execute the current Jupyter cell
(define (execute-cell)
  (helix.run-shell-command "echo 'execute-cell called!' > /tmp/exec-debug.log")
  (helix.run-shell-command "date >> /tmp/exec-debug.log")
  (set-status! "Executing cell...")

    ;; Get document text and cursor position
    (define focus (editor-focus))
    (define doc-id (editor->doc-id focus))
    (define rope (editor->text doc-id))
    (define current-line (get-current-line-number *helix.cx*))

  ;; Use rope API to get line count and individual lines
  (define total-lines (text.rope-len-lines rope))

  ;; Helper to get a line from rope
  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  ;; Find cell start (search backwards for cell header)
  (define (find-cell-start line-idx)
    (if (< line-idx 0)
        0
        (let ([line (get-line line-idx)])
          (if (or (string-contains? line "# ─── Code Cell")
                  (string-contains? line "# %%"))
              line-idx
              (find-cell-start (- line-idx 1))))))

  ;; Find cell end (search forwards for next cell marker or end)
  (define (find-cell-end line-idx)
    (if (>= line-idx total-lines)
        total-lines
        (let ([line (get-line line-idx)])
          (if (or (string-contains? line "# ─── Code Cell")
                  (string-contains? line "# ─── Markdown Cell")
                  (string-contains? line "# ─── Output")
                  (string-contains? line "# %%"))
              line-idx
              (find-cell-end (+ line-idx 1))))))

  (define cell-start (find-cell-start current-line))
  (define cell-end (find-cell-end (+ cell-start 1)))

  ;; Extract cell code (skip header, exclude only cell markers)
  (define (collect-code start end)
    (let loop ([idx (+ start 1)]
               [acc '()])
      (if (>= idx end)
          (reverse acc)
          (let ([line (get-line idx)])
            ;; Skip only cell marker lines, keep everything else including comments
            (if (or (string-contains? line "# ─── ")
                    (string-contains? line "# ───"))
                (loop (+ idx 1) acc)
                (loop (+ idx 1) (cons line acc)))))))

  (define cell-lines (collect-code cell-start cell-end))
  (define code (string-join cell-lines "\n"))

  ;; Check if code is empty
  (when (equal? (string-length code) 0)
    (set-status! "Error: Cell is empty")
    (helix.redraw)
    (void))

  ;; Detect language from file path
  (define path (editor-document->path doc-id))
  (define lang (cond
                 [(string-contains? path ".ipynb") "julia"]  ; TODO: detect from metadata
                 [(string-contains? path ".jl") "julia"]
                 [(string-contains? path ".py") "python"]
                 [else "julia"]))

  ;; Validate language is supported
  (when (not (or (equal? lang "julia") (equal? lang "python")))
    (set-status! (string-append "Error: Unsupported language: " lang))
    (helix.redraw)
    (void))

  ;; Show what we detected
  (set-status! (string-append "Executing " lang " cell at line " (number->string cell-start)))

  ;; Check if output section already exists and delete it
  (define (find-output-section line-idx)
    (if (>= line-idx total-lines)
        #f
        (let ([line (get-line line-idx)])
          (cond
            [(string-contains? line "# ─── Output ───") line-idx]
            [(or (string-contains? line "# ─── Code Cell")
                 (string-contains? line "# ─── Markdown Cell")) #f]
            [else (find-output-section (+ line-idx 1))]))))

  ;; Move cursor to end of cell first
  (helix.goto (number->string cell-end))
  (helix.static.goto_line_end)

  ;; Check if output section exists right after current position
  (define output-start (find-output-section cell-end))

  ;; If output exists, delete it
  (when output-start
    ;; Find end of output section (stop at closing marker OR next cell)
    (define (find-output-end line-idx)
      (if (>= line-idx total-lines)
          total-lines
          (let ([line (get-line line-idx)])
            (cond
              [(string-contains? line "# ─────────────") (+ line-idx 1)]
              [(or (string-contains? line "# ─── Code Cell")
                   (string-contains? line "# ─── Markdown Cell")
                   (string-contains? line "# %%")) line-idx]
              [else (find-output-end (+ line-idx 1))]))))

    (define output-end (find-output-end (+ output-start 1)))

    ;; Select and delete the output region
    (helix.goto (number->string output-start))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (let loop ([i output-start])
      (when (< i output-end)
        (helix.static.extend_line_below)
        (loop (+ i 1))))
    (helix.static.delete_selection))

  ;; Insert output section (cursor is already at the right position after deletion or at cell-end)
  (helix.static.insert_string "\n\n# ─── Output ───\n")

  ;; Get or start kernel for this notebook
  (define notebook-path (editor-document->path doc-id))
  (define kernel-state (kernel.kernel-get-for-notebook notebook-path lang))

  ;; Get kernel files
  (define input-file (hash-get kernel-state 'input-file))
  (define output-file (hash-get kernel-state 'output-file))

  ;; Write code to kernel input file - redirect output to avoid buffer pollution
  (helix.run-shell-command (string-append "cat > " input-file " <<'EOF'\n" code "\nEOF"))

  (define marker "__COMPLETE__")

  ;; Show executing feedback
  (set-status! "⚙ Executing cell...")
  (helix.redraw)

  ;; Wait for kernel to complete synchronously (blocks editor)
  (define done-file (string-append output-file ".done"))
  (helix.run-shell-command (string-append "while [ ! -f " done-file " ]; do sleep 0.1; done"))

  ;; Read and insert output using shell commands
  (define output-tmp "/tmp/helix-cell-output.txt")
  (helix.run-shell-command (string-append "grep -v '" marker "' " output-file " > " output-tmp " 2>/dev/null || echo '' > " output-tmp))

  ;; Insert the output into the buffer
  (helix.insert-output (string-append "cat " output-tmp))
  (helix.static.insert_string "# ─────────────")

  (helix.redraw)
  (set-status! "✓ Cell execution complete!"))

;;@doc Jump to next cell
(define (next-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (get-current-line-number *helix.cx*))
  (define total-lines (text.rope-len-lines rope))

  ;; Helper to get a line
  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  ;; Check if line is a cell marker
  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-contains? line "# ─── Code Cell")
          (string-contains? line "# ─── Markdown Cell"))))

  ;; Skip ahead from current position to search for next cell
  ;; If on a marker, skip 2 lines; otherwise skip 1
  (define search-start
    (if (is-cell-marker? current-line)
        (+ current-line 2)
        (+ current-line 1)))

  ;; Find next cell marker starting from search-start
  (define (find-next-cell line-idx)
    (cond
      [(>= line-idx total-lines) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-next-cell (+ line-idx 1))]))

  (define next-cell-line (find-next-cell search-start))

  (if next-cell-line
      (begin
        (helix.goto (number->string next-cell-line))
        (set-status! "Jumped to next cell"))
      (set-status! "No next cell found")))

;;@doc Jump to previous cell
(define (previous-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (get-current-line-number *helix.cx*))
  (define total-lines (text.rope-len-lines rope))

  ;; Helper to get a line
  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  ;; Check if line is a cell marker
  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-contains? line "# ─── Code Cell")
          (string-contains? line "# ─── Markdown Cell"))))

  ;; Skip back from current position to search for previous cell
  ;; If on a marker, skip 2 lines; otherwise skip 1
  (define search-start
    (if (is-cell-marker? current-line)
        (- current-line 2)
        (- current-line 1)))

  ;; Find previous cell marker starting from search-start
  (define (find-prev-cell line-idx)
    (cond
      [(< line-idx 0) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-prev-cell (- line-idx 1))]))

  (define prev-cell-line (find-prev-cell search-start))

  (if prev-cell-line
      (begin
        (helix.goto (number->string prev-cell-line))
        (set-status! "Jumped to previous cell"))
      (set-status! "No previous cell found")))

;; Helper: string-contains?
(define (string-contains? str substr)
  (and (>= (string-length str) (string-length substr))
       (let loop ([i 0])
         (cond
           [(> (+ i (string-length substr)) (string-length str)) #f]
           [(equal? (substring str i (+ i (string-length substr))) substr) #t]
           [else (loop (+ i 1))]))))

;; Helper: string-replace-all
(define (string-replace-all str old new)
  (define old-len (string-length old))
  (define (replace-at-pos s pos)
    (string-append
      (substring s 0 pos)
      new
      (substring s (+ pos old-len) (string-length s))))
  (let loop ([s str] [pos 0])
    (if (>= pos (string-length s))
        s
        (if (and (<= (+ pos old-len) (string-length s))
                 (equal? (substring s pos (+ pos old-len)) old))
            (loop (replace-at-pos s pos) (+ pos (string-length new)))
            (loop s (+ pos 1))))))

;; Register keybindings for notebook files
;; 'g' menu for goto/execution, 'space' menu for picker
;; Note: Steel commands must be prefixed with ':' in keymaps
(define notebook-keymap
  (helix.keymaps.helix-string->keymap
    "{
      \"normal\": {
        \"]\": {
          \"l\": \":next-cell\"
        },
        \"[\": {
          \"l\": \":previous-cell\"
        },
        \"g\": {
          \"n\": {
            \"r\": \":execute-cell\"
          }
        },
        \"space\": {
          \"n\": {
            \"j\": \":cell-picker\"
          }
        }
      }
    }"))

(helix.keymaps.#%add-extension-or-labeled-keymap "ipynb" notebook-keymap)

;; ====== Cell Picker Component ======

(struct CellPickerState (cells selected) #:mutable)

;; Get all cells in the current document
(define (get-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  ;; Find all cell markers
  (define (find-cells line-idx acc)
    (if (>= line-idx total-lines)
        (reverse acc)
        (let ([line (get-line line-idx)])
          (cond
            [(string-contains? line "# ─── Code Cell")
             (find-cells (+ line-idx 1) (cons (list line-idx "Code" line) acc))]
            [(string-contains? line "# ─── Markdown Cell")
             (find-cells (+ line-idx 1) (cons (list line-idx "Markdown" line) acc))]
            [else (find-cells (+ line-idx 1) acc)]))))

  (find-cells 0 '()))

;; Get cell content preview (first N lines after the header)
(define (get-cell-preview line-num max-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  ;; Skip the header line and collect content
  (let loop ([idx (+ line-num 1)] [collected 0] [lines '()])
    (if (or (>= idx total-lines) (>= collected max-lines))
        (reverse lines)
        (let ([line (get-line idx)])
          (if (or (string-contains? line "# ─── Code Cell")
                  (string-contains? line "# ─── Markdown Cell")
                  (string-contains? line "# ─── Output"))
              (reverse lines)
              (loop (+ idx 1) (+ collected 1) (cons line lines)))))))

;; Render the cell picker
(define (render-cell-picker state rect buf)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [rect-width (area-width rect)]
         [rect-height (area-height rect)]
         [total-width (min 100 (- rect-width 4))]
         [list-width 35]
         [preview-width (- total-width list-width 2)]
         [height (min (+ (max (length cells) 5) 2) (- rect-height 4))]
         [x (ceiling (max 0 (- (ceiling (/ rect-width 2)) (floor (/ total-width 2)))))]
         [y (ceiling (max 0 (- (ceiling (/ rect-height 2)) (floor (/ height 2)))))]
         [list-area (area x y list-width height)]
         [preview-area (area (+ x list-width 2) y preview-width height)]
         [popup-style (theme-scope "ui.popup")]
         [active-style (theme-scope "ui.text.focus")]
         [number-style (theme-scope "markup.list")]
         [preview-style (theme-scope "ui.text")])

    ;; Render list panel
    (buffer/clear buf list-area)
    (block/render buf list-area
      (make-block popup-style (style) "all" "plain"))

    ;; Title
    (frame-set-string! buf (+ x 2) y "Jump to Cell" active-style)

    ;; Render cells
    (let loop ([i 0])
      (when (< i (length cells))
        (let* ([cell (list-ref cells i)]
               [line-num (list-ref cell 0)]
               [cell-type (list-ref cell 1)]
               [current-style (if (= i selected) active-style popup-style)])
          (frame-set-string! buf (+ x 2) (+ y i 1)
            (string-append (number->string (+ i 1)) ". " cell-type " [" (number->string line-num) "]")
            current-style)
          (loop (+ i 1)))))

    ;; Render preview panel
    (buffer/clear buf preview-area)
    (block/render buf preview-area
      (make-block popup-style (style) "all" "plain"))

    ;; Preview title
    (frame-set-string! buf (+ x list-width 4) y "Preview" active-style)

    ;; Preview content
    (when (and (>= selected 0) (< selected (length cells)))
      (let* ([cell (list-ref cells selected)]
             [line-num (list-ref cell 0)]
             [preview-lines (get-cell-preview line-num (- height 3))]
             [max-preview-width (- preview-width 4)])  ;; Leave padding
        (let loop ([i 0])
          (when (< i (length preview-lines))
            (let* ([line (list-ref preview-lines i)]
                   ;; Truncate line if too long
                   [truncated (if (> (string-length line) max-preview-width)
                                  (string-append (substring line 0 (- max-preview-width 3)) "...")
                                  line)])
              (frame-set-string! buf (+ x list-width 4) (+ y i 1) truncated preview-style)
              (loop (+ i 1)))))))))

;; Handle events
(define (handle-cell-picker-event state event)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [char (key-event-char event)])
    (cond
      ;; ESC or q to close
      [(or (key-event-escape? event) (eqv? char #\q))
       event-result/close]

      ;; j - move down
      [(eqv? char #\j)
       (when (< selected (- (length cells) 1))
         (set-CellPickerState-selected! state (+ selected 1)))
       event-result/consume]

      ;; k - move up
      [(eqv? char #\k)
       (when (> selected 0)
         (set-CellPickerState-selected! state (- selected 1)))
       event-result/consume]

      ;; Enter - jump to selected cell
      [(key-event-enter? event)
       (when (< selected (length cells))
         (let* ([cell (list-ref cells selected)]
                [line-num (list-ref cell 0)])
           (helix.goto (number->string line-num))))
       event-result/close]

      ;; Number key - jump directly
      [else
       (let ([num (char->number (or char #\null))])
         (if (and (not (eqv? num #false))
                  (>= num 1)
                  (<= num (length cells)))
             (begin
               (let* ([cell (list-ref cells (- num 1))]
                      [line-num (list-ref cell 0)])
                 (helix.goto (number->string line-num)))
               event-result/close)
             event-result/consume))])))

(define (make-cell-picker-component)
  (new-component!
    "cell-picker"
    (CellPickerState (get-all-cells) 0)
    render-cell-picker
    (hash "handle_event" handle-cell-picker-event)))

;;@doc Open cell picker to jump to any cell
(define (cell-picker)
  (push-component! (make-cell-picker-component)))

