import XCTest
@testable import PeerRoute

final class ChatGPTTraceTests: XCTestCase {
    private func log(_ events: [[String: Any]]) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "constants": ["logEventTypes": ["URL_REQUEST_START_JOB": 1, "HTTP_TRANSACTION_READ_RESPONSE_HEADERS": 2, "URL_REQUEST_JOB_BYTES_READ": 3, "HTTP2_SESSION_RECV_HEADERS": 4, "WEBSOCKET_ALIVE": 5, "WEBSOCKET_SENT_FRAME_HEADER": 6, "WEBSOCKET_RECV_FRAME_HEADER": 7]],
            "events": events,
            "clientInfo": ["private": "DO_NOT_EXPORT"]
        ])
    }
    private func event(_ type: Int, _ id: Int, _ params: [String: Any]) -> [String: Any] {
        ["type": type, "source": ["id": id, "type": 123], "params": params, "time": "PRIVATE_TIMESTAMP"]
    }
    func testMetadataAndCredentialRedaction() throws {
        let input = try log([
            event(1, 42, ["url": "https://chatgpt.com/backend-api/f/conversation?token=SECRET_QUERY", "method": "POST", "headers": ["Authorization: Bearer SECRET_AUTH"], "body": "SECRET_PROMPT"]),
            event(2, 42, ["headers": ["HTTP/1.1 200 OK", "Content-Type: text/event-stream; charset=utf-8", "Set-Cookie: SECRET_COOKIE", "X-Secret: SECRET_HEADER"]]),
            event(3, 42, ["bytes": "SECRET_BYTES", "byte_count": 123]),
            event(1, 99, ["url": "https://chatgpt.com/backend-api/files/SECRET_FILE/download?sig=SECRET_SIGNATURE", "method": "GET"]),
            event(2, 99, ["headers": "HTTP/2 200\r\ncontent-type: image/png\r\nSet-Cookie: SECRET_COOKIE"]),
            event(1, 100, ["url": "https://PRIVATE_TENANT.oaiusercontent.com/SECRET_FILENAME.png", "method": "GET"]),
            event(1, 101, ["url": "https://bank.example/SECRET_BANK", "method": "GET"])
        ])
        let result = try ChatGPTTrace.parse(input)
        XCTAssertEqual(result.requests.count, 3)
        XCTAssertEqual(result.requests[0].path, "/backend-api/f/conversation")
        XCTAssertEqual(result.requests[0].method, "POST")
        XCTAssertEqual(result.requests[0].status, 200)
        XCTAssertEqual(result.requests[0].responseKind, "SSE")
        XCTAssertEqual(result.requests[0].outcome, "Response bytes observed")
        XCTAssertEqual(result.requests[1].path, "/backend-api/files/:redacted/download")
        XCTAssertEqual(result.requests[1].responseKind, "Image")
        XCTAssertEqual(result.requests[2].host, "*.oaiusercontent.com")
        let exported = String(decoding: try JSONEncoder().encode(result), as: UTF8.self)
        for forbidden in ["SECRET", "PRIVATE", "bank.example", "Authorization", "Cookie", "byte_count", "42", "99"] {
            XCTAssertFalse(exported.contains(forbidden), forbidden)
        }
    }
    func testNoCrossRequestOrHTTP2SessionStatusGuessing() throws {
        let result = try ChatGPTTrace.parse(log([
            event(1, 10, ["url": "https://chatgpt.com/backend-api/f/conversation", "method": "POST"]),
            event(4, 900, ["headers": [":status: 200", "content-type: text/event-stream"]]),
            event(2, 900, ["headers": ["HTTP/1.1 403 Forbidden"]]),
            event(1, 10, ["url": "https://other.example/private", "method": "GET"]),
            event(2, 10, ["headers": ["HTTP/1.1 401 Unauthorized"]]),
            event(1, 20, ["url": "wss://ws.chatgpt.com/ws?token=SECRET", "method": "GET"]),
            event(2, 20, ["headers": ["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket"]])
        ]))
        XCTAssertEqual(result.requests.count, 2)
        XCTAssertNil(result.requests[0].status)
        XCTAssertNil(result.requests[0].responseKind)
        XCTAssertEqual(result.requests[1].status, 101)
        XCTAssertEqual(result.requests[1].responseKind, "WebSocket upgrade")
    }
    func testEndpointFilteringAndMalformedLogs() throws {
        for value in ["http://chatgpt.com/private", "https://chatgpt.com.evil.example/private", "https://chatgpt.com:8443/private", "garbage"] {
            XCTAssertNil(ChatGPTTrace.safeEndpoint(value))
        }
        XCTAssertEqual(ChatGPTTrace.safeEndpoint("https://chatgpt.com/backend-api/conversation/%53ECRET?x=SECRET")?.1,
                       "/backend-api/conversation/:redacted")
        XCTAssertThrowsError(try ChatGPTTrace.parse(Data("{unfinished".utf8)))
        XCTAssertThrowsError(try ChatGPTTrace.parse(Data("{}".utf8)))
        XCTAssertThrowsError(try ChatGPTTrace.parse(Data(repeating: 65, count: ChatGPTTrace.maxBytes + 1)))
    }
    func testRequestStartEndKeepsResponseCorrelation() throws {
        let result = try ChatGPTTrace.parse(log([
            event(1, 7, ["url": "https://chatgpt.com/backend-api/f/conversation/prepare", "method": "POST"]),
            event(1, 7, [:]), // URL_REQUEST_START_JOB END has no URL.
            event(2, 7, ["headers": ["HTTP/1.1 200 OK", "Content-Type: application/json"]]),
            event(3, 7, ["byte_count": 100])
        ]))
        XCTAssertEqual(result.requests.count, 1)
        XCTAssertEqual(result.requests[0].status, 200)
        XCTAssertEqual(result.requests[0].responseKind, "JSON")
        XCTAssertEqual(result.requests[0].outcome, "Response bytes observed")
    }

    func testWebSocketShapesExcludePayloadsAndUnrelatedSockets() throws {
        let result = try ChatGPTTrace.parse(log([
            event(5, 700, ["url": "wss://ws.chatgpt.com/celsius/ws/user/SECRET_USER?token=SECRET_TOKEN"]),
            event(5, 700, [:]), // ALIVE END must not discard source attribution.
            event(6, 700, ["opcode": 1, "payload_length": 300, "final": true, "masked": true, "payload": "SECRET_PROMPT", "headers": "SECRET_COOKIE"]),
            event(7, 700, ["opcode": 2, "payload_length": 20000, "final": false, "masked": false, "bytes": "SECRET_IMAGE"]),
            event(5, 800, ["url": "wss://bank.example/SECRET_ACCOUNT"]),
            event(7, 800, ["opcode": 1, "payload_length": 10, "payload": "SECRET_BANK"]),
            event(7, 900, ["opcode": 1, "payload_length": 30]), // No ALIVE source: keep separately as unattributed metadata.
            event(7, 700, ["opcode": 1, "payload_length": -1])
        ]))
        let sockets = try XCTUnwrap(result.webSockets)
        XCTAssertEqual(sockets.count, 1)
        XCTAssertEqual(sockets[0].path, "/celsius/ws/user/:redacted")
        XCTAssertEqual(sockets[0].frames.count, 2)
        XCTAssertEqual(result.unattributedFrames?.count, 1)
        XCTAssertEqual(sockets[0].frames[0].kind, "text")
        XCTAssertEqual(sockets[0].frames[0].sizeBand, "126 B-1 KB")
        XCTAssertEqual(sockets[0].frames[1].direction, "received")
        XCTAssertEqual(sockets[0].frames[1].kind, "binary")
        let export = String(decoding: try JSONEncoder().encode(result), as: UTF8.self)
        XCTAssertFalse(export.contains("SECRET"))
        XCTAssertFalse(export.contains("bank.example"))
        XCTAssertFalse(export.contains("700"))
    }

    func testMissingWebSocketURLIsExplicitlyUnattributedAndBounded() throws {
        let events = [event(5, 1, ["url": ""])] + (0..<5001).map { _ in event(7, 1, ["opcode": 9, "payload_length": 0, "payload": "SECRET"]) }
        let result = try ChatGPTTrace.parse(log(events))
        let socket = try XCTUnwrap(result.webSockets?.first)
        XCTAssertEqual(socket.host, "unattributed")
        XCTAssertTrue(socket.attribution.contains("not mapped"))
        XCTAssertEqual(socket.frames.count, 5000)
        XCTAssertTrue(result.truncated)
        XCTAssertEqual(socket.frames[0].kind, "ping")
        XCTAssertEqual(socket.frames[0].sizeBand, "empty")
    }

    func testCaptureCommandUsesDefaultModeAndShellQuoting() {
        let command = ChatGPTTrace.command(for: URL(fileURLWithPath: "/tmp/test 'quoted' $(literal)/trace.json"))
        XCTAssertTrue(command.contains("--net-log-capture-mode=Default"))
        XCTAssertTrue(command.contains("'\"'\"'quoted'\"'\"'"))
        XCTAssertFalse(command.contains("IncludeSensitive"))
        XCTAssertFalse(command.contains("Everything"))
        XCTAssertFalse(command.contains("ignore-certificate"))
        XCTAssertFalse(command.contains("remote-debugging"))
    }
}
