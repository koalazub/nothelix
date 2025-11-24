import XCTest
import SwiftTreeSitter
import TreeSitterNotebook

final class TreeSitterNotebookTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_notebook())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Notebook grammar")
    }
}
