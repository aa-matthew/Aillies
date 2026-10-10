import XCTest
import Foundation
import CryptoKit
@testable import PeerRoute

final class TransportTests: XCTestCase {
    func testPinnedTLSAndClientAuthenticationAgainstBundledEngine() async throws {
        let project = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let binary = project.appendingPathComponent("vendor/cli-proxy-api")
        guard FileManager.default.isExecutableFile(atPath: binary.path) else { throw XCTSkip("Fetch the pinned proxy engine to run the transport integration test.") }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Files.prepare(directory)
        defer { try? FileManager.default.removeItem(at: directory) }
        func run(_ args: [String]) throws {
            let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/bin/openssl"); p.arguments = args
            p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice
            try p.run(); p.waitUntilExit(); XCTAssertEqual(p.terminationStatus, 0)
        }
        let cert = directory.appendingPathComponent("cert.pem"), key = directory.appendingPathComponent("key.pem"), der = directory.appendingPathComponent("cert.der")
        try run(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key.path, "-out", cert.path, "-days", "1", "-subj", "/CN=Peer Route test"])
        try run(["x509", "-in", cert.path, "-outform", "DER", "-out", der.path])
        let digest = SHA256.hash(data: try Data(contentsOf: der)).map { String(format: "%02x", $0) }.joined()
        let peerPort = Int.random(in: 24000...28000), localPort = Int.random(in: 29000...33000), badPort = Int.random(in: 34000...38000)
        let config = """
        config-version: 8
        server:
          host: "127.0.0.1"
          port: \(peerPort)
          tls:
            enable: true
            cert: \(try CodexConfig.quoted(cert.path))
            key: \(try CodexConfig.quoted(key.path))
        management:
          secret-key: ""
          disable-control-panel: true
        access:
          api-keys: ["transport-test-key"]
        oauth:
          auth-dir: \(try CodexConfig.quoted(directory.appendingPathComponent("oauth").path))
        """
        let path = directory.appendingPathComponent("config.yaml")
        try Data(config.utf8).write(to: path)
        let core = Process(); core.executableURL = binary; core.arguments = ["-config", path.path, "-local-model"]
        core.currentDirectoryURL = directory
        core.standardOutput = FileHandle.nullDevice; core.standardError = FileHandle.nullDevice; core.standardInput = FileHandle.nullDevice
        try core.run()
        defer { if core.isRunning { core.terminate(); core.waitUntilExit() } }
        let bridge = try TLSBridge(host: "127.0.0.1", port: peerPort, fingerprint: digest)
        try bridge.start(localPort: localPort, onFailure: {})
        defer { bridge.stop() }
        let sessionConfig = URLSessionConfiguration.ephemeral
        sessionConfig.timeoutIntervalForRequest = 2; sessionConfig.connectionProxyDictionary = [:]
        let session = URLSession(configuration: sessionConfig)
        defer { session.invalidateAndCancel() }
        func get(port: Int, key: String) async throws -> (Data, URLResponse) {
            var request = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/v1/models")!)
            request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
            return try await session.data(for: request)
        }
        var status: Int?
        for _ in 0..<10 {
            try await Task.sleep(for: .milliseconds(300))
            if let response = try? await get(port: localPort, key: "transport-test-key") {
                status = (response.1 as? HTTPURLResponse)?.statusCode
                if status == 200 { break }
            }
        }
        XCTAssertTrue(core.isRunning)
        XCTAssertEqual(status, 200, "Pinned TLS should reach the real engine without installing a system certificate")
        let rejected = try await get(port: localPort, key: "wrong-key")
        XCTAssertEqual((rejected.1 as? HTTPURLResponse)?.statusCode, 401)
        let invalid = try TLSBridge(host: "127.0.0.1", port: peerPort, fingerprint: String(repeating: "0", count: 64))
        try invalid.start(localPort: badPort, onFailure: {})
        defer { invalid.stop() }
        try await Task.sleep(for: .milliseconds(200))
        do {
            _ = try await get(port: badPort, key: "transport-test-key")
            XCTFail("A mismatched certificate must not accept an HTTP request")
        } catch { /* Expected TLS pin rejection. */ }
    }
}
