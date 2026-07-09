;;; lsp-statusline.scm — Persistent statusline element reporting the LSP
;;; attached to the current Julia notebook buffer.
;;;
;;; nothelix's actions are LSP-agnostic (LanguageServer.jl, JETLS, …); this
;;; element only *reports* which server is currently driving the buffer so the
;;; user can see it at a glance. It renders only on the focused view's
;;; statusline (get-active-lsp-clients reflects the focused buffer) and only on
;;; `.jl` files — the Julia notebook context.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require "helix/components.scm")

;; Style the indicator to match the bar so it reads as a calm, always-there
;; element rather than an alert.
(define (lsp-status-style)
  (theme-scope-ref "ui.statusline"))

(define (julia-notebook-path? path)
  (and path (string-suffix? path ".jl")))

;; Names of the initialized LSP clients attached to the focused buffer.
(define (active-lsp-names)
  (filter
    string?
    (map (lambda (client)
           (if (lsp-client-initialized? client)
               (lsp-client-name client)
               #false))
         (get-active-lsp-clients))))

;; Render closure: (ViewID? bool?) -> (listof Span?)
;; NOTE: the fork's render bridge passes a *ViewId*, not a DocumentID (the cog
;; docstring is wrong — see components.rs render_custom_status), so convert it
;; via editor->doc-id like the rest of nothelix does. Empty on non-focused
;; views and non-Julia files so the indicator is scoped to the active notebook.
(define (julia-lsp-status-element view-id focused)
  (if (not focused)
      '()
      (let* ([doc-id (editor->doc-id view-id)]
             [path (and doc-id (editor-document->path doc-id))])
        (if (not (julia-notebook-path? path))
            '()
            (let ([names (active-lsp-names)]
                  [style (lsp-status-style)])
              (if (null? names)
                  (list (span " jl:no-lsp " style))
                  (list (span (string-append " jl:" (string-join names ",") " ")
                              style))))))))

;; Register at module load (config phase, like the keymaps in nothelix.scm).
(push-status-element! 'right (status-element julia-lsp-status-element))
