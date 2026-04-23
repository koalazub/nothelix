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

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          set-math-layout-hide))

(provide math-render-buffer
         math-render-clear)

;;@doc
;; The fork-side FFI bindings (`set-math-lines-above!`,
;; `set-math-lines-below!`, `clear-math-lines!`, `clear-all-math-lines!`)
;; only exist when the user is running the koalazub/helix fork tip that
;; has the Phase 2/3 LineAnnotation + Decoration work merged. On a stock
;; or older `hx` binary they're `FreeIdentifier` errors at load time —
;; same story the image-cache module solved for `clear-raw-content!`.
;;
;; Deferred-lookup wrappers via `eval` + `with-handler` let the plugin
;; load clean regardless. If the binding is missing the call is a silent
;; no-op; once you darwin-rebuild onto the new fork SHA they start
;; working without any plugin edit.
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
;; Count visual columns consumed by the portion of `line` up to source
;; byte `source-col`, accounting for conceal substitutions. The concealer
;; replaces each `\\<cmd>` with a single Unicode glyph, so every such
;; command "saves" `(string-length cmd)` visible columns vs its raw
;; source. Also discount hidden braces in `_{...}` / `^{...}` groups (2
;; chars each) and the `\\`s of `\\left` / `\\right`. Result is the visual
;; column where a glyph emitted at source col `source-col` ends up.
;;
;; Not perfectly exact — doesn't account for multi-byte Unicode in the
;; source itself — but gets within 1–2 cells in practice, which is
;; enough for limit stacks to land under their operator.
(define (visual-column-of line source-col)
  (let loop ([i 0] [visual 0])
    (cond
      [(>= i source-col) visual]
      [(char=? (string-ref line i) #\\)
       (let ([name-end (scan-backslash-name line (+ i 1))])
         (cond
           [(= name-end (+ i 1))
            ;; `\<non-letter>` — usually a spacing / delim escape.
            (loop (+ i 2) (+ visual 1))]
           [else
            (let ([name-len (- name-end i 1)])
              ;; `\cmd` renders as 1 grapheme. Source consumed = 1+name-len.
              (loop name-end (+ visual 1)))]))]
      [(char=? (string-ref line i) #\{)
       ;; Braces are hidden in sub/super groups — close enough to assume
       ;; every brace is a hidden grapheme. Slightly aggressive but math
       ;; lines typically only have braces in conceal'd positions.
       (loop (+ i 1) visual)]
      [(char=? (string-ref line i) #\})
       (loop (+ i 1) visual)]
      [else
       (loop (+ i 1) (+ visual 1))])))

;;@doc
;; Attempt to parse a `\\frac{num}{den}` (also `\\dfrac`, `\\tfrac`) at
;; position `i` of `line`. Returns `(num-text . den-text . after-pos)`
;; or #f if not a frac-style command.
(define (scan-frac-at line i)
  (cond
    [(not (char=? (string-ref line i) #\\)) #false]
    [else
     (define name-end (scan-backslash-name line (+ i 1)))
     (define name (substring line (+ i 1) name-end))
     (cond
       [(not (member name '("frac" "dfrac" "tfrac"))) #false]
       [(>= name-end (string-length line)) #false]
       [(not (char=? (string-ref line name-end) #\{)) #false]
       [else
        (define num-close (find-matching-brace line (+ name-end 1)))
        (cond
          [(not num-close) #false]
          [(or (>= num-close (string-length line))
               (not (char=? (string-ref line num-close) #\{)))
           #false]
          [else
           (define den-close (find-matching-brace line (+ num-close 1)))
           (cond
             [(not den-close) #false]
             [else
              (list (substring line (+ name-end 1) (- num-close 1))
                    (substring line (+ num-close 1) (- den-close 1))
                    den-close)])])])]))

;;@doc
;; Scan for `\\frac` patterns. The fraction renders as a stack: the
;; numerator above the bar, the bar on the source line (the concealer
;; replaces `\\frac{…}{…}` with a slash char — we overwrite that column
;; with a horizontal bar), and the denominator below.
(define (scan-line-for-fractions line line-idx)
  (let loop ([i 0] [entries '()])
    (cond
      [(>= i (string-length line)) (reverse entries)]
      [else
       (define frac (scan-frac-at line i))
       (cond
         [(not frac) (loop (+ i 1) entries)]
         [else
          (define num (car frac))
          (define den (cadr frac))
          (define after (caddr frac))
          (define op-visual-col (visual-column-of line i))
          (define padding (spaces op-visual-col))
          (define above (string-append padding num))
          (define below (string-append padding den))
          (loop after
                (cons (list line-idx above below) entries))])])))

;;@doc
;; Given a source line and its 0-based index, scan for big-operator
;; patterns and return a list of `(line-idx above-line below-line)`
;; triples to stage. Column padding uses `visual-column-of` so limits
;; land under the concealed glyph rather than under the raw `\\cmd`
;; position.
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
                      (define op-visual-col (visual-column-of line i))
                      (define padding (spaces op-visual-col))
                      (define above-line
                        (if sup (string-append padding sup) #false))
                      (define below-line
                        (if sub (string-append padding sub) #false))
                      (loop after
                            (cons (list line-idx above-line below-line)
                                  entries))]))]
                [else (loop name-end entries)]))]))])))

;;@doc
;; True for ASCII letters. Steel's base env doesn't ship
;; `char-alphabetic?`, so we do the range check ourselves — good enough
;; for LaTeX command names, which are ASCII by convention.
(define (ascii-letter? ch)
  (let ([code (char->integer ch)])
    (or (and (>= code 65) (<= code 90))       ; A-Z
        (and (>= code 97) (<= code 122)))))   ; a-z

;;@doc
;; Bytewise scan past `\cmd` ASCII letters starting at `i`, returning
;; the position past the last letter. If `s[i]` isn't a letter we return
;; `i` unchanged so the caller can detect no-match.
(define (scan-backslash-name s i)
  (let loop ([j i])
    (cond
      [(>= j (string-length s)) j]
      [(ascii-letter? (string-ref s j)) (loop (+ j 1))]
      [else j])))

;;@doc
;; Whole-buffer pass: strip any existing math annotations, then walk
;; each `# `-prefixed line looking for big-operator patterns inside
;; math delimiters and stage the above/below strings.
(define (math-render-buffer)
  (try-clear-all-math-lines!)
  ;; Flip the scanner into "hide inline limits + \\frac" mode so the
  ;; concealer doesn't paint redundant inline renderings alongside the
  ;; stacked virtual rows we're about to register.
  (set-math-layout-hide #true)
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
        (define op-entries (scan-line-for-operators trimmed line-idx))
        (define frac-entries (scan-line-for-fractions trimmed line-idx))
        (stage-merged line-idx (append op-entries frac-entries)))
      (loop (+ line-idx 1)))))

;;@doc
;; Merge all entries for a single source line into one combined set of
;; above/below lists. Multiple big operators / fractions on one source
;; line each contribute a pre-padded string; they share the virtual-row
;; space so column positions don't collide.
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
;; user wants to revert to raw source display.
(define (math-render-clear)
  (try-clear-all-math-lines!)
  (set-math-layout-hide #false))

