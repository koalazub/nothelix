;;; nothelix.scm - Jupyter notebooks for Helix
;;;
;;; Commands:
;;;   :convert-notebook - Convert .ipynb to cell format (Rust, non-blocking)
;;;   :execute-cell     - Run current cell
;;;   :next-cell        - Jump to next cell
;;;   :previous-cell    - Jump to previous cell
;;;   :cell-picker      - Interactive cell navigation

(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/keymaps.scm")
(require "helix/configuration.scm")
(require "helix/ext.scm")
(require (prefix-in helix.static. "helix/static.scm"))
(require-builtin helix/core/text as text.)
(require-builtin helix/core/keymaps as helix.keymaps.)
(require (prefix-in helix. "helix/commands.scm"))
(require "helix/components.scm")

;; Native Rust library for fast notebook parsing + image support
(#%require-dylib "libnothelix"
                 (only-in notebook-convert-sync
                          convert-to-ipynb
                          notebook-cell-count
                          notebook-parse-cell
                          notebook-list-cells
                          notebook-get-cell-code
                          get-cell-at-line
                          notebook-is-valid
                          notebook-validate
                          notebook-cell-image-data
                          ;; Protocol detection and config
                          detect-graphics-protocol
                          config-get-protocol
                          config-load
                          protocol-capabilities
                          protocol-supports-format
                          ;; Image rendering (main API - all encoding in Rust)
                          render-image-for-protocol
                          render-b64-for-protocol
                          ;; Image format utilities
                          image-detect-format
                          image-detect-format-bytes
                          image-convert-to-png
                          file->base64
                          ;; Kernel utilities
                          find-julia-executable
                          kernel-execute-code
                          kernel-execute-start
                          kernel-execution-status
                          kernel-interrupt
                          read-kernel-output
                          check-kernel-running))

(provide convert-notebook sync-to-ipynb execute-cell execute-all-cells execute-cells-above
         cancel-cell  ;; Interrupt running execution
         next-cell previous-cell cell-picker
         select-cell select-cell-code select-output
         render-image render-cell-image
         ;; Protocol API
         graphics-protocol graphics-check nothelix-status)

;;; ============================================================================
;;; STRING UTILITIES
;;; ============================================================================

;; Convert string to byte vector (for terminal escape sequences)
(define (string->bytes str)
  (if (not (string? str))
      (list->vector '())
      (list->vector (map char->integer (string->list str)))))

;; Simple JSON value extractor (for our specific use case)
;; Extracts value for a key from JSON string
(define (json-get-string json-str key)
  ;; Find "key": "value" or "key": value pattern
  (define pattern (string-append "\"" key "\":"))
  (define key-pos (string-find json-str pattern 0))
  (if (not key-pos)
      #f
      (let* ([value-start (+ key-pos (string-length pattern))]
             [rest (substring json-str value-start (string-length json-str))]
             [trimmed (string-trim-left rest)])
        (cond
          ;; String value
          [(string-starts-with? trimmed "\"")
           (define end-quote (string-find trimmed "\"" 1))
           (if end-quote
               (substring trimmed 1 end-quote)
               "")]
          ;; Boolean/null
          [(string-starts-with? trimmed "true") "true"]
          [(string-starts-with? trimmed "false") "false"]
          [(string-starts-with? trimmed "null") ""]
          ;; Everything else, read until comma or brace
          [else
           (define end-pos (or (string-find trimmed "," 0)
                              (string-find trimmed "}" 0)
                              (string-length trimmed)))
           (string-trim (substring trimmed 0 end-pos))]))))

(define (string-find str substr start)
  ;; Find position of substr in str starting from start
  (let loop ([pos start])
    (cond
      [(>= pos (string-length str)) #f]
      [(>= (+ pos (string-length substr)) (string-length str)) #f]
      [(equal? (substring str pos (+ pos (string-length substr))) substr) pos]
      [else (loop (+ pos 1))])))

(define (string-trim-left s)
  (if (not (string? s))
      ""
      (let loop ([i 0])
        (cond
          [(>= i (string-length s)) ""]
          [(char-whitespace? (string-ref s i)) (loop (+ i 1))]
          [else (substring s i (string-length s))]))))

;;; ============================================================================
;;; STRING UTILITIES
;;; ============================================================================

(define (string-trim str)
  ;; Remove leading and trailing whitespace
  ;; Handle void/non-string inputs gracefully
  (cond
    [(not str) ""]
    [(void? str) ""]
    [(not (string? str)) ""]
    [else
     (define (trim-start s)
       (let loop ([i 0])
         (cond
           [(>= i (string-length s)) ""]
           [(char-whitespace? (string-ref s i)) (loop (+ i 1))]
           [else (substring s i (string-length s))])))
     (define (trim-end s)
       (let loop ([i (- (string-length s) 1)])
         (cond
           [(< i 0) ""]
           [(char-whitespace? (string-ref s i)) (loop (- i 1))]
           [else (substring s 0 (+ i 1))])))
     (trim-end (trim-start str))]))

(define (string-suffix? str suffix)
  ;; Check if str ends with suffix. Returns #f for void/non-string inputs.
  (and (string? str)
       (string? suffix)
       (let [(str-len (string-length str))
             (suf-len (string-length suffix))]
         (and (>= str-len suf-len)
              (equal? (substring str (- str-len suf-len) str-len) suffix)))))

(define (string-starts-with? str prefix)
  ;; Check if str starts with prefix. Returns #f for void/non-string inputs.
  (and (string? str)
       (string? prefix)
       (>= (string-length str) (string-length prefix))
       (equal? (substring str 0 (string-length prefix)) prefix)))

(define (string-contains? str substr)
  ;; Check if str contains substr. Returns #f for void/non-string inputs.
  (and (string? str)
       (string? substr)
       (>= (string-length str) (string-length substr))
       (let loop ([i 0])
         (cond
           [(> (+ i (string-length substr)) (string-length str)) #f]
           [(equal? (substring str i (+ i (string-length substr))) substr) #t]
           [else (loop (+ i 1))]))))

(define (string-join strings sep)
  (if (null? strings)
      ""
      (let loop ([rest (cdr strings)] [result (car strings)])
        (if (null? rest)
            result
            (loop (cdr rest) (string-append result sep (car rest)))))))

;;; ============================================================================
;;; CURSOR HELPERS
;;; ============================================================================

;; Get current line number (0-indexed)
(define (current-line-number)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (text.rope-char->line rope pos))

;;; ============================================================================
;;; GRAPHICS PROTOCOL SYSTEM
;;; ============================================================================

;; Current active protocol (cached after first detection)
(define *active-protocol* #f)
(define *protocol-checked* #f)

;; Image ID counter for unique IDs
(define *image-id-counter* 1)

(define (next-image-id)
  (define id *image-id-counter*)
  (set! *image-id-counter* (+ *image-id-counter* 1))
  id)

;; Check if add-raw-content! API is available in this helix build
;; feature/inline-image-rendering branch has this binding
;; Default to #t - the API should be available in the target helix build
;; If rendering fails, we'll get an error that helps diagnose
(define *raw-content-available* #t)

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Protocol Detection and Configuration
;;; ─────────────────────────────────────────────────────────────────────────────

;; Get the active graphics protocol
;; Checks config first (for user override), then auto-detects
;; Returns: "kitty", "iterm2", "sixel", or "none"
(define (graphics-protocol)
  (when (not *protocol-checked*)
    (define config-protocol (config-get-protocol))
    (set! *active-protocol*
          (if (equal? config-protocol "auto")
              (detect-graphics-protocol)
              config-protocol))
    (set! *protocol-checked* #t))
  *active-protocol*)

;; Check graphics capability and report to user
;; Returns: #t if graphics available, #f otherwise
(define (graphics-check)
  (define protocol (graphics-protocol))
  (define msg
    (cond
      [(equal? protocol "kitty")
       "Graphics: Kitty protocol (full colour, efficient caching)"]
      [(equal? protocol "iterm2")
       "Graphics: iTerm2 protocol (inline images)"]
      [(equal? protocol "sixel")
       "Graphics: Sixel protocol (limited colour support)"]
      [else
       "Graphics: None (text placeholders only)"]))
  (set-status! msg)
  (not (equal? protocol "none")))

;; Show full nothelix status
(define (nothelix-status)
  (define protocol (graphics-protocol))
  (define config-protocol (config-get-protocol))
  (define raw-api (if *raw-content-available* "available" "not available"))
  (set-status!
    (string-append
      "Nothelix | Protocol: " protocol
      " (config: " config-protocol ")"
      " | RawContent API: " raw-api)))

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Image Rendering (Rust-backed, format-aware)
;;; ─────────────────────────────────────────────────────────────────────────────

;; All escape sequence generation is handled in Rust.
;; Steel is purely orchestration - no terminal escape codes here.

;; Render an image file for the current protocol
;; Returns: escape sequence string, or error message
(define (render-image path rows char-idx)
  (define protocol (graphics-protocol))
  (define id (next-image-id))

  (cond
    [(equal? protocol "none")
     (set-status! "No graphics protocol available")
     #f]
    [else
     ;; Rust handles: format detection, conversion, escape sequence generation
     (define escape-seq (render-image-for-protocol path protocol id))
     (cond
       [(string-starts-with? escape-seq "ERROR:")
        (set-status! escape-seq)
        #f]
       [(not *raw-content-available*)
        ;; RawContent API not yet available - show info
        (define format (image-detect-format path))
        (set-status!
          (string-append "[" protocol "/" format " image ready] (RawContent API pending)"))
        #f]
       [else
        ;; Full rendering path (when API available)
        ;; (add-raw-content! escape-seq rows char-idx)
        rows])]))

;; Render base64 image data for the current protocol
(define (render-image-b64 b64-data rows char-idx)
  (define protocol (graphics-protocol))
  (define id (next-image-id))

  (cond
    [(equal? protocol "none")
     (set-status! "No graphics protocol available")
     #f]
    [else
     ;; Rust handles everything
     (define escape-seq (render-b64-for-protocol b64-data protocol id))
     (cond
       [(string-starts-with? escape-seq "ERROR:")
        (set-status! escape-seq)
        #f]
       [(not *raw-content-available*)
        (define format (image-detect-format-bytes b64-data))
        (set-status!
          (string-append "[" protocol "/" format " image: "
                         (number->string (quotient (string-length b64-data) 1024))
                         "KB] (RawContent API pending)"))
        #f]
       [else
        ;; Convert string escape sequence to bytes and add to document
        (define payload (string->bytes escape-seq))
        (add-raw-content! payload rows char-idx)
        rows])]))

;; Render image output from a notebook cell
(define (render-cell-image notebook-path cell-index char-idx rows)
  (define b64-data (notebook-cell-image-data notebook-path cell-index))
  (cond
    [(equal? b64-data "") #f]  ;; No image in cell
    [(string-starts-with? b64-data "ERROR:")
     (set-status! b64-data)
     #f]
    [else
     (render-image-b64 b64-data rows char-idx)]))

;;; ============================================================================
;;; KERNEL MANAGER
;;; ============================================================================

(define *kernels* (hash))
(define *kernel-id-counter* 1)

;; Track currently executing kernel dir for cancellation
(define *executing-kernel-dir* #f)
(define *executing-running-line* #f)  ;; Track line number of "Running..." indicator

(define (get-kernel-id)
  (define id *kernel-id-counter*)
  (set! *kernel-id-counter* (+ *kernel-id-counter* 1))
  id)

(define (kernel-start lang notebook-path)
  ;; Find Julia executable using Rust FFI
  (define julia-path (find-julia-executable))

  ;; Check if Julia was found (error messages start with "ERROR:")
  (when (string-starts-with? julia-path "ERROR:")
    (set-status! julia-path)
    (error julia-path))

  (define kernel-dir (string-append "/tmp/helix-kernel-" (number->string (get-kernel-id))))
  (define input-file (string-append kernel-dir "/input.jl"))
  (define output-file (string-append kernel-dir "/output.txt"))
  (define pid-file (string-append kernel-dir "/pid"))
  (define runner-file (string-append kernel-dir "/runner.jl"))

  (helix.run-shell-command (string-append "mkdir -p " kernel-dir))

  (define runner-script
    (string-append
      "const input_file = \"" input-file "\"\n"
      "const output_file = \"" output-file "\"\n"
      "const image_file = \"" kernel-dir "/plot.png\"\n"
      "const image_b64_file = \"" kernel-dir "/plot.b64\"\n"
      "const marker = \"__COMPLETE__\"\n"
      "const image_marker = \"__IMAGE__\"\n"
      "const exec_id_file = \"" kernel-dir "/exec_id.txt\"\n\n"
      "using Base64  # Always available in Julia stdlib\n\n"
      "# Dynamically detect if result is a displayable plot/figure\n"
      "function is_plot_result(x)\n"
      "    x === nothing && return false\n"
      "    t = string(typeof(x))\n"
      "    # Check for common plotting types across different libraries\n"
      "    return any(pattern -> occursin(pattern, t), [\n"
      "        \"Plot\",      # Plots.jl\n"
      "        \"Figure\",    # Makie.jl, PyPlot, Gadfly\n"
      "        \"Scene\",     # Makie.jl\n"
      "        \"Chart\",     # VegaLite.jl\n"
      "        \"Canvas\",    # UnicodePlots, Luxor\n"
      "        \"Drawing\",   # Luxor.jl\n"
      "    ])\n"
      "end\n\n"
      "# Dynamically save plot using available display methods\n"
      "function save_plot_b64(p)\n"
      "    try\n"
      "        saved = false\n"
      "        \n"
      "        # Try Plots.jl savefig (if available)\n"
      "        if isdefined(Main, :Plots) && applicable(Main.Plots.savefig, p, image_file)\n"
      "            Main.Plots.savefig(p, image_file)\n"
      "            saved = true\n"
      "        # Try Makie save (if available)\n"
      "        elseif isdefined(Main, :Makie) && applicable(Main.Makie.save, image_file, p)\n"
      "            Main.Makie.save(image_file, p)\n"
      "            saved = true\n"
      "        # Try FileIO save (generic, if available)\n"
      "        elseif isdefined(Main, :FileIO) && applicable(Main.FileIO.save, image_file, p)\n"
      "            Main.FileIO.save(image_file, p)\n"
      "            saved = true\n"
      "        # Try PyPlot savefig (if available)\n"
      "        elseif isdefined(Main, :PyPlot) && applicable(Main.PyPlot.savefig, image_file)\n"
      "            Main.PyPlot.savefig(image_file)\n"
      "            saved = true\n"
      "        # Try display system (fallback for VegaLite, etc.)\n"
      "        elseif applicable(display, p)\n"
      "            # Use display system to write to file\n"
      "            open(image_file, \"w\") do io\n"
      "                show(io, MIME(\"image/png\"), p)\n"
      "            end\n"
      "            saved = true\n"
      "        end\n"
      "        \n"
      "        if saved && isfile(image_file)\n"
      "            data = read(image_file)\n"
      "            b64 = base64encode(data)\n"
      "            write(image_b64_file, b64)\n"
      "            return true\n"
      "        else\n"
      "            println(stderr, \"Could not save plot: no applicable save method found\")\n"
      "            return false\n"
      "        end\n"
      "    catch e\n"
      "        println(stderr, \"Plot save error: \", e)\n"
      "        return false\n"
      "    end\n"
      "end\n\n"
      "println(stderr, \"Kernel ready (dynamic plot capture)\")\n\n"
      "while true\n"
      "  if isfile(input_file)\n"
      "    code = read(input_file, String)\n"
      "    rm(input_file)\n"
      "    write(output_file, \"\")\n"
      "    isfile(output_file * \".done\") && rm(output_file * \".done\")\n"
      "    isfile(image_b64_file) && rm(image_b64_file)\n"
      "    local result = nothing\n"
      "    local has_image = false\n"
      "    local execution_error = false\n"
      "    open(output_file, \"w\") do io\n"
      "      redirect_stdout(io) do\n"
      "        redirect_stderr(io) do\n"
      "          try\n"
      "            result = include_string(Main, code)\n"
      "            # Only print non-plot results\n"
      "            if result !== nothing && !is_plot_result(result)\n"
      "              println(io, result)\n"
      "            end\n"
      "          catch e\n"
      "            execution_error = true\n"
      "            showerror(io, e, catch_backtrace())\n"
      "          end\n"
      "        end\n"
      "      end\n"
      "    end\n"
      "    \n"
      "    # Try to capture plot even if result isn't a plot object\n"
      "    # (handles plot!(), xlabel!(), etc. which modify current plot)\n"
      "    if !execution_error\n"
      "      local plot_to_save = nothing\n"
      "      \n"
      "      # 1. Check if result itself is a plot\n"
      "      if result !== nothing && is_plot_result(result)\n"
      "        plot_to_save = result\n"
      "      # 2. Check if Plots.jl has a current plot\n"
      "      elseif isdefined(Main, :Plots)\n"
      "        try\n"
      "          current = Main.Plots.current()\n"
      "          if current !== nothing && is_plot_result(current)\n"
      "            plot_to_save = current\n"
      "          end\n"
      "        catch\n"
      "          # No current plot\n"
      "        end\n"
      "      # 3. Check for Makie current figure\n"
      "      elseif isdefined(Main, :Makie)\n"
      "        try\n"
      "          fig = Main.Makie.current_figure()\n"
      "          if fig !== nothing\n"
      "            plot_to_save = fig\n"
      "          end\n"
      "        catch\n"
      "          # No current figure\n"
      "        end\n"
      "      end\n"
      "      \n"
      "      if plot_to_save !== nothing\n"
      "        has_image = save_plot_b64(plot_to_save)\n"
      "      end\n"
      "    end\n"
      "    open(output_file, \"a\") do io\n"
      "      if has_image\n"
      "        println(io, image_marker)\n"
      "      end\n"
      "      println(io)\n"
      "      println(io, marker)\n"
      "    end\n"
      "    touch(output_file * \".done\")\n"
      "  end\n"
      "  sleep(0.1)\n"
      "end\n"))

  (helix.run-shell-command (string-append "cat > " runner-file " <<'EOF'\n" runner-script "\nEOF"))

  (define start-cmd
    (string-append "(" julia-path " " runner-file " 2>&1 | tee -a " kernel-dir "/kernel.log) & echo $! > " pid-file))
  (helix.run-shell-command start-cmd)

  ;; Give Julia a moment to start
  (helix.run-shell-command "sleep 0.5")

  ;; Verify kernel started by checking for PID file and kernel.log
  (define pid-check
    (string-trim (helix.run-shell-command (string-append "[ -f " pid-file " ] && echo 'yes' || echo 'no'"))))

  (when (equal? pid-check "no")
    (define log-contents
      (string-trim (helix.run-shell-command (string-append "tail -5 " kernel-dir "/kernel.log 2>&1 || echo 'No log file'"))))
    (set-status! (string-append "Kernel failed to start. Log: " log-contents))
    (error "Kernel startup failed"))

  (define kernel-state
    (hash 'lang lang
          'kernel-dir kernel-dir
          'input-file input-file
          'output-file output-file
          'pid-file pid-file
          'julia-path julia-path
          'ready #t))

  (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
  (set-status! (string-append "✓ Started " lang " kernel (" julia-path ")"))
  kernel-state)

(define (kernel-get-for-notebook notebook-path lang)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing existing (kernel-start lang notebook-path)))

;;; ============================================================================
;;; HELPERS
;;; ============================================================================

(define (string-replace-all str old new)
  (define old-len (string-length old))
  (define (replace-at-pos s pos)
    (string-append (substring s 0 pos) new (substring s (+ pos old-len) (string-length s))))
  (let loop ([s str] [pos 0])
    (if (>= pos (string-length s))
        s
        (if (and (<= (+ pos old-len) (string-length s))
                 (equal? (substring s pos (+ pos old-len)) old))
            (loop (replace-at-pos s pos) (+ pos (string-length new)))
            (loop s (+ pos 1))))))

(define (char->number c)
  (cond
    [(eqv? c #\0) 0]
    [(eqv? c #\1) 1]
    [(eqv? c #\2) 2]
    [(eqv? c #\3) 3]
    [(eqv? c #\4) 4]
    [(eqv? c #\5) 5]
    [(eqv? c #\6) 6]
    [(eqv? c #\7) 7]
    [(eqv? c #\8) 8]
    [(eqv? c #\9) 9]
    [else #f]))

;;; ============================================================================
;;; NOTEBOOK CONVERSION (Rust FFI)
;;; ============================================================================

(define (replace-document-contents! content)
  (helix.static.select_all)
  (helix.static.delete_selection)
  (helix.static.insert_string content)
  (helix.static.goto_file_start))

;;@doc
;; Convert current .ipynb to readable cell format (fast Rust parsing)
(define (convert-notebook)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]

    [(not (string-suffix? path ".ipynb"))
     (set-status! "Error: Not a .ipynb file")]

    [else
     ;; Use detailed validation with error message
     (define validation-error (notebook-validate path))
     (if (not (equal? validation-error ""))
         ;; Show detailed error
         (set-status! (string-append "Invalid notebook: " validation-error))
         ;; Valid, proceed with conversion
         (begin
           (set-status! "Converting...")
           (spawn-native-thread
             (lambda ()
               (define result (notebook-convert-sync path))
               (define cell-count (notebook-cell-count path))
               (hx.with-context
                 (lambda ()
                   (if (string-starts-with? result "ERROR:")
                       (set-status! result)
                       (begin
                         ;; Generate output path: notebook.ipynb -> notebook.jl
                         (define output-path
                           (string-append
                             (substring path 0 (- (string-length path) 6))  ; Remove ".ipynb"
                             ".jl"))
                         ;; Write converted content to .jl file
                         (helix.run-shell-command
                           (string-append "cat > " output-path " <<'NOTHELIXEOF'\n"
                                         result
                                         "\nNOTHELIXEOF"))
                         (set-status! (string-append "Converted to " output-path ": "
                                                    (number->string cell-count)
                                                    " cells. Run :open " output-path))))))))))]))

;;@doc
;; Sync changes from .jl file back to .ipynb file
;; Updates cell sources in the original .ipynb from the edited .jl file
(define (sync-to-ipynb)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))

  (cond
    [(not path)
     (set-status! "Error: No file path")]

    [(not (string-suffix? path ".jl"))
     (set-status! "Error: Not a .jl file. Only converted notebooks can be synced back.")]

    [else
     (set-status! "Syncing to .ipynb...")
     (define result (convert-to-ipynb path))
     (if (string-starts-with? result "ERROR:")
         (set-status! result)
         (set-status! "✓ Synced changes back to .ipynb"))]))

;;; ============================================================================
;;; CELL NAVIGATION
;;; ============================================================================

;;@doc
;; Jump to next cell in the notebook
(define (next-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-starts-with? line "@cell ")
          (string-starts-with? line "@markdown "))))

  (define (find-next-cell line-idx)
    (cond
      [(>= line-idx total-lines) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-next-cell (+ line-idx 1))]))

  (define next-cell-line (find-next-cell (+ current-line 1)))

  (if next-cell-line
      (begin
        (helix.goto (number->string (+ next-cell-line 1)))
        (set-status! (string-append "Cell at line " (number->string (+ next-cell-line 1)))))
      (set-status! "No next cell")))

;;@doc
;; Jump to previous cell in the notebook
(define (previous-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (is-cell-marker? line-idx)
    (let ([line (get-line line-idx)])
      (or (string-starts-with? line "@cell ")
          (string-starts-with? line "@markdown "))))

  (define (find-prev-cell line-idx)
    (cond
      [(< line-idx 0) #f]
      [(is-cell-marker? line-idx) line-idx]
      [else (find-prev-cell (- line-idx 1))]))

  (define prev-cell-line (find-prev-cell (- current-line 1)))

  (if prev-cell-line
      (begin
        (helix.goto (number->string (+ prev-cell-line 1)))
        (set-status! (string-append "Cell at line " (number->string (+ prev-cell-line 1)))))
      (set-status! "No previous cell")))

;;; ============================================================================
;;; CELL EXECUTION
;;; ============================================================================

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

  ;; Insert output header with "running" indicator
  (helix.static.insert_string "\n\n# ─── Output ───\n# ⚙ Running...\n")

  ;; Track the line number of "Running..." for later deletion
  (set! *executing-running-line* (current-line-number))
  (helix.redraw)

  ;; Get kernel for this notebook
  (define notebook-path (editor-document->path doc-id))
  (define kernel-state (kernel-get-for-notebook notebook-path lang))
  (define kernel-dir (hash-get kernel-state 'kernel-dir))

  ;; Check kernel is running before execution
  (define kernel-status (check-kernel-running kernel-dir))
  (when (string-starts-with? kernel-status "ERROR:")
    ;; Delete the "Running..." line first
    (helix.static.move_line_up)
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (set-status! kernel-status)
    (error kernel-status))

  ;; Start execution asynchronously (non-blocking)
  (set! *executing-kernel-dir* kernel-dir)  ;; Track for cancellation
  (define start-result (kernel-execute-start kernel-dir code))
  (when (string-starts-with? start-result "ERROR:")
    (set! *executing-kernel-dir* #f)  ;; Clear on error
    (helix.static.move_line_up)
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (helix.static.insert_string (string-append "# " start-result "\n"))
    (helix.static.insert_string "# ─────────────\n")
    (set-status! start-result)
    (helix.redraw)
    (error start-result))

  ;; Spawn background thread to poll for completion
  (spawn-native-thread
    (lambda ()
      ;; Poll until execution completes (check every 100ms, timeout 120s)
      (define max-polls 1200)
      (define (poll-loop count)
        (when (< count max-polls)
          (define status-json (kernel-execution-status kernel-dir))
          (define status (json-get-string status-json "status"))
          (cond
            [(equal? status "done")
             ;; Execution complete - update UI
             (hx.with-context
               (lambda ()
                 (execute-cell-finish kernel-dir)))]
            [(equal? status "error")
             (define err-msg (or (json-get-string status-json "message") "Unknown error"))
             (hx.with-context
               (lambda ()
                 (execute-cell-error err-msg)))]
            [else
             ;; Still running - sleep and poll again
             (helix.run-shell-command "sleep 0.1")
             (poll-loop (+ count 1))])))
      (poll-loop 0))))

;; Helper: Complete cell execution (called from background thread via hx.with-context)
(define (execute-cell-finish kernel-dir)
  ;; Clear execution tracking
  (set! *executing-kernel-dir* #f)

  ;; Delete the "Running..." line using tracked line number
  (when *executing-running-line*
    (helix.goto (number->string *executing-running-line*))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (set! *executing-running-line* #f))

  ;; Read output using Rust
  (define output-json (read-kernel-output kernel-dir))

  ;; Check for error
  (define error-msg (json-get-string output-json "error"))
  (when error-msg
    (helix.static.insert_string (string-append "# ERROR: " error-msg "\n"))
    (helix.static.insert_string "# ─────────────\n")
    (set-status! (string-append "Execution failed: " error-msg))
    (helix.redraw)
    (void))

  ;; Extract output data from JSON
  (define output-text (or (json-get-string output-json "text") ""))
  (define has-image (equal? (json-get-string output-json "has_image") "true"))
  (define b64-data (or (json-get-string output-json "image_b64") ""))

  ;; Insert text output
  (when (> (string-length output-text) 0)
    (helix.static.insert_string (string-append output-text "\n")))

  ;; Insert image if present
  (when (and has-image (> (string-length b64-data) 0))
    (define protocol (graphics-protocol))
    (define char-idx (cursor-position))
    (define size-kb (quotient (string-length b64-data) 1024))

    (cond
      ;; If graphics protocol available and RawContent API works
      [(and (not (equal? protocol "none")) *raw-content-available*)
       (helix.static.insert_string
         (string-append "# [Plot: " protocol " | " (number->string size-kb) "KB]\n"))
       (render-image-b64 b64-data 10 char-idx)]

      ;; Graphics protocol available but no RawContent API - save to file
      [(not (equal? protocol "none"))
       (define plot-file (string-append kernel-dir "/plot_output.png"))
       (helix.static.insert_string
         (string-append "# [Plot saved: " plot-file " | " (number->string size-kb) "KB]\n"))
       (helix.static.insert_string
         (string-append "# Protocol: " protocol " (inline rendering pending RawContent API)\n"))]

      ;; No graphics support at all - just note there's an image
      [else
       (define plot-file (string-append kernel-dir "/plot_output.png"))
       (helix.static.insert_string
         (string-append "# [Plot saved: " plot-file " | " (number->string size-kb) "KB]\n"))
       (helix.static.insert_string "# (No terminal graphics support detected)\n")]))

  ;; Insert output footer
  (helix.static.insert_string "# ─────────────\n")

  (helix.redraw)
  (set-status! (if has-image "✓ Cell executed (with plot)" "✓ Cell executed")))

;; Helper: Handle execution error (called from background thread via hx.with-context)
(define (execute-cell-error err-msg)
  ;; Clear execution tracking
  (set! *executing-kernel-dir* #f)

  ;; Delete the "Running..." line using tracked line number
  (when *executing-running-line*
    (helix.goto (number->string *executing-running-line*))
    (helix.static.goto_line_start)
    (helix.static.extend_to_line_bounds)
    (helix.static.delete_selection)
    (set! *executing-running-line* #f))

  (helix.static.insert_string (string-append "# ERROR: " err-msg "\n"))
  (helix.static.insert_string "# ─────────────\n")
  (set-status! (string-append "Execution failed: " err-msg))
  (helix.redraw))

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
  (define err (json-get-string cell-info-json "error"))
  (when err
    (set-status! "Error: Not a converted notebook file")
    (error "Not a converted notebook"))

  (define notebook-path (json-get-string cell-info-json "source_path"))
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
      (define cell-code (json-get-string cell-data-json "code"))
      (define cell-type (json-get-string cell-data-json "type"))

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

            ;; Execute via kernel (Rust API)
            (define exec-result (kernel-execute-code kernel-dir cell-code))
            (when (string-starts-with? exec-result "ERROR:")
              (set-status! exec-result)
              (error exec-result))

            ;; Read output
            (define output-json (read-kernel-output kernel-dir))
            (define error-msg (json-get-string output-json "error"))
            (when error-msg
              (set-status! (string-append "Kernel error: " error-msg))
              (error error-msg))

            (define output-text (or (json-get-string output-json "text") ""))
            (define has-image (equal? (json-get-string output-json "has_image") "true"))

            ;; Insert output section
            (helix.static.insert_string "\n\n# ─── Output ───\n")
            (when (> (string-length output-text) 0)
              (helix.static.insert_string (string-append output-text "\n")))
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
  (define err (json-get-string cell-info-json "error"))
  (when err
    (set-status! "Error: Not in a notebook file")
    (error "Not in a notebook file"))

  (define notebook-path (json-get-string cell-info-json "source_path"))
  (define current-cell-idx (string->number (json-get-string cell-info-json "cell_index")))
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
      (define cell-code (json-get-string cell-data-json "code"))
      (define cell-type (json-get-string cell-data-json "type"))

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

            ;; Execute via kernel (Rust API)
            (define exec-result (kernel-execute-code kernel-dir cell-code))
            (when (string-starts-with? exec-result "ERROR:")
              (set-status! exec-result)
              (error exec-result))

            ;; Read output
            (define output-json (read-kernel-output kernel-dir))
            (define error-msg (json-get-string output-json "error"))
            (when error-msg
              (set-status! (string-append "Kernel error: " error-msg))
              (error error-msg))

            (define output-text (or (json-get-string output-json "text") ""))
            (define has-image (equal? (json-get-string output-json "has_image") "true"))

            ;; Insert output section
            (helix.static.insert_string "\n\n# ─── Output ───\n")
            (when (> (string-length output-text) 0)
              (helix.static.insert_string (string-append output-text "\n")))
            (helix.static.insert_string "# ─────────────\n")

            ;; Show progress after each cell
            (set-status! (string-append "✓ Cell " (number->string (+ cell-idx 1)) "/" (number->string cells-to-execute) " done"))
            (helix.redraw))))

      (loop (+ cell-idx 1) (+ executed 1))))

  ;; Return to original position (approximately - line numbers have changed)
  (helix.goto (number->string (+ original-line 1)))
  (set-status! (string-append "✓ Executed " (number->string cells-to-execute) " cells up to current")))

;;; ============================================================================
;;; CELL/OUTPUT SELECTION (text objects)
;;; ============================================================================

;; Helper: Find the full cell end (including output if present)
(define (find-full-cell-end get-line total-lines code-end)
  (define output-start (find-output-start get-line total-lines code-end (+ code-end 3)))
  (if output-start
      (find-output-end-line get-line total-lines (+ output-start 1))
      code-end))

;; Helper: Select line range (1-indexed lines, end exclusive)
(define (select-line-range start-line end-line)
  (helix.goto (number->string (+ start-line 1)))
  (helix.static.goto_line_start)
  (helix.static.extend_to_line_bounds)
  (let ([lines-to-extend (- end-line start-line 1)])
    (when (> lines-to-extend 0)
      (let loop ([i 0])
        (when (< i lines-to-extend)
          (helix.static.extend_line_below)
          (loop (+ i 1)))))))

;;@doc
;; Select the entire current cell (header + code + output)
(define (select-cell)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define cell-end (find-full-cell-end get-line total-lines cell-code-end))

  (select-line-range cell-start cell-end)
  (set-status! (string-append "Selected cell: lines "
                              (number->string (+ cell-start 1))
                              "-"
                              (number->string cell-end))))

;;@doc
;; Select just the code portion of the current cell (excluding header and output)
(define (select-cell-code)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define code-start (+ cell-start 1))  ;; Skip the header line

  (if (< code-start cell-code-end)
      (begin
        (select-line-range code-start cell-code-end)
        (set-status! (string-append "Selected code: lines "
                                    (number->string (+ code-start 1))
                                    "-"
                                    (number->string cell-code-end))))
      (set-status! "Cell has no code")))

;;@doc
;; Select the output section of the current cell
(define (select-output)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define current-line (current-line-number))
  (define total-lines (text.rope-len-lines rope))
  (define (get-line idx) (doc-get-line rope total-lines idx))

  (define cell-start (find-cell-start-line get-line current-line))
  (define cell-code-end (find-cell-code-end get-line total-lines (+ cell-start 1)))
  (define output-start (find-output-start get-line total-lines cell-code-end (+ cell-code-end 5)))

  (if output-start
      (let ([output-end (find-output-end-line get-line total-lines (+ output-start 1))])
        (select-line-range output-start output-end)
        (set-status! (string-append "Selected output: lines "
                                    (number->string (+ output-start 1))
                                    "-"
                                    (number->string output-end))))
      (set-status! "No output section found")))

;;; ============================================================================
;;; CELL PICKER
;;; ============================================================================

(struct CellPickerState (cells selected) #:mutable)

(define (get-all-cells)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (define (find-cells line-idx acc)
    (if (>= line-idx total-lines)
        (reverse acc)
        (let ([line (get-line line-idx)])
          (cond
            [(string-starts-with? line "@cell ")
             (find-cells (+ line-idx 1) (cons (list line-idx "Code" line) acc))]
            [(string-starts-with? line "@markdown ")
             (find-cells (+ line-idx 1) (cons (list line-idx "Markdown" line) acc))]
            [else (find-cells (+ line-idx 1) acc)]))))

  (find-cells 0 '()))

(define (get-cell-preview line-num max-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (define (get-line line-idx)
    (if (< line-idx total-lines)
        (text.rope->string (text.rope->line rope line-idx))
        ""))

  (let loop ([idx (+ line-num 1)] [collected 0] [lines '()])
    (if (or (>= idx total-lines) (>= collected max-lines))
        (reverse lines)
        (let ([line (get-line idx)])
          (if (or (string-starts-with? line "@cell ")
                  (string-starts-with? line "@markdown ")
                  (string-starts-with? line "# ═══")
                  (string-contains? line "# ─── Output"))
              (reverse lines)
              (loop (+ idx 1) (+ collected 1) (cons line lines)))))))

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
         [preview-style (theme-scope "ui.text")])

    (buffer/clear buf list-area)
    (block/render buf list-area (make-block popup-style (style) "all" "plain"))
    (frame-set-string! buf (+ x 2) y "Jump to Cell" active-style)

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

    (buffer/clear buf preview-area)
    (block/render buf preview-area (make-block popup-style (style) "all" "plain"))
    (frame-set-string! buf (+ x list-width 4) y "Preview" active-style)

    (when (and (>= selected 0) (< selected (length cells)))
      (let* ([cell (list-ref cells selected)]
             [line-num (list-ref cell 0)]
             [preview-lines (get-cell-preview line-num (- height 3))]
             [max-preview-width (- preview-width 4)])
        (let loop ([i 0])
          (when (< i (length preview-lines))
            (let* ([line (list-ref preview-lines i)]
                   [truncated (if (> (string-length line) max-preview-width)
                                  (string-append (substring line 0 (- max-preview-width 3)) "...")
                                  line)])
              (frame-set-string! buf (+ x list-width 4) (+ y i 1) truncated preview-style)
              (loop (+ i 1)))))))))

(define (handle-cell-picker-event state event)
  (let* ([cells (CellPickerState-cells state)]
         [selected (CellPickerState-selected state)]
         [char (key-event-char event)])
    (cond
      [(or (key-event-escape? event) (eqv? char #\q))
       event-result/close]
      [(eqv? char #\j)
       (when (< selected (- (length cells) 1))
         (set-CellPickerState-selected! state (+ selected 1)))
       event-result/consume]
      [(eqv? char #\k)
       (when (> selected 0)
         (set-CellPickerState-selected! state (- selected 1)))
       event-result/consume]
      [(key-event-enter? event)
       (when (< selected (length cells))
         (let* ([cell (list-ref cells selected)]
                [line-num (list-ref cell 0)])
           (helix.goto (number->string line-num))))
       event-result/close]
      [else
       (let ([num (char->number (or char #\null))])
         (if (and (not (eqv? num #false)) (>= num 1) (<= num (length cells)))
             (begin
               (let* ([cell (list-ref cells (- num 1))]
                      [line-num (list-ref cell 0)])
                 (helix.goto (number->string line-num)))
               event-result/close)
             event-result/consume))])))

(define (make-cell-picker-component)
  (new-component! "cell-picker"
    (CellPickerState (get-all-cells) 0)
    render-cell-picker
    (hash "handle_event" handle-cell-picker-event)))

;;@doc
;; Open interactive cell picker
(define (cell-picker)
  (push-component! (make-cell-picker-component)))

;;; ============================================================================
;;; KEYBINDINGS
;;; ============================================================================

(define notebook-bindings
  (keymap
    (normal
      ("]" ("l" ":next-cell"))
      ("[" ("l" ":previous-cell"))
      ("g" ("n" ("r" ":execute-cell")))
      (space ("n" ("j" ":cell-picker")
                  ("c" ":select-cell")
                  ("s" ":select-cell-code")
                  ("o" ":select-output"))))))

(helix.keymaps.#%add-extension-or-labeled-keymap "ipynb"
  (merge-keybindings (get-keybindings) notebook-bindings))
