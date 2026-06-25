;;; table-image.scm - Render markdown pipe tables as inline Typst images.
;;;
;;; A markdown table can't be aligned as inline overlay text — the fork's
;;; overlay layer renders one grapheme per source char, so padding columns
;;; is impossible and a multi-char box row degrades to tofu. Instead a table
;;; is typeset by Typst into a transparent, theme-coloured image and placed
;;; inline exactly like display math, reusing math-image.scm's placement
;;; (kitty placeholder + RawContent), sizing, and theme-colour helpers.

(require "string-utils.scm")
(require "debug.scm")
(require "conceal.scm")
(require "math-image.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix render-table-to-svg))

(provide render-all-tables)

;; Kitty placeholder ids must stay < 2^24; tables own the 4M band (paths
;; use 1M, math 8M — see math-image.scm's *math-image-id-base* note).
(define *table-image-id-base* 4000000)

;; Typst font size for table glyphs. Slightly smaller than display math so
;; a wide table fits without dominating the buffer.
(define *table-image-font-pt* (box 13))

;;; ---------------------------------------------------------------------------
;;; Block detection
;;; ---------------------------------------------------------------------------

;; The `# `-stripped body of a comment line, or #false if line `idx` is not
;; a `# ` comment line.
(define (comment-body rope total idx)
  (and (>= idx 0) (< idx total)
       (let* ([s (text.rope->string (text.rope->line rope idx))]
              [t (if (string-suffix? s "\n")
                     (substring s 0 (- (string-length s) 1))
                     s)])
         (and (string-starts-with? t "# ")
              (substring t 2 (string-length t))))))

;; #true when `pred` holds for every element of `lst` (Steel has no andmap
;; in this module's scope).
(define (list-all? pred lst)
  (cond
    [(null? lst) #true]
    [(pred (car lst)) (list-all? pred (cdr lst))]
    [else #false]))

;; #true when a table body line is the `|:--|--:|` rule: every cell is
;; dashes/colons and at least one dash.
(define (separator-body? body)
  (and (string-contains? body "|")
       (let ([cells (filter (lambda (c) (> (string-length (string-trim c)) 0))
                            (string-split body "|"))])
         (and (not (null? cells))
              (list-all?
                (lambda (cell)
                  (let ([t (string-trim cell)])
                    (and (> (string-length t) 0)
                         (string-contains? t "-")
                         (list-all? (lambda (ch) (or (char=? ch #\-) (char=? ch #\:)))
                                    (string->list t)))))
                cells)))))

;; Every (anchor-line . block-text) for the buffer's markdown tables. A
;; table is a run of consecutive `# | ... |` comment lines that includes a
;; `|---|` separator row; block-text is the run joined with newlines, each
;; line `# `-stripped (the shape render-table-to-svg parses).
(define (collect-table-jobs rope total)
  (let loop ([idx 0] [acc '()])
    (if (>= idx total)
        (reverse acc)
        (let ([body (comment-body rope total idx)])
          (if (and body (string-contains? body "|"))
              (let scan ([end (+ idx 1)] [lines (list body)] [saw-sep (separator-body? body)])
                (let ([nb (comment-body rope total end)])
                  (if (and nb (string-contains? nb "|"))
                      (scan (+ end 1) (cons nb lines) (or saw-sep (separator-body? nb)))
                      (if (and saw-sep (> (length lines) 1))
                          (loop end (cons (cons idx (string-join (reverse lines) "\n")) acc))
                          (loop end acc)))))
              (loop (+ idx 1) acc))))))

;;; ---------------------------------------------------------------------------
;;; Rendering + placement
;;; ---------------------------------------------------------------------------

;; djb2, same algorithm as math-image.scm — a stable id keeps re-renders
;; replacing in place rather than stacking duplicate RawContent entries.
(define (table-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

(define (table-image-id anchor block)
  (+ *table-image-id-base*
     (modulo (+ (table-hash (number->string anchor)) (table-hash block))
             4000000)))

;; In test mode reuse math-image's mock JSON so detection + placement run
;; without invoking Typst (which would garble captured test output).
(define (call-render-table block font-pt color)
  (if (math-image-test-mode?)
      (math-image-mock-result)
      (render-table-to-svg block font-pt color)))

(define (place-table-job rope job result-json)
  (define anchor (car job))
  (place-svg-image-at-line! result-json
                            (table-image-id anchor (cdr job))
                            rope anchor "table-image"))

(define (table-count-phrase n)
  (string-append (number->string n) " table" (if (= n 1) "" "s")))

;; Compile every table off the main thread (Typst is the heavy cost), then
;; register the images back on the main thread so the editor stays live.
(define (render-all-tables-async jobs doc-id)
  (define color (effective-math-text-color))
  (define font-pt (unbox *table-image-font-pt*))
  (set-status! (string-append "table-image: rendering " (table-count-phrase (length jobs)) "…"))
  (spawn-native-thread
    (lambda ()
      (define rendered
        (map (lambda (job) (cons job (render-table-to-svg (cdr job) font-pt color))) jobs))
      (hx.with-context
        (lambda ()
          (define rope (editor->text doc-id))
          (let place ([rs rendered] [placed 0])
            (if (null? rs)
                (set-status! (string-append "table-image: rendered " (table-count-phrase placed)))
                (place (cdr rs)
                       (if (place-table-job rope (car (car rs)) (cdr (car rs)))
                           (+ placed 1)
                           placed)))))))))

;;@doc
;; Scan the current buffer for every markdown pipe table and render each as
;; a transparent Typst image inline. No-ops on non-conceal file types.
(define (render-all-tables)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (when (file-has-conceal-extension? path)
    (define rope (editor->text doc-id))
    (define total (text.rope-len-lines rope))
    (define jobs (collect-table-jobs rope total))
    (when (not (null? jobs))
      (if (math-image-test-mode?)
          (for-each
            (lambda (job)
              (place-table-job rope job
                               (call-render-table (cdr job) (unbox *table-image-font-pt*)
                                                  (effective-math-text-color))))
            jobs)
          (render-all-tables-async jobs doc-id)))))
