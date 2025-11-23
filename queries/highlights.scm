; Cell headers as special comments
[
  (code_cell_header)
  (markdown_cell_header)
  (output_header)
  (output_footer)
] @comment.block.documentation

; Execution count as constant
(code_cell_header
  (execution_count) @constant.numeric)

; Cell markers (the ─── symbols) as punctuation
(code_cell_header
  (execution_count) @constant.numeric)

; "Code Cell", "Markdown Cell", "Output" as keywords
(code_cell_header) @keyword.directive
(markdown_cell_header) @keyword.directive
(output_header) @keyword.directive
