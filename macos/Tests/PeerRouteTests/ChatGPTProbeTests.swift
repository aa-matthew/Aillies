import XCTest
import Network
@testable import PeerRoute

private final class ProbeTestConnections: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [NWConnection] = []
    func append(_ connection: NWConnection) { lock.lock(); defer { lock.unlock() }; values.append(connection) }
    func cancelAll() { lock.lock(); let active = values; values = []; lock.unlock(); active.forEach { $0.cancel() } }
}

final class ChatGPTProbeTests: XCTestCase {
    func testConnectParsingAndScope() {
        XCTAssertEqual(ChatGPTProbe.parse(Data("CONNECT chatgpt.com:443 HTTP/1.1\r\n".utf8)), .incomplete)
        XCTAssertEqual(ChatGPTProbe.parse(Data("CONNECT CHATGPT.com:443 HTTP/1.1\r\nHost: chatgpt.com\r\n\r\nabc".utf8)),
                       .connect(.init(host: "chatgpt.com", port: 443), Data("abc".utf8)))
        for request in ["GET https://chatgpt.com/ HTTP/1.1", "CONNECT user@chatgpt.com:443 HTTP/1.1", "CONNECT chatgpt.com:99999 HTTP/1.1", "CONNECT chatgpt.com:443 HTTP/2", "CONNECT chatgpt.com:443/ HTTP/1.1"] {
            XCTAssertEqual(ChatGPTProbe.parse(Data((request + "\r\n\r\n").utf8)), .rejected)
        }
        XCTAssertEqual(ChatGPTProbe.parse(Data(repeating: 65, count: 16385)), .rejected)
        XCTAssertTrue(ChatGPTProbe.allowed(.init(host: "files.oaiusercontent.com", port: 443)))
        for host in ["127.0.0.1", "localhost", "chatgpt.com.attacker.example", "fakeopenai.com", "evil.example"] {
            XCTAssertFalse(ChatGPTProbe.allowed(.init(host: host, port: 443)))
        }
        XCTAssertFalse(ChatGPTProbe.allowed(.init(host: "chatgpt.com", port: 80)))
    }

    func testRawTunnelPreservesBytesAndRejectsOutOfScope() async throws {
        let queue = DispatchQueue(label: "probe-test")
        let echo = try NWListener(using: .tcp, on: .any)
        let ready = expectation(description: "echo listener")
        echo.stateUpdateHandler = { if case .ready = $0 { ready.fulfill() } }
        let connections = ProbeTestConnections()
        echo.newConnectionHandler = { connection in
            connections.append(connection)
            connection.start(queue: queue)
            func pump() {
                connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, done, error in
                    if error != nil || done { connection.cancel(); return }
                    connection.send(content: data, completion: .contentProcessed { error in
                        if error == nil { pump() } else { connection.cancel() }
                    })
                }
            }
            pump()
        }
        echo.start(queue: queue)
        defer { echo.cancel(); connections.cancelAll() }
        await fulfillment(of: [ready], timeout: 3)
        let echoPort = try XCTUnwrap(echo.port?.rawValue)
        let proxyPort = UInt16.random(in: 39000...44000)
        let probe = ChatGPTProbe(policy: { $0.host == "127.0.0.1" && $0.port == echoPort })
        let proxyReady = expectation(description: "proxy listener")
        try probe.start(port: proxyPort, event: { _, _ in }, state: { if $0 { proxyReady.fulfill() } })
        defer { probe.stop() }
        await fulfillment(of: [proxyReady], timeout: 3)
        func connect() async -> NWConnection {
            let connection = NWConnection(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: proxyPort)!, using: .tcp)
            let connected = expectation(description: "connected")
            connection.stateUpdateHandler = { if case .ready = $0 { connected.fulfill() } }
            connection.start(queue: queue)
            await fulfillment(of: [connected], timeout: 3)
            return connection
        }
        func send(_ data: Data, on connection: NWConnection) async throws {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                connection.send(content: data, completion: .contentProcessed { error in
                    if let error { continuation.resume(throwing: error) } else { continuation.resume() }
                })
            }
        }
        func receive(_ connection: NWConnection) async throws -> Data {
            try await withCheckedThrowingContinuation { continuation in
                connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, _, error in
                    if let error { continuation.resume(throwing: error) } else { continuation.resume(returning: data ?? Data()) }
                }
            }
        }
        let client = await connect(); defer { client.cancel() }
        try await send(Data("CONNECT 127.0.0.1:\(echoPort) HTTP/1.1\r\n".utf8), on: client)
        try await send(Data("Host: ignored\r\n\r\nEARLY".utf8), on: client)
        var handshake = Data()
        while handshake.count < 44 { handshake.append(try await receive(client)) }
        XCTAssertTrue(String(decoding: handshake, as: UTF8.self).hasPrefix("HTTP/1.1 200 Connection Established\r\n\r\nEARLY"))
        let bytes = Data((0..<120000).map { UInt8($0 % 256) })
        try await send(bytes, on: client)
        var received = Data()
        while received.count < bytes.count { received.append(try await receive(client)) }
        XCTAssertEqual(received, bytes)
        let rejected = await connect(); defer { rejected.cancel() }
        try await send(Data("CONNECT evil.example:443 HTTP/1.1\r\n\r\n".utf8), on: rejected)
        let response = try await receive(rejected)
        XCTAssertTrue(String(decoding: response, as: UTF8.self).hasPrefix("HTTP/1.1 403"))
    }
}
