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

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          compute-conceal-overlays-ffi
                          compute-conceal-overlays-for-comments
                          latex-overlays))

(provide compute-conceal-overlays compute-and-apply-conceal-async
         find-math-regions build-overlays-for-region parse-overlay-json)

;;; ─── Math region scanner (kept for external use) ─────────────────────────────

;;;@doc
;;; Scan text for $...$ and $$...$$ regions.
;;; Returns a list of (start . end) pairs for the CONTENTS
;;; (not including the delimiters themselves).
(define (find-math-regions text)
  (define len (string-length text))
  (let loop ([i 0] [regions '()])
    (cond
      [(>= i len) (reverse regions)]
      [(and (char=? (string-ref text i) #\$)
            (< (+ i 1) len)
            (char=? (string-ref text (+ i 1)) #\$))
       (let inner ([j (+ i 2)])
         (cond
           [(>= j (- len 1)) (reverse regions)]
           [(and (char=? (string-ref text j) #\$)
                 (char=? (string-ref text (+ j 1)) #\$))
            (loop (+ j 2) (cons (cons (+ i 2) j) regions))]
           [else (inner (+ j 1))]))]
      [(char=? (string-ref text i) #\$)
       (let inner ([j (+ i 1)])
         (cond
           [(>= j len) (reverse regions)]
           [(char=? (string-ref text j) #\$)
            (loop (+ j 1) (cons (cons (+ i 1) j) regions))]
           [else (inner (+ j 1))]))]
      [(and (char=? (string-ref text i) #\\)
            (< (+ i 1) len)
            (char=? (string-ref text (+ i 1)) #\())
       (let inner ([j (+ i 2)])
         (cond
           [(>= j (- len 1)) (reverse regions)]
           [(and (char=? (string-ref text j) #\\)
                 (char=? (string-ref text (+ j 1)) #\)))
            (loop (+ j 2) (cons (cons (+ i 2) j) regions))]
           [else (inner (+ j 1))]))]
      [else (loop (+ i 1) regions)])))

;;; ─── Overlay builder (kept for external use) ────────────────────────────────

;;;@doc
;;; Parse JSON overlay data from the Rust FFI and convert to Helix overlay pairs.
;;; region-start is the char offset of the math region within the document.
(define (build-overlays-for-region full-text region-start region-end)
  (define math-text (substring full-text region-start region-end))
  (define json-str (latex-overlays math-text))

  (let parse-loop ([pos 0] [result '()])
    (cond
      [(>= pos (string-length json-str)) (reverse result)]
      [(char=? (string-ref json-str pos) #\{)
       (let* ([offset-key-pos (+ pos 1)]
              [colon1-pos (find-char json-str #\: offset-key-pos)]
              [offset-start (skip-whitespace json-str (+ colon1-pos 1))]
              [offset-end (find-non-digit json-str offset-start)]
              [offset-val (string->number (substring json-str offset-start offset-end))]
              [colon2-pos (find-char json-str #\: offset-end)]
              [quote1-pos (find-char json-str #\" (+ colon2-pos 1))]
              [replacement-str (extract-json-string json-str (+ quote1-pos 1))]
              [after-str (+ quote1-pos 1 (json-string-raw-length json-str (+ quote1-pos 1)) 1)]
              [close-pos (find-char json-str #\} after-str)])
         (parse-loop (+ close-pos 1)
                     (cons (cons (+ region-start offset-val) replacement-str)
                           result)))]
      [else (parse-loop (+ pos 1) result)])))

;;; ─── Minimal JSON helpers (kept for build-overlays-for-region) ──────────────

(define (find-char str ch start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(char=? (string-ref str i) ch) i]
      [else (loop (+ i 1))])))

(define (skip-whitespace str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(or (char=? (string-ref str i) #\space)
           (char=? (string-ref str i) #\tab)) (loop (+ i 1))]
      [else i])))

(define (find-non-digit str start)
  (let loop ([i start])
    (cond
      [(>= i (string-length str)) i]
      [(and (char>=? (string-ref str i) #\0)
            (char<=? (string-ref str i) #\9)) (loop (+ i 1))]
      [(char=? (string-ref str i) #\-) (loop (+ i 1))]
      [else i])))

(define (extract-json-string str start)
  (let loop ([i start] [chars '()])
    (cond
      [(>= i (string-length str)) (list->string (reverse chars))]
      [(and (char=? (string-ref str i) #\\)
            (< (+ i 1) (string-length str)))
       (loop (+ i 2) (cons (string-ref str (+ i 1)) chars))]
      [(char=? (string-ref str i) #\") (list->string (reverse chars))]
      [else (loop (+ i 1) (cons (string-ref str i) chars))])))

(define (json-string-raw-length str start)
  (let loop ([i start] [len 0])
    (cond
      [(>= i (string-length str)) len]
      [(and (char=? (string-ref str i) #\\)
            (< (+ i 1) (string-length str)))
       (loop (+ i 2) (+ len 2))]
      [(char=? (string-ref str i) #\") len]
      [else (loop (+ i 1) (+ len 1))])))

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
  (define json-str
    (if (and path (ends-with-jl? path))
        (compute-conceal-overlays-for-comments text)
        (compute-conceal-overlays-ffi text)))
  (parse-overlay-json json-str))

;;; Parse the JSON overlay string into a list of (offset . replacement) pairs.
;;; This is pure computation — safe to run on any thread.
(define (parse-overlay-json json-str)
  (if (string=? json-str "[]")
      '()
      (let parse-loop ([pos 0] [result '()])
        (cond
          [(>= pos (string-length json-str)) (reverse result)]
          [(char=? (string-ref json-str pos) #\{)
           (let* ([colon1-pos (find-char json-str #\: (+ pos 1))]
                  [offset-start (skip-whitespace json-str (+ colon1-pos 1))]
                  [offset-end (find-non-digit json-str offset-start)]
                  [offset-val (string->number (substring json-str offset-start offset-end))]
                  [colon2-pos (find-char json-str #\: offset-end)]
                  [quote1-pos (find-char json-str #\" (+ colon2-pos 1))]
                  [replacement-str (extract-json-string json-str (+ quote1-pos 1))]
                  [after-str (+ quote1-pos 1 (json-string-raw-length json-str (+ quote1-pos 1)) 1)]
                  [close-pos (find-char json-str #\} after-str)])
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
            (compute-conceal-overlays-for-comments text)
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
