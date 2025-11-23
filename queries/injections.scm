; Inject Julia syntax highlighting into code cells
; This assumes Julia by default - can be extended for language detection
(code_cell
  (cell_content) @injection.content
  (#set! injection.language "julia")
  (#set! injection.include-children))

; Inject Markdown into markdown cells
(markdown_cell
  (cell_content) @injection.content
  (#set! injection.language "markdown")
  (#set! injection.include-children))

; Output is plain text for now
; Future: detect and highlight HTML, LaTeX, JSON, etc.
(output_section
  (output_content) @injection.content
  (#set! injection.language "text"))
