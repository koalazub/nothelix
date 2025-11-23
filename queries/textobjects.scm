; Text objects for cell navigation and selection
; Enables motions like 'ac' (around cell) and 'ic' (inside cell)

; Code cells
(code_cell) @class.around
(code_cell
  (cell_content) @class.inside)

; Markdown cells
(markdown_cell) @class.around
(markdown_cell
  (cell_content) @class.inside)

; Output sections
(output_section) @class.around
(output_section
  (output_content) @class.inside)
