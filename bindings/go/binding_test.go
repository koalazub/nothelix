package tree_sitter_notebook_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_notebook "github.com/koalazub/nothelix/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_notebook.Language())
	if language == nil {
		t.Errorf("Error loading Notebook grammar")
	}
}
