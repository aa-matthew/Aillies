import SwiftUI
import AppKit
import Security

struct DesktopRelayPairing: Codable {
    var version = 1
    var kind = "desktop-encrypted-relay"
    var name: String
    var host: String
    var port = 18444
    var fingerprint: String
    var accessKey: String
    func peer() throws -> Peer {
        guard version == 1, kind == "desktop-encrypted-relay", accessKey.count == 64,
              accessKey.allSatisfy({ $0.isHexDigit }) else { throw RouteError.message("Invalid desktop relay pairing file.") }
        let peer = Peer(name: name, alias: host, port: port, fingerprint: fingerprint)
        try peer.validate(); return peer
    }
}

@MainActor final class DesktopRelayStore: ObservableObject {
    @Published var name = Host.current().localizedName ?? "My Mac"
    @Published var host = RelayDiscovery.localHostname
    @Published var fingerprint = ""
    @Published var sharing = false
    @Published var routing = false
    @Published var busy = false
    @Published var status = "Desktop relay is off"
    @Published var hostStatus = "Sharing is off"
    @Published var error = ""
    @Published var peers: [Peer] = []
    @Published var selected: UUID?
    @Published var includeNodeProxy = true
    @Published var authSelected = false
    @Published var turnModel = UserDefaults.standard.string(forKey: "turnModel") ?? "gpt-5.5"
    @Published var testingTurn = false
    @Published var turnStatus = "Not tested"
    @Published var turnVerified = false
    private var turnBridge: TLSBridge?
    private var testTask: Task<Void, Never>?
    private var turnTestGeneration = UUID()
    private var configuredPeer: UUID? {
        guard FileManager.default.fileExists(atPath: Files.directory.appendingPathComponent("codex-restore.json").path),
              let value = UserDefaults.standard.string(forKey: "turnConfiguredPeer") else { return nil }
        return UUID(uuidString: value)
    }
    private var authURL: URL? {
        guard let bookmark = UserDefaults.standard.data(forKey: "codexOAuthFile") else { return nil }
        var stale = false
        return try? URL(resolvingBookmarkData: bookmark, options: .withoutUI, bookmarkDataIsStale: &stale)
    }
    func configureCodex() {
        do {
            guard routing, turnVerified, let peer = selectedPeer else { throw RouteError.message("Run a generation check first.") }
            try CodexConfig.install(port: 18322, key: Vault.read(peer.id), model: turnModel, turnEndpoint: true)
            UserDefaults.standard.set(peer.id.uuidString, forKey: "turnConfiguredPeer")
            status = "Codex configured. Restart Codex and start a new chat."
        } catch { self.error = error.localizedDescription }
    }
    func chooseAuth() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        panel.showsHiddenFiles = true; panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")
        panel.message = "Choose this Mac’s Codex OAuth auth.json. It stays on this Mac."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let account = try CodexOAuth.read(url)
            let bookmark = try url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
            stopSharing(); UserDefaults.standard.set(account.accountID, forKey: "codexOAuthAccount"); UserDefaults.standard.set(bookmark, forKey: "codexOAuthFile"); authSelected = true; error = ""
        } catch { self.error = error.localizedDescription }
    }
    func forgetAuth() { stopSharing(); UserDefaults.standard.removeObject(forKey: "codexOAuthFile"); UserDefaults.standard.removeObject(forKey: "codexOAuthAccount"); authSelected = false }
    func checkTurn(image: Bool) {
        guard routing, !testingTurn, let peer = selectedPeer else { return }
        testingTurn = true; error = ""; turnVerified = false; turnStatus = image ? "Generating test image…" : "Checking text stream…"
        let ticket = UUID(); turnTestGeneration = ticket
        testTask = Task {
            defer { if turnTestGeneration == ticket { testingTurn = false; testTask = nil } }
            do {
                let key = try Vault.read(peer.id)
                var request = URLRequest(url: URL(string: "http://127.0.0.1:18322/v1/responses")!)
                request.httpMethod = "POST"; request.timeoutInterval = 120
                request.setValue("Bearer " + key, forHTTPHeaderField: "Authorization")
                request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                var body: [String: Any] = ["model": turnModel, "input": [["type": "message", "role": "user", "content": [["type": "input_text", "text": image ? "Generate a simple blue square on a white background." : "Reply with OK."]]]], "stream": true, "store": false]
                if image { body["tools"] = [["type": "image_generation", "output_format": "png"]]; body["tool_choice"] = ["type": "image_generation"] }
                request.httpBody = try JSONSerialization.data(withJSONObject: body)
                let config = URLSessionConfiguration.ephemeral; config.timeoutIntervalForResource = 900; config.connectionProxyDictionary = [:]
                let session = URLSession(configuration: config); defer { session.invalidateAndCancel() }
                var preflight = URLRequest(url: URL(string: "http://127.0.0.1:18322/peer-route-status")!)
                preflight.timeoutInterval = 10; preflight.setValue("Bearer " + key, forHTTPHeaderField: "Authorization")
                let (statusBytes, statusResponse) = try await session.bytes(for: preflight)
                var statusData = Data()
                for try await byte in statusBytes {
                    guard statusData.count < 4096 else { throw RouteError.message("Invalid peer status response.") }
                    statusData.append(byte)
                }
                let peerHTTP = (statusResponse as? HTTPURLResponse)?.statusCode ?? 502
                guard peerHTTP == 200 else { throw RouteError.message(TurnFailure.httpMessage(peerHTTP, body: statusData)) }
                guard let peerStatus = try? JSONDecoder().decode(TurnPeerStatus.self, from: statusData), peerStatus.version == 1 else { throw RouteError.message("Update Peer Route on the sharing Mac to v0.8.2.") }
                if let failure = peerStatus.failure { throw failure }
                try Task.checkCancellation()
                let (bytes, response) = try await session.bytes(for: request)
                let http = (response as? HTTPURLResponse)?.statusCode ?? 502
                guard http == 200 else {
                    var failureBody = Data()
                    for try await byte in bytes { if failureBody.count == 4096 { break }; failureBody.append(byte) }
                    throw RouteError.message(TurnFailure.httpMessage(http, body: failureBody))
                }
                var line = Data(), output = false, completed = false, total = 0
                for try await byte in bytes {
                    try Task.checkCancellation(); total += 1
                    guard total <= 64 * 1024 * 1024, line.count <= 24 * 1024 * 1024 else { throw RouteError.message("Test response exceeded its size limit.") }
                    if byte != 10 { line.append(byte); continue }
                    defer { line.removeAll(keepingCapacity: true) }
                    guard line.starts(with: Data("data:".utf8)), let event = try? JSONSerialization.jsonObject(with: Data(line.dropFirst(5))) as? [String: Any] else { continue }
                    let type = event["type"] as? String
                    if type == "response.failed" || type == "error" {
                        if let data = try? JSONSerialization.data(withJSONObject: event), let failure = TurnFailure.decode(data) { throw failure }
                        throw TurnFailure(code: .failedStream)
                    }
                    if !image, type == "response.output_text.delta", !(event["delta"] as? String ?? "").isEmpty { output = true }
                    if !image, type == "response.output_item.done", let item = event["item"] as? [String: Any], let content = item["content"] as? [[String: Any]], content.contains(where: { $0["type"] as? String == "output_text" && !($0["text"] as? String ?? "").isEmpty }) { output = true }
                    if image, type == "response.output_item.done", let item = event["item"] as? [String: Any], item["type"] as? String == "image_generation_call", let result = item["result"] as? String, let data = Data(base64Encoded: result), NSImage(data: data) != nil { output = true }
                    if type == "response.completed" || type == "response.done" { completed = true }
                }
                guard turnTestGeneration == ticket else { return }
                guard output && completed else { throw RouteError.message("The stream ended without a completed result.") }
                turnVerified = true; turnStatus = image ? "Image generation verified through peer" : "Text generation verified through peer"
            } catch { if !Task.isCancelled && turnTestGeneration == ticket { turnStatus = "Check failed"; self.error = error.localizedDescription } }
        }
    }
    @Published var trustedPins: Set<String> = Set(UserDefaults.standard.stringArray(forKey: "trustedDesktopRelayPins") ?? [])
    let discovery = RelayDiscovery()
    private let airDrop = PairingAirDrop()
    var selectedPeer: Peer? { peers.first { $0.id == selected } }
    var selectedTrusted: Bool { selectedPeer.map { trustedPins.contains($0.fingerprint.lowercased()) } ?? false }
    func trustSelected() {
        guard let peer = selectedPeer else { return }
        trustedPins.insert(peer.fingerprint.lowercased())
        UserDefaults.standard.set(Array(trustedPins), forKey: "trustedDesktopRelayPins")
    }
    private var server: DesktopRelayHost?
    private var gateway: DesktopRelayGateway?
    private var monitor: Task<Void, Never>?
    private var routeGeneration = UUID()
    private var hostGeneration = UUID()
    private let keyID = UUID(uuidString: "B9B27BC8-467A-4136-8F17-C0EC9E39E03D")!
    private var path: URL { Files.directory.appendingPathComponent("desktop-relay-machines.json") }
    var command: String { ChatGPTLaunch.command(port: 18319, includeNode: includeNodeProxy) }
    init() {
        authSelected = authURL != nil
        do {
            if FileManager.default.fileExists(atPath: path.path) {
                let saved = try JSONDecoder().decode(SavedState.self, from: Data(contentsOf: path))
                for peer in saved.peers { try peer.validate() }
                peers = saved.peers; selected = saved.selected
            }
        } catch { self.error = "Could not load relay pairings: \(error.localizedDescription)" }
    }
    private func persist() throws {
        try Files.prepare(Files.directory)
        try Files.privateWrite(JSONEncoder().encode(SavedState(peers: peers, selected: selected)), to: path)
    }
    private func key(forceNew: Bool = false) throws -> String {
        if !forceNew, let value = try? Vault.read(keyID) { return value }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else { throw RouteError.message("Could not generate relay key.") }
        let value = bytes.map { String(format: "%02x", $0) }.joined()
        try Vault.save(value, for: keyID); return value
    }
    func startSharing() {
        stopSharing(); error = ""
        let generation = hostGeneration
        do {
            try Peer(name: name, alias: host, port: 18444, fingerprint: String(repeating: "0", count: 64)).validate()
            let (identity, pin) = try RelayTLS.identity(directory: Files.directory.appendingPathComponent("desktop-relay-host"))
            fingerprint = pin
            let executor: TurnExecutor?
            if let url = authURL {
                let account = try CodexOAuth.read(url)
                guard UserDefaults.standard.string(forKey: "codexOAuthAccount") == account.accountID else { throw RouteError.message("Codex account changed. Choose auth.json again to confirm.") }
                executor = TurnExecutor(authFile: url, expectedAccountID: account.accountID)
            } else { executor = nil }
            let server = DesktopRelayHost(executor: executor); self.server = server
            hostStatus = "Starting encrypted relay…"
            try server.start(identity: identity, key: key()) { [weak self] ready in
                Task { @MainActor in
                    guard let self, self.hostGeneration == generation else { return }
                    if ready { self.sharing = true; self.hostStatus = self.authSelected ? "Sharing Codex account" : "Sharing network only"; self.discovery.publish(name: self.name, fingerprint: self.fingerprint) }
                    else { self.stopSharing(); self.hostStatus = "Relay listener failed; stop and retry" }
                }
            }
        } catch { self.error = error.localizedDescription; stopSharing() }
    }
    func stopSharing() {
        discovery.unpublish()
        hostGeneration = UUID(); server?.stop(); server = nil; sharing = false; hostStatus = "Sharing is off"
    }
    func exportPairing() {
        do {
            guard sharing, !fingerprint.isEmpty else { throw RouteError.message("Enable network sharing before exporting a pairing.") }
            let file = DesktopRelayPairing(name: name, host: host, fingerprint: fingerprint, accessKey: try key())
            _ = try file.peer()
            let panel = NSSavePanel(); panel.nameFieldStringValue = "Desktop relay.peerrelay"
            guard panel.runModal() == .OK, let url = panel.url else { return }
            try Files.privateWrite(JSONEncoder().encode(file), to: url)
            hostStatus = "Pairing exported. Transfer privately and compare fingerprints."
        } catch { self.error = error.localizedDescription }
    }
    func importPairing() {
        do {
            let panel = NSOpenPanel(); panel.allowsMultipleSelection = false; panel.canChooseDirectories = false
            guard panel.runModal() == .OK, let url = panel.url else { return }
            importPairing(from: url)
        }
    }
    func sharePairing() {
        do {
            guard sharing, !fingerprint.isEmpty else { throw RouteError.message("Turn on sharing first.") }
            let value = DesktopRelayPairing(name: name, host: host, fingerprint: fingerprint, accessKey: try key())
            _ = try value.peer()
            try airDrop.share(JSONEncoder().encode(value))
        } catch { self.error = error.localizedDescription }
    }
    func importPairing(from url: URL) {
        do {
            guard configuredPeer == nil else { throw RouteError.message("Restore Codex settings before importing another pairing.") }
            guard url.isFileURL, url.pathExtension.lowercased() == "peerrelay" else { throw RouteError.message("Open a Peer Route .peerrelay pairing file.") }
            let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
            guard size < 65536 else { throw RouteError.message("Pairing file is too large.") }
            let file = try JSONDecoder().decode(DesktopRelayPairing.self, from: Data(contentsOf: url))
            var peer = try file.peer()
            // Discovery can supply a current address, but never supplies trust or a key.
            if let nearby = discovery.nearby.first(where: { $0.fingerprint == peer.fingerprint.lowercased() }) { peer.alias = nearby.host }
            try peer.validate()
            // Reimport updates the same trust identity, while changed certificates require a new explicit pairing.
            if let existing = peers.first(where: { $0.alias == peer.alias && $0.port == peer.port && $0.fingerprint == peer.fingerprint }) { peer.id = existing.id }
            stopRouting()
            try Vault.save(file.accessKey, for: peer.id)
            peers.removeAll { $0.id == peer.id }; peers.append(peer); selected = peer.id
            try persist(); status = "Pairing received. Compare the device code with the sharing Mac, then confirm it matches."
        } catch { self.error = error.localizedDescription }
    }
    func removeSelected() {
        guard configuredPeer == nil else { error = "Restore Codex settings before forgetting this Mac."; return }
        guard let id = selected else { return }
        if let peer = selectedPeer { trustedPins.remove(peer.fingerprint.lowercased()); UserDefaults.standard.set(Array(trustedPins), forKey: "trustedDesktopRelayPins") }
        stopRouting(); Vault.delete(id); peers.removeAll { $0.id == id }; selected = peers.first?.id
        do { try persist() } catch { self.error = error.localizedDescription }
    }
    func revokeAll() {
        stopSharing()
        do { _ = try key(forceNew: true); hostStatus = "All exported relay keys revoked. Export new pairings to reconnect." }
        catch { self.error = error.localizedDescription }
    }
    func startRouting() async {
        guard !busy else { return }
        stopRouting(); error = ""; busy = true
        let generation = routeGeneration
        defer { if routeGeneration == generation { busy = false } }
        do {
            guard let peer = peers.first(where: { $0.id == selected }) else { throw RouteError.message("Import and select a desktop relay pairing first.") }
            guard configuredPeer == nil || configuredPeer == peer.id else { throw RouteError.message("Restore Codex settings before switching peers.") }
            guard selectedTrusted else { throw RouteError.message("Compare the device code on both Macs and click Codes match before connecting.") }
            try persist()
            guard let accessKey = try? Vault.read(peer.id) else { throw RouteError.message("Relay key unavailable in Keychain. Import the desktop relay pairing again.") }
            let gateway = try DesktopRelayGateway(peer: peer, key: accessKey)
            self.gateway = gateway; status = "Checking pinned TLS and relay access…"
            let healthy = await gateway.check()
            guard routeGeneration == generation else { gateway.stop(); return }
            guard healthy else { throw RouteError.message("Peer unavailable, certificate mismatch, or relay key rejected. Check sharing, hostname, firewall and pairing.") }
            let turnBridge = try TLSBridge(host: peer.alias, port: peer.port, fingerprint: peer.fingerprint)
            self.turnBridge = turnBridge
            try turnBridge.start(localPort: 18322) { [weak self] in Task { @MainActor in self?.stopRouting(); self?.error = "Turn endpoint could not start. Port 18322 may be in use." } }
            try gateway.start { [weak self] ready in
                Task { @MainActor in
                    guard let self, self.routeGeneration == generation else { return }
                    if ready { self.routing = true; self.status = "Connected to \(peer.name)" }
                    else { self.stopRouting(); self.error = "Local relay listener failed. Port 18319 may be in use." }
                }
            }
            monitor = Task { [weak self, weak gateway] in
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(15)) } catch { return }
                    guard let self, let gateway, self.routeGeneration == generation else { return }
                    if !(await gateway.check()) {
                        guard self.routeGeneration == generation else { return }
                        self.stopRouting(); self.error = "Peer disconnected. Routing stopped; no direct fallback was attempted."; return
                    }
                }
            }
        } catch { if routeGeneration == generation { stopRouting(); self.error = error.localizedDescription } }
    }
    func stopRouting() {
        routeGeneration = UUID(); monitor?.cancel(); monitor = nil; gateway?.stop(); gateway = nil
        turnTestGeneration = UUID(); testTask?.cancel(); testTask = nil; testingTurn = false; turnVerified = false; turnStatus = "Not tested"
        turnBridge?.stop(); turnBridge = nil
        routing = false; busy = false; status = "Network routing is off. Reopen ChatGPT normally to restore its direct connection."
    }
    func launchChatGPT() {
        do { try ChatGPTLaunch.launch(port: 18319, includeNode: includeNodeProxy) }
        catch { self.error = error.localizedDescription }
    }
    func chooseNearby(_ nearby: NearbyRelay) {
        guard let index = peers.firstIndex(where: { $0.fingerprint.lowercased() == nearby.fingerprint }) else {
            error = "On \(nearby.name), click Share pairing via AirDrop and choose this Mac. Accept the transfer here and open the pairing file."; return
        }
        guard configuredPeer == nil || configuredPeer == peers[index].id else { error = "Restore Codex settings before switching peers."; return }
        stopRouting(); peers[index].alias = nearby.host; selected = peers[index].id
        do { try persist(); error = "" } catch { self.error = error.localizedDescription }
    }

    func stopAll() { stopRouting(); stopSharing(); discovery.stop() }
    func copyCommand() { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(command, forType: .string) }
}

struct DesktopRelayView: View {
    var embedded = false
    @EnvironmentObject var relay: DesktopRelayStore
    @EnvironmentObject var codex: RouteStore
    @Environment(\.dismiss) private var dismiss
    @AppStorage("preferredRelayRole") private var shareThisMac = false
    @State private var imageCheck = false
    @State private var confirmRevoke = false
    @State private var confirmCodex = false
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack { Text(embedded ? "Peer Route" : "Connect your Macs").font(.title2.bold()); Spacer(); if !embedded { Button("Done") { dismiss() } } }
            Picker("Mode", selection: $shareThisMac) {
                Text("Use another Mac").tag(false); Text("Share this Mac").tag(true)
            }.pickerStyle(.segmented)
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if shareThisMac {
                        TextField("Mac name", text: $relay.name).disabled(relay.sharing)
                        HStack {
                            Label(relay.authSelected ? "Codex login selected" : "Add your Codex login", systemImage: relay.authSelected ? "checkmark.shield" : "person.crop.circle")
                            Spacer(); Button(relay.authSelected ? "Change…" : "Choose auth.json…") { relay.chooseAuth() }
                        }
                        Text("Login stays here. Codex manages renewal.").font(.caption).foregroundStyle(.secondary)
                        Toggle("Allow my devices", isOn: Binding(get: { relay.sharing }, set: { if $0 { relay.startSharing() } else { relay.stopSharing() } }))
                        if relay.sharing {
                            Button("Pair another Mac…") { relay.sharePairing() }.buttonStyle(.borderedProminent)
                            Text(RelayDiscovery.code(relay.fingerprint)).font(.title2.monospaced())
                            Text("Compare this code on both Macs.").font(.caption).foregroundStyle(.secondary)
                        }
                        Text(relay.hostStatus).font(.callout).foregroundStyle(.secondary)
                        DisclosureGroup("Advanced") {
                            VStack(alignment: .leading, spacing: 10) {
                                TextField("Network address", text: $relay.host).disabled(relay.sharing)
                                Button("Export pairing…") { relay.exportPairing() }.disabled(!relay.sharing)
                                Button("Remove Codex login") { relay.forgetAuth() }.disabled(!relay.authSelected)
                                Button("Revoke pairings…", role: .destructive) { confirmRevoke = true }
                                Text("Existing pairings can submit turns when you enable an account. Hosting requires macOS 15+. Keep both Macs awake on the same network or private VPN.").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    } else {
                        if relay.peers.isEmpty {
                            NearbyRelaysView(discovery: relay.discovery, choose: relay.chooseNearby)
                            Text("On the other Mac, enable sharing and AirDrop its pairing here.").foregroundStyle(.secondary)
                            Button("Open pairing…") { relay.importPairing() }.buttonStyle(.borderedProminent)
                        } else {
                            Picker("Use", selection: $relay.selected) {
                                ForEach(relay.peers) { Text($0.name).tag(Optional($0.id)) }
                            }.disabled(relay.routing || relay.busy || codex.installed)
                            if let peer = relay.selectedPeer, !relay.selectedTrusted {
                                Text(RelayDiscovery.code(peer.fingerprint)).font(.title2.monospaced())
                                Button("Codes match") { relay.trustSelected() }
                            }
                            Toggle("Connect", isOn: Binding(get: { relay.routing }, set: { if $0 { Task { await relay.startRouting() } } else { relay.stopRouting() } })).disabled(relay.busy || !relay.selectedTrusted)
                            if relay.busy { ProgressView().controlSize(.small) }
                            Text(relay.status).foregroundStyle(.secondary)
                            Text("Peer text + images: experimental API. Official ChatGPT: network relay, local account.").font(.caption).foregroundStyle(.secondary)
                            DisclosureGroup("Test and integration") {
                                VStack(alignment: .leading, spacing: 10) {
                                    TextField("Model", text: $relay.turnModel).disabled(relay.testingTurn || codex.installed).onChange(of: relay.turnModel) { _, value in relay.turnVerified = false; UserDefaults.standard.set(value, forKey: "turnModel") }
                                    Picker("Check", selection: $imageCheck) { Text("Text").tag(false); Text("Image").tag(true) }.pickerStyle(.segmented)
                                    Button(relay.testingTurn ? "Checking…" : "Run generation check") { relay.checkTurn(image: imageCheck) }.disabled(!relay.routing || relay.testingTurn)
                                    Text(relay.turnStatus).font(.caption)
                                    if codex.installed {
                                        Button("Restore Codex") { codex.restoreConfig() }
                                    } else {
                                        Button("Use in Codex…") { confirmCodex = true }.disabled(!relay.turnVerified)
                                    }
                                    Text("Checks submit one prompt using the peer’s account. Endpoint: http://127.0.0.1:18322/v1/responses. Use the pairing access key as bearer authentication; stream=true, full history in input. Image tool: image_generation.").font(.caption).textSelection(.enabled)
                                }
                            }
                            DisclosureGroup("ChatGPT network relay") {
                                VStack(alignment: .leading, spacing: 10) {
                                    Text("Uses this Mac’s ChatGPT account. Quit ChatGPT before launching.").font(.caption)
                                    Button("Launch ChatGPT") { relay.launchChatGPT() }.disabled(!relay.routing)
                                    Toggle("Include Node networking", isOn: $relay.includeNodeProxy)
                                }
                            }
                            DisclosureGroup("More") {
                                Button("Pair another Mac…") { relay.importPairing() }
                                Button("Forget this Mac", role: .destructive) { relay.removeSelected() }.disabled(relay.routing || codex.installed)
                            }
                        }
                    }
                    if !relay.error.isEmpty { Text(relay.error).font(.callout).foregroundStyle(.orange).textSelection(.enabled) }
                }.padding(.top, 4)
            }
        }.padding(28).frame(width: 560, height: 610)
        .onAppear { relay.discovery.start() }
        .alert("Use this peer in Codex?", isPresented: $confirmCodex) {
            Button("Cancel", role: .cancel) {}
            Button("Configure") { relay.configureCodex(); codex.updateInstalled() }
        } message: { Text("Quit Codex first. Your settings are backed up. Restart and start a new chat. Restore before switching peers. Pausing blocks this endpoint.") }
        .alert("Revoke pairings?", isPresented: $confirmRevoke) {
            Button("Cancel", role: .cancel) {}; Button("Revoke all", role: .destructive) { relay.revokeAll() }
        } message: { Text("Disconnects your Macs. Pair again to reconnect.") }
    }
}

private struct NearbyRelaysView: View {
    @ObservedObject var discovery: RelayDiscovery
    let choose: (NearbyRelay) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if discovery.nearby.isEmpty { Text("No sharing Macs found yet. Turn on sharing on your other Mac.").foregroundStyle(.secondary) }
            ForEach(discovery.nearby) { mac in
                Button { choose(mac) } label: { Label(mac.name, systemImage: "desktopcomputer") }
            }
            Text(discovery.status).font(.caption).foregroundStyle(.secondary)
        }
    }
}
