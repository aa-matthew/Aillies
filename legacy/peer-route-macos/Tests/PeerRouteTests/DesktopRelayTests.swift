import XCTest
import Network
@testable import PeerRoute

private final class RelayEcho: @unchecked Sendable {
    let queue = DispatchQueue(label: "relay-test-echo")
    let listener: NWListener
    var connections: [NWConnection] = []
    init(port: UInt16) throws { listener = try NWListener(using: .tcp, on: NWEndpoint.Port(rawValue: port)!) }
    func start(ready: @escaping @Sendable () -> Void) {
        listener.stateUpdateHandler = { if case .ready = $0 { ready() } }
        listener.newConnectionHandler = { [self] c in
            connections.append(c); c.start(queue: queue); pump(c)
        }
        listener.start(queue: queue)
    }
    private func pump(_ c: NWConnection) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, done, error in
            if error != nil { c.cancel(); return }
            c.send(content: data, contentContext: done ? .finalMessage : .defaultMessage, isComplete: true, completion: .contentProcessed { error in
                if done || error != nil { c.cancel() } else { self?.pump(c) }
            })
        }
    }
    func stop() { queue.sync { listener.cancel(); connections.forEach { $0.cancel() }; connections.removeAll() } }
}

private final class RelayTestClient: @unchecked Sendable {
    let queue = DispatchQueue(label: "relay-test-client")
    var connection: NWConnection?
    var result = Data()
    var finished = false
    func exchange(port: UInt16, input: Data, expected: Int, onHeader: (() -> Void)? = nil, untilEOF: Bool = false, halfClose: Bool = false) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                let c = NWConnection(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: port)!, using: .tcp)
                connection = c
                func complete(_ error: Error? = nil) {
                    guard !finished else { return }; finished = true; c.stateUpdateHandler = nil; c.cancel(); connection = nil
                    if let error { continuation.resume(throwing: error) } else { continuation.resume(returning: result) }
                }
                var headerNotified = false
                func receive() {
                    c.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, done, error in
                        if let data { self.result.append(data) }
                        if !headerNotified, self.result.range(of: Data("\r\n\r\n".utf8)) != nil { headerNotified = true; onHeader?() }
                        if !untilEOF && self.result.count >= expected { complete() }
                        else if done { complete() }
                        else if let error { complete(error) }
                        else { receive() }
                    }
                }
                c.stateUpdateHandler = { s in
                    if case .ready = s {
                        c.send(content: input, contentContext: halfClose ? .finalMessage : .defaultMessage, isComplete: true, completion: .contentProcessed { error in
                            if let error { complete(error) } else { receive() }
                        })
                    } else if case let .failed(error) = s { complete(error) }
                }
                c.start(queue: queue)
                queue.asyncAfter(deadline: .now() + 12) { complete(RouteError.message("Relay test timed out: received \(self.result.count) bytes, expected \(expected), EOF \(untilEOF)")) }
            }
        }
    }
}

final class DesktopRelayTests: XCTestCase {
    func testPairingAndAuthorizationValidation() throws {
        let file = DesktopRelayPairing(name: "Test Mac", host: "test.local", fingerprint: String(repeating: "a", count: 64), accessKey: String(repeating: "b", count: 64))
        XCTAssertEqual(try file.peer().port, 18444)
        var bad = file; bad.kind = "codex"; XCTAssertThrowsError(try bad.peer())
        bad = file; bad.accessKey = "bad\r\nkey"; XCTAssertThrowsError(try bad.peer())
        XCTAssertTrue(RelayProtocol.compatible(RelayProtocol.handshake(nil, key: "secret")))
        XCTAssertFalse(RelayProtocol.compatible(Data("HTTP/1.1 200 OK\r\nPeer-Route-Protocol: unknown\r\n\r\n".utf8)))
        XCTAssertTrue(RelayProtocol.authorized(RelayProtocol.handshake(nil, key: "secret"), key: "secret"))
        XCTAssertFalse(RelayProtocol.authorized(RelayProtocol.handshake(nil, key: "secret"), key: "wrong"))
        XCTAssertFalse(RelayProtocol.authorized(Data("GET / HTTP/1.1\r\nProxy-Authorization: Bearer secret\r\nProxy-Authorization: Bearer secret\r\n\r\n".utf8), key: "secret"))
    }
    func testEncryptedRelayAndFailureBoundaries() async throws {
        guard #available(macOS 15.0, *) else { throw XCTSkip("Memory-only relay identity requires macOS 15") }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let (identity, pin) = try RelayTLS.identity(directory: dir)
        let (reloaded, samePin) = try RelayTLS.identity(directory: dir)
        _ = reloaded
        XCTAssertEqual(samePin, pin)
        let hostPort = UInt16.random(in: 40000...42999), localPort = UInt16.random(in: 43000...45999), echoPort = UInt16.random(in: 46000...48999)
        let policy: (ChatGPTProbe.Destination) -> Bool = { $0.host == "127.0.0.1" && $0.port == echoPort }
        let echo = try RelayEcho(port: echoPort)
        let echoReady = expectation(description: "echo ready"); echo.start { echoReady.fulfill() }
        defer { echo.stop() }
        let host = DesktopRelayHost(policy: policy)
        let hostReady = expectation(description: "host ready")
        try host.start(identity: identity, key: "test-relay-key", port: hostPort, bindHost: "127.0.0.1") { if $0 { hostReady.fulfill() } }
        defer { host.stop() }
        await fulfillment(of: [echoReady, hostReady], timeout: 5)
        let peer = Peer(name: "Test peer", alias: "127.0.0.1", port: Int(hostPort), fingerprint: pin)
        let gateway = try DesktopRelayGateway(peer: peer, key: "test-relay-key", policy: policy)
        let healthy = await gateway.check(); XCTAssertTrue(healthy)
        let wrongKey = try DesktopRelayGateway(peer: peer, key: "wrong-key", policy: policy)
        let badAuth = await wrongKey.check(); XCTAssertFalse(badAuth)
        var badPeer = peer; badPeer.fingerprint = String(repeating: "0", count: 64)
        let wrongPin = try DesktopRelayGateway(peer: badPeer, key: "test-relay-key", policy: policy)
        let badTLS = await wrongPin.check(); XCTAssertFalse(badTLS)
        let localReady = expectation(description: "gateway ready")
        try gateway.start(port: localPort) { if $0 { localReady.fulfill() } }
        defer { gateway.stop() }
        await fulfillment(of: [localReady], timeout: 5)
        // Early binary bytes after CONNECT must survive both parsers and multiple 64 KiB pumps.
        let payload = Data((0..<200000).map { UInt8($0 % 251) })
        let header = Data("CONNECT 127.0.0.1:\(echoPort) HTTP/1.1\r\nHost: local\r\n\r\n".utf8)
        let responseHeader = Data("HTTP/1.1 200 Connection Established\r\n\r\n".utf8)
        let result = try await RelayTestClient().exchange(port: localPort, input: header + payload, expected: responseHeader.count + payload.count, untilEOF: true, halfClose: true)
        XCTAssertEqual(result, responseHeader + payload)
        let rejected = try await RelayTestClient().exchange(port: localPort, input: Data("CONNECT unrelated.example:443 HTTP/1.1\r\n\r\n".utf8), expected: 1)
        XCTAssertTrue(rejected.starts(with: Data("HTTP/1.1 403 ".utf8)))
        let badLocalPort = UInt16.random(in: 49000...50999)
        let badReady = expectation(description: "wrong-key gateway ready")
        try wrongKey.start(port: badLocalPort) { if $0 { badReady.fulfill() } }
        defer { wrongKey.stop() }
        await fulfillment(of: [badReady], timeout: 5)
        let authRejected = try await RelayTestClient().exchange(port: badLocalPort, input: header, expected: 1)
        XCTAssertTrue(authRejected.starts(with: Data("HTTP/1.1 502 ".utf8)))
        // Host enforces its own scope even if a paired client tries to bypass local policy.
        let loose = try DesktopRelayGateway(peer: peer, key: "test-relay-key", policy: { _ in true })
        let loosePort = UInt16.random(in: 51000...52999)
        let looseReady = expectation(description: "loose gateway ready")
        try loose.start(port: loosePort) { if $0 { looseReady.fulfill() } }
        defer { loose.stop() }
        await fulfillment(of: [looseReady], timeout: 5)
        let hostRejected = try await RelayTestClient().exchange(port: loosePort, input: Data("CONNECT unrelated.example:443 HTTP/1.1\r\n\r\n".utf8), expected: 1)
        XCTAssertTrue(hostRejected.starts(with: Data("HTTP/1.1 502 ".utf8)))
        // A live tunnel must end when sharing stops, without falling back to the direct echo destination.
        let closed = try await RelayTestClient().exchange(port: localPort, input: header, expected: 1, onHeader: { host.stop() }, untilEOF: true)
        XCTAssertEqual(closed, responseHeader)
        let unavailable = await gateway.check(); XCTAssertFalse(unavailable)
    }
}
