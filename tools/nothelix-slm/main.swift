import Foundation
import FoundationModels

let args = CommandLine.arguments

if args.contains("--probe") {
    switch SystemLanguageModel.default.availability {
    case .available:
        print("available")
        exit(0)
    case .unavailable(let reason):
        print("unavailable: \(reason)")
        exit(1)
    @unknown default:
        print("unavailable")
        exit(1)
    }
}

let input = String(data: FileHandle.standardInput.readDataToEndOfFile(), encoding: .utf8) ?? ""
let cells = input.split(separator: "\u{1e}", omittingEmptySubsequences: true)
let session = LanguageModelSession(
    instructions: "You label notebook cells for a picker menu. Reply with only a terse label of at most six words, lowercase, no trailing punctuation. Example: question 6: null spaces")
var out = ""
for cell in cells {
    do {
        let r = try await session.respond(to: "Label this cell:\n\(String(cell.prefix(2000)))")
        let clean = r.content
            .components(separatedBy: .newlines)
            .joined(separator: " ")
            .trimmingCharacters(in: .whitespaces)
        out += clean + "\n"
    } catch {
        out += "\n"
    }
}
FileHandle.standardOutput.write(out.data(using: .utf8)!)
