import Foundation

enum CodexConfig {
    static let provider = "peer_route"
    static func quoted(_ value: String) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .withoutEscapingSlashes
        let data = try encoder.encode(value)
        return String(decoding: data, as: UTF8.self)
    }

    // Locate TOML statements without rewriting unrelated values or comments.
    // Multiline strings remain unsupported; arrays and quoted table keys are preserved.
    private struct Statement {
        var start: Int
        var end: Int
        var path: [String]
        var header: Bool
        var arrayTable: Bool = false
    }
    private static func keyPath(_ raw: String) throws -> [String] {
        let chars = Array(raw), count = chars.count
        var index = 0, result: [String] = []
        func whitespace() { while index < count && chars[index].isWhitespace { index += 1 } }
        while true {
            whitespace()
            guard index < count else { throw RouteError.message("Unsupported TOML key. No changes were made.") }
            let start = index
            if chars[index] == "\"" || chars[index] == "'" {
                let quote = chars[index]; index += 1
                var closed = false
                while index < count {
                    if quote == "\"" && chars[index] == "\\" { index += 2; continue }
                    if chars[index] == quote { index += 1; closed = true; break }
                    index += 1
                }
                guard closed, index <= count else { throw RouteError.message("Unclosed TOML key. No changes were made.") }
                let token = String(chars[start..<index])
                if quote == "'" { result.append(String(token.dropFirst().dropLast())) }
                else {
                    guard let decoded = try? JSONDecoder().decode(String.self, from: Data(token.utf8)) else {
                        throw RouteError.message("Unsupported escape in TOML key. No changes were made.")
                    }
                    result.append(decoded)
                }
            } else {
                while index < count && (chars[index].isASCII && (chars[index].isLetter || chars[index].isNumber || chars[index] == "_" || chars[index] == "-")) { index += 1 }
                guard index > start else { throw RouteError.message("Unsupported TOML key. No changes were made.") }
                result.append(String(chars[start..<index]))
            }
            whitespace()
            if index == count { return result }
            guard chars[index] == "." else { throw RouteError.message("Unsupported TOML key. No changes were made.") }
            index += 1
        }
    }
    private static func statements(_ lines: [String]) throws -> [Statement] {
        var result: [Statement] = [], current = "", first = 0
        var depth = 0, quote: Character?, escaped = false
        for (lineIndex, line) in lines.enumerated() {
            if current.isEmpty { first = lineIndex }
            var clean = ""
            for char in line {
                if let active = quote {
                    clean.append(char)
                    if active == "\"" && escaped { escaped = false }
                    else if active == "\"" && char == "\\" { escaped = true }
                    else if char == active { quote = nil }
                } else {
                    if char == "#" { break }
                    clean.append(char)
                    if char == "\"" || char == "'" { quote = char }
                    else if char == "[" || char == "{" { depth += 1 }
                    else if char == "]" || char == "}" { depth -= 1 }
                    guard depth >= 0 else { throw RouteError.message("Unbalanced TOML value. No changes were made.") }
                }
            }
            guard quote == nil else { throw RouteError.message("Multiline strings require a manual configuration merge. No changes were made.") }
            current += clean + "\n"
            if depth > 0 { continue }
            let text = current.trimmingCharacters(in: .whitespacesAndNewlines)
            current = ""
            if text.isEmpty { continue }
            if text.hasPrefix("[") {
                let array = text.hasPrefix("[["), width = array ? 2 : 1
                guard text.hasSuffix(array ? "]]" : "]") else { throw RouteError.message("Invalid TOML table. No changes were made.") }
                let raw = String(text.dropFirst(width).dropLast(width))
                result.append(Statement(start: first, end: lineIndex, path: try keyPath(raw), header: true, arrayTable: array))
            } else {
                // Find '=' outside a quoted key; values may contain any number of '='.
                var active: Character?, escape = false, split: String.Index?
                for index in text.indices {
                    let char = text[index]
                    if let q = active {
                        if q == "\"" && escape { escape = false }
                        else if q == "\"" && char == "\\" { escape = true }
                        else if char == q { active = nil }
                    } else if char == "\"" || char == "'" { active = char }
                    else if char == "=" { split = index; break }
                }
                guard let split else { throw RouteError.message("Unsupported TOML statement. No changes were made.") }
                result.append(Statement(start: first, end: lineIndex, path: try keyPath(String(text[..<split])), header: false))
            }
        }
        guard depth == 0 else { throw RouteError.message("Unclosed TOML array or table. No changes were made.") }
        return result
    }

    static func render(original: String, port: Int, key: String, model: String, turnEndpoint: Bool = false) throws -> String {
        guard !original.contains("\"\"\""), !original.contains("'''"), !original.contains("\r"),
              !key.isEmpty, !model.isEmpty, (1024...65535).contains(port) else {
            throw RouteError.message("This configuration needs a manual merge, or the model/client key is missing. No changes were made.")
        }
        let lines = original.components(separatedBy: "\n")
        let entries = try statements(lines), headers = entries.filter { $0.header }
        let rootEnd = headers.first?.start ?? lines.count
        let roots = entries.filter { !$0.header && $0.start < rootEnd }
        var context: [String] = []
        for entry in entries {
            if entry.header { context = entry.path }
            let fullPath = entry.header ? entry.path : context + entry.path
            guard !fullPath.starts(with: ["model_providers", provider]),
                  !(entry.header && fullPath.starts(with: ["features", "api_key_model_discovery"])) else {
                throw RouteError.message("An existing Peer Route provider or conflicting discovery table requires a manual merge. No changes were made.")
            }
        }
        guard !roots.contains(where: { ["model", "model_provider"].contains($0.path.first ?? "") && $0.path.count != 1 }),
              !roots.contains(where: { ["features", "model_providers"].contains($0.path.first ?? "") }) else {
            throw RouteError.message("An existing Peer Route provider or inline/dotted provider/features setting requires a manual merge. No changes were made.")
        }
        var replacements: [Int: String] = [:], insertions: [Int: [String]] = [:]
        for (name, value) in [("model_provider", try quoted(provider)), ("model", try quoted(model))] {
            let matches = roots.filter { $0.path == [name] }
            guard matches.count <= 1, matches.allSatisfy({ $0.start == $0.end }) else {
                throw RouteError.message("Duplicate or multiline \(name) setting requires a manual merge. No changes were made.")
            }
            if let entry = matches.first { replacements[entry.start] = "\(name) = \(value)" }
            else { insertions[rootEnd, default: []].append("\(name) = \(value)") }
        }
        let features = headers.filter { $0.path == ["features"] }
        guard features.count <= 1, features.allSatisfy({ !$0.arrayTable }) else { throw RouteError.message("Invalid or duplicate features table. No changes were made.") }
        if let feature = features.first {
            let end = headers.first { $0.start > feature.start }?.start ?? lines.count
            let settings = entries.filter { !$0.header && $0.start > feature.start && $0.start < end && $0.path.first == "api_key_model_discovery" }
            guard settings.count <= 1, settings.allSatisfy({ $0.path.count == 1 && $0.start == $0.end }) else { throw RouteError.message("Ambiguous model discovery setting. No changes were made.") }
            if let setting = settings.first { replacements[setting.start] = "api_key_model_discovery = \(!turnEndpoint)" }
            else { insertions[feature.end + 1, default: []].append("api_key_model_discovery = \(!turnEndpoint)") }
        }
        var output: [String] = []
        for index in 0...lines.count {
            output += insertions[index] ?? []
            if index < lines.count { output.append(replacements[index] ?? lines[index]) }
        }
        if features.isEmpty { output += ["", "[features]", "api_key_model_discovery = \(!turnEndpoint)"] }
        output += ["", "# Managed by Peer Route. Quit Peer Route to block requests; restore to remove.",
            "[model_providers.peer_route]", "name = \"Peer Route\"",
            "base_url = \"http://127.0.0.1:\(port)/v1\""]
        if !turnEndpoint { output.append("model_catalog_url = \"http://127.0.0.1:\(port)/v1/models\"") }
        output += ["experimental_bearer_token = \(try quoted(key))", "wire_api = \"responses\"",
            "requires_openai_auth = \(!turnEndpoint)", "supports_websockets = \(!turnEndpoint)", ""]
        return output.joined(separator: "\n")
    }

    struct Receipt: Codable { var original: Data?; var installed: Data }

    static func install(port: Int, key: String, model: String, home: URL = FileManager.default.homeDirectoryForCurrentUser,
                        receiptDirectory: URL = Files.directory, turnEndpoint: Bool = false) throws {
        try Files.prepare(receiptDirectory)
        let receiptPath = receiptDirectory.appendingPathComponent("codex-restore.json")
        guard !FileManager.default.fileExists(atPath: receiptPath.path) else {
            throw RouteError.message("Restore the previous Codex configuration before installing another route.")
        }
        let folder = home.appendingPathComponent(".codex")
        let path = folder.appendingPathComponent("config.toml")
        if let type = try? FileManager.default.attributesOfItem(atPath: path.path)[.type] as? FileAttributeType,
           type != .typeRegular { throw RouteError.message("Refusing to replace a linked or non-regular config file.") }
        let original = FileManager.default.fileExists(atPath: path.path) ? try Data(contentsOf: path) : nil
        let text: String
        if let original {
            guard let decoded = String(data: original, encoding: .utf8) else { throw RouteError.message("Codex configuration is not UTF-8.") }
            text = decoded
        } else { text = "" }
        let installed = Data(try render(original: text, port: port, key: key, model: model, turnEndpoint: turnEndpoint).utf8)
        try Files.prepare(folder)
        try Files.privateWrite(JSONEncoder().encode(Receipt(original: original, installed: installed)), to: receiptPath)
        try Files.privateWrite(installed, to: path)
    }

    static func restore(home: URL = FileManager.default.homeDirectoryForCurrentUser, receiptDirectory: URL = Files.directory) throws {
        let receiptPath = receiptDirectory.appendingPathComponent("codex-restore.json")
        let receipt = try JSONDecoder().decode(Receipt.self, from: Data(contentsOf: receiptPath))
        let path = home.appendingPathComponent(".codex/config.toml")
        let current = try Data(contentsOf: path)
        guard current == receipt.installed || current == receipt.original else {
            throw RouteError.message("Codex settings changed after installation. Your backup is preserved; merge it manually before restoring.")
        }
        if let original = receipt.original { try Files.privateWrite(original, to: path) }
        else { try FileManager.default.removeItem(at: path) }
        try FileManager.default.removeItem(at: receiptPath)
    }
}
