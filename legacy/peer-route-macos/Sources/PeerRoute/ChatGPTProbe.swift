import Foundation
import Network
import AppKit

/// CONNECT-only diagnostics. TLS bytes remain encrypted between ChatGPT and its server.
final class ChatGPTProbe: @unchecked Sendable {
    struct Destination: Equatable {
        let host: String
        let port: UInt16
    }
    enum ParseResult: Equatable {
        case incomplete
        case rejected
        case connect(Destination, Data)
    }
    static func parse(_ data: Data) -> ParseResult {
        guard data.count <= 16384 else { return .rejected }
        guard let end = data.range(of: Data("\r\n\r\n".utf8)) else { return .incomplete }
        guard let header = String(data: data[..<end.lowerBound], encoding: .utf8),
              let first = header.components(separatedBy: "\r\n").first else { return .rejected }
        let parts = first.split(separator: " ", omittingEmptySubsequences: false)
        guard parts.count == 3, parts[0] == "CONNECT", parts[2] == "HTTP/1.1" || parts[2] == "HTTP/1.0" else { return .rejected }
        let authority = parts[1].split(separator: ":", omittingEmptySubsequences: false)
        guard authority.count == 2, let port = UInt16(authority[1]), port > 0 else { return .rejected }
        let host = String(authority[0]).lowercased()
        guard host.count <= 253,
              host.split(separator: ".", omittingEmptySubsequences: false).allSatisfy({ label in
                  !label.isEmpty && label.count <= 63 && label.first != "-" && label.last != "-" &&
                  label.utf8.allSatisfy({ (97...122).contains($0) || (48...57).contains($0) || $0 == 45 })
              }) else { return .rejected }
        return .connect(Destination(host: host, port: port), Data(data[end.upperBound...]))
    }
    static func allowed(_ destination: Destination) -> Bool {
        guard destination.port == 443 else { return false }
        return ["chatgpt.com", "openai.com", "oaistatic.com", "oaiusercontent.com", "auth0.com"].contains {
            destination.host == $0 || destination.host.hasSuffix("." + $0)
        }
    }
    private let queue = DispatchQueue(label: "io.peerroute.chatgpt-probe")
    private var listener: NWListener?
    private var sessions: [UUID: ProbeTunnel] = [:]
    private let policy: (Destination) -> Bool
    init(policy: @escaping (Destination) -> Bool = ChatGPTProbe.allowed) { self.policy = policy }
    func start(port: UInt16 = 18318, event: @escaping @Sendable (String, String) -> Void,
               state: @escaping @Sendable (Bool) -> Void) throws {
        let params = NWParameters.tcp
        params.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: port)!)
        let server = try NWListener(using: params)
        listener = server
        server.stateUpdateHandler = { value in
            switch value { case .ready: state(true); case .failed: state(false); default: break }
        }
        server.newConnectionHandler = { [weak self] incoming in
            guard let self, self.sessions.count < 64 else { incoming.cancel(); return }
            let id = UUID()
            let tunnel = ProbeTunnel(incoming, policy: self.policy, event: event) { [weak self] in self?.sessions.removeValue(forKey: id) }
            self.sessions[id] = tunnel
            tunnel.start(self.queue)
        }
        server.start(queue: queue)
    }
    func stop() {
        queue.async { [self] in
            listener?.cancel(); listener = nil
            let active = Array(sessions.values); sessions.removeAll()
            active.forEach { $0.stop() }
        }
    }
}

private final class ProbeTunnel {
    let incoming: NWConnection
    var outgoing: NWConnection?
    let policy: (ChatGPTProbe.Destination) -> Bool
    let event: @Sendable (String, String) -> Void
    let finished: () -> Void
    var header = Data()
    var stopped = false
    var ends = 0
    init(_ incoming: NWConnection, policy: @escaping (ChatGPTProbe.Destination) -> Bool,
         event: @escaping @Sendable (String, String) -> Void, finished: @escaping () -> Void) {
        self.incoming = incoming; self.policy = policy; self.event = event; self.finished = finished
    }
    func start(_ queue: DispatchQueue) {
        incoming.stateUpdateHandler = { [weak self] value in if case .failed = value { self?.stop() } }
        incoming.start(queue: queue)
        readHeader(queue)
        queue.asyncAfter(deadline: .now() + 15) { [weak self] in
            if self?.outgoing?.state != .ready { self?.stop() }
        }
    }
    func readHeader(_ queue: DispatchQueue) {
        incoming.receive(minimumIncompleteLength: 1, maximumLength: 16384 - header.count) { [weak self] data, _, done, error in
            guard let self, !self.stopped else { return }
            if let data { self.header.append(data) }
            switch ChatGPTProbe.parse(self.header) {
            case .incomplete:
                if done || error != nil || self.header.count >= 16384 { self.reject(400) }
                else { self.readHeader(queue) }
            case .rejected: self.reject(400)
            case let .connect(destination, trailing):
                self.header.removeAll()
                guard self.policy(destination) else { self.event(destination.host, "Blocked by diagnostic scope"); self.reject(403); return }
                self.connect(destination, trailing: trailing, queue: queue)
            }
        }
    }
    func connect(_ destination: ChatGPTProbe.Destination, trailing: Data, queue: DispatchQueue) {
        let remote = NWConnection(host: NWEndpoint.Host(destination.host), port: NWEndpoint.Port(rawValue: destination.port)!, using: .tcp)
        outgoing = remote
        remote.stateUpdateHandler = { [weak self] value in
            guard let self, !self.stopped else { return }
            switch value {
            case .ready:
                self.event(destination.host, "TCP tunnel established; TLS uninspected")
                self.incoming.send(content: Data("HTTP/1.1 200 Connection Established\r\n\r\n".utf8), completion: .contentProcessed { [weak self] error in
                    guard let self, !self.stopped else { return }
                    if error != nil { self.stop(); return }
                    remote.send(content: trailing.isEmpty ? nil : trailing, completion: .contentProcessed { [weak self] error in
                        guard let self, !self.stopped else { return }
                        if error != nil { self.stop(); return }
                        self.pump(self.incoming, remote); self.pump(remote, self.incoming)
                    })
                })
            case .failed:
                self.event(destination.host, "TCP connection failed"); self.reject(502)
            default: break
            }
        }
        remote.start(queue: queue)
    }
    func reject(_ code: Int) {
        incoming.send(content: Data("HTTP/1.1 \(code) Rejected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8), completion: .contentProcessed { [weak self] _ in self?.stop() })
    }
    func pump(_ source: NWConnection, _ target: NWConnection) {
        source.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, done, error in
            guard let self, !self.stopped else { return }
            if error != nil { self.stop(); return }
            target.send(content: data, contentContext: done ? .finalMessage : .defaultMessage, isComplete: done,
                        completion: .contentProcessed { [weak self] error in
                guard let self, !self.stopped else { return }
                if error != nil { self.stop() }
                else if done { self.ends += 1; if self.ends == 2 { self.stop() } }
                else { self.pump(source, target) }
            })
        }
    }
    func stop() {
        guard !stopped else { return }
        stopped = true; incoming.cancel(); outgoing?.cancel(); finished()
    }
}

@MainActor final class ChatGPTProbeStore: ObservableObject {
    struct Observation: Identifiable {
        let id = UUID()
        let host: String
        var connections = 1
        var status: String
    }
    @Published var running = false
    @Published var status = "Diagnostics are off"
    @Published var observations: [Observation] = []
    private var probe: ChatGPTProbe?
    private var generation = UUID()
    let command = "open -a ChatGPT --args --proxy-server=http://127.0.0.1:18318"
    func start() {
        stop(); observations = []; status = "Starting local diagnostics…"
        let ticket = generation
        let service = ChatGPTProbe(); probe = service
        do {
            try service.start(event: { [weak self] host, status in Task { @MainActor in
                guard let self, self.generation == ticket else { return }
                if let i = self.observations.firstIndex(where: { $0.host == host }) {
                    self.observations[i].connections += 1; self.observations[i].status = status
                } else if self.observations.count < 100 { self.observations.append(Observation(host: host, status: status)) }
            } }, state: { [weak self] ready in Task { @MainActor in
                guard let self, self.generation == ticket else { return }
                self.running = ready
                self.status = ready ? "Listening on 127.0.0.1:18318" : "Listener failed. Port 18318 may be in use."
            } })
        } catch { status = error.localizedDescription; probe = nil }
    }
    func stop() {
        generation = UUID(); probe?.stop(); probe = nil; running = false; status = "Diagnostics are off"
    }
    func copyCommand() { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(command, forType: .string) }
}
