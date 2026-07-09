;;; conceal.scm - Render inline LaTeX math as Unicode via Helix overlays, without modifying the buffer.

(require "helix/editor.scm")
(require-builtin helix/core/text as text.)
(require "helix/misc.scm")
(require "string-utils.scm")
(require "conceal-state.scm")
(require "json-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          compute-conceal-overlays-ffi
                          compute-conceal-overlays-for-comments-with-options
                          compute-conceal-overlays-for-typst))

(provide compute-conceal-overlays
         parse-overlay-json
         file-has-conceal-extension?
         conceal-math!
         clear-conceal!
         apply-conceal-for-cursor!
         schedule-reconceal
         *math-render-active*
         *math-render-refresh-hook*
         *markdown-marker-hook*
         *markdown-style-hook*)

;; Installed by markdown-render.scm; inert by default.
(define *markdown-marker-hook* (box (lambda () '())))
(define *markdown-style-hook* (box (lambda (line-start line-end) #f)))

;; Set by math-render; when on, the scanner hides the inline form it stacks as virtual rows.
(define *math-render-active* (box #false))

;; Installed by math-render so the conceal cycle can restage its virtual rows. No-op by default.
(define *math-render-refresh-hook* (box (lambda () #f)))

;; Public API

;;;@doc
;;; Compute conceal overlay pairs for the current document.
(define (compute-conceal-overlays)
  (append (compute-math-conceal-overlays)
          ((unbox *markdown-marker-hook*))))

(define (compute-math-conceal-overlays)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define text (text.rope->string rope))
  (define path (editor-document->path doc-id))
  (cond
    [(and path (string-suffix? path ".jl"))
     ;; .jl: per-line comment scan; 2nd arg hides layout math-render already stacks.
     (parse-tsv-overlays
       (compute-conceal-overlays-for-comments-with-options
         text (unbox *math-render-active*)))]
    [(and path (string-suffix? path ".typ"))
     (parse-tsv-overlays (compute-conceal-overlays-for-typst text))]
    [else
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
                  ;; split-once returns a 2-list on a tab, else a non-list; guard so tab-less lines are skipped.
                  (let ([parts (split-once line "\t")])
                    (if (list? parts)
                        (loop (cdr lines)
                              (cons (cons (string->number (car parts)) (cadr parts))
                                    result))
                        (loop (cdr lines) result)))))))))

;; Parse the JSON `[{"offset":N,"replacement":"X"},...]` string into (offset . replacement) pairs.
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

;; Conceal orchestration

(define *conceal-extensions* '("md" "markdown" "tex" "jl" "typ" "qmd" "rmd"))

;;@doc
;; #true if the file extension is one that should get LaTeX concealment.
(define (file-has-conceal-extension? path)
  (and path
       (let loop ((exts *conceal-extensions*))
         (cond
           [(null? exts) #false]
           [(string-suffix? path (string-append "." (car exts))) #true]
           [else (loop (cdr exts))]))))

;;@doc
;; Compute and apply conceal overlays for the current buffer, tagging the cache with the doc fingerprint.
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
;; Re-filter cached overlays to exclude the cursor's current line; fails closed on a stale fingerprint.
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
        (set-overlays! filtered)
        ((unbox *markdown-style-hook*) line-start-char line-end-char)])]))

(define *reconceal-generation* 0)

;;@doc
;; Schedule a reconceal after `delay-ms` ms, collapsing rapid successive calls.
(define (schedule-reconceal delay-ms)
  (set! *reconceal-generation* (+ *reconceal-generation* 1))
  (define my-gen *reconceal-generation*)
  (enqueue-thread-local-callback-with-delay delay-ms
    (lambda ()
      (when (= my-gen *reconceal-generation*)
        (define path (editor-document->path (editor->doc-id (editor-focus))))
        (when (file-has-conceal-extension? path)
          (conceal-math!))))))
