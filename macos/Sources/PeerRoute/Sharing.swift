import Foundation
import CryptoKit
import AppKit
import Security

struct PairingFile: Codable {
    var version = 1
    var name: String
    var host: String
    var port: Int
    var fingerprint: String
    var clientKey: String
    func peer() throws -> Peer {
        guard version == 1, !clientKey.isEmpty, !clientKey.contains("\n"), !clientKey.contains("\r") else { throw RouteError.message("Invalid pairing file.") }
        let peer = Peer(name: name, alias: host, port: port, fingerprint: fingerprint)
        try peer.validate()
        return peer
    }
}

@MainActor
final class SharingStore: ObservableObject {
    @Published var running = false
    @Published var signingIn = false
    @Published var status = "Sharing is off"
    @Published var error = ""
    @Published var host = ProcessInfo.processInfo.hostName
    @Published var name = Host.current().localizedName ?? "My Mac"
    @Published var fingerprint = ""
    private var engine: Process?
    private var login: Process?
    private let port = 18443
    private let vaultID = UUID(uuidString: "73B00ED2-A480-48E9-B7D4-80C43BA678FC")!
    private var directory: URL { Files.directory.appendingPathComponent("host", isDirectory: true) }
    private var configPath: URL { directory.appendingPathComponent("config.yaml") }
    private var binary: URL { Bundle.main.resourceURL!.appendingPathComponent("cli-proxy-api") }

    private func execute(_ executable: String, _ args: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable); process.arguments = args
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        try process.run(); process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw RouteError.message("Could not generate the local TLS identity.") }
    }
    private func prepare() throws {
        guard FileManager.default.isExecutableFile(atPath: binary.path) else { throw RouteError.message("Bundled proxy engine is missing. Rebuild using scripts/build.sh.") }
        try Files.prepare(directory)
        let auth = directory.appendingPathComponent("oauth")
        try Files.prepare(auth)
        let cert = directory.appendingPathComponent("certificate.pem"), key = directory.appendingPathComponent("private-key.pem")
        if !FileManager.default.fileExists(atPath: cert.path) {
            try execute("/usr/bin/openssl", ["req", "-x509", "-newkey", "rsa:2048", "-sha256", "-nodes", "-keyout", key.path, "-out", cert.path, "-days", "365", "-subj", "/CN=Peer Route"])
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: key.path)
        }
        let der = directory.appendingPathComponent("certificate.der")
        try execute("/usr/bin/openssl", ["x509", "-in", cert.path, "-outform", "DER", "-out", der.path])
        fingerprint = SHA256.hash(data: try Data(contentsOf: der)).map { String(format: "%02x", $0) }.joined()
        let clientKey: String
        if let existing = try? Vault.read(vaultID) { clientKey = existing }
        else {
            var bytes = [UInt8](repeating: 0, count: 32)
            guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else { throw RouteError.message("Could not create a device access key.") }
            clientKey = bytes.map { String(format: "%02x", $0) }.joined()
            try Vault.save(clientKey, for: vaultID)
        }
        let yaml = """
        config-version: 8
        server:
          host: "0.0.0.0"
          port: \(port)
          tls:
            enable: true
            cert: \(try CodexConfig.quoted(cert.path))
            key: \(try CodexConfig.quoted(key.path))
        management:
          allow-remote: false
          secret-key: ""
          disable-control-panel: true
          disable-auto-update-panel: true
        access:
          api-keys:
            - \(try CodexConfig.quoted(clientKey))
        oauth:
          auth-dir: \(try CodexConfig.quoted(auth.path))
        routing:
          strategy: "fill-first"
          retry:
            request-retry: 0
            max-retry-credentials: 1
        requests:
          streaming:
            bootstrap-retries: 0
        observability:
          logs:
            debug: false
            logging-to-file: false
            request-log: false
        """
        try Files.privateWrite(Data(yaml.utf8), to: configPath)
    }
    func start() {
        guard !running else { return }
        error = ""
        do {
            try prepare()
            let process = Process()
            process.executableURL = binary; process.arguments = ["-config", configPath.path, "-local-model"]
            process.currentDirectoryURL = directory
            process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
            process.standardInput = FileHandle.nullDevice
            engine = process
            process.terminationHandler = { [weak self] process in Task { @MainActor in
                guard let self, self.engine === process else { return }
                self.running = false; self.engine = nil; self.status = "Sharing stopped"
                self.error = "Proxy engine exited. Check that port 18443 is free and sign in again if needed."
            } }
            try process.run(); running = true; status = "Starting encrypted peer endpoint…"
            Task {
                try? await Task.sleep(for: .seconds(2))
                if running && process.isRunning { status = "Sharing enabled on port \(port) · run a connection test from the other Mac" }
            }
        } catch { self.error = error.localizedDescription }
    }
    func stop() {
        engine?.terminationHandler = nil
        if engine?.isRunning == true { engine?.terminate() }
        engine = nil; running = false; status = "Sharing is off"
    }
    func signIn() {
        guard !signingIn else { return }
        error = ""
        do {
            try prepare()
            // Keep a single credential file: require explicit local removal before
            // replacing an existing identity, rather than pooling user accounts.
            let existing = try FileManager.default.contentsOfDirectory(at: directory.appendingPathComponent("oauth"), includingPropertiesForKeys: nil).filter { $0.pathExtension == "json" }
            guard existing.isEmpty else { throw RouteError.message("This host already has a saved OAuth account. Use that account, or remove it from the private host OAuth folder before signing in to another.") }
            let process = Process(); process.executableURL = binary
            process.arguments = ["-config", configPath.path, "-codex-login"]
            process.currentDirectoryURL = directory
            process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
            process.standardInput = FileHandle.nullDevice
            login = process; signingIn = true; status = "Complete Codex sign-in in your browser…"
            process.terminationHandler = { [weak self] process in Task { @MainActor in
                guard let self else { return }
                self.signingIn = false; self.login = nil
                self.status = process.terminationStatus == 0 ? "Sign-in completed · start sharing and test from the other Mac" : "Sign-in did not complete. Retry and finish the browser flow."
            } }
            try process.run()
        } catch { self.error = error.localizedDescription; signingIn = false }
    }
    func exportPairing() {
        error = ""
        do {
            try prepare()
            let value = PairingFile(name: name, host: host, port: port, fingerprint: fingerprint, clientKey: try Vault.read(vaultID))
            _ = try value.peer()
            let panel = NSSavePanel(); panel.nameFieldStringValue = "\(name).peerroute"
            panel.message = "This file grants access to this Mac's configured Codex account. Transfer it only to your trusted Mac; compare the certificate fingerprint shown on both devices."
            guard panel.runModal() == .OK, let url = panel.url else { return }
            try Files.privateWrite(JSONEncoder().encode(value), to: url)
            status = "Pairing file exported · keep it private and compare fingerprints"
        } catch { self.error = error.localizedDescription }
    }
    func revokeAll() {
        stop(); Vault.delete(vaultID)
        do { try prepare(); status = "All previous pairing keys revoked. Export a new pairing file to reconnect." }
        catch { self.error = error.localizedDescription }
    }
    func quit() {
        stop(); login?.terminationHandler = nil
        if login?.isRunning == true { login?.terminate() }
    }
}
