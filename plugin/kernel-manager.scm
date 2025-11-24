;; Kernel Manager - Persistent REPL processes for notebooks
;; Simpler approach: Use Julia's -L flag to load a runner script
(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))

(provide kernel-start
         kernel-execute
         kernel-shutdown
         kernel-get-for-notebook)

;; Global state: notebook-path -> kernel-state
(define *kernels* (hash))

;; Kernel state structure:
;; { lang: "julia"
;;   kernel-dir: "/tmp/kernel-123/"
;;   pid-file: "/tmp/kernel-123/pid"
;;   input-file: "/tmp/kernel-123/input.jl"
;;   output-file: "/tmp/kernel-123/output.txt"
;;   counter: 0 }

;; Start a new kernel process
(define (kernel-start lang notebook-path)
  (define kernel-dir (string-append "/tmp/helix-kernel-" (number->string (get-kernel-id))))
  (define input-file (string-append kernel-dir "/input.jl"))
  (define output-file (string-append kernel-dir "/output.txt"))
  (define pid-file (string-append kernel-dir "/pid"))
  (define runner-file (string-append kernel-dir "/runner.jl"))

  ;; Create kernel directory
  (helix.run-shell-command (string-append "mkdir -p " kernel-dir))

  ;; Create Julia runner script that watches for input (REPL-style)
  (define runner-script
    (string-append
      "# Helix Kernel Runner - REPL-style\n"
      "const input_file = \"" input-file "\"\n"
      "const output_file = \"" output-file "\"\n"
      "const marker = \"__COMPLETE__\"\n\n"
      "println(stderr, \"Kernel ready (REPL mode)\")\n\n"
      "while true\n"
      "  if isfile(input_file)\n"
      "    code = read(input_file, String)\n"
      "    rm(input_file)\n"
      "    \n"
      "    # Clear output and remove completion flag\n"
      "    write(output_file, \"\")\n"
      "    isfile(output_file * \".done\") && rm(output_file * \".done\")\n"
      "    \n"
      "    # Execute and capture output like REPL\n"
      "    open(output_file, \"w\") do io\n"
      "      # Redirect both stdout and stderr\n"
      "      redirect_stdout(io) do\n"
      "        redirect_stderr(io) do\n"
      "          try\n"
      "            # Parse and evaluate code\n"
      "            result = include_string(Main, code)\n"
      "            \n"
      "            # Print result if not nothing (REPL behavior)\n"
      "            if result !== nothing\n"
      "              println(io, result)\n"
      "            end\n"
      "          catch e\n"
      "            showerror(io, e, catch_backtrace())\n"
      "          end\n"
      "        end\n"
      "      end\n"
      "    end\n"
      "    \n"
      "    # Append completion marker (with newline separator)\n"
      "    open(output_file, \"a\") do io\n"
      "      println(io)  # Ensure newline before marker\n"
      "      println(io, marker)\n"
      "    end\n"
      "    \n"
      "    # Create completion flag file for event-based notification\n"
      "    touch(output_file * \".done\")\n"
      "  end\n"
      "  sleep(0.1)\n"
      "end\n"))

  ;; Write runner script
  (helix.run-shell-command (string-append "cat > " runner-file " <<'EOF'\n" runner-script "\nEOF"))

  ;; Start Julia with runner script in background
  (define start-cmd
    (string-append
      "(julia " runner-file " 2>&1 | tee -a " kernel-dir "/kernel.log) &"
      " echo $! > " pid-file))
  (helix.run-shell-command start-cmd)

  ;; Store kernel state
  (define kernel-state
    (hash
      'lang lang
      'kernel-dir kernel-dir
      'input-file input-file
      'output-file output-file
      'pid-file pid-file
      'counter 0
      'ready #t))

  (set! *kernels* (hash-insert *kernels* notebook-path kernel-state))
  (set-status! (string-append "Started " lang " kernel"))
  kernel-state)

;; Get kernel command for language
(define (kernel-command lang)
  (cond
    [(equal? lang "julia") "julia -i --banner=no --color=no"]
    [(equal? lang "python") "python -i -u"]
    [else (error (string-append "Unsupported language: " lang))]))

;; Execute code in kernel
(define (kernel-execute kernel-state code)
  (define stdin-fifo (hash-get kernel-state 'stdin-fifo))
  (define stdout-file (hash-get kernel-state 'stdout-file))

  ;; Clear previous output
  (helix.run-shell-command (string-append "echo -n '' > " stdout-file))

  ;; Create unique end marker
  (define end-marker "__HELIX_EXEC_COMPLETE__")

  ;; Build command to send to kernel
  ;; For Julia: wrap in try/catch, print marker at end
  (define wrapped-code
    (string-append
      "try\n"
      code "\n"
      "catch e\n"
      "  println(stderr, e)\n"
      "  Base.show_backtrace(stderr, catch_backtrace())\n"
      "end\n"
      "println(\"" end-marker "\")\n"))

  ;; Escape single quotes for shell
  (define escaped-code (string-replace-all (string-replace-all wrapped-code "\\" "\\\\") "'" "'\\''"))

  ;; Send code to kernel via fifo
  (define send-cmd
    (string-append "printf '%s' '" escaped-code "' > " stdin-fifo " &"))

  (helix.run-shell-command send-cmd)

  ;; Wait for completion marker (polling)
  ;; TODO: Make this async
  (let poll-output ()
    (define check-cmd
      (string-append "tail -n 1 " stdout-file " | grep -q '" end-marker "' && echo 'done' || echo 'waiting'"))

    ;; This is synchronous for now - need async polling
    (sleep-ms 100)
    ;; TODO: Check if done, if not recurse
    )

  ;; Read output (excluding marker line)
  (define output-cmd
    (string-append "grep -v '" end-marker "' " stdout-file))

  ;; Return output file path for now
  stdout-file)

;; Get or create kernel for notebook
(define (kernel-get-for-notebook notebook-path lang)
  (define existing (hash-try-get *kernels* notebook-path))
  (if existing
      existing
      (kernel-start lang notebook-path)))

;; Shutdown kernel
(define (kernel-shutdown kernel-state)
  (define pid-file (hash-get kernel-state 'pid-file))
  (define stdin-fifo (hash-get kernel-state 'stdin-fifo))
  (define stdout-file (hash-get kernel-state 'stdout-file))

  ;; Read PID and kill process
  (define kill-cmd
    (string-append
      "PID=$(cat " pid-file ")"
      " && kill $PID"
      " && rm -f " stdin-fifo " " stdout-file " " pid-file))

  (helix.run-shell-command kill-cmd)
  (set-status! "Kernel shutdown"))

;; Helper: sleep milliseconds (if available)
(define (sleep-ms ms)
  ;; TODO: Check if Steel has time/sleep-ms
  ;; For now, use shell
  (helix.run-shell-command (string-append "sleep " (number->string (/ ms 1000.0)))))

;; Helper: get unique ID using simple counter
;; Steel's current-milliseconds may require module import
;; Using simple increment instead
(define *kernel-id-counter* 1)
(define (get-kernel-id)
  (define id *kernel-id-counter*)
  (set! *kernel-id-counter* (+ *kernel-id-counter* 1))
  id)

;; Helper: string-replace-all (from helix.scm)
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
