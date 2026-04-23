;;; math-format.scm - Multi-line formatter for LaTeX block envs in notebook
;;; comments.
;;;
;;; Wraps the Rust `format-math` FFI in a buffer-rewriting command. Reads the
;;; document, asks Rust to expand any single-line `\begin{cases}...`,
;;; `\begin{pmatrix}...`, etc. into multi-line `$$` blocks, then replaces the
;;; buffer content in place. Idempotent — running it against an
;;; already-formatted buffer is a no-op.

(require "common.scm")
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
;; Rewrite single-line math environments in the current buffer into their
;; multi-line block form. Leaves prose and already-multi-line envs alone.
;;
;; `silent?` (optional, default #false) suppresses status-bar messages so
;; the save-hook invocation doesn't stomp on Helix's own "wrote N bytes"
;; notification. User-invoked calls (from the command palette) use the
;; default chatty behavior so the user gets feedback.
(define (format-math-buffer . args)
  (define silent? (and (not (null? args)) (car args)))
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (and path (string-suffix? path ".jl")))
     (unless silent?
       (set-status! "format-math: only runs on .jl notebook files"))]
    [else
     (define rope (editor->text doc-id))
     (define doc-len (text.rope-len-chars rope))
     (define current-text (text.rope->string rope))
     (define rewritten (format-math current-text))

     (cond
       [(equal? current-text rewritten)
        (unless silent?
          (set-status! "format-math: no single-line math envs to expand"))]
       [else
        (define r (helix.static.range 0 doc-len))
        (define sel (helix.static.range->selection r))
        (helix.static.set-current-selection-object! sel)
        (helix.static.replace-selection-with rewritten)
        (helix.static.collapse_selection)
        (helix.static.commit-changes-to-history)
        ;; Always confirm a real rewrite happened — even in silent (save-
        ;; hook) mode — so the user has a breadcrumb that their on-disk
        ;; copy will update on the next `:w`. Suppress only the
        ;; no-op/wrong-file-type noise, not the actual "I did a thing"
        ;; signal.
        (set-status!
          (if silent?
              "math formatted (save again to flush to disk)"
              "format-math: expanded single-line math envs"))])]))
