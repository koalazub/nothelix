;;; math-render.scm - Stack big-operator limits onto virtual rows above/below
;;;
;;; The conceal layer already handles tiny Unicode super/subscripts, but for
;;; big operators (∑, ∫, ∏, ⋃, ⋂, ⋁, ⋀) the limits are *meant* to sit
;;; directly above and below the glyph, not inline. This module detects
;;; those patterns in the buffer and stages the limit text onto the
;;; Document's `math_lines_above` / `math_lines_below` buckets via the
;;; fork's Steel FFI (`set-math-lines-above!`, `set-math-lines-below!`).
;;;
;;; The renderer paints the staged strings verbatim — we're responsible
;;; for leading whitespace that aligns the limits with the operator
;;; column. Column computation uses the source-line character offset of
;;; the operator; the conceal layer's hidden bytes shift visible columns
;;; but most big-op rendering happens in display-math `$$` blocks where
;;; the source is already close to what gets drawn.

(require "common.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))

(provide math-render-buffer
         math-render-clear)

;;@doc
;; Map of LaTeX big-operator names to the glyph we render them as. Only
;; commands that take stacked `_{sub}^{sup}` limits live here — `\cos`
;; etc. are handled by the concealer's operator path.
(define *big-op-glyph*
  (hash
    "int"      "∫"
    "iint"     "∬"
    "iiint"    "∭"
    "oint"     "∮"
    "oiint"    "∯"
    "oiiint"   "∰"
    "sum"      "∑"
    "prod"     "∏"
    "coprod"   "∐"
    "bigcup"   "⋃"
    "bigcap"   "⋂"
    "bigvee"   "⋁"
    "bigwedge" "⋀"
    "bigoplus" "⨁"
    "bigotimes" "⨂"
    "bigodot"  "⨀"
    "biguplus" "⨄"
    "bigsqcup" "⨆"))

;;@doc
;; Make a string of N spaces — the renderer wants explicit leading
;; whitespace for column alignment.
(define (spaces n)
  (if (<= n 0)
      ""
      (let loop ([i 0] [acc ""])
        (if (>= i n)
            acc
            (loop (+ i 1) (string-append acc " "))))))

;;@doc
;; Find the byte index of the matching close `}` given the byte index
;; just past an opening `{`. Returns #f if unbalanced.
(define (find-matching-brace s start)
  (let loop ([i start] [depth 1])
    (cond
      [(>= i (string-length s)) #false]
      [(= depth 0) i]
      [(char=? (string-ref s i) #\{) (loop (+ i 1) (+ depth 1))]
      [(char=? (string-ref s i) #\}) (loop (+ i 1) (- depth 1))]
      [else (loop (+ i 1) depth)])))

;;@doc
;; Extract the content of a `_{…}` / `^{…}` group starting at position
;; `i` (which should point at `_` or `^`). Returns `(content, next-pos)`
;; or #f on no match. Also handles single-char limits like `_n`, `^2`.
(define (scan-limit-group s i)
  (cond
    [(or (>= i (string-length s))
         (and (not (char=? (string-ref s i) #\_))
              (not (char=? (string-ref s i) #\^))))
     #false]
    [(and (< (+ i 1) (string-length s))
          (char=? (string-ref s (+ i 1)) #\{))
     (let* ([content-start (+ i 2)]
            [close (find-matching-brace s content-start)])
       (if close
           (cons (substring s content-start (- close 1))
                 close)
           #false))]
    [(< (+ i 1) (string-length s))
     ;; single char limit — eat exactly one grapheme-ish unit
     (cons (substring s (+ i 1) (+ i 2)) (+ i 2))]
    [else #false]))

;;@doc
;; After an operator command position `after-cmd`, parse up to two
;; limit groups (any order of `_` and `^`). Returns
;; `((sub . sup) . next-pos)` where either side may be `#f` if absent.
(define (scan-op-limits s after-cmd)
  (define (skip-ws i)
    (if (and (< i (string-length s))
             (char=? (string-ref s i) #\space))
        (skip-ws (+ i 1))
        i))
  (let* ([p0 (skip-ws after-cmd)]
         [g1 (scan-limit-group s p0)])
    (cond
      [(not g1) (cons (cons #false #false) after-cmd)]
      [else
       (let* ([c1 (car g1)]
              [after1 (cdr g1)]
              [first-is-sub (char=? (string-ref s p0) #\_)]
              [p1 (skip-ws after1)]
              [g2 (scan-limit-group s p1)])
         (cond
           [(not g2)
            (if first-is-sub
                (cons (cons c1 #false) after1)
                (cons (cons #false c1) after1))]
           [else
            (let ([c2 (car g2)] [after2 (cdr g2)])
              (if first-is-sub
                  (cons (cons c1 c2) after2)
                  (cons (cons c2 c1) after2)))]))])))

;;@doc
;; Given a source line and its 0-based index, scan for big-operator
;; patterns and return a list of `(line-idx above-lines below-lines)`
;; triples to stage. Currently emits one entry per operator found; if
;; multiple operators appear on the same line, the triples are merged
;; by the caller.
(define (scan-line-for-operators line line-idx)
  (let loop ([i 0] [entries '()])
    (cond
      [(>= i (string-length line)) (reverse entries)]
      [(not (char=? (string-ref line i) #\\))
       (loop (+ i 1) entries)]
      [else
       (let ([name-end (scan-backslash-name line (+ i 1))])
         (cond
           [(= name-end (+ i 1))
            (loop (+ i 1) entries)]
           [else
            (let ([name (substring line (+ i 1) name-end)])
              (cond
                [(hash-contains? *big-op-glyph* name)
                 (let* ([limits (scan-op-limits line name-end)]
                        [sub (car (car limits))]
                        [sup (cdr (car limits))]
                        [after (cdr limits)])
                   (cond
                     [(and (not sub) (not sup))
                      (loop after entries)]
                     [else
                      (define padding (spaces (+ i 2))) ; `# ` prefix + operator col
                      (define above-line
                        (if sup (string-append padding sup) #false))
                      (define below-line
                        (if sub (string-append padding sub) #false))
                      (loop after
                            (cons (list line-idx above-line below-line)
                                  entries))]))]
                [else (loop name-end entries)]))]))])))

;;@doc
;; Bytewise scan past `\cmd` alphabetic chars starting at `i`, returning
;; the position past the last letter. If `s[i]` isn't alphabetic we
;; return `i` unchanged so the caller can detect no-match.
(define (scan-backslash-name s i)
  (let loop ([j i])
    (cond
      [(>= j (string-length s)) j]
      [(char-alphabetic? (string-ref s j)) (loop (+ j 1))]
      [else j])))

;;@doc
;; Whole-buffer pass: strip any existing math annotations, then walk
;; each `# `-prefixed line looking for big-operator patterns inside
;; math delimiters and stage the above/below strings.
(define (math-render-buffer)
  (helix.static.clear-all-math-lines!)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))

  (let loop ([line-idx 0])
    (when (< line-idx total-lines)
      (define line (text.rope->string (text.rope->line rope line-idx)))
      (define trimmed
        (if (string-suffix? "\n" line)
            (substring line 0 (- (string-length line) 1))
            line))
      ;; Only scan comment lines — the converter produces `# `-prefixed
      ;; markdown cells, and that's where the math lives in .jl files.
      (when (string-starts-with? trimmed "# ")
        (define entries (scan-line-for-operators trimmed line-idx))
        (for-each stage-entry entries))
      (loop (+ line-idx 1)))))

(define (stage-entry entry)
  (define line-idx (car entry))
  (define above (cadr entry))
  (define below (caddr entry))
  (when above
    (helix.static.set-math-lines-above! line-idx (list above)))
  (when below
    (helix.static.set-math-lines-below! line-idx (list below))))

;;@doc
;; Drop every math annotation on the current document — used when the
;; user wants to revert to raw source display.
(define (math-render-clear)
  (helix.static.clear-all-math-lines!))

(define (string-suffix? suffix s)
  (let ([slen (string-length s)]
        [xlen (string-length suffix)])
    (and (>= slen xlen)
         (equal? (substring s (- slen xlen) slen) suffix))))
