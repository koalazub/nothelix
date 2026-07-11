;;; math-format.scm - Expand single-line LaTeX block envs in notebook comments into multi-line $$ blocks.

(require "common.scm")
(require "conceal.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/ext.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "string-utils.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          format-math))

(provide format-math-buffer)

;;@doc
;; Rewrite single-line math environments in the current buffer into multi-line block form. Second optional arg commit? (default #true) controls whether the undo-history commit happens here or is deferred to the caller. Returns #true if the buffer was rewritten, #false otherwise.
(define (format-math-buffer . args)
  (define silent? (and (not (null? args)) (car args)))
  (define commit? (if (or (null? args) (null? (cdr args))) #true (cadr args)))
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (unless silent?
       (set-status! "format-math: only runs on .jl notebook files"))
     #false]
    [else
     (define rope (editor->text doc-id))
     (define doc-len (text.rope-len-chars rope))
     (define current-text (text.rope->string rope))
     (define rewritten (format-math current-text))

     (cond
       [(equal? current-text rewritten)
        (unless silent?
          (set-status! "format-math: no single-line math envs to expand"))
        #false]
       [else
        (define r (helix.static.range 0 doc-len))
        (define sel (helix.static.range->selection r))
        (helix.static.set-current-selection-object! sel)
        (helix.static.replace-selection-with rewritten)
        (helix.static.collapse_selection)
        (when commit? (helix.static.commit-changes-to-history))
        (schedule-reconceal 50)
        (set-status!
          (if silent?
              "math formatted (save again to flush to disk)"
              "format-math: expanded single-line math envs"))
        #true])]))
