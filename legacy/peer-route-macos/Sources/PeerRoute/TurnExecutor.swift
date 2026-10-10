import Foundation
import Network

struct CodexOAuth {
    let accessToken: String
    let accountID: String
    static func claims(_ token: String) -> [String: Any] {
        let parts = token.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3 else { return [:] }
        var value = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        value += String(repeating: "=", count: (4 - value.count % 4) % 4)
        guard let data = Data(base64Encoded: value), let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [:] }
        return object
    }
    static func load(_ data: Data, now: Date = Date()) throws -> CodexOAuth {
        guard data.count <= 65536, let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              root["OPENAI_API_KEY"] == nil || root["OPENAI_API_KEY"] is NSNull,
              let tokens = root["tokens"] as? [String: Any], let access = tokens["access_token"] as? String,
              !access.isEmpty, access.count < 16384, !access.contains(where: { $0.isWhitespace }) else {
            throw TurnFailure(code: .invalidAuth)
        }
        let accessClaims = claims(access)
        if let exp = accessClaims["exp"] as? Double, exp <= now.timeIntervalSince1970 + 30 {
            throw TurnFailure(code: .expiredAuth)
        }
        let idClaims = claims(tokens["id_token"] as? String ?? "")
        let claimAccount = (accessClaims["https://api.openai.com/auth"] as? [String: Any])?["chatgpt_account_id"] as? String
            ?? (idClaims["https://api.openai.com/auth"] as? [String: Any])?["chatgpt_account_id"] as? String
        guard let account = tokens["account_id"] as? String ?? claimAccount,
              !account.isEmpty, account.count <= 200,
              account.range(of: "^[A-Za-z0-9_-]+$", options: .regularExpression) != nil,
              claimAccount == nil || claimAccount == account else { throw TurnFailure(code: .invalidAuth) }
        return CodexOAuth(accessToken: access, accountID: account)
    }
    static func read(_ url: URL) throws -> CodexOAuth {
        let active = url.startAccessingSecurityScopedResource(); defer { if active { url.stopAccessingSecurityScopedResource() } }
        guard (try url.resourceValues(forKeys: [.fileSizeKey])).fileSize ?? Int.max <= 65536 else { throw RouteError.message("auth.json is too large.") }
        return try load(Data(contentsOf: url))
    }
}

enum TurnRequest {
    static let maximumBytes = 16 * 1024 * 1024
    // Codex text transport categories inspected in 9router ce4460e. Image requests
    // retain its image adapter's ordinary Responses-tool shape.
    static let liteModels: Set<String> = ["gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"]
    static func usesLite(_ body: [String: Any]) -> Bool {
        let tools = body["tools"] as? [[String: Any]] ?? []
        return liteModels.contains(body["model"] as? String ?? "") && !tools.contains { $0["type"] as? String == "image_generation" }
    }
    static func normalize(_ data: Data) throws -> Data {
        guard data.count <= maximumBytes, var body = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let model = body["model"] as? String, !model.isEmpty, model.count <= 100,
              body["input"] is String || body["input"] is [Any],
              body["previous_response_id"] == nil, body["conversation"] == nil else {
            throw RouteError.message("Send a model and full turn history in input. Server conversation references are unsupported.")
        }
        if let tools = body["tools"] as? [[String: Any]] {
            guard tools.count <= 128, tools.allSatisfy({ ["image_generation", "function", "custom"].contains($0["type"] as? String ?? "") }) else {
                throw RouteError.message("Unsupported hosted tool. Text, image generation and client-executed function tools are supported.")
            }
        } else if body["tools"] != nil && !(body["tools"] is NSNull) { throw RouteError.message("Invalid tools.") }
        let allowed: Set<String> = ["model", "input", "instructions", "tools", "tool_choice", "reasoning", "text", "include", "parallel_tool_calls", "stream", "store", "service_tier", "prompt_cache_key", "client_metadata"]
        guard Set(body.keys).isSubset(of: allowed) else { throw RouteError.message("Unsupported turn field.") }
        if let text = body["input"] as? String {
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw RouteError.message("Input cannot be empty.") }
            body["input"] = [["type": "message", "role": "user", "content": [["type": "input_text", "text": text]]]]
        }
        guard let items = body["input"] as? [[String: Any]], !items.isEmpty else { throw RouteError.message("Input must contain message items.") }
        if let input = body["input"] as? [[String: Any]] {
            guard !input.contains(where: { $0["type"] as? String == "item_reference" }) else { throw RouteError.message("Include full history instead of item references.") }
            body["input"] = input.map { item -> [String: Any] in
                var value = item
                if value["role"] as? String == "system" { value["role"] = "developer" }
                if let id = value["id"] as? String, ["rs_", "fc_", "resp_", "msg_"].contains(where: { id.hasPrefix($0) }) { value.removeValue(forKey: "id") }
                return value
            }
        }
        body["stream"] = true; body["store"] = false
        if body["instructions"] == nil { body["instructions"] = "" }
        if usesLite(body), var input = body["input"] as? [[String: Any]] {
            if !input.contains(where: { $0["type"] as? String == "additional_tools" }) {
                var prefix: [[String: Any]] = [["type": "additional_tools", "role": "developer", "tools": body["tools"] as? [[String: Any]] ?? []]]
                if let instructions = body["instructions"] as? String, !instructions.isEmpty {
                    prefix.append(["type": "message", "role": "developer", "content": [["type": "input_text", "text": instructions]]])
                }
                input.insert(contentsOf: prefix, at: 0)
            }
            body["input"] = input; body["instructions"] = ""; body["tools"] = NSNull()
            body["parallel_tool_calls"] = false
            if body["tool_choice"] == nil { body["tool_choice"] = "auto" }
            var reasoning = body["reasoning"] as? [String: Any] ?? ["effort": "medium"]
            reasoning.removeValue(forKey: "summary"); reasoning["context"] = "all_turns"
            body["reasoning"] = reasoning
        }
        return try JSONSerialization.data(withJSONObject: body)
    }
    static func upstream(_ data: Data, credentials: CodexOAuth) throws -> URLRequest {
        var request = URLRequest(url: URL(string: "https://chatgpt.com/backend-api/codex/responses")!)
        request.httpMethod = "POST"; request.httpBody = try normalize(data); request.timeoutInterval = 120
        if let body = try JSONSerialization.jsonObject(with: data) as? [String: Any], usesLite(body) {
            request.setValue("true", forHTTPHeaderField: "x-openai-internal-codex-responses-lite")
        }
        request.setValue("Bearer " + credentials.accessToken, forHTTPHeaderField: "Authorization")
        request.setValue(credentials.accountID, forHTTPHeaderField: "ChatGPT-Account-Id")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.setValue("codex_cli_rs", forHTTPHeaderField: "originator")
        request.setValue("codex_cli_rs/0.159.0", forHTTPHeaderField: "User-Agent")
        request.setValue("0.159.0", forHTTPHeaderField: "version")
        request.setValue(UUID().uuidString, forHTTPHeaderField: "x-client-request-id")
        request.setValue(UUID().uuidString, forHTTPHeaderField: "session_id")
        return request
    }
    static func contentLength(_ header: Data) -> Int? {
        guard let text = String(data: header, encoding: .utf8) else { return nil }
        let lines = text.components(separatedBy: "\r\n")
        guard !lines.contains(where: { $0.lowercased().hasPrefix("transfer-encoding:") }) else { return nil }
        let values = lines.filter { $0.lowercased().hasPrefix("content-length:") }.map { $0.dropFirst(15).trimmingCharacters(in: .whitespaces) }
        guard values.count == 1, values[0].allSatisfy({ $0.isASCII && $0.isNumber }), let length = Int(values[0]), (1...maximumBytes).contains(length) else { return nil }
        return length
    }
    static func authorized(_ header: Data, key: String) -> Bool {
        guard let text = String(data: header, encoding: .utf8) else { return false }
        let values = text.components(separatedBy: "\r\n").filter { $0.lowercased().hasPrefix("authorization:") }.map { $0.dropFirst(14).trimmingCharacters(in: .whitespaces) }
        guard values.count == 1 else { return false }
        let a = Array(values[0].utf8), b = Array(("Bearer " + key).utf8)
        return !key.isEmpty && a.count == b.count && zip(a,b).reduce(UInt8(0)) { $0 | ($1.0 ^ $1.1) } == 0
    }
}

private final class NoTurnRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}

/// Only B opens the selected OAuth file. No prompt, token, image or response logging.
final class TurnExecutor: @unchecked Sendable {
    let authFile: URL
    private let expectedAccountID: String?
    private let session: URLSession
    init(authFile: URL, expectedAccountID: String? = nil, configuration: URLSessionConfiguration = .ephemeral) {
        self.authFile = authFile; self.expectedAccountID = expectedAccountID
        configuration.httpCookieStorage = nil; configuration.urlCache = nil
        configuration.httpShouldSetCookies = false; configuration.timeoutIntervalForResource = 900
        configuration.connectionProxyDictionary = [:]
        session = URLSession(configuration: configuration, delegate: NoTurnRedirects(), delegateQueue: nil)
    }
    deinit { session.invalidateAndCancel() }
    func credentials() throws -> CodexOAuth {
        let credentials: CodexOAuth
        do { credentials = try CodexOAuth.read(authFile) }
        catch let failure as TurnFailure { throw failure }
        catch { throw TurnFailure(code: .unavailableAuth) }
        guard expectedAccountID == nil || credentials.accountID == expectedAccountID else { throw TurnFailure(code: .changedAccount) }
        return credentials
    }
    func preflight() -> TurnFailure? {
        do { _ = try credentials(); return nil }
        catch { return (error as? TurnFailure) ?? TurnFailure(code: .unavailableAuth) }
    }
    func execute(_ data: Data, send: @escaping (Data) async throws -> Void) async throws {
        let request = try TurnRequest.upstream(data, credentials: credentials())
        let (bytes, response) = try await session.bytes(for: request)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
            let status = (response as? HTTPURLResponse)?.statusCode ?? 502
            throw TurnFailure(code: .rejected, upstreamStatus: status)
        }
        let header = Data("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n".utf8)
        var iterator = bytes.makeAsyncIterator(), prefix = Data()
        var format = TurnWire.Format.pending
        while prefix.count < 4096, let byte = try await iterator.next() {
            try Task.checkCancellation(); prefix.append(byte); format = TurnWire.detect(prefix)
            if format != .pending { break }
        }
        if format == .json {
            // JSON is buffered only when the server didn't send SSE. Keep the same
            // output bound and reject errors/HTML/arbitrary JSON as failed turns.
            var data = prefix
            while let byte = try await iterator.next() {
                try Task.checkCancellation()
                guard data.count < 64 * 1024 * 1024 else { throw TurnFailure(code: .invalidStream, upstreamStatus: 200) }
                data.append(byte)
            }
            let events: [Data]
            do { events = try TurnWire.completedJSON(data) }
            catch { throw TurnFailure(code: .jsonResponse, upstreamStatus: 200) }
            try await send(header)
            for event in events { try await send(event) }
            return
        }
        guard format == .sse else { throw TurnFailure(code: .invalidStream, upstreamStatus: 200) }
        // A valid SSE prefix is accepted even with a missing/incorrect MIME header,
        // matching 9router's Codex stream parser. Preserve all original bytes.
        try await send(header); try await send(prefix)
        var chunk = Data(), total = prefix.count
        while let byte = try await iterator.next() {
            try Task.checkCancellation(); chunk.append(byte); total += 1
            guard total <= 64 * 1024 * 1024 else { throw RouteError.message("Turn exceeded the 64 MiB limit.") }
            if byte == 10 || chunk.count == 16384 { try await send(chunk); chunk.removeAll(keepingCapacity: true) }
        }
        if !chunk.isEmpty { try await send(chunk) }
    }
}
