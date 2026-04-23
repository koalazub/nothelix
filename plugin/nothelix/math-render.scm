;;; math-render.scm - Stack big-operator limits and fraction bodies onto
;;; virtual rows above/below the source line, via the fork's
;;; set-math-lines-{above,below}! FFI.
;;;
;;; The LaTeX parsing that used to live in this file moved to the Rust
;;; `parse-math-spans` FFI, which returns a JSON list of structural
;;; spans. This module now iterates that list and stages padded virtual
;;; lines — it does no string walking of its own.

(require "common.scm")
(require "string-utils.scm")
(require "json-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "conceal.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          parse-math-spans))

(provide math-render-buffer
         math-render-clear)

;; Probe + graceful-degrade wrappers over the fork FFI. When the user is
;; on an older `hx` binary without the Phase-2 math-lines bindings, we
;; must NOT flip `*math-render-active*` (it would tell the concealer to
;; hide inline `\frac` without anything replacing it). `probe` tests
;; once per call, returning #t if the FFI is reachable.
(define (try-set-math-lines-above! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.set-math-lines-above! ,line-idx ',lines))))

(define (try-set-math-lines-below! line-idx lines)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.set-math-lines-below! ,line-idx ',lines))))

(define (try-clear-all-math-lines!)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-math-lines!))))

(define (math-render-ffi-available?)
  (with-handler
    (lambda (_) #false)
    (eval '(helix.static.clear-all-math-lines!))
    #true))

;; Right-pad (or build from scratch) a string of length `n` using
;; spaces. Used to place the stacked limit strings under the concealed
;; operator column.
(define (spaces n)
  (if (<= n 0)
      ""
      (let loop ([i 0] [acc ""])
        (if (>= i n)
            acc
            (loop (+ i 1) (string-append acc " "))))))

;; Parse one line of the TSV emitted by `parse-math-spans`. Returns a
;; list of field strings, or '() on a blank line.
;;
;; Format per row:
;;   big_op\tCMD\tSTART\tEND\tCOL\tSUB\tSUP
;;   frac\tCMD\tSTART\tEND\tCOL\tNUM\tDEN
;;
;; Rust side escapes `\\` / `\t` / `\n` inside the last two fields; we
;; unescape after the split so arbitrary sub/sup contents survive.
(define (parse-math-span-row row)
  (if (= (string-length row) 0)
      '()
      (map unescape-field (string-split row "\t"))))

(define (unescape-field s)
  ;; Minimal string-based unescape. `string-replace-all` from string-utils
  ;; operates on substrings — the order matters (backslash last) since
  ;; we don't want to un-escape a `\\t` into a literal tab.
  (let* ([step1 (string-replace-all s "\\n" "\n")]
         [step2 (string-replace-all step1 "\\t" "\t")])
    (string-replace-all step2 "\\\\" "\\")))

;; Turn one parsed-row into zero or more (line-idx above below) entries.
(define (row->entries fields line-idx)
  (cond
    [(null? fields) '()]
    [else
     (define kind (car fields))
     (define visual-col (string->number (list-ref fields 4)))
     (define padding (spaces (or visual-col 0)))
     (cond
       [(equal? kind "big_op")
        (define sub-text (list-ref fields 5))
        (define sup-text (list-ref fields 6))
        (list (list line-idx
                    (if (equal? sup-text "") #false (string-append padding sup-text))
                    (if (equal? sub-text "") #false (string-append padding sub-text))))]
       [(equal? kind "frac")
        (define num-text (list-ref fields 5))
        (define den-text (list-ref fields 6))
        (list (list line-idx
                    (string-append padding num-text)
                    (string-append padding den-text)))]
       [else '()])]))

;;@doc
;; Scan every comment line in the buffer, parse it with the Rust
;; `parse-math-spans` FFI, and stage the resulting above/below strings
;; via the fork's math-line FFI. Guarded: if the fork FFI isn't
;; available we surface a status note and leave the concealer alone.
(define (math-render-buffer)
  (cond
    [(not (math-render-ffi-available?))
     (set-box! *math-render-active* #false)
     (set-status! "math-render: hx fork FFI missing — run darwin-rebuild to enable")]
    [else (math-render-buffer-impl)]))

(define (math-render-buffer-impl)
  (try-clear-all-math-lines!)
  (set-box! *math-render-active* #true)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (let loop ([line-idx 0])
    (when (< line-idx total-lines)
      (define line (text.rope->string (text.rope->line rope line-idx)))
      (define trimmed
        (if (string-suffix? line "\n")
            (substring line 0 (- (string-length line) 1))
            line))
      ;; Only scan comment lines — the converter produces `# `-prefixed
      ;; markdown cells, and that's where the math lives in .jl files.
      (when (string-starts-with? trimmed "# ")
        (define content (substring trimmed 2 (string-length trimmed)))
        (define tsv (parse-math-spans content))
        (define rows
          (if (= (string-length tsv) 0)
              '()
              (filter
                (lambda (r) (> (string-length r) 0))
                (string-split tsv "\n"))))
        (define entries
          (apply append
                 (map (lambda (row)
                        (row->entries (parse-math-span-row row) line-idx))
                      rows)))
        (stage-merged line-idx entries))
      (loop (+ line-idx 1)))))

;; Merge all per-line contributions into one pair of above/below lists
;; and hand them to the FFI in one call each.
(define (stage-merged line-idx entries)
  (define above-lines '())
  (define below-lines '())
  (for-each
    (lambda (entry)
      (define a (cadr entry))
      (define b (caddr entry))
      (when a (set! above-lines (cons a above-lines)))
      (when b (set! below-lines (cons b below-lines))))
    entries)
  (when (not (null? above-lines))
    (try-set-math-lines-above! line-idx (reverse above-lines)))
  (when (not (null? below-lines))
    (try-set-math-lines-below! line-idx (reverse below-lines))))

;;@doc
;; Drop every math annotation on the current document — used when the
;; user wants to revert to raw inline rendering.
(define (math-render-clear)
  (try-clear-all-math-lines!)
  (set-box! *math-render-active* #false))
