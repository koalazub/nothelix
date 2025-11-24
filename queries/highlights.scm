; Cell headers as special comments
[
  (code_cell_header)
  (markdown_cell_header)
  (output_header)
  (output_footer)
] @comment.block.documentation

; Execution count as constant (using field syntax)
(code_cell_header
  execution_count: (number) @constant.numeric)
