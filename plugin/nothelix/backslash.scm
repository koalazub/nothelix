;;; backslash.scm — Julia backslash-to-unicode symbol completion

(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require (prefix-in helix. "helix/commands.scm"))
(require "conceal.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          unicode-lookup
                          unicode-completions-for-prefix))

(provide julia-tab-complete
         unicode-lookup
         unicode-completions-for-prefix)

;; Text helpers

;;@doc
;; Return the text from line start up to the cursor as a string.
(define (line-text-before-cursor)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define pos (cursor-position))
  (define line-no (text.rope-char->line rope pos))
  (define line-start (text.rope-line->char rope line-no))
  (text.rope->string (text.rope->slice rope line-start pos)))

;;@doc
;; Return the Julia symbol name after the last backslash at the end of str, or #false.
(define (extract-backslash-word str)
  (define len (string-length str))
  (let loop ([i (- len 1)])
    (cond
      [(< i 0) #false]
      [(char=? (string-ref str i) #\\)
       (define word (substring str (+ i 1) len))
       (if (= (string-length word) 0)
           #false
           word)]
       [(let ([c (string-ref str i)])
          (or (and (char>=? c #\a) (char<=? c #\z))
              (and (char>=? c #\A) (char<=? c #\Z))
              (and (char>=? c #\0) (char<=? c #\9))
              (char=? c #\^)
              (char=? c #\_)
              (char=? c #\-)
              (char=? c #\/)))
        (loop (- i 1))]
      [else #false])))

;; Completion command

;;@doc
;; Bound to Tab in insert mode for .jl files: expand \<name> to its unicode char, else insert a tab.
(define (julia-tab-complete)
  (define before (line-text-before-cursor))
  (define word (extract-backslash-word before))
  (if (not word)
      (helix.static.insert_tab)
      (let ([unicode (unicode-lookup word)])
        (if (= (string-length unicode) 0)
            (helix.static.insert_tab)
            (let* ([before-len (string-length before)]
                   [word-len   (string-length word)]
                   [bs-col     (- before-len word-len 1)]
                   [cur-col    before-len])
              (helix.goto-column bs-col)
              (helix.goto-column cur-col #true)
              (helix.static.replace-selection-with unicode)
              (schedule-reconceal 50))))))
