import XCTest
import Network
@testable import PeerRoute

private final class TurnFixture: URLProtocol {
    static let events = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\nevent: response.image_generation_call.partial_image\ndata: {\"type\":\"response.image_generation_call.partial_image\",\"partial_image_b64\":\"cHJldmlldw==\"}\n\nevent: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"result\":\"aW1hZ2U=\"}}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\"}\n\n"
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    private func bodyData() -> Data? {
        if let data = request.httpBody { return data }
        guard let stream = request.httpBodyStream else { return nil }
        stream.open(); defer { stream.close() }
        var result = Data(), buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = stream.read(&buffer, maxLength: buffer.count)
            if count < 0 { return nil }; if count == 0 { return result }
            result.append(contentsOf: buffer.prefix(count))
            if result.count > 65536 { return nil }
        }
    }
    override func startLoading() {
        guard request.url?.absoluteString == "https://chatgpt.com/backend-api/codex/responses",
              request.value(forHTTPHeaderField: "Authorization") == "Bearer host-token",
              request.value(forHTTPHeaderField: "ChatGPT-Account-Id") == "host-account",
              let body = bodyData(), let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
              let input = json["input"] as? [[String: Any]], !input.isEmpty else {
            client?.urlProtocol(self, didFailWithError: URLError(.userAuthenticationRequired)); return
        }
        if json["model"] as? String == "reject-fixture" {
            client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 400, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "application/json"])!, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(#"{"error":{"message":"private-prompt host-token unused-secret"}}"#.utf8))
            client?.urlProtocolDidFinishLoading(self); return
        }
        let model = json["model"] as? String ?? ""
        let mime = model == "mislabeled-fixture" ? "application/octet-stream" : "text/event-stream"
        let headers = model == "missing-mime-fixture" ? [:] : ["Content-Type": mime]
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers)!, cacheStoragePolicy: .notAllowed)
        let alternate: String?
        switch model {
        case "json-fixture": alternate = #"{"object":"response","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Buffered OK"}]}]}"#
        case "json-error-fixture": alternate = #"{"error":{"message":"private-prompt host-token"}}"#
        case "html-fixture": alternate = "<!doctype html><p>private-prompt host-token</p>"
        default: alternate = nil
        }
        if let alternate {
            for byte in alternate.utf8 { client?.urlProtocol(self, didLoad: Data([byte])) }
            client?.urlProtocolDidFinishLoading(self); return
        }
        // Deliberately split stream events and base64 across arbitrary network boundaries.
        for byte in Self.events.utf8 { client?.urlProtocol(self, didLoad: Data([byte])) }
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

final class TurnExecutorTests: XCTestCase {
    private let auth = Data(#"{"tokens":{"access_token":"host-token","refresh_token":"unused-secret","account_id":"host-account"},"OPENAI_API_KEY":null}"#.utf8)
    func testWireDetectionAndCompletedJSONValidation() throws {
        for prefix in [Data(), Data([0xef]), Data([0xef, 0xbb]), Data("  ev".utf8)] { XCTAssertEqual(TurnWire.detect(prefix), .pending) }
        XCTAssertEqual(TurnWire.detect(Data("\u{feff}data:".utf8)), .sse)
        XCTAssertEqual(TurnWire.detect(Data(": heartbeat".utf8)), .sse)
        XCTAssertEqual(TurnWire.detect(Data("  {".utf8)), .json)
        XCTAssertEqual(TurnWire.detect(Data("<html>".utf8)), .other)
        for body in [#"{"error":{"message":"no"}}"#, #"{"object":"response","status":"failed","output":[]}"#, #"{"object":"response","status":"in_progress","output":[]}"#] {
            XCTAssertThrowsError(try TurnWire.completedJSON(Data(body.utf8)))
        }
        let image = Data(#"{"object":"response","status":"completed","output":[{"type":"image_generation_call","result":"aW1hZ2U="}]}"#.utf8)
        let events = try TurnWire.completedJSON(image)
        XCTAssertEqual(events.count, 2)
        XCTAssertTrue(String(decoding: events[0], as: UTF8.self).contains("aW1hZ2U="))
        XCTAssertTrue(String(decoding: events[1], as: UTF8.self).contains("response.completed"))
    }
    func testOAuthAndRequestBoundaries() throws {
        let credentials = try CodexOAuth.load(auth)
        XCTAssertEqual(credentials.accountID, "host-account")
        XCTAssertThrowsError(try CodexOAuth.load(Data(#"{"OPENAI_API_KEY":"sk-no"}"#.utf8)))
        XCTAssertThrowsError(try CodexOAuth.load(Data(#"{"tokens":{"access_token":"bad\r\nheader","account_id":"x"}}"#.utf8)))
        let expiredPayload = Data(#"{"exp":1}"#.utf8).base64EncodedString().replacingOccurrences(of: "=", with: "")
        XCTAssertThrowsError(try CodexOAuth.load(JSONSerialization.data(withJSONObject: ["tokens": ["access_token": "x.\(expiredPayload).x", "account_id": "x"]])))
        let request = try TurnRequest.upstream(Data(#"{"model":"test","input":"hello","tools":[{"type":"image_generation"}],"store":true}"#.utf8), credentials: credentials)
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: request.httpBody!) as? [String: Any])
        let input = try XCTUnwrap(body["input"] as? [[String: Any]])
        XCTAssertEqual(input.first?["role"] as? String, "user")
        XCTAssertEqual((input.first?["content"] as? [[String: Any]])?.first?["text"] as? String, "hello")
        XCTAssertEqual(body["store"] as? Bool, false); XCTAssertEqual(body["stream"] as? Bool, true)
        XCTAssertFalse(String(decoding: request.httpBody!, as: UTF8.self).contains("host-token"))
        XCTAssertThrowsError(try TurnRequest.normalize(Data(#"{"model":"test","input":[],"previous_response_id":"other-account"}"#.utf8)))
        XCTAssertThrowsError(try TurnRequest.normalize(Data(#"{"model":"test","input":[],"tools":[{"type":"computer"}]}"#.utf8)))
        XCTAssertNil(TurnRequest.contentLength(Data("POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n".utf8)))
        XCTAssertNil(TurnRequest.contentLength(Data("POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n".utf8)))
        XCTAssertFalse(TurnRequest.authorized(Data("POST /v1/responses HTTP/1.1\r\nAuthorization: Bearer key\r\nAuthorization: Bearer key\r\n\r\n".utf8), key: "key"))
    }
    func testLiteTextAndOrdinaryImageRequestShapes() throws {
        let credentials = try CodexOAuth.load(auth)
        let text = try TurnRequest.upstream(Data(#"{"model":"gpt-5.6-luna","input":"hello","instructions":"Be concise"}"#.utf8), credentials: credentials)
        XCTAssertEqual(text.value(forHTTPHeaderField: "x-openai-internal-codex-responses-lite"), "true")
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: text.httpBody!) as? [String: Any])
        XCTAssertTrue(body["tools"] is NSNull)
        XCTAssertEqual((body["reasoning"] as? [String: Any])?["context"] as? String, "all_turns")
        let input = try XCTUnwrap(body["input"] as? [[String: Any]])
        XCTAssertEqual(input.first?["type"] as? String, "additional_tools")
        XCTAssertEqual(input.last?["role"] as? String, "user")
        let image = try TurnRequest.upstream(Data(#"{"model":"gpt-5.6-luna","input":"hello","tools":[{"type":"image_generation"}]}"#.utf8), credentials: credentials)
        XCTAssertNil(image.value(forHTTPHeaderField: "x-openai-internal-codex-responses-lite"))
        let imageBody = try XCTUnwrap(JSONSerialization.jsonObject(with: image.httpBody!) as? [String: Any])
        XCTAssertEqual((imageBody["tools"] as? [[String: Any]])?.first?["type"] as? String, "image_generation")
        XCTAssertThrowsError(try TurnRequest.normalize(Data(#"{"model":"x","input":" "}"#.utf8)))
        XCTAssertThrowsError(try TurnRequest.normalize(Data(#"{"model":"x","input":[]}"#.utf8)))
    }
    func testErrorsOnlyExposeFixedCodesAndStatuses() throws {
        XCTAssertTrue(TurnFailure.httpMessage(503).contains("choose auth.json"))
        XCTAssertTrue(TurnFailure.httpMessage(407).contains("Update Peer Route"))
        XCTAssertTrue(TurnFailure.httpMessage(401).contains("Pairing"))
        XCTAssertNil(TurnFailure.decode(Data(#"{"error":{"code":"private-token","message":"private-prompt"}}"#.utf8)))
        let failure = TurnFailure(code: .rejected, upstreamStatus: 400)
        XCTAssertEqual(TurnFailure.decode(failure.json)?.upstreamStatus, 400)
        XCTAssertTrue(TurnFailure.httpMessage(502, body: failure.json).contains("HTTP 400"))
    }
    func testTurnCodexConfigurationAndHistory() throws {
        let rendered = try CodexConfig.render(original: "sandbox_mode = \"workspace-write\"\n", port: 18322, key: "paired-key", model: "test", turnEndpoint: true)
        XCTAssertTrue(rendered.contains("supports_websockets = false"))
        XCTAssertTrue(rendered.contains("requires_openai_auth = false"))
        XCTAssertTrue(rendered.contains("api_key_model_discovery = false"))
        XCTAssertTrue(rendered.contains("sandbox_mode = \"workspace-write\""))
        XCTAssertFalse(rendered.contains("model_catalog_url"))
        let normalized = try TurnRequest.normalize(Data(#"{"model":"test","input":[{"type":"reasoning","id":"rs_old","encrypted_content":"opaque"}],"tools":[{"type":"function","name":"local_tool"}]}"#.utf8))
        let text = String(decoding: normalized, as: UTF8.self)
        XCTAssertTrue(text.contains("opaque")); XCTAssertFalse(text.contains("rs_old"))
    }
    func testNetworkOnlyHostReportsMissingAccountWithoutGeneration() async throws {
        guard #available(macOS 15.0, *) else { throw XCTSkip("Hosting requires macOS 15") }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let (identity, pin) = try RelayTLS.identity(directory: dir)
        let port = UInt16.random(in: 53000...54999), localPort = Int.random(in: 55000...56999)
        let host = DesktopRelayHost()
        let ready = expectation(description: "network-only host")
        try host.start(identity: identity, key: "pair-key", port: port, bindHost: "127.0.0.1") { if $0 { ready.fulfill() } }
        defer { host.stop() }
        await fulfillment(of: [ready], timeout: 5)
        let bridge = try TLSBridge(host: "127.0.0.1", port: Int(port), fingerprint: pin)
        try bridge.start(localPort: localPort) {}; defer { bridge.stop() }
        try await Task.sleep(for: .milliseconds(200))
        let config = URLSessionConfiguration.ephemeral; config.connectionProxyDictionary = [:]; config.timeoutIntervalForRequest = 5
        let client = URLSession(configuration: config); defer { client.invalidateAndCancel() }
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(localPort)/peer-route-status")!)
        request.setValue("Bearer pair-key", forHTTPHeaderField: "Authorization")
        let (data, response) = try await client.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
        let status = try JSONDecoder().decode(TurnPeerStatus.self, from: data)
        XCTAssertEqual(status.appVersion, TurnPeerStatus.currentVersion)
        XCTAssertEqual(status.failure?.code, .noAccount)
        request.url = URL(string: "http://127.0.0.1:\(localPort)/v1/responses")!
        request.httpMethod = "POST"; request.httpBody = Data(#"{"model":"fixture","input":"hello"}"#.utf8)
        let (_, rejected) = try await client.data(for: request)
        XCTAssertEqual((rejected as? HTTPURLResponse)?.statusCode, 503)
    }
    func testPinnedPeerExecutesWithHostCredentialsAndStreamsImages() async throws {
        guard #available(macOS 15.0, *) else { throw XCTSkip("Hosting requires macOS 15") }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Files.prepare(dir); defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("auth.json"); try Files.privateWrite(auth, to: file)
        let config = URLSessionConfiguration.ephemeral; config.protocolClasses = [TurnFixture.self]
        let executor = TurnExecutor(authFile: file, expectedAccountID: "host-account", configuration: config)
        let (identity, pin) = try RelayTLS.identity(directory: dir.appendingPathComponent("tls"))
        let hostPort = UInt16.random(in: 49000...50999), localPort = Int.random(in: 51000...52999)
        let host = DesktopRelayHost(executor: executor)
        let ready = expectation(description: "host ready")
        try host.start(identity: identity, key: "pair-key", port: hostPort, bindHost: "127.0.0.1") { if $0 { ready.fulfill() } }
        defer { host.stop() }
        await fulfillment(of: [ready], timeout: 5)
        let bridge = try TLSBridge(host: "127.0.0.1", port: Int(hostPort), fingerprint: pin)
        try bridge.start(localPort: localPort) {}; defer { bridge.stop() }
        try await Task.sleep(for: .milliseconds(200))
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(localPort)/v1/responses")!)
        request.httpMethod = "POST"; request.httpBody = Data(#"{"model":"fixture","input":"hello","tools":[{"type":"image_generation"}]}"#.utf8)
        request.setValue("Bearer pair-key", forHTTPHeaderField: "Authorization")
        let clientConfig = URLSessionConfiguration.ephemeral; clientConfig.connectionProxyDictionary = [:]; clientConfig.timeoutIntervalForRequest = 10
        let client = URLSession(configuration: clientConfig); defer { client.invalidateAndCancel() }
        var statusRequest = URLRequest(url: URL(string: "http://127.0.0.1:\(localPort)/peer-route-status")!)
        statusRequest.setValue("Bearer pair-key", forHTTPHeaderField: "Authorization")
        let (readyStatus, readyResponse) = try await client.data(for: statusRequest)
        XCTAssertEqual((readyResponse as? HTTPURLResponse)?.statusCode, 200)
        XCTAssertNil(try JSONDecoder().decode(TurnPeerStatus.self, from: readyStatus).failure)
        let (data, response) = try await client.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
        XCTAssertEqual(String(decoding: data, as: UTF8.self), TurnFixture.events)
        XCTAssertFalse(data.contains(Data("host-token".utf8)))
        // The actual pinned TLS path must preserve a split SSE body even when
        // Content-Type is missing or incorrect, and accept only completed JSON.
        for model in ["mislabeled-fixture", "missing-mime-fixture", "json-fixture", "json-error-fixture", "html-fixture"] {
            var probe = request
            probe.httpBody = try JSONSerialization.data(withJSONObject: ["model": model, "input": "hello"])
            let (body, response) = try await client.data(for: probe)
            if model == "json-error-fixture" || model == "html-fixture" {
                XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 502, model)
                XCTAssertNotNil(TurnFailure.decode(body), model)
                XCTAssertFalse(String(decoding: body, as: UTF8.self).contains("private-prompt"))
                XCTAssertFalse(String(decoding: body, as: UTF8.self).contains("host-token"))
            } else {
                XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, model)
                let result = String(decoding: body, as: UTF8.self)
                if model == "json-fixture" {
                    XCTAssertTrue(result.contains("Buffered OK"))
                    XCTAssertTrue(result.contains("response.output_item.done"))
                    XCTAssertTrue(result.contains("response.completed"))
                    XCTAssertFalse(result.contains("response.output_text.delta"))
                } else { XCTAssertEqual(result, TurnFixture.events, model) }
            }
        }
        var rejectedRequest = request
        rejectedRequest.httpBody = Data(#"{"model":"reject-fixture","input":"hello"}"#.utf8)
        let (failureBody, failureResponse) = try await client.data(for: rejectedRequest)
        XCTAssertEqual((failureResponse as? HTTPURLResponse)?.statusCode, 502)
        XCTAssertEqual(TurnFailure.decode(failureBody)?.upstreamStatus, 400)
        XCTAssertFalse(String(decoding: failureBody, as: UTF8.self).contains("private-prompt"))
        XCTAssertFalse(String(decoding: failureBody, as: UTF8.self).contains("host-token"))
        request.setValue("Bearer wrong-key", forHTTPHeaderField: "Authorization")
        let (_, denied) = try await client.data(for: request)
        XCTAssertEqual((denied as? HTTPURLResponse)?.statusCode, 401)
        // A changed account must not silently inherit an existing pairing's active route.
        try Files.privateWrite(Data(#"{"tokens":{"access_token":"host-token","account_id":"different-account"}}"#.utf8), to: file)
        request.setValue("Bearer pair-key", forHTTPHeaderField: "Authorization")
        let (_, changed) = try await client.data(for: request)
        XCTAssertEqual((changed as? HTTPURLResponse)?.statusCode, 502)
        let (changedStatus, _) = try await client.data(for: statusRequest)
        XCTAssertEqual(try JSONDecoder().decode(TurnPeerStatus.self, from: changedStatus).failure?.code, .changedAccount)
        // Reload every turn: removing the host file must prevent future generation.
        try FileManager.default.removeItem(at: file)
        request.setValue("Bearer pair-key", forHTTPHeaderField: "Authorization")
        let (error, unavailable) = try await client.data(for: request)
        XCTAssertEqual((unavailable as? HTTPURLResponse)?.statusCode, 502)
        XCTAssertFalse(String(decoding: error, as: UTF8.self).contains(file.path))
        XCTAssertEqual(TurnFailure.decode(error)?.code, .unavailableAuth)
        let (missingStatus, _) = try await client.data(for: statusRequest)
        XCTAssertEqual(try JSONDecoder().decode(TurnPeerStatus.self, from: missingStatus).failure?.code, .unavailableAuth)
    }
}
