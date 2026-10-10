import SwiftUI
import AppKit

/// Opt-in compatibility research only. Does not install trust, patch ChatGPT, or transfer sessions.
@MainActor final class ChatGPTTLSProbeStore: ObservableObject {
    @Published var pythonPath: String = {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return [home + "/.local/bin/python3", "/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"].first { FileManager.default.isExecutableFile(atPath: $0) } ?? "/usr/bin/python3"
    }()
    @Published var busy = false
    @Published var running = false
    @Published var status = "TLS compatibility experiment is off"
    @Published var error = ""
    @Published var summary = "No live compatibility evidence yet"
    @Published var includeNodeProxy = true
    private var setupProcess: Process?
    private var process: Process?
    private var poll: Task<Void, Never>?
    private var generation = UUID()
    var directory: URL { Files.directory.appendingPathComponent("experimental-tls", isDirectory: true) }
    var conf: URL { directory.appendingPathComponent("ca", isDirectory: true) }
    var report: URL { directory.appendingPathComponent("schema-report.json") }
    var python: URL { directory.appendingPathComponent("helper/bin/python") }
    var certificate: URL { conf.appendingPathComponent("mitmproxy-ca-cert.cer") }
    var ready: Bool { FileManager.default.isExecutableFile(atPath: python.path) && FileManager.default.fileExists(atPath: certificate.path) }
    var command: String { ChatGPTLaunch.command(port: 18320, includeNode: includeNodeProxy) }
    private func resource(_ name: String) throws -> URL {
        guard let url = Bundle.main.resourceURL?.appendingPathComponent(name), FileManager.default.fileExists(atPath: url.path) else { throw RouteError.message("Experimental helper resources missing. Use the packaged app build.") }
        return url
    }
    private func execute(_ executable: URL, _ arguments: [String]) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let p = Process(); p.executableURL = executable; p.arguments = arguments
            p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice; p.standardInput = FileHandle.nullDevice
            var environment = ProcessInfo.processInfo.environment
            environment.removeValue(forKey: "SSLKEYLOGFILE"); environment.removeValue(forKey: "MITMPROXY_SSLKEYLOGFILE")
            p.environment = environment
            p.terminationHandler = { p in
                if p.terminationStatus == 0 { continuation.resume() }
                else { continuation.resume(throwing: RouteError.message("Experimental helper setup failed (\(p.terminationStatus)). Check Python 3.12+ and network access, then retry.")) }
            }
            setupProcess = p
            do { try p.run() } catch { p.terminationHandler = nil; continuation.resume(throwing: error) }
        }
        setupProcess = nil
    }
    func prepare() async {
        guard !busy, !running else { return }
        busy = true; error = ""; defer { busy = false }
        do {
            let script = try resource("tls_probe.py"), requirements = try resource("tls-probe-requirements.txt")
            guard pythonPath.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: pythonPath) else { throw RouteError.message("Choose an absolute path to a Python 3.12+ executable.") }
            try Files.prepare(directory); try Files.prepare(conf)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
            let base = URL(fileURLWithPath: pythonPath)
            status = "Checking Python 3.12+…"
            try await execute(base, ["-c", "import sys;sys.exit(0 if sys.version_info >= (3,12) else 1)"])
            if !FileManager.default.isExecutableFile(atPath: python.path) { try await execute(base, ["-m", "venv", directory.appendingPathComponent("helper").path]) }
            status = "Installing locked mitmproxy helper dependencies…"
            try await execute(python, ["-m", "pip", "install", "--disable-pip-version-check", "--require-hashes", "--requirement", requirements.path])
            status = "Generating a private experimental CA; no trust changes…"
            try await execute(python, [script.path, "--directory", conf.path, "--report", report.path, "--prepare"])
            status = "Helper ready. Trust only the public certificate for this test; remove its trust afterward."
        } catch { self.error = error.localizedDescription; status = "Setup incomplete" }
    }
    func start() {
        guard ready, !busy, !running else { return }
        stop(); error = ""
        let ticket = generation
        do {
            let p = Process(); p.executableURL = python
            p.arguments = [try resource("tls_probe.py").path, "--directory", conf.path, "--report", report.path, "--consent-local-tls"]
            var environment = ProcessInfo.processInfo.environment
            environment.removeValue(forKey: "SSLKEYLOGFILE"); environment.removeValue(forKey: "MITMPROXY_SSLKEYLOGFILE")
            p.environment = environment; p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice; p.standardInput = FileHandle.nullDevice
            try Files.privateWrite(Data("{}".utf8), to: report)
            p.terminationHandler = { [weak self] _ in Task { @MainActor in
                guard let self, self.generation == ticket else { return }
                self.running = false; self.busy = false; self.status = "TLS experiment stopped. Reopen ChatGPT normally to restore direct networking."
            } }
            process = p; busy = true; status = "Starting loopback TLS probe…"; try p.run()
            poll = Task { [weak self] in
                var attempts = 0
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(1)) } catch { return }
                    guard let self, self.generation == ticket, self.process?.isRunning == true else { return }
                    attempts += 1
                    if let data = try? Data(contentsOf: self.report), data.count <= 2_000_000,
                       let document = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
                        if document["listenerReady"] as? Bool == true { self.running = true; self.busy = false; self.status = "Listening on 127.0.0.1:18320 · current local account" }
                        let events = document["events"] as? [[String: Any]] ?? []
                        let requests = events.filter { $0["kind"] as? String == "request" }.count
                        let streams = events.filter { $0["responseKind"] as? String == "sse" }.count
                        let sockets = events.filter { $0["kind"] as? String == "websocket-shape" }.reduce(0) { $0 + ($1["occurrences"] as? Int ?? 1) }
                        let failures = events.filter { $0["kind"] as? String == "client-tls-failed" }.count
                        self.summary = "\(requests) requests · \(streams) SSE responses · \(sockets) WebSocket shapes · \(failures) client TLS failures"
                    }
                    if attempts >= 10 && !self.running { self.stop(); self.error = "Probe did not become ready. Port 18320 may be occupied or helper setup is incomplete."; return }
                }
            }
        } catch { stop(); self.error = error.localizedDescription }
    }
    func stop() {
        generation = UUID(); poll?.cancel(); poll = nil
        if let p = process, p.isRunning { p.terminate() }; process = nil
        running = false; busy = false; status = "TLS experiment is off. Quit and reopen ChatGPT normally to stop using its proxy flag."
    }
    func launchChatGPT() {
        do { try ChatGPTLaunch.launch(port: 18320, includeNode: includeNodeProxy) }
        catch { self.error = error.localizedDescription }
    }

    func quit() { stop(); if let p = setupProcess, p.isRunning { p.terminate() } }
    func revealCertificate() { NSWorkspace.shared.activateFileViewerSelecting([certificate]) }
    func revealReport() { NSWorkspace.shared.activateFileViewerSelecting([report]) }
    func copyCommand() { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(command, forType: .string) }
}

struct ChatGPTTLSProbeView: View {
    @EnvironmentObject var probe: ChatGPTTLSProbeStore
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("ChatGPT · local TLS compatibility experiment").font(.title2.bold())
            Text("This test decrypts only chatgpt.com, ws.chatgpt.com and codex-cloud-backend.chatgpt.com HTTPS on port 443 in a local proxy. It forwards requests with your existing local account and exports sanitized protocol shapes. B’s account is not used yet.").foregroundStyle(.secondary)
            TextField("Python 3.12+ executable", text: $probe.pythonPath).disabled(probe.busy || probe.running)
            HStack {
                Button("Install helper and prepare certificate") { Task { await probe.prepare() } }.disabled(probe.busy || probe.running)
                Button("Reveal public certificate") { probe.revealCertificate() }.disabled(!probe.ready || probe.busy)
            }
            Text("1. Prepare the helper (downloads locked Python dependencies).\n2. Open the public mitmproxy-ca-cert.cer in Keychain Access. Add it to your login keychain and explicitly trust it for SSL for this experiment. Never transfer the CA private key.\n3. Start the probe. Save work and fully quit ChatGPT yourself, then run the launch command.\n4. Send one harmless text prompt and generate one harmless image.\n5. Fully quit ChatGPT, stop the probe, remove the experimental CA from Keychain Access, and reopen ChatGPT normally.").font(.callout)
            HStack {
                if probe.running || probe.busy { Button("Stop probe") { probe.stop() }.disabled(probe.busy && !probe.running) }
                else { Button("Start local TLS probe") { probe.start() }.disabled(!probe.ready) }
                Button("Copy launch command") { probe.copyCommand() }.disabled(!probe.running)
                Button("Reveal sanitized report") { probe.revealReport() }.disabled(!FileManager.default.fileExists(atPath: probe.report.path))
            }
            Text(probe.command).font(.caption.monospaced()).textSelection(.enabled)
            Toggle("Include Node networking (experimental)", isOn: $probe.includeNodeProxy)
            Button("Launch ChatGPT with capture") { probe.launchChatGPT() }.disabled(!probe.running)
            Text("Uses Node’s built-in proxy environment for the separate cloud WebSocket, without changing its endpoint or account. Coverage remains unverified in the running app. Fully quit ChatGPT yourself first. This also affects Node child processes; local destinations bypass the Node proxy.").font(.caption).foregroundStyle(.secondary)
            if probe.busy { ProgressView().controlSize(.small) }
            Text(probe.status).foregroundStyle(probe.running ? Color.green : Color.secondary)
            Text(probe.summary).font(.caption.monospaced())
            if !probe.error.isEmpty { Text(probe.error).font(.caption).foregroundStyle(.orange) }
            Text("The CA can decrypt matching traffic while trusted. Use it only for this brief experiment and remove it afterward. The probe does not patch ChatGPT, disable its certificate checks, change system proxy settings, extract saved credentials, write TLS session keys, save raw flows, or replay prompts. If the app rejects the CA, stop and remove trust; pinning is not bypassed.").font(.caption).foregroundStyle(.orange)
            Text("Only fixed field names/types, known event categories, redacted path templates, header-name presence and numeric HTTP status are exported. Prompt text, response text, images, cookies, tokens, IDs, arbitrary JSON keys/values and query values are excluded. Compressed SSE may remain opaque. A passing local fixture does not establish ChatGPT compatibility, image generation or B-account execution.").font(.caption).foregroundStyle(.secondary)
            HStack { Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(26).frame(width: 760)
    }
}
