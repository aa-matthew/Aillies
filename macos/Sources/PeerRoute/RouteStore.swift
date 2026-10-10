import Foundation
import SwiftUI
import AppKit

@MainActor
final class RouteStore: ObservableObject {
    @Published var saved = SavedState()
    @Published var activePeer: UUID?
    @Published var busy = false
    @Published var status = "Routing is off"
    @Published var error = ""
    @Published var models: [String] = []
    @Published var latency: Int?
    @Published var probePassed = false
    @Published var installed = false
    @Published var showEditor = false
    @Published var editing: Peer?
    private var tunnel: TLSBridge?
    private var generation = UUID()
    private var monitor: Task<Void, Never>?
    let localPort = 18317
    private let session: URLSession

    init() {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 60
        config.connectionProxyDictionary = [:]
        session = URLSession(configuration: config)
        do { saved = try Files.load() } catch { self.error = "Could not load machines: \(error.localizedDescription)" }
        updateInstalled()
    }

    var current: Peer? { saved.peers.first { $0.id == activePeer } }
    var selected: Peer? { saved.peers.first { $0.id == saved.selected } }
    func persist() { do { try Files.save(saved) } catch { self.error = error.localizedDescription } }
    func updateInstalled() { installed = FileManager.default.fileExists(atPath: Files.directory.appendingPathComponent("codex-restore.json").path) }

    func savePeer(_ peer: Peer, key: String) throws {
        try peer.validate()
        guard !key.isEmpty, !key.contains("\n"), !key.contains("\r") else { throw RouteError.message("Enter the peer's CLIProxyAPI client key.") }
        try Vault.save(key, for: peer.id)
        if let index = saved.peers.firstIndex(where: { $0.id == peer.id }) { saved.peers[index] = peer }
        else { saved.peers.append(peer) }
        if saved.selected == nil { saved.selected = peer.id }
        persist()
    }

    func remove(_ peer: Peer) {
        if activePeer == peer.id { pause() }
        saved.peers.removeAll { $0.id == peer.id }
        Vault.delete(peer.id)
        if saved.selected == peer.id { saved.selected = saved.peers.first?.id }
        persist()
    }

    func pause() {
        generation = UUID()
        monitor?.cancel(); monitor = nil
        tunnel?.stop()
        tunnel = nil; activePeer = nil; latency = nil; models = []; probePassed = false
        status = installed ? "Paused · configured Codex requests are blocked" : "Routing is off"
    }

    func connect(_ peer: Peer) async {
        guard !busy else { return }
        guard !installed || saved.selected == peer.id else {
            error = "Restore Codex settings before switching accounts, then connect and enable the new peer."
            return
        }
        busy = true; defer { busy = false }
        pause(); error = ""; status = "Connecting to \(peer.name)…"
        let ticket = generation
        do {
            try peer.validate()
            let key = try Vault.read(peer.id)
            let bridge = try TLSBridge(host: peer.alias, port: peer.port, fingerprint: peer.fingerprint)
            tunnel = bridge
            try bridge.start(localPort: localPort) { [weak self] in
                Task { @MainActor in
                    guard let self, self.generation == ticket else { return }
                    self.pause(); self.error = "Local endpoint could not start. Check that port 18317 is free."
                }
            }
            var found: [String]?
            var measured = 0
            for _ in 0..<3 {
                guard ticket == generation else { return }
                try await Task.sleep(for: .milliseconds(400))
                do {
                    let start = Date()
                    found = try await discover(key: key)
                    measured = Int(Date().timeIntervalSince(start) * 1000)
                    break
                } catch let failure as RouteError { throw failure }
                catch { continue }
            }
            guard ticket == generation else { return }
            guard let found, !found.isEmpty else {
                throw RouteError.message("Could not reach a signed-in peer. Check sharing, the Mac address, firewall access, pairing fingerprint, and Codex sign-in on that Mac.")
            }
            models = found; latency = measured; activePeer = peer.id; saved.selected = peer.id
            if !models.contains(saved.model) { saved.model = models[0] }
            persist()
            status = "Connected over pinned TLS · model endpoint reachable"
            monitor = Task { [weak self] in
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(15)) } catch { return }
                    guard let self, self.generation == ticket else { return }
                    do { _ = try await self.discover(key: key) }
                    catch {
                        guard self.generation == ticket else { return }
                        self.pause(); self.error = "Peer became unavailable or rejected access. Requests are blocked; reconnect or import a new pairing file."
                        return
                    }
                }
            }
        } catch {
            guard ticket == generation else { return }
            pause(); self.error = error.localizedDescription
        }
    }

    func importPairing() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        panel.message = "Choose the private pairing file from your trusted Mac. Compare its fingerprint with the one on that Mac before connecting."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            guard (attributes[.size] as? Int ?? 0) < 65536 else { throw RouteError.message("Pairing file is too large.") }
            let file = try JSONDecoder().decode(PairingFile.self, from: Data(contentsOf: url))
            var peer = try file.peer()
            if let existing = saved.peers.first(where: { $0.alias == peer.alias && $0.port == peer.port && $0.fingerprint == peer.fingerprint }) { peer.id = existing.id }
            guard !installed else { throw RouteError.message("Restore Codex settings before importing a new pairing key.") }
            try savePeer(peer, key: file.clientKey)
            status = "Machine imported · compare the certificate fingerprint before connecting"
        } catch { self.error = error.localizedDescription }
    }

    private func request(_ path: String, key: String) -> URLRequest {
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(localPort)/v1/\(path)")!)
        request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        return request
    }

    private func discover(key: String) async throws -> [String] {
        let (data, response) = try await session.data(for: request("models", key: key))
        guard let response = response as? HTTPURLResponse else { throw RouteError.message("Invalid proxy response.") }
        guard response.statusCode == 200 else {
            throw RouteError.message("Peer model endpoint returned HTTP \(response.statusCode). Check the client key and proxy setup.")
        }
        struct Catalog: Decodable { struct Model: Decodable { var id: String }; var data: [Model] }
        return try JSONDecoder().decode(Catalog.self, from: data).data.map(\.id).sorted()
    }

    func testResponse() async {
        guard !busy, let peer = current else { return }
        busy = true; defer { busy = false }
        error = ""; status = "Testing a model response…"; probePassed = false
        let ticket = generation
        do {
            var request = request("responses", key: try Vault.read(peer.id))
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: ["model": saved.model, "input": "Reply with exactly PEER_ROUTE_OK. Do not use tools.", "stream": false, "store": false])
            let (data, response) = try await session.data(for: request)
            guard generation == ticket else { return }
            guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
                throw RouteError.message("The model test failed. Check the peer's OAuth session, model access, and proxy logs.")
            }
            let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
            let output = object?["output"] as? [[String: Any]] ?? []
            let text = output.flatMap { $0["content"] as? [[String: Any]] ?? [] }.compactMap { $0["text"] as? String }.joined()
            guard text.contains("PEER_ROUTE_OK"), object?["error"] == nil else {
                throw RouteError.message("The endpoint responded, but did not complete the expected model test.")
            }
            probePassed = true; status = "Model response verified through \(peer.name)"
        } catch { if ticket == generation { self.error = error.localizedDescription; status = "Model test failed" } }
    }

    func installConfig() {
        guard let peer = current, probePassed else { error = "Connect and pass the response test first."; return }
        do {
            try CodexConfig.install(port: localPort, key: Vault.read(peer.id), model: saved.model)
            updateInstalled(); status = "Codex configured · restart its app before using the route"
        } catch { self.error = error.localizedDescription; updateInstalled() }
    }
    func restoreConfig() {
        do { try CodexConfig.restore(); updateInstalled(); status = "Original Codex configuration restored" }
        catch { self.error = error.localizedDescription }
    }
}
