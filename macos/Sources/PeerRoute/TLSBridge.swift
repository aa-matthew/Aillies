import Foundation
import Network
import Security
import CryptoKit

final class TLSBridge: @unchecked Sendable {
    private let queue = DispatchQueue(label: "io.peerroute.tls-bridge")
    private var listener: NWListener?
    private var streams: [UUID: StreamPair] = [:]
    private let host: String
    private let port: UInt16
    private let fingerprint: String

    init(host: String, port: Int, fingerprint: String) throws {
        guard let port = UInt16(exactly: port), fingerprint.count == 64,
              fingerprint.allSatisfy({ $0.isHexDigit }) else { throw RouteError.message("A valid pinned certificate fingerprint is required. Import a pairing file from the peer.") }
        self.host = host; self.port = port; self.fingerprint = fingerprint.lowercased()
    }
    func start(localPort: Int, onFailure: @escaping @Sendable () -> Void) throws {
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: UInt16(localPort))!)
        parameters.allowLocalEndpointReuse = false
        let listener = try NWListener(using: parameters)
        self.listener = listener
        listener.stateUpdateHandler = { state in if case .failed = state { onFailure() } }
        listener.newConnectionHandler = { [weak self] incoming in self?.accept(incoming) }
        listener.start(queue: queue)
    }
    private func accept(_ incoming: NWConnection) {
        guard streams.count < 64 else { incoming.cancel(); return }
        let options = NWProtocolTLS.Options()
        let expected = fingerprint
        sec_protocol_options_set_min_tls_protocol_version(options.securityProtocolOptions, .TLSv12)
        sec_protocol_options_set_verify_block(options.securityProtocolOptions, { _, trust, complete in
            let value = sec_trust_copy_ref(trust).takeRetainedValue()
            guard let cert = (SecTrustCopyCertificateChain(value) as? [SecCertificate])?.first else { complete(false); return }
            let digest = SHA256.hash(data: SecCertificateCopyData(cert) as Data).map { String(format: "%02x", $0) }.joined()
            guard digest == expected else { complete(false); return }
            SecTrustSetPolicies(value, SecPolicyCreateBasicX509())
            SecTrustSetAnchorCertificates(value, [cert] as CFArray)
            SecTrustSetAnchorCertificatesOnly(value, true)
            complete(SecTrustEvaluateWithError(value, nil))
        }, queue)
        let remote = NWConnection(host: NWEndpoint.Host(host), port: NWEndpoint.Port(rawValue: port)!, using: NWParameters(tls: options))
        let id = UUID()
        let pair = StreamPair(local: incoming, remote: remote) { [weak self] in self?.streams.removeValue(forKey: id) }
        streams[id] = pair
        pair.start(queue: queue)
    }
    func stop() {
        queue.async { [self] in
            listener?.cancel(); listener = nil
            let current = Array(streams.values); streams.removeAll()
            current.forEach { $0.stop() }
        }
    }
}

private final class StreamPair: @unchecked Sendable {
    let local: NWConnection
    let remote: NWConnection
    let finished: () -> Void
    var stopped = false
    var ends = 0
    init(local: NWConnection, remote: NWConnection, finished: @escaping () -> Void) {
        self.local = local; self.remote = remote; self.finished = finished
    }
    func start(queue: DispatchQueue) {
        local.stateUpdateHandler = { [weak self] state in if case .failed = state { self?.stop() } }
        remote.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready: self.pump(self.local, self.remote); self.pump(self.remote, self.local)
            case .failed: self.stop()
            default: break
            }
        }
        local.start(queue: queue); remote.start(queue: queue)
        queue.asyncAfter(deadline: .now() + 15) { [weak self] in
            guard let self else { return }
            if self.remote.state != .ready { self.stop() }
        }
    }
    func pump(_ source: NWConnection, _ destination: NWConnection) {
        guard !stopped else { return }
        source.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, complete, error in
            guard let self, !self.stopped else { return }
            if error != nil { self.stop(); return }
            destination.send(content: data, contentContext: complete ? .finalMessage : .defaultMessage,
                             isComplete: complete, completion: .contentProcessed { [weak self] failure in
                guard let self, !self.stopped else { return }
                if failure != nil { self.stop() }
                else if complete { self.ends += 1; if self.ends == 2 { self.stop() } }
                else { self.pump(source, destination) }
            })
        }
    }
    func stop() {
        guard !stopped else { return }
        stopped = true; local.cancel(); remote.cancel(); finished()
    }
}
