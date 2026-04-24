;;; conceal.scm - LaTeX math symbol concealment for Helix
;;;
;;; Renders LaTeX commands as their Unicode equivalents without modifying
;;; the buffer. Uses Helix's overlay system to replace displayed characters.
;;;
;;; For example: `$\kappa(A)$` displays as `κ(A)` while the buffer
;;; text remains unchanged.
;;;
;;; Works on any file with inline LaTeX math regions ($...$).

(require "helix/editor.scm")
(require-builtin helix/core/text as text.)
(require "helix/ext.scm")
(require "helix/misc.scm")
(require "string-utils.scm")
(require "conceal-state.scm")
(require "json-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          compute-conceal-overlays-ffi
                          compute-conceal-overlays-for-comments
                          compute-conceal-overlays-for-comments-with-options
                          compute-conceal-overlays-for-typst))

(provide compute-conceal-overlays compute-and-apply-conceal-async
         parse-overlay-json
         file-has-conceal-extension?
         conceal-math!
         clear-conceal!
         apply-conceal-for-cursor!
         schedule-reconceal
         *math-render-active*
         *math-render-refresh-hook*)

;; Flag flipped by the math-render plugin when it stages virtual above/
;; below rows for big operators and `\frac`. When on, the scanner hides
;; the inline form of those groups so both representations don't
;; collide. Lives here (not in math-render.scm) so the concealer can
;; read it at each scan without a circular require.
(define *math-render-active* (box #false))

;; Callback the math-render plugin installs so the conceal cycle can
;; restage its virtual rows against the live buffer without a cyclic
;; require. Default no-op so this module still works standalone.
;; See math-render.scm — it sets this to `math-render-buffer-impl`
;; after defining it.
(define *math-render-refresh-hook* (box (lambda () #f)))

;;; ─── Public API ──────────────────────────────────────────────────────────────

;;;@doc
;;; Compute conceal overlay pairs for the current document.
;;; For .jl files, only scans comment lines (# ...) to avoid false $ matches.
;;; For other files (.md, .tex, etc.), scans the full document.
(define (compute-conceal-overlays)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))
  (define path (editor-document->path doc-id))
  (cond
    [(and path (ends-with-jl? path))
     ;; .jl: per-line comment scanning, returns tab-separated format.
     ;; Pass hide_math_layout when the math-render plugin is active so
     ;; the concealer suppresses inline `_{…}^{…}` / `\frac{…}{…}` that
     ;; math-render is already painting as virtual above/below rows.
     (parse-tsv-overlays
       (compute-conceal-overlays-for-comments-with-options
         text (unbox *math-render-active*)))]
    [(and path (string-suffix? path ".typ"))
     ;; .typ: Typst math scanning, returns tab-separated format
     (parse-tsv-overlays (compute-conceal-overlays-for-typst text))]
    [else
     ;; Other files: full-document LaTeX scan, returns JSON
     (parse-overlay-json (compute-conceal-overlays-ffi text))]))

;;;@doc
;;; Parse tab-separated overlay format: "offset\\treplacement\\n..."
(define (parse-tsv-overlays tsv-str)
  (if (equal? tsv-str "")
      '()
      (let loop ([lines (string-split tsv-str "\n")] [result '()])
        (if (null? lines)
            (reverse result)
            (let ([line (car lines)])
              (if (equal? line "")
                  (loop (cdr lines) result)
                  (let ([tab-pos (find-tab line)])
                    (if (not tab-pos)
                        (loop (cdr lines) result)
                        (let ([offset (string->number (substring line 0 tab-pos))]
                              [replacement (substring line (+ tab-pos 1) (string-length line))])
                          (loop (cdr lines)
                                (cons (cons offset replacement) result)))))))))))

(define (find-tab str)
  (let loop ([i 0])
    (cond
      [(>= i (string-length str)) #false]
      [(char=? (string-ref str i) #\tab) i]
      [else (loop (+ i 1))])))

;;; Parse the JSON overlay string into a list of (offset . replacement) pairs.
;;; This is pure computation — safe to run on any thread.
;;;
;;; Input format is the serde_json output of `Vec<Overlay>`, i.e.
;;;   `[{"offset":N,"replacement":"X"},...]`. We walk the string with
;;;   position indices and lean on json-utils for the shared primitives.
(define (parse-overlay-json json-str)
  (if (string=? json-str "[]")
      '()
      (let parse-loop ([pos 0] [result '()])
        (cond
          [(>= pos (string-length json-str)) (reverse result)]
          [(char=? (string-ref json-str pos) #\{)
           (let* ([colon1-pos (json-find-char json-str #\: (+ pos 1))]
                  [offset-start (json-skip-whitespace json-str (+ colon1-pos 1))]
                  [offset-end (json-find-non-digit json-str offset-start)]
                  [offset-val (string->number (substring json-str offset-start offset-end))]
                  [colon2-pos (json-find-char json-str #\: offset-end)]
                  [quote1-pos (json-find-char json-str #\" (+ colon2-pos 1))]
                  [replacement-str (json-extract-string json-str (+ quote1-pos 1))]
                  [after-str (+ quote1-pos 1 (json-string-raw-length json-str (+ quote1-pos 1)) 1)]
                  [close-pos (json-find-char json-str #\} after-str)])
             (parse-loop (+ close-pos 1)
                         (cons (cons offset-val replacement-str) result)))]
          [else (parse-loop (+ pos 1) result)]))))

;;;@doc
;;; Compute conceal overlays on a background thread and apply them
;;; when ready. The editor remains responsive during computation.
(define (compute-and-apply-conceal-async)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))
  (define path (editor-document->path doc-id))
  (define is-jl (and path (ends-with-jl? path)))

  ;; Spawn background thread for the heavy FFI work.
  (spawn-native-thread
    (lambda ()
      (define json-str
        (if is-jl
            (compute-conceal-overlays-for-comments-with-options
              text (unbox *math-render-active*))
            (compute-conceal-overlays-ffi text)))

      ;; Parse overlays on the background thread too (no editor access needed).
      (define overlays (parse-overlay-json json-str))

      ;; Deliver results to the main thread.
      ;; Guard: only apply if the user hasn't switched buffers.
      (hx.with-context
        (lambda ()
          (define current-doc-id (editor->doc-id (editor-focus)))
          (when (equal? current-doc-id doc-id)
            (if (null? overlays)
                (clear-overlays!)
                (begin
                  (set-overlays! overlays)
                  (set-status! (string-append "nothelix: " (number->string (length overlays)) " overlays"))))))))))

(define (ends-with-jl? path)
  (define len (string-length path))
  (and (>= len 3)
       (string=? (substring path (- len 3) len) ".jl")))

;;; ─── Conceal orchestration ───────────────────────────────────────────────────
;;;
;;; The orchestration layer owns the "when should conceal run" logic and
;;; the interaction with the fingerprinted cache in conceal-state.scm.
;;; Every public function here is a safe entry point for hooks and commands —
;;; it validates the document fingerprint and fails closed rather than
;;; apply stale char offsets.

(define *conceal-extensions* '("md" "markdown" "tex" "jl" "typ" "qmd" "rmd"))

(define (ends-with? str suffix)
  (define slen (string-length suffix))
  (define tlen (string-length str))
  (and (>= tlen slen)
       (string=? (substring str (- tlen slen) tlen) suffix)))

;;@doc
;; #true if the file extension is one that should get LaTeX concealment.
(define (file-has-conceal-extension? path)
  (and path
       (let loop ((exts *conceal-extensions*))
         (cond
           [(null? exts) #false]
           [(ends-with? path (string-append "." (car exts))) #true]
           [else (loop (cdr exts))]))))

;;@doc
;; Compute and apply conceal overlays for the current buffer synchronously.
;; Tags the cache with the current document fingerprint so later cursor
;; moves can validate it.
;;
;; When math-render is active we ALSO re-stage the above/below virtual
;; rows. The annotations are keyed to absolute line numbers — if a user
;; adds or removes a line, every annotation below the edit sits at the
;; wrong index until something refreshes it. Previously only `:w`
;; rebuilt them, so the buffer visibly drifted while typing (operators
;; gained/lost virtual rows as lines shifted under stale annotations).
;; Piggy-backing the refresh onto the debounced conceal cycle keeps the
;; annotations aligned without adding a second timer.
(define (conceal-math!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define overlays (compute-conceal-overlays))
  (conceal-cache-update! doc-id overlays)
  (cond
    [(null? overlays)
     (clear-overlays!)]
    [else
     (apply-conceal-for-cursor!)])
  (when (unbox *math-render-active*)
    ((unbox *math-render-refresh-hook*))))

;;@doc
;; Drop both the cache and the view overlays.
(define (clear-conceal!)
  (conceal-cache-clear!)
  (clear-overlays!))

;;@doc
;; Re-filter the cached overlays to exclude the cursor's current line.
;; Fails closed: if the cache fingerprint no longer matches the current
;; document, the view overlays are cleared and we wait for the next
;; reconceal to rebuild them.
(define (apply-conceal-for-cursor!)
  (cond
    [(conceal-cache-empty?) #f]
    [else
     (define fp (conceal-fingerprint-current))
     (cond
       [(not (conceal-fingerprint-matches? fp))
        (clear-overlays!)]
       [else
        (define doc-id (car fp))
        (define rope (editor->text doc-id))
        (define cursor-pos (cursor-position))
        (define cursor-line (text.rope-char->line rope cursor-pos))
        (define line-start-char (text.rope-line->char rope cursor-line))
        (define line-end-char
          (if (< (+ cursor-line 1) (text.rope-len-lines rope))
              (text.rope-line->char rope (+ cursor-line 1))
              (text.rope-len-chars rope)))
        (define cached (conceal-cache-overlays))
        (define filtered
          (filter (lambda (pair)
                    (define off (car pair))
                    (or (< off line-start-char)
                        (>= off line-end-char)))
                  cached))
        (set-overlays! filtered)])]))

;; Debounce generation counter. Every call to schedule-reconceal bumps it so
;; only the most recent scheduled callback actually runs.
(define *reconceal-generation* 0)

;;@doc
;; Schedule a reconceal after `delay-ms` milliseconds, collapsing rapid
;; successive calls via a generation counter. Safe to call from hooks that
;; fire many times in quick succession.
(define (schedule-reconceal delay-ms)
  (set! *reconceal-generation* (+ *reconceal-generation* 1))
  (define my-gen *reconceal-generation*)
  (enqueue-thread-local-callback-with-delay delay-ms
    (lambda ()
      (when (= my-gen *reconceal-generation*)
        (define path (editor-document->path (editor->doc-id (editor-focus))))
        (when (file-has-conceal-extension? path)
          (conceal-math!))))))
