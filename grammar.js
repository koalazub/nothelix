/**
 * Tree-sitter grammar for notebook cell format
 * Used by Nothelix plugin for Helix editor
 *
 * Parses notebook cells with headers like:
 *   # ─── Code Cell [1] ───
 *   # ─── Markdown Cell ───
 *   # ─── Output ───
 */

module.exports = grammar({
  name: 'notebook',

  extras: $ => [/[ \t]/],

  rules: {
    source_file: $ => seq(
      repeat($.blank_line),
      repeat(
        choice(
          $.code_cell,
          $.markdown_cell,
          $.output_section
        )
      )
    ),

    // Code Cell
    code_cell: $ => prec.right(seq(
      $.code_cell_header,
      optional($.cell_content)
    )),

    code_cell_header: $ => seq(
      '#',
      /─+/,
      'Code Cell',
      optional(seq(
        '[',
        field('execution_count', choice($.number, /\s+/)),
        ']'
      )),
      /─+/,
      '\n'
    ),

    // Markdown Cell
    markdown_cell: $ => prec.right(seq(
      $.markdown_cell_header,
      optional($.cell_content)
    )),

    markdown_cell_header: $ => seq(
      '#',
      /─+/,
      'Markdown Cell',
      /─+/,
      '\n'
    ),

    // Output Section
    output_section: $ => prec.right(seq(
      $.output_header,
      optional($.output_content),
      $.output_footer
    )),

    output_header: $ => seq(
      '#',
      /─+/,
      'Output',
      /─+/,
      '\n'
    ),

    output_footer: $ => seq(
      '#',
      /─+/,
      '\n'
    ),

    // Cell content - everything until next cell marker or output section
    cell_content: $ => repeat1(
      choice(
        $.content_line,
        $.blank_line
      )
    ),

    content_line: $ => seq(
      choice(
        /[^#\n][^\n]*/,      // Lines not starting with #
        /#[^─ \t\n][^\n]*/,  // # followed by non-dash/non-space (##, #A, etc.)
        /#[ \t]+[^─\n][^\n]*/ // # + spaces + non-dash
      ),
      '\n'
    ),

    // Output content - everything until footer
    output_content: $ => repeat1(
      choice(
        $.output_line,
        $.blank_line
      )
    ),

    output_line: $ => seq(
      choice(
        /[^#\n][^\n]*/,      // Lines not starting with #
        /#[^─ \t\n][^\n]*/,  // # followed by non-dash/non-space (##, #A, etc.)
        /#[ \t]+[^─\n][^\n]*/ // # + spaces + non-dash
      ),
      '\n'
    ),

    // Helpers
    blank_line: $ => /\n/,

    number: $ => /\d+/,
  }
});
