;;; resume.scm — Cross-session notebook resume position.
;;;
;;; Persists the focused notebook's cursor anchor (cell ordinal, line offset,
;;; column) to ~/.local/share/nothelix/resume via the dylib, and restores it
;;; when the notebook is reopened. Best-effort: a missing or stale entry leaves
;;; the cursor at the top.

(require "cursor-restore.scm")
(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          resume-get
                          resume-set))

(provide save-resume-position!
         restore-resume-position!)

;;@doc
;; Capture the focused notebook's cursor anchor to the resume store.
(define (save-resume-position!)
  (define doc-id (editor->doc-id (editor-focus)))
  (define path (and doc-id (editor-document->path doc-id)))
  (when path
    (define anchor (compute-cursor-anchor doc-id))
    (resume-set path (list-ref anchor 0) (list-ref anchor 1) (list-ref anchor 2))))

;;@doc
;; Restore `doc-id`'s stored cursor anchor if one exists; no-op otherwise.
(define (restore-resume-position! doc-id)
  (define path (and doc-id (editor-document->path doc-id)))
  (when path
    (define stored (resume-get path))
    (when (> (string-length stored) 0)
      (define parts (string-split stored "\t"))
      (when (>= (length parts) 3)
        (define ord (string->number (list-ref parts 0)))
        (define off (string->number (list-ref parts 1)))
        (define col (string->number (list-ref parts 2)))
        (when (and ord off col)
          (move-cursor-to-anchor! doc-id ord off col))))))
