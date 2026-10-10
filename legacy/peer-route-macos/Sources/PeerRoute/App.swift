import SwiftUI
import AppKit

@main
struct PeerRouteApp: App {
    @StateObject private var store = RouteStore()
    @StateObject private var sharing = SharingStore()
    @StateObject private var chatGPTProbe = ChatGPTProbeStore()
    @StateObject private var desktopRelay = DesktopRelayStore()
    @StateObject private var tlsProbe = ChatGPTTLSProbeStore()
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    var body: some Scene {
        WindowGroup("Peer Route", id: "main") {
            MainView(store: store).environmentObject(store).environmentObject(sharing).environmentObject(chatGPTProbe).environmentObject(desktopRelay).environmentObject(tlsProbe)
                .frame(minWidth: 620, minHeight: 700)
                .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in store.pause(); sharing.quit(); chatGPTProbe.stop(); desktopRelay.stopAll(); tlsProbe.quit() }
        }
        .defaultSize(width: 660, height: 760)
        .windowStyle(.hiddenTitleBar)
        MenuBarExtra("Peer Route", systemImage: store.activePeer == nil && !desktopRelay.routing ? "point.3.connected.trianglepath.dotted" : "network") {
            TrayView(store: store).environmentObject(sharing).environmentObject(chatGPTProbe).environmentObject(desktopRelay).environmentObject(tlsProbe)
        }.menuBarExtraStyle(.window)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
}

struct TrayView: View {
    @EnvironmentObject var tlsProbe: ChatGPTTLSProbeStore
    @EnvironmentObject var desktopRelay: DesktopRelayStore
    @EnvironmentObject var chatGPTProbe: ChatGPTProbeStore
    @EnvironmentObject var sharing: SharingStore
    @ObservedObject var store: RouteStore
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack { Text("Peer Route").font(.headline); Spacer(); Circle().fill(desktopRelay.routing ? Color.green : Color.secondary).frame(width: 7, height: 7) }
            Text("Send through").font(.caption).foregroundStyle(.secondary)
            if desktopRelay.peers.isEmpty { Text("Pair a Mac in Settings.").foregroundStyle(.secondary) }
            ForEach(desktopRelay.peers) { peer in
                Button {
                    desktopRelay.selected = peer.id; Task { await desktopRelay.startRouting() }
                } label: { HStack { Image(systemName: desktopRelay.routing && desktopRelay.selected == peer.id ? "checkmark.circle.fill" : "circle"); Text(peer.name); Spacer() } }.disabled(desktopRelay.busy || store.installed)
            }
            if desktopRelay.routing { Button("Pause") { desktopRelay.stopRouting() } }
            if desktopRelay.sharing { Label("Sharing this Mac", systemImage: "personalhotspot").font(.caption) }
            if !desktopRelay.error.isEmpty { Text(desktopRelay.error).font(.caption).foregroundStyle(.orange).lineLimit(3) }
            Divider()
            Button("Settings…") { openWindow(id: "main"); NSApp.activate(ignoringOtherApps: true) }
            Button("Quit") { store.pause(); sharing.quit(); chatGPTProbe.stop(); desktopRelay.stopAll(); tlsProbe.quit(); NSApp.terminate(nil) }
        }.padding(20).frame(width: 280)
    }
}

struct MainView: View {
    @ObservedObject var store: RouteStore
    @EnvironmentObject var desktopRelay: DesktopRelayStore
    @State private var advanced = false
    var body: some View {
        VStack(spacing: 0) {
            DesktopRelayView(embedded: true)
            Divider()
            HStack {
                Text("v0.8.3 · macOS preview").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Advanced…") { advanced = true }.buttonStyle(.plain)
            }.padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .sheet(isPresented: $advanced) { LegacyRouteView(store: store).frame(width: 980, height: 760) }
        .onOpenURL { url in
            if url.isFileURL && url.pathExtension.lowercased() == "peerrelay" { desktopRelay.importPairing(from: url) }
        }
    }
}

struct LegacyRouteView: View {
    @EnvironmentObject var tlsProbe: ChatGPTTLSProbeStore
    @EnvironmentObject var desktopRelay: DesktopRelayStore
    @EnvironmentObject var chatGPTProbe: ChatGPTProbeStore
    @EnvironmentObject var sharing: SharingStore
    @ObservedObject var store: RouteStore
    @State private var selection: UUID?
    @State private var confirmInstall = false
    @State private var confirmRemove: Peer?
    @State private var showSetup = false
    @State private var showSharing = false
    @State private var showChatGPTProbe = false
    @State private var showChatGPTTrace = false
    @State private var showTLSProbe = false
    @State private var showDesktopRelay = false
    private let accent = Color(red: 0.66, green: 0.86, blue: 0.45)
    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 18) {
                HStack { Image(systemName: "arrow.triangle.branch").foregroundStyle(accent); Text("Peer Route").font(.title3.bold()) }.padding(.top, 20)
                Text("YOUR MACHINES").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                ScrollView {
                    VStack(spacing: 8) {
                        ForEach(store.saved.peers) { peer in
                            Button { selection = peer.id } label: {
                                HStack(spacing: 10) {
                                    Image(systemName: "desktopcomputer").font(.title3)
                                    VStack(alignment: .leading, spacing: 3) { Text(peer.name).fontWeight(.medium); Text(peer.alias).font(.caption).foregroundStyle(.secondary) }
                                    Spacer()
                                    if store.activePeer == peer.id { Circle().fill(accent).frame(width: 7, height: 7) }
                                }.padding(12).background(selection == peer.id ? Color.white.opacity(0.09) : .clear, in: RoundedRectangle(cornerRadius: 10))
                            }.buttonStyle(.plain)
                        }
                    }
                }
                Button { store.importPairing() } label: { Label("Pair a machine…", systemImage: "plus") }.disabled(store.busy || store.installed)
                Button { showSharing = true } label: { Label("Share this Mac…", systemImage: "personalhotspot") }
                Spacer()
                Button { showSetup.toggle() } label: { Label("Setup guide", systemImage: "questionmark.circle") }.buttonStyle(.plain).foregroundStyle(.secondary)
                Text("macOS preview").font(.caption2).foregroundStyle(.tertiary)
            }.padding(22).frame(width: 250).background(Color(nsColor: .windowBackgroundColor).opacity(0.6))
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    HStack {
                        Text("YOUR ACCOUNT. YOUR MACHINE.").font(.caption.weight(.medium)).tracking(1.8).foregroundStyle(accent)
                        Spacer()
                        Label(store.activePeer == nil ? "Offline" : "Connected", systemImage: "circle.fill").font(.caption).foregroundStyle(store.activePeer == nil ? Color.secondary : accent)
                    }
                    Text("Choose where\nthe answer comes from.").font(.system(size: 38, weight: .medium)).tracking(-1.3)
                    Text("Use Codex through the account configured on a trusted peer. Your local tools keep running on this Mac.")
                        .foregroundStyle(.secondary)
                    if let peer = store.saved.peers.first(where: { $0.id == selection }) {
                        machineCard(peer)
                    } else {
                        VStack(alignment: .leading, spacing: 12) {
                            Image(systemName: "desktopcomputer.and.arrow.down").font(.largeTitle).foregroundStyle(accent)
                            Text("Add your first machine").font(.title3.bold())
                            Text("On the other Mac, open Share this Mac, sign in, and export a private pairing file. Import it here to connect securely.").foregroundStyle(.secondary)
                            Button("Import pairing file…") { store.importPairing() }.buttonStyle(.borderedProminent).tint(accent).foregroundStyle(.black)
                        }.card()
                    }
                    VStack(alignment: .leading, spacing: 14) {
                        Text("CLIENT SUPPORT").font(.caption2).foregroundStyle(.secondary)
                        HStack { Label("Codex", systemImage: "terminal"); Spacer(); Text(store.probePassed ? "Model response verified" : "Adapter available · test required").font(.caption).foregroundStyle(store.probePassed ? accent : Color.secondary) }
                        Divider()
                        HStack { Label("ChatGPT desktop", systemImage: "bubble.left.and.bubble.right"); Spacer(); Text("Network relay · experimental").font(.caption).foregroundStyle(.orange) }
                        Text("Peer-account Responses streaming is available in Connect your Macs. Automatic ChatGPT UI translation remains unfinished; network relay keeps the local account.").font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Connect your Macs…") { showDesktopRelay = true }
                            Button("TLS compatibility experiment…") { showTLSProbe = true }
                        }
                        HStack {
                            Button("Connection diagnostics…") { showChatGPTProbe = true }
                            Button("Trace ChatGPT API metadata…") { showChatGPTTrace = true }
                        }
                    }.card()
                    VStack(alignment: .leading, spacing: 12) {
                        Text("CODEX INTEGRATION").font(.caption2).foregroundStyle(.secondary)
                        if !store.models.isEmpty {
                            Picker("Model", selection: $store.saved.model) { ForEach(store.models, id: \.self) { Text($0).tag($0) } }
                                .disabled(store.installed || store.busy)
                                .onChange(of: store.saved.model) { _, _ in store.probePassed = false; store.persist() }
                        }
                        HStack {
                            Button("Test model response") { Task { await store.testResponse() } }.disabled(store.current == nil || store.busy)
                            if store.installed {
                                Button("Restore Codex settings") { store.restoreConfig() }.disabled(store.busy)
                            } else {
                                Button("Enable in Codex…") { confirmInstall = true }.disabled(!store.probePassed || store.busy)
                            }
                        }
                        Text(store.installed ? "A Codex configuration is installed. Restart Codex after any configuration change. Pause or quit blocks this endpoint; restore to return to your prior configuration. Restore before switching peers or editing their keys." : "After the model test passes, enable this route in Codex. Existing config is backed up. The peer client key is written to a private Codex config; OAuth credentials stay on the peer.")
                            .font(.caption).foregroundStyle(.secondary)
                        Text("Account label is provided by you. A successful model test confirms a response through the proxy, not the upstream account identity. Configure a single Codex OAuth account on the peer to make routing deterministic.").font(.caption).foregroundStyle(.secondary)
                    }.card()
                    if store.busy { ProgressView().controlSize(.small) }
                    Text(store.status).font(.callout).foregroundStyle(.secondary)
                    if !store.error.isEmpty { Label(store.error, systemImage: "exclamationmark.triangle").font(.callout).foregroundStyle(.orange).textSelection(.enabled) }
                }.padding(32)
            }.background(Color(nsColor: .controlBackgroundColor))
        }
        .preferredColorScheme(.dark)
        .onAppear { selection = store.saved.selected ?? store.saved.peers.first?.id }
        .onChange(of: store.saved.peers) { _, peers in if !peers.contains(where: { $0.id == selection }) { selection = peers.first?.id } }
        .sheet(isPresented: $store.showEditor) { PeerEditor(store: store, peer: store.editing) }
        .sheet(isPresented: $showSetup) { SetupView() }
        .sheet(isPresented: $showChatGPTProbe) { ChatGPTProbeView().environmentObject(chatGPTProbe) }
        .sheet(isPresented: $showDesktopRelay) { DesktopRelayView().environmentObject(desktopRelay) }
        .onOpenURL { url in
            guard url.isFileURL, url.pathExtension.lowercased() == "peerrelay" else { return }
            desktopRelay.importPairing(from: url); showDesktopRelay = true
        }
        .sheet(isPresented: $showTLSProbe) { ChatGPTTLSProbeView().environmentObject(tlsProbe) }
        .sheet(isPresented: $showChatGPTTrace) { ChatGPTTraceView() }
        .sheet(isPresented: $showSharing) { SharingView().environmentObject(sharing) }
        .alert("Configure Codex to use this peer?", isPresented: $confirmInstall) {
            Button("Cancel", role: .cancel) {}
            Button("Enable route") { store.installConfig() }
        } message: {
            Text("Quit Codex before applying. This updates ~/.codex/config.toml and stores a private restore backup. New chats use the selected proxy; start a new chat when changing accounts. The proxy client key is written to the config with owner-only permissions. Existing ChatGPT sign-in is preserved. Voice and other account-dependent features have not been verified.")
        }
        .alert("Remove machine?", isPresented: Binding(get: { confirmRemove != nil }, set: { if !$0 { confirmRemove = nil } })) {
            Button("Cancel", role: .cancel) { confirmRemove = nil }
            Button("Remove", role: .destructive) { if let peer = confirmRemove { store.remove(peer) }; confirmRemove = nil }
        } message: { Text("Removes this machine and its saved client key from Peer Route. This does not revoke the key on the peer.") }
    }

    @ViewBuilder private func machineCard(_ peer: Peer) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack { Image(systemName: "desktopcomputer").font(.title); VStack(alignment: .leading) { Text(peer.name).font(.title2.weight(.semibold)); Text(peer.alias).font(.caption).foregroundStyle(.secondary) }; Spacer(); if let ms = store.latency, store.activePeer == peer.id { Text("\(ms) ms").font(.caption.monospacedDigit()).foregroundStyle(accent) } }
            LabeledContent("Account label", value: peer.accountLabel.isEmpty ? "Not provided" : peer.accountLabel).font(.callout)
            LabeledContent("Peer endpoint", value: "\(peer.alias):\(peer.port)").font(.caption).foregroundStyle(.secondary)
            HStack {
                if store.activePeer == peer.id {
                    Button("Pause routing") { store.pause() }.disabled(store.busy)
                } else {
                    Button("Connect to machine") { Task { await store.connect(peer) } }
                        .buttonStyle(.borderedProminent).tint(accent).foregroundStyle(.black)
                        .disabled(store.busy || (store.installed && store.saved.selected != peer.id))
                }
                Spacer()
                Button("Edit") { store.editing = peer; store.showEditor = true }.disabled(store.busy || store.installed)
                Button { confirmRemove = peer } label: { Image(systemName: "trash") }.disabled(store.busy || store.installed)
            }
            Text("Certificate fingerprint: \(peer.fingerprint)").textSelection(.enabled)
            Text("TLS is encrypted and pinned to this certificate. Compare this fingerprint with Share this Mac on the peer before connecting.").font(.caption).foregroundStyle(.secondary)
        }.card()
    }
}

extension View {
    func card() -> some View { self.padding(20).frame(maxWidth: .infinity, alignment: .leading).background(.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 16)).overlay(RoundedRectangle(cornerRadius: 16).stroke(.white.opacity(0.09))) }
}

struct PeerEditor: View {
    @ObservedObject var store: RouteStore
    let peer: Peer?
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var alias = ""
    @State private var port = "18443"
    @State private var fingerprint = ""
    @State private var account = ""
    @State private var key = ""
    @State private var error = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(peer == nil ? "Add a machine" : "Edit machine").font(.title2.bold())
            Text("Connect directly to a trusted Mac using its pinned TLS identity.").foregroundStyle(.secondary)
            Form {
                TextField("Machine name", text: $name)
                TextField("Mac hostname or IPv4 address", text: $alias)
                TextField("Peer port", text: $port)
                TextField("Certificate SHA-256 fingerprint", text: $fingerprint)
                TextField("Account label (optional)", text: $account)
                SecureField("Proxy client key", text: $key)
            }
            Text("Use a CLIProxyAPI client key, not an OpenAI API key or ChatGPT session token. The key is stored in macOS Keychain.").font(.caption).foregroundStyle(.secondary)
            if !error.isEmpty { Text(error).foregroundStyle(.orange).font(.caption) }
            HStack {
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Spacer()
                Button("Save machine") {
                    do {
                        var value = peer ?? Peer(name: name, alias: alias)
                        value.name = name; value.alias = alias; value.port = Int(port) ?? 0; value.accountLabel = account; value.fingerprint = fingerprint
                        if store.activePeer == value.id { store.pause() }
                        try store.savePeer(value, key: key); dismiss()
                    } catch { self.error = error.localizedDescription }
                }.keyboardShortcut(.defaultAction)
            }
        }.padding(28).frame(width: 480)
        .onAppear { if let peer { name = peer.name; alias = peer.alias; port = String(peer.port); account = peer.accountLabel; fingerprint = peer.fingerprint } }
    }
}

struct SetupView: View {
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Pair your Macs").font(.title2.bold())
            Text("1. Install this app on both Macs. No SSH setup or separately installed CLIProxyAPI is required.")
            Text("2. On the account-serving Mac, open Share this Mac. Sign in to Codex with your ChatGPT account, then enable sharing.")
            Text("3. Set its reachable .local hostname or LAN IPv4 address and export a private pairing file. Transfer it to your trusted Mac and import it with Pair a machine.")
            Text("4. Compare the certificate fingerprint on both Macs. Connect, choose a model, and run the response test.")
            Text("5. Quit Codex, enable its route, then restart Codex and start a new chat. Restore settings before switching account peers.")
            Text("Pairing files contain a bearer access key. Keep them private. The host uses TLS on port 18443; macOS may ask you to allow incoming connections. Use the same LAN or your own private VPN. The app does not open router ports or install certificates into system trust.").font(.caption).foregroundStyle(.secondary)
            Text("ChatGPT network relaying and TLS compatibility research are separate from the Codex adapter. ChatGPT peer-account session reuse remains unimplemented.").font(.caption).foregroundStyle(.orange)
            HStack { Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(28).frame(width: 580)
    }
}

struct SharingView: View {
    @EnvironmentObject var sharing: SharingStore
    @Environment(\.dismiss) private var dismiss
    @State private var confirmShare = false
    @State private var confirmRevoke = false
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label("Share this Mac", systemImage: "personalhotspot").font(.title2.bold())
            Text("The bundled engine serves requests using the Codex OAuth account saved on this Mac.").foregroundStyle(.secondary)
            Form {
                TextField("Machine name", text: $sharing.name)
                TextField("Reachable hostname or IPv4", text: $sharing.host)
            }.disabled(sharing.running)
            HStack {
                Button(sharing.signingIn ? "Finish in browser…" : "Sign in with ChatGPT") { sharing.signIn() }.disabled(sharing.signingIn)
                if sharing.running { Button("Stop sharing") { sharing.stop() } }
                else { Button("Enable sharing…") { confirmShare = true }.buttonStyle(.borderedProminent) }
            }
            HStack {
                Button("Export pairing file…") { sharing.exportPairing() }
                Button("Revoke all pairings…", role: .destructive) { confirmRevoke = true }
            }
            if !sharing.fingerprint.isEmpty {
                Text("CERTIFICATE SHA-256").font(.caption2).foregroundStyle(.secondary)
                Text(sharing.fingerprint).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            }
            Text(sharing.status).font(.callout)
            if !sharing.error.isEmpty { Text(sharing.error).font(.caption).foregroundStyle(.orange) }
            Text("Each pairing file grants access to your configured model account. Transfer it privately and compare fingerprints on both Macs. Revoke all invalidates every exported key. The app must stay running to share. This preview uses pairing files rather than short codes or per-device approval.").font(.caption).foregroundStyle(.secondary)
            HStack { Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(28).frame(width: 580)
        .alert("Allow paired devices to use this account?", isPresented: $confirmShare) {
            Button("Cancel", role: .cancel) {}
            Button("Enable sharing") { sharing.start() }
        } message: { Text("Starts an authenticated HTTPS endpoint on port 18443 on this Mac's network interfaces. Devices with your pairing key can consume this account's model allowance. Remote management is disabled. No router ports are opened.") }
        .alert("Revoke all pairing keys?", isPresented: $confirmRevoke) {
            Button("Cancel", role: .cancel) {}
            Button("Revoke all", role: .destructive) { sharing.revokeAll() }
        } message: { Text("Stops sharing and rotates the access key. All previously exported pairing files stop working. Export new files to reconnect your trusted Macs.") }
    }
}


struct ChatGPTProbeView: View {
    @EnvironmentObject var probe: ChatGPTProbeStore
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("ChatGPT desktop · experimental investigation").font(.title2.bold())
            Text("Stage 1: observe destination hosts from the official local app. This diagnostic tunnel connects directly to OpenAI; it does not use the peer’s account.").foregroundStyle(.secondary)
            Text("1. Start diagnostics.\n2. Save your work and fully quit ChatGPT yourself.\n3. Copy and run the launch command in Terminal.\n4. Send a harmless test chat, generate a test image, then inspect the host list.")
            Text(probe.command).font(.caption.monospaced()).textSelection(.enabled)
            HStack {
                if probe.running { Button("Stop diagnostics") { probe.stop() } }
                else { Button("Start diagnostics") { probe.start() } }
                Button("Copy launch command") { probe.copyCommand() }
                Spacer()
                Button("Clear observations") { probe.observations = [] }
            }
            Text(probe.status).font(.callout).foregroundStyle(probe.running ? Color.green : Color.secondary)
            List(probe.observations) { item in
                VStack(alignment: .leading, spacing: 4) {
                    HStack { Text(item.host).font(.callout.monospaced()); Spacer(); Text("\(item.connections) attempts").font(.caption) }
                    Text(item.status).font(.caption).foregroundStyle(.secondary)
                }
            }.frame(height: 170)
            Text("No certificate installation or TLS decryption. The list proves only that a TCP tunnel was opened, not which API, feature, or account was used. Some app networking may ignore the proxy flag. Empty results do not prove no traffic occurred. Only HTTPS to OpenAI/ChatGPT, their asset domains, and Auth0 is allowed.").font(.caption).foregroundStyle(.secondary)
            Text("To finish: quit ChatGPT, stop diagnostics, then reopen ChatGPT normally without the launch flag. Stopping while the flagged app is running will interrupt its proxied connections. Closing this sheet keeps diagnostics running.").font(.caption).foregroundStyle(.orange)
            HStack { Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(28).frame(width: 680)
    }
}


struct ChatGPTTraceView: View {
    @State private var showFrames = false
    private var socketRows: [WebSocketTrace] {
        var rows = trace.report?.webSockets ?? []
        if let frames = trace.report?.unattributedFrames, !frames.isEmpty {
            rows.append(WebSocketTrace(id: 0, host: "unattributed", path: "/:unmapped", attribution: "App-wide frame aggregate; source not matched to a socket URL. May overlap other records.", frames: frames))
        }
        return rows
    }
    @StateObject private var trace = ChatGPTTraceStore()
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("ChatGPT API metadata · experimental").font(.title2.bold())
            Text("Stage 2 uses Chromium’s own NetLog. No interception certificate is needed. This still uses your local ChatGPT account.").foregroundStyle(.secondary)
            Text("1. Prepare trace and copy the command.\n2. Fully quit ChatGPT, stop the old connection diagnostics, then run the command.\n3. Send a harmless text prompt and generate one test image.\n4. Fully quit ChatGPT again to finish the log, then import it here.\n5. Export the sanitized report and reopen ChatGPT normally.").font(.callout)
            HStack {
                Button("Prepare trace") { trace.prepare() }.disabled(trace.busy)
                Button("Copy trace command") { trace.copyCommand() }.disabled(trace.command.isEmpty)
                Button("Show private trace") { trace.revealRawFolder() }.disabled(trace.command.isEmpty)
            }
            if !trace.command.isEmpty { Text(trace.command).font(.caption.monospaced()).textSelection(.enabled).lineLimit(5) }
            HStack {
                Button("Import and sanitize NetLog…") { trace.importTrace() }.disabled(trace.busy)
                Button("Export sanitized report…") { trace.exportReport() }.disabled(trace.report == nil || trace.busy)
                if trace.busy { ProgressView().controlSize(.small) }
            }
            Text(trace.status).font(.caption).foregroundStyle(.secondary)
            Toggle("Show WebSocket frame shapes", isOn: $showFrames).toggleStyle(.switch)
            if showFrames {
                List(socketRows) { socket in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(socket.attribution).font(.caption2).foregroundStyle(.secondary)
                        Text("\(socket.host)\(socket.path)").font(.caption.monospaced()).textSelection(.enabled)
                        Text("\(socket.frames.filter { $0.direction == "sent" }.count) sent · \(socket.frames.filter { $0.direction == "received" }.count) received · payloads excluded").font(.caption2).foregroundStyle(.secondary)
                        ForEach(Array(socket.frames.prefix(12).enumerated()), id: \.offset) { _, frame in
                            Text("\(frame.direction) · \(frame.kind) · \(frame.sizeBand)").font(.caption2.monospaced())
                        }
                        if socket.frames.count > 12 { Text("More frame shapes are available in the sanitized export.").font(.caption2).foregroundStyle(.secondary) }
                    }
                }.frame(height: 180)
            } else {
                List(trace.report?.requests ?? []) { request in
                    VStack(alignment: .leading, spacing: 3) {
                        Text("\(request.method) \(request.host)\(request.path)").font(.caption.monospaced()).textSelection(.enabled)
                        Text("Status: \(request.status.map(String.init) ?? "unobserved") · \(request.responseKind ?? "type unobserved") · \(request.outcome)").font(.caption2).foregroundStyle(.secondary)
                    }
                }.frame(height: 180)
            }
            if trace.report?.truncated == true { Text("Report truncated by size/event limits. Capture a shorter test.").font(.caption).foregroundStyle(.orange) }
            Text("Keep the raw NetLog private: even Default mode can contain sensitive URLs and network metadata. Only export the sanitized report. It removes queries, path identifiers, header values, bodies, timestamps, and unrelated hosts; unknown path segments become :redacted. Raw files remain until you remove them in Finder.").font(.caption).foregroundStyle(.orange)
            Text("NetLog may omit non-Chromium traffic. This importer reports WebSocket opcodes and size bands, without reading payloads, and only labels SSE or WebSocket when matching response headers establish it. A missing status/type is unknown, not failure. No account switching is implemented.").font(.caption).foregroundStyle(.secondary)
            HStack { Spacer(); Button("Done") { dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(26).frame(width: 740)
    }
}
