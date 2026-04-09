;;; backslash.scm - Julia backslash-to-unicode symbol completion
;;;
;;; Type `\alpha` then press Tab to insert α, `\in` → ∈, `\pi` → π, etc.
;;; The symbol table is the same one used by the Julia REPL and Pluto.jl
;;; (extracted from Julia stdlib/REPL/src/latex_symbols.jl, ~2544 entries).
;;;
;;; Binding: Tab in insert mode for .jl files calls `julia-tab-complete`.
;;; If the text immediately before the cursor ends with `\<name>` and `<name>`
;;; is a known Julia symbol, the entire `\<name>` span is replaced with the
;;; corresponding Unicode character.  Otherwise Tab falls through to normal
;;; indentation.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          unicode-lookup
                          unicode-completions-for-prefix))

(provide julia-tab-complete
         unicode-lookup
         unicode-completions-for-prefix)

;; ─── Text helpers ─────────────────────────────────────────────────────────────

;;@doc
;; Return the text from the start of the current line up to (not including)
;; the cursor position, as a plain string.
(define (line-text-before-cursor)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (define line-no (text.rope-char->line rope pos))
  (define line-start (text.rope-line->char rope line-no))
  (text.rope->string (text.rope->slice rope line-start pos)))

;;@doc
;; Scan backwards through STR looking for the last backslash followed by a
;; valid Julia symbol name.  Returns the name (without the backslash) if found
;; at the very end of STR, or #false otherwise.
;;
;; Valid name characters: letters, digits, ^, _, -, /
;; (Covers regular names like "alpha", superscripts like "^2", subscripts
;; like "_beta", and fraction shorthands like "1/2".)
(define (extract-backslash-word str)
  (define len (string-length str))
  (let loop ([i (- len 1)])
    (cond
      ;; Ran off the left end without finding a backslash — no match.
      [(< i 0) #false]

      ;; Found the backslash — everything to its right is the candidate name.
      [(char=? (string-ref str i) #\\)
       (define word (substring str (+ i 1) len))
       (if (= (string-length word) 0)
           #false
           word)]

       ;; Keep scanning left if this is a valid symbol-name character.
       [(let ([c (string-ref str i)])
          (or (and (char>=? c #\a) (char<=? c #\z))
              (and (char>=? c #\A) (char<=? c #\Z))
              (and (char>=? c #\0) (char<=? c #\9))
              (char=? c #\^)
              (char=? c #\_)
              (char=? c #\-)
              (char=? c #\/)))
        (loop (- i 1))]

      ;; Any other character (space, paren, operator…) — stop, no match.
      [else #false])))

;; ─── Completion command ───────────────────────────────────────────────────────

;;@doc
;; Bound to Tab in insert mode for .jl files.
;;
;; Checks whether the text immediately before the cursor ends with `\<name>`.
;; If `<name>` is a known Julia LaTeX symbol, replaces `\<name>` with the
;; corresponding Unicode character and stays in insert mode.
;; Falls through to normal tab insertion when there is no match.
(define (julia-tab-complete)
  (define before (line-text-before-cursor))
  (define word (extract-backslash-word before))
  (if (not word)
      ;; Nothing that looks like \name before the cursor.
      (helix.static.insert_tab)
      (let ([unicode (unicode-lookup word)])
        (if (= (string-length unicode) 0)
            ;; Name not in the symbol table.
            (helix.static.insert_tab)
            ;; Replace \<name> with the unicode character.
            ;;
            ;; `before` runs from column 0 to the cursor.  The backslash is at
            ;; column  (- before-len word-len 1)  and the cursor is at column
            ;; before-len.  We move the cursor back to the backslash column,
            ;; then extend the selection forward to the cursor, then replace.
            (let* ([before-len (string-length before)]
                   [word-len   (string-length word)]
                   [bs-col     (- before-len word-len 1)]
                   [cur-col    before-len])
              (helix.goto-column bs-col)
              (helix.goto-column cur-col #true)
              (helix.static.replace-selection-with unicode))))))
