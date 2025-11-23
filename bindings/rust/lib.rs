//! Tree-sitter grammar for notebook cell format
//!
//! This crate provides Notebook language support for the tree-sitter parsing library.
//! It's designed for use with the Nothelix plugin for Helix editor.

use tree_sitter::Language;

extern "C" {
    fn tree_sitter_notebook() -> Language;
}

/// Get the tree-sitter Language for Notebook files.
///
/// # Example
/// ```
/// let language = tree_sitter_notebook::language();
/// let mut parser = tree_sitter::Parser::new();
/// parser.set_language(language).expect("Error loading Notebook grammar");
/// ```
pub fn language() -> Language {
    unsafe { tree_sitter_notebook() }
}

/// The content of the [`node-types.json`][] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The syntax highlighting query for this language.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

/// The injection query for this language.
pub const INJECTIONS_QUERY: &str = include_str!("../../queries/injections.scm");

/// The text objects query for this language.
pub const TEXTOBJECTS_QUERY: &str = include_str!("../../queries/textobjects.scm");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_load_grammar() {
        let language = language();
        assert_eq!(language.version(), tree_sitter::LANGUAGE_VERSION);
    }
}
