import Foundation
import AppKit

/// Only these derived fields can leave the importer. No raw NetLog event is retained.
struct APITraceRequest: Codable, Identifiable, Equatable {
    let id: Int
    var method: String
    var host: String
    var path: String
    var status: Int?
    var responseKind: String?
    var outcome: String = "Not observed"
}
struct WebSocketFrameShape: Codable, Equatable {
    let direction: String
    let kind: String
    let sizeBand: String
    let final: Bool?
    let masked: Bool?
}
struct WebSocketTrace: Codable, Identifiable {
    let id: Int
    let host: String
    let path: String
    var attribution: String = "URL observed"
    var frames: [WebSocketFrameShape] = []
}
struct APITraceReport: Codable {
    let formatVersion: Int
    let source: String
    let eventsExamined: Int
    let requests: [APITraceRequest]
    let truncated: Bool
    var webSockets: [WebSocketTrace]? = nil
    var unattributedFrames: [WebSocketFrameShape]? = nil
}

enum ChatGPTTrace {
    static let maxBytes = 128 * 1024 * 1024
    static func safeEndpoint(_ value: String) -> (String, String)? {
        guard value.count <= 16384, let url = URLComponents(string: value),
              ["https", "wss"].contains(url.scheme?.lowercased() ?? ""), let host = url.host?.lowercased(),
              ChatGPTProbe.allowed(.init(host: host, port: 443)), url.port == nil || url.port == 443 else { return nil }
        // Keep public service names, never arbitrary tenant/subdomain identifiers.
        let knownHosts: Set<String> = ["chatgpt.com", "chat.openai.com", "ws.chatgpt.com", "ab.chatgpt.com", "cdn.openai.com", "cdn.platform.openai.com", "cdn.auth0.com", "learn.chatgpt.com", "web-sandbox.oaiusercontent.com"]
        let safeHost = knownHosts.contains(host) ? host : ["chatgpt.com", "openai.com", "oaistatic.com", "oaiusercontent.com", "auth0.com"].first(where: { host == $0 || host.hasSuffix("." + $0) }).map { "*." + $0 }!
        // A finite vocabulary prevents conversation IDs, filenames, tokens, or encoded text leaking through paths.
        let vocabulary: Set<String> = ["backend-api", "api", "f", "conversation", "conversations", "prepare", "resume", "stop", "cancel", "models", "files", "file", "download", "upload", "uploaded", "content", "sentinel", "chat-requirements", "requirements", "accounts", "check", "me", "settings", "user", "users", "history", "library", "attachments", "images", "image", "generations", "generate", "audio", "speech", "transcriptions", "realtime", "v1", "v2", "ws", "bootstrap", "subscriptions", "capabilities", "config", "status", "auth", "session", "messaging", "rooms", "messages", "live", "read", "turns", "completions", "stream", "subscribe", "pubsub", "celsius", "ios", "attestation_challenge"]
        let components = url.percentEncodedPath.split(separator: "/", omittingEmptySubsequences: true)
        let safePath = "/" + components.prefix(24).map { vocabulary.contains(String($0)) ? String($0) : ":redacted" }.joined(separator: "/") + (components.count > 24 ? "/:truncated" : "")
        return (safeHost, safePath)
    }
    static func parse(_ data: Data) throws -> APITraceReport {
        guard data.count <= maxBytes else { throw RouteError.message("Trace exceeds 128 MB. Capture a shorter test and fully quit ChatGPT before importing.") }
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let events = root["events"] as? [[String: Any]],
              let constants = root["constants"] as? [String: Any],
              let types = constants["logEventTypes"] as? [String: Any] else {
            throw RouteError.message("Not a supported Chromium NetLog. Fully quit ChatGPT to finish writing the trace, then import again.")
        }
        var typeNames: [Int: String] = [:]
        for (name, value) in types { if let number = value as? Int { typeNames[number] = name } }
        var records: [APITraceRequest] = []
        var current: [Int: Int] = [:]
        var sockets: [WebSocketTrace] = []
        var socketSources: [Int: Int] = [:]
        var totalFrames = 0
        var unattributedFrames: [WebSocketFrameShape] = []
        var excludedSocketSources: Set<Int> = []
        var truncated = events.count > 200000
        for event in events.prefix(200000) {
            guard let type = event["type"] as? Int, let name = typeNames[type],
                  let source = event["source"] as? [String: Any], let sourceID = source["id"] as? Int else { continue }
            let params = event["params"] as? [String: Any] ?? [:]
            if name == "WEBSOCKET_ALIVE", let value = params["url"] as? String {
                socketSources.removeValue(forKey: sourceID)
                excludedSocketSources.remove(sourceID)
                let endpoint = value.isEmpty ? ("unattributed", "/:unknown") : safeEndpoint(value)
                if let (host, path) = endpoint {
                    if sockets.count < 100 {
                        socketSources[sourceID] = sockets.count
                        sockets.append(WebSocketTrace(id: sockets.count + 1, host: host, path: path, attribution: value.isEmpty ? "URL omitted by NetLog; app-wide socket, not mapped to an endpoint" : "URL observed"))
                    } else { truncated = true }
                } else { excludedSocketSources.insert(sourceID) }
            }
            if name == "WEBSOCKET_SENT_FRAME_HEADER" || name == "WEBSOCKET_RECV_FRAME_HEADER",
               !excludedSocketSources.contains(sourceID), let opcode = params["opcode"] as? Int,
               let length = params["payload_length"] as? Int, length >= 0 {
                if totalFrames < 5000 {
                    let kind = [0: "continuation", 1: "text", 2: "binary", 8: "close", 9: "ping", 10: "pong"][opcode] ?? "reserved"
                    let size = length == 0 ? "empty" : length <= 125 ? "1-125 B" : length <= 1024 ? "126 B-1 KB" : length <= 16384 ? "1-16 KB" : length <= 65536 ? "16-64 KB" : ">64 KB"
                    let shape = WebSocketFrameShape(direction: name == "WEBSOCKET_SENT_FRAME_HEADER" ? "sent" : "received", kind: kind, sizeBand: size, final: params["final"] as? Bool, masked: params["masked"] as? Bool)
                    if let index = socketSources[sourceID] { sockets[index].frames.append(shape) }
                    else { unattributedFrames.append(shape) }
                    totalFrames += 1
                } else { truncated = true }
            }
            if name == "URL_REQUEST_START_JOB", params["url"] is String {
                // Clear on an actual start/redirect, never the paired END event without a URL.
                current.removeValue(forKey: sourceID)
                guard let rawURL = params["url"] as? String, let (host, path) = safeEndpoint(rawURL) else { continue }
                guard records.count < 5000 else { truncated = true; continue }
                let method = params["method"] as? String ?? ""
                let index = records.count
                records.append(APITraceRequest(id: index + 1, method: ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].contains(method) ? method : "OTHER", host: host, path: path))
                current[sourceID] = index
            } else if let index = current[sourceID] {
                // Response headers on the same URL_REQUEST source only. Do not guess HTTP/2 session-to-request mappings.
                if name == "HTTP_TRANSACTION_READ_RESPONSE_HEADERS" || name == "HTTP_TRANSACTION_READ_HEADERS" {
                    let lines: [String]
                    if let array = params["headers"] as? [String] { lines = array }
                    else if let text = params["headers"] as? String { lines = text.components(separatedBy: "\n") }
                    else { lines = [] }
                    for line in lines {
                        let words = line.split(whereSeparator: { $0.isWhitespace })
                        if let first = words.first, first.hasPrefix("HTTP/"), words.count >= 2,
                           let code = Int(words[1]), (100...599).contains(code) {
                            records[index].status = code
                            if code == 101 { records[index].responseKind = "WebSocket upgrade" }
                        }
                        guard let colon = line.firstIndex(of: ":"), line[..<colon].lowercased() == "content-type" else { continue }
                        let mime = line[line.index(after: colon)...].split(separator: ";").first?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
                        if mime == "text/event-stream" { records[index].responseKind = "SSE" }
                        else if mime == "application/json" { records[index].responseKind = "JSON" }
                        else if ["image/png", "image/jpeg", "image/webp", "image/avif", "image/gif"].contains(mime) { records[index].responseKind = "Image" }
                        else if mime == "application/octet-stream" { records[index].responseKind = "Binary" }
                    }
                }
                if name == "URL_REQUEST_JOB_BYTES_READ" || name == "URL_REQUEST_JOB_FILTERED_BYTES_READ" {
                    records[index].outcome = "Response bytes observed"
                }
                if let error = params["net_error"] as? Int, error < 0 { records[index].outcome = "Network error" }
            }
        }
        return APITraceReport(formatVersion: 2, source: "Chromium NetLog; metadata allowlist; queries, identifiers, headers, bodies and timestamps omitted", eventsExamined: min(events.count, 200000), requests: records, truncated: truncated, webSockets: sockets, unattributedFrames: unattributedFrames)
    }
    static func shellQuote(_ value: String) -> String { "'" + value.replacingOccurrences(of: "'", with: "'\"'\"'") + "'" }
    static func command(for url: URL) -> String {
        "open -a ChatGPT --args " + shellQuote("--log-net-log=" + url.path) + " --net-log-capture-mode=Default"
    }
}

@MainActor final class ChatGPTTraceStore: ObservableObject {
    @Published var command = ""
    @Published var status = "Prepare a short local trace to inspect API metadata."
    @Published var report: APITraceReport?
    @Published var busy = false
    private var rawURL: URL?
    func prepare() {
        do {
            let directory = Files.directory.appendingPathComponent("private-traces", isDirectory: true)
            try Files.prepare(directory)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
            let url = directory.appendingPathComponent("chatgpt-\(UUID().uuidString).netlog.json")
            try Files.privateWrite(Data(), to: url)
            rawURL = url; command = ChatGPTTrace.command(for: url); report = nil
            status = "Command ready. Raw trace stays private on this Mac. Quit ChatGPT before launching."
        } catch { status = "Could not prepare private trace storage." }
    }
    func copyCommand() { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(command, forType: .string) }
    func importTrace() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        panel.title = "Choose a completed ChatGPT NetLog — processed only on this Mac"
        if let rawURL { panel.directoryURL = rawURL.deletingLastPathComponent(); panel.nameFieldStringValue = rawURL.lastPathComponent }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        busy = true; report = nil; status = "Sanitizing locally…"
        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey])
                    guard values.isRegularFile == true, values.isSymbolicLink != true, let size = values.fileSize, size <= ChatGPTTrace.maxBytes else { throw RouteError.message("Choose a regular NetLog file no larger than 128 MB.") }
                    let data = try Data(contentsOf: url)
                    return try ChatGPTTrace.parse(data)
                }.value
                report = result
                status = result.requests.isEmpty ? "No supported first-party request events found. This does not prove no traffic occurred." : "Sanitized \(result.requests.count) requests and \(result.webSockets?.count ?? 0) WebSockets. Frame shapes exclude payloads."
            } catch { status = "Import failed. Use a completed Chromium NetLog under 128 MB; fully quit ChatGPT before importing." }
            busy = false
        }
    }
    func exportReport() {
        guard let report else { return }
        let panel = NSSavePanel(); panel.nameFieldStringValue = "chatgpt-api-metadata.json"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try Files.privateWrite(encoder.encode(report), to: url)
            status = "Exported sanitized metadata. Raw NetLog was not copied."
        } catch { status = "Could not export sanitized metadata." }
    }
    func revealRawFolder() {
        if let rawURL { NSWorkspace.shared.activateFileViewerSelecting([rawURL]) }
    }
}
