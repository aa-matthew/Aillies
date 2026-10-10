import Foundation
import Network
import Security
import CryptoKit

/// A network relay only. Inner TLS and application authentication remain end-to-end.
enum RelayTLS {
    static func options(pin: String, queue: DispatchQueue) -> NWProtocolTLS.Options {
        let options = NWProtocolTLS.Options()
        sec_protocol_options_set_min_tls_protocol_version(options.securityProtocolOptions, .TLSv12)
        sec_protocol_options_set_verify_block(options.securityProtocolOptions, { _, trust, complete in
            let value = sec_trust_copy_ref(trust).takeRetainedValue()
            guard let certificate = (SecTrustCopyCertificateChain(value) as? [SecCertificate])?.first else { complete(false); return }
            let digest = SHA256.hash(data: SecCertificateCopyData(certificate) as Data).map { String(format: "%02x", $0) }.joined()
            guard digest == pin.lowercased() else { complete(false); return }
            SecTrustSetPolicies(value, SecPolicyCreateBasicX509())
            SecTrustSetAnchorCertificates(value, [certificate] as CFArray)
            SecTrustSetAnchorCertificatesOnly(value, true)
            complete(SecTrustEvaluateWithError(value, nil))
        }, queue)
        return options
    }
    static func identity(directory: URL) throws -> (SecIdentity, String) {
        guard #available(macOS 15.0, *) else { throw RouteError.message("Hosting the desktop relay requires macOS 15 or later.") }
        try Files.prepare(directory)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
        let cert = directory.appendingPathComponent("certificate.pem"), key = directory.appendingPathComponent("private-key.pem")
        func run(_ arguments: [String], input: Data? = nil) throws {
            let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/bin/openssl"); p.arguments = arguments
            p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice
            let pipe = Pipe(); p.standardInput = pipe
            try p.run()
            if let input { try pipe.fileHandleForWriting.write(contentsOf: input) }
            try pipe.fileHandleForWriting.close(); p.waitUntilExit()
            guard p.terminationStatus == 0 else { throw RouteError.message("Could not prepare relay TLS identity.") }
        }
        if !FileManager.default.fileExists(atPath: cert.path) {
            try run(["req", "-x509", "-newkey", "rsa:2048", "-sha256", "-nodes", "-keyout", key.path, "-out", cert.path, "-days", "365", "-subj", "/CN=Peer Route Desktop Relay"])
        }
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: key.path)
        let p12 = directory.appendingPathComponent(UUID().uuidString + ".p12"), der = directory.appendingPathComponent(UUID().uuidString + ".der")
        defer { try? FileManager.default.removeItem(at: p12); try? FileManager.default.removeItem(at: der) }
        let password = UUID().uuidString
        // Password goes through stdin, never command arguments or logs.
        try run(["pkcs12", "-export", "-inkey", key.path, "-in", cert.path, "-out", p12.path, "-passout", "stdin"], input: Data((password + "\n").utf8))
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: p12.path)
        let data = try Data(contentsOf: p12)
        var result: CFArray?
        let status = SecPKCS12Import(data as CFData, [kSecImportExportPassphrase as String: password, kSecImportToMemoryOnly as String: true] as CFDictionary, &result)
        guard status == errSecSuccess, let entries = result as? [[String: Any]],
              let identity = entries.first?[kSecImportItemIdentity as String] else { throw RouteError.message("Could not load relay TLS identity.") }
        try run(["x509", "-in", cert.path, "-outform", "DER", "-out", der.path])
        let pin = SHA256.hash(data: try Data(contentsOf: der)).map { String(format: "%02x", $0) }.joined()
        return (identity as! SecIdentity, pin)
    }
}

/// Frames carry data and directional EOF inside TLS, avoiding TLS close-notify truncating the reverse stream.
private final class RelayPipe {
    let local: NWConnection, remote: NWConnection
    let localFramed: Bool
    let finished: () -> Void
    var stopped = false, ends = 0
    init(_ local: NWConnection, _ remote: NWConnection, localFramed: Bool, finished: @escaping () -> Void) {
        self.local = local; self.remote = remote; self.localFramed = localFramed; self.finished = finished
    }
    private var wire: NWConnection { localFramed ? local : remote }
    private var raw: NWConnection { localFramed ? remote : local }
    func start(trailing: Data = Data()) {
        local.stateUpdateHandler = { [weak self] s in if case .failed = s { self?.stop() } }
        remote.stateUpdateHandler = { [weak self] s in if case .failed = s { self?.stop() } }
        if localFramed { receiveFrame(buffer: trailing); pumpRaw() }
        else {
            sendFrame(trailing, eof: false) { [weak self] in self?.receiveFrame(); self?.pumpRaw() }
        }
    }
    private func sendFrame(_ data: Data, eof: Bool, complete: @escaping () -> Void) {
        guard !stopped, data.count <= 65536 else { stop(); return }
        if data.isEmpty && !eof { complete(); return }
        let n = UInt32(data.count)
        var frame = Data([eof ? 1 : 0, UInt8((n >> 24) & 255), UInt8((n >> 16) & 255), UInt8((n >> 8) & 255), UInt8(n & 255)])
        frame.append(data)
        wire.send(content: frame, completion: .contentProcessed { [weak self] error in
            guard let self, !self.stopped else { return }
            if error != nil { self.stop() } else { complete() }
        })
    }
    private func ended() { ends += 1; if ends == 2 { stop() } }
    private func pumpRaw() {
        raw.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, done, error in
            guard let self, !self.stopped else { return }
            if error != nil { self.stop(); return }
            self.sendFrame(data ?? Data(), eof: false) { [weak self] in
                guard let self, !self.stopped else { return }
                if done { self.sendFrame(Data(), eof: true) { [weak self] in self?.ended() } }
                else { self.pumpRaw() }
            }
        }
    }
    private func receiveFrame(buffer: Data = Data()) {
        guard !stopped else { return }
        let count: Int
        if buffer.count < 5 { count = 5 }
        else {
            let n = buffer.dropFirst().prefix(4).reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
            guard n <= 65536, (buffer[0] == 0 && n > 0) || (buffer[0] == 1 && n == 0) else { stop(); return }
            count = 5 + Int(n)
            if buffer.count >= count {
                let eof = buffer[0] == 1
                let remaining = Data(buffer.dropFirst(count))
                guard !eof || remaining.isEmpty else { stop(); return }
                let payload = Data(buffer.dropFirst(5).prefix(Int(n)))
                raw.send(content: payload.isEmpty ? nil : payload, contentContext: eof ? .finalMessage : .defaultMessage, isComplete: true, completion: .contentProcessed { [weak self] error in
                    guard let self, !self.stopped else { return }
                    if error != nil { self.stop() }
                    else if eof { self.ended() }
                    else { self.receiveFrame(buffer: remaining) }
                })
                return
            }
        }
        wire.receive(minimumIncompleteLength: 1, maximumLength: count - buffer.count) { [weak self] data, _, done, error in
            guard let self, !self.stopped else { return }
            var next = buffer; if let data { next.append(data) }
            if done || error != nil { self.stop() } else { self.receiveFrame(buffer: next) }
        }
    }
    func stop() {
        guard !stopped else { return }; stopped = true
        local.stateUpdateHandler = nil; remote.stateUpdateHandler = nil; local.cancel(); remote.cancel(); finished()
    }
}

/// All mutable connection state is confined to one serial queue.
private final class RelaySession: @unchecked Sendable {
    let local: NWConnection
    let queue: DispatchQueue
    let finished: () -> Void
    var remote: NWConnection?, pipe: RelayPipe?
    var turnTask: Task<Void, Never>?
    var readingTurn = false
    var stopped = false
    init(_ local: NWConnection, queue: DispatchQueue, finished: @escaping () -> Void) { self.local = local; self.queue = queue; self.finished = finished }
    func start() {
        local.stateUpdateHandler = { [weak self] s in if case .failed = s { self?.stop() } }
        local.start(queue: queue)
        queue.asyncAfter(deadline: .now() + 15) { [weak self] in if self?.pipe == nil && self?.turnTask == nil { self?.stop() } }
    }
    func readHeaders(_ connection: NWConnection, accumulated: Data = Data(), complete: @escaping (Data, Data) -> Void) {
        guard accumulated.count < 16384 else { reject(431); return }
        connection.receive(minimumIncompleteLength: 1, maximumLength: 16384 - accumulated.count) { [weak self] data, _, done, error in
            guard let self, !self.stopped else { return }
            var buffer = accumulated; if let data { buffer.append(data) }
            if let end = buffer.range(of: Data("\r\n\r\n".utf8)) {
                complete(Data(buffer[..<end.upperBound]), Data(buffer[end.upperBound...]))
            } else if done || error != nil { self.stop() }
            else { self.readHeaders(connection, accumulated: buffer, complete: complete) }
        }
    }
    func reject(_ status: Int) {
        guard !stopped else { return }
        local.send(content: Data("HTTP/1.1 \(status) Rejected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8), completion: .contentProcessed { [weak self] _ in self?.stop() })
    }
    func attach(_ remote: NWConnection, trailing: Data, localFramed: Bool) {
        let pipe = RelayPipe(local, remote, localFramed: localFramed) { [weak self] in self?.stop() }
        self.pipe = pipe
        pipe.start(trailing: trailing)
    }
    func readTurn(length: Int, buffer: Data, executor: TurnExecutor) {
        guard !stopped, buffer.count <= length else { reject(400); return }
        if buffer.count < length {
            local.receive(minimumIncompleteLength: 1, maximumLength: min(65536, length - buffer.count)) { [weak self] data, _, done, error in
                guard let self, !self.stopped else { return }
                var next = buffer; if let data { next.append(data) }
                if error != nil || (done && next.count != length) { self.stop() }
                else { self.readTurn(length: length, buffer: next, executor: executor) }
            }
            return
        }
        do { _ = try TurnRequest.normalize(buffer) } catch { reject(400); return }
        // Detect disconnects while streaming; cancel upstream rather than retrying a prompt.
        local.receive(minimumIncompleteLength: 1, maximumLength: 1) { [weak self] _, _, _, _ in self?.stop() }
        turnTask = Task { [weak self] in
            guard let self else { return }
            var began = false
            do {
                try await executor.execute(buffer) { data in
                    try Task.checkCancellation()
                    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                        self.local.send(content: data, completion: .contentProcessed { error in
                            if let error { continuation.resume(throwing: error) } else { continuation.resume() }
                        })
                    }
                    began = true
                }
            } catch {
                if !Task.isCancelled {
                    let failure = (error as? TurnFailure) ?? TurnFailure(code: .unavailable)
                    let errorObject = (try? JSONSerialization.jsonObject(with: failure.json)) as? [String: Any] ?? [:]
                    let failedEvent = (try? JSONSerialization.data(withJSONObject: ["type": "response.failed", "error": errorObject["error"] ?? [:]])) ?? Data()
                    let payload: Data
                    if began { payload = Data("event: response.failed\ndata: ".utf8) + failedEvent + Data("\n\n".utf8) }
                    else { payload = Data("HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: \(failure.json.count)\r\nConnection: close\r\n\r\n".utf8) + failure.json }
                    await withCheckedContinuation { continuation in self.local.send(content: payload, completion: .contentProcessed { _ in continuation.resume() }) }
                }
            }
            self.queue.async { self.stop() }
        }
    }
    func stop() {
        guard !stopped else { return }; stopped = true
        turnTask?.cancel(); turnTask = nil
        pipe?.stop(); pipe = nil; local.stateUpdateHandler = nil; remote?.stateUpdateHandler = nil; local.cancel(); remote?.cancel(); finished()
    }
}

enum RelayProtocol {
    static let versionHeader = "Peer-Route-Protocol: framed-v1\r\n"
    static func compatible(_ header: Data) -> Bool {
        String(data: header, encoding: .utf8)?.components(separatedBy: "\r\n").filter { $0.lowercased().hasPrefix("peer-route-protocol:") } == ["Peer-Route-Protocol: framed-v1"]
    }
    static func authorized(_ header: Data, key: String) -> Bool {
        guard !key.isEmpty, let text = String(data: header, encoding: .utf8) else { return false }
        let values = text.components(separatedBy: "\r\n").dropFirst().compactMap { line -> String? in
            guard let i = line.firstIndex(of: ":"), line[..<i].lowercased() == "proxy-authorization" else { return nil }
            return line[line.index(after: i)...].trimmingCharacters(in: .whitespaces)
        }
        guard values.count == 1 else { return false }
        let a = Array(values[0].utf8), b = Array(("Bearer " + key).utf8)
        guard a.count == b.count else { return false }
        return zip(a, b).reduce(UInt8(0)) { $0 | ($1.0 ^ $1.1) } == 0
    }
    static func handshake(_ destination: ChatGPTProbe.Destination?, key: String) -> Data {
        let start = destination.map { "CONNECT \($0.host):\($0.port) HTTP/1.1" } ?? "GET /peer-route-health HTTP/1.1"
        return Data((start + "\r\n" + versionHeader + "Proxy-Authorization: Bearer " + key + "\r\n\r\n").utf8)
    }
}

final class DesktopRelayHost: @unchecked Sendable {
    private let queue = DispatchQueue(label: "io.peerroute.desktop-relay-host")
    private var listener: NWListener?
    private var sessions: [UUID: RelaySession] = [:]
    private let policy: (ChatGPTProbe.Destination) -> Bool
    private let executor: TurnExecutor?
    init(policy: @escaping (ChatGPTProbe.Destination) -> Bool = ChatGPTProbe.allowed, executor: TurnExecutor? = nil) { self.policy = policy; self.executor = executor }
    func start(identity: SecIdentity, key: String, port: UInt16 = 18444, bindHost: String = "0.0.0.0", state: @escaping @Sendable (Bool) -> Void) throws {
        guard !key.isEmpty, !key.contains("\r"), !key.contains("\n"), let identity = sec_identity_create(identity) else { throw RouteError.message("Relay identity or access key is invalid.") }
        let tls = NWProtocolTLS.Options(); sec_protocol_options_set_local_identity(tls.securityProtocolOptions, identity)
        sec_protocol_options_set_min_tls_protocol_version(tls.securityProtocolOptions, .TLSv12)
        let params = NWParameters(tls: tls)
        params.requiredLocalEndpoint = .hostPort(host: NWEndpoint.Host(bindHost), port: NWEndpoint.Port(rawValue: port)!)
        let listener = try NWListener(using: params); self.listener = listener
        listener.stateUpdateHandler = { s in switch s { case .ready: state(true); case .failed: state(false); default: break } }
        listener.newConnectionHandler = { [weak self] connection in
            guard let self, self.sessions.count < 64 else { connection.cancel(); return }
            let id = UUID()
            let session = RelaySession(connection, queue: self.queue) { [weak self] in self?.sessions.removeValue(forKey: id) }
            self.sessions[id] = session; session.start()
            session.readHeaders(connection) { [weak self, weak session] header, trailing in
                guard let self, let session else { return }
                if header.starts(with: Data("GET /peer-route-status HTTP/1.1\r\n".utf8)) {
                    guard TurnRequest.authorized(header, key: key) else { session.reject(401); return }
                    guard trailing.isEmpty else { session.reject(400); return }
                    let status = TurnPeerStatus(version: 1, appVersion: TurnPeerStatus.currentVersion, failure: self.executor?.preflight() ?? (self.executor == nil ? TurnFailure(code: .noAccount) : nil))
                    let body = (try? JSONEncoder().encode(status)) ?? Data()
                    let response = Data("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: \(body.count)\r\nConnection: close\r\n\r\n".utf8) + body
                    connection.send(content: response, completion: .contentProcessed { _ in session.stop() }); return
                }
                if header.starts(with: Data("POST /v1/responses HTTP/1.1\r\n".utf8)) {
                    guard TurnRequest.authorized(header, key: key) else { session.reject(401); return }
                    guard let executor = self.executor else { session.reject(503); return }
                    guard self.sessions.values.filter({ $0.readingTurn }).count < 4 else { session.reject(429); return }
                    guard let length = TurnRequest.contentLength(header) else { session.reject(400); return }
                    session.readingTurn = true
                    session.readTurn(length: length, buffer: trailing, executor: executor); return
                }
                guard RelayProtocol.authorized(header, key: key), RelayProtocol.compatible(header) else { session.reject(407); return }
                if header.starts(with: Data("GET /peer-route-health HTTP/1.1\r\n".utf8)), trailing.isEmpty {
                    connection.send(content: Data("HTTP/1.1 200 OK\r\nPeer-Route-Protocol: framed-v1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8), completion: .contentProcessed { _ in session.stop() }); return
                }
                guard case let .connect(destination, _) = ChatGPTProbe.parse(header), self.policy(destination) else { session.reject(403); return }
                let remote = NWConnection(host: NWEndpoint.Host(destination.host), port: NWEndpoint.Port(rawValue: destination.port)!, using: .tcp)
                session.remote = remote
                remote.stateUpdateHandler = { [weak session] s in
                    guard let session, !session.stopped else { return }
                    switch s {
                    case .ready:
                        connection.send(content: Data("HTTP/1.1 200 Connection Established\r\nPeer-Route-Protocol: framed-v1\r\n\r\n".utf8), completion: .contentProcessed { [weak session] error in
                            guard let session else { return }
                            if error != nil { session.stop() } else { session.attach(remote, trailing: trailing, localFramed: true) }
                        })
                    case .failed: session.reject(502)
                    default: break
                    }
                }
                remote.start(queue: self.queue)
            }
        }
        listener.start(queue: queue)
    }
    func stop() { queue.async { [self] in listener?.cancel(); listener = nil; let active = Array(sessions.values); sessions.removeAll(); active.forEach { $0.stop() } } }
}

final class DesktopRelayGateway: @unchecked Sendable {
    private let queue = DispatchQueue(label: "io.peerroute.desktop-relay-gateway")
    private var listener: NWListener?
    private var sessions: [UUID: RelaySession] = [:]
    private let peer: Peer, key: String
    private let policy: (ChatGPTProbe.Destination) -> Bool
    init(peer: Peer, key: String, policy: @escaping (ChatGPTProbe.Destination) -> Bool = ChatGPTProbe.allowed) throws {
        try peer.validate()
        guard !key.isEmpty, !key.contains("\r"), !key.contains("\n") else { throw RouteError.message("Invalid relay key.") }
        self.peer = peer; self.key = key; self.policy = policy
    }
    private func remote() -> NWConnection {
        NWConnection(host: NWEndpoint.Host(peer.alias), port: NWEndpoint.Port(rawValue: UInt16(peer.port))!, using: NWParameters(tls: RelayTLS.options(pin: peer.fingerprint, queue: queue)))
    }
    func start(port: UInt16 = 18319, state: @escaping @Sendable (Bool) -> Void) throws {
        let params = NWParameters.tcp
        params.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: port)!)
        let listener = try NWListener(using: params); self.listener = listener
        listener.stateUpdateHandler = { s in switch s { case .ready: state(true); case .failed: state(false); default: break } }
        listener.newConnectionHandler = { [weak self] incoming in
            guard let self, self.sessions.count < 64 else { incoming.cancel(); return }
            let id = UUID(), session = RelaySession(incoming, queue: self.queue) { [weak self] in self?.sessions.removeValue(forKey: id) }
            self.sessions[id] = session; session.start()
            session.readHeaders(incoming) { [weak self, weak session] header, trailing in
                guard let self, let session else { return }
                guard case let .connect(destination, _) = ChatGPTProbe.parse(header), self.policy(destination) else { session.reject(403); return }
                let remote = self.remote(); session.remote = remote
                remote.stateUpdateHandler = { [weak session] s in
                    guard let session, !session.stopped else { return }
                    switch s {
                    case .ready:
                        remote.send(content: RelayProtocol.handshake(destination, key: self.key), completion: .contentProcessed { [weak session] error in
                            guard let session else { return }
                            if error != nil { session.reject(502); return }
                            session.readHeaders(remote) { [weak session] response, leftover in
                                guard let session else { return }
                                guard response.starts(with: Data("HTTP/1.1 200 ".utf8)), RelayProtocol.compatible(response), leftover.isEmpty else { session.reject(502); return }
                                incoming.send(content: Data("HTTP/1.1 200 Connection Established\r\n\r\n".utf8), completion: .contentProcessed { [weak session] error in
                                    guard let session else { return }
                                    if error != nil { session.stop() } else { session.attach(remote, trailing: trailing, localFramed: false) }
                                })
                            }
                        })
                    case .failed: session.reject(502)
                    default: break
                    }
                }
                remote.start(queue: self.queue)
            }
        }
        listener.start(queue: queue)
    }
    func check() async -> Bool {
        await withCheckedContinuation { continuation in
            queue.async { [self] in
                let connection = remote()
                var finished = false
                func complete(_ result: Bool) {
                    guard !finished else { return }; finished = true; connection.stateUpdateHandler = nil; connection.cancel(); continuation.resume(returning: result)
                }
                connection.stateUpdateHandler = { s in
                    switch s {
                    case .ready:
                        connection.send(content: RelayProtocol.handshake(nil, key: self.key), completion: .contentProcessed { error in
                            if error != nil { complete(false); return }
                            var response = Data()
                            func receive() {
                                connection.receive(minimumIncompleteLength: 1, maximumLength: 1024) { data, _, done, error in
                                    if let data { response.append(data) }
                                    if response.range(of: Data("\r\n\r\n".utf8)) != nil { complete(response.starts(with: Data("HTTP/1.1 200 ".utf8)) && RelayProtocol.compatible(response)) }
                                    else if done || error != nil || response.count > 4096 { complete(false) }
                                    else { receive() }
                                }
                            }
                            receive()
                        })
                    case .failed: complete(false)
                    default: break
                    }
                }
                connection.start(queue: queue)
                queue.asyncAfter(deadline: .now() + 8) { complete(false) }
            }
        }
    }
    func stop() { queue.async { [self] in listener?.cancel(); listener = nil; let active = Array(sessions.values); sessions.removeAll(); active.forEach { $0.stop() } } }
}
