import AppKit
import SystemConfiguration

struct NearbyRelay: Identifiable {
    var id: String
    var name: String
    var host: String
    var fingerprint: String
}

@MainActor final class RelayDiscovery: NSObject, ObservableObject, @preconcurrency NetServiceBrowserDelegate, @preconcurrency NetServiceDelegate {
    static let type = "_peerroute._tcp."
    @Published var nearby: [NearbyRelay] = []
    @Published var status = "Looking for nearby Macs…"
    private let browser = NetServiceBrowser()
    private var services: [String: NetService] = [:]
    private var published: NetService?
    private var publishedFingerprint = ""
    private var browsing = false
    static var localHostname: String {
        if let value = SCDynamicStoreCopyLocalHostName(nil) as String? { return value + ".local" }
        return ProcessInfo.processInfo.hostName
    }
    static func code(_ fingerprint: String) -> String {
        let chars = Array(fingerprint.uppercased().prefix(16))
        return stride(from: 0, to: chars.count, by: 4).map { String(chars[$0..<min($0+4, chars.count)]) }.joined(separator: "-")
    }
    func start() {
        guard !browsing else { return }; browsing = true
        browser.delegate = self; browser.searchForServices(ofType: Self.type, inDomain: "local.")
    }
    func publish(name: String, fingerprint: String) {
        unpublish()
        let service = NetService(domain: "local.", type: Self.type, name: name, port: 18444)
        service.delegate = self
        publishedFingerprint = fingerprint
        published = service; service.publish()
    }
    func unpublish() { published?.stop(); published = nil; publishedFingerprint = "" }
    func netServiceDidPublish(_ sender: NetService) {
        guard sender === published else { return }
        sender.setTXTRecord(NetService.data(fromTXTRecord: ["version": Data("1".utf8), "fingerprint": Data(publishedFingerprint.utf8)]))
    }
    func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
        status = "Nearby announcement unavailable. AirDrop pairing can still be used."
    }
    func stop() {
        browser.stop(); browsing = false
        services.values.forEach { $0.stop() }; services.removeAll(); nearby.removeAll(); unpublish()
    }
    private func id(_ service: NetService) -> String { service.domain + service.type + service.name }
    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        guard services.count < 64 else { return }
        services[id(service)] = service; service.delegate = self; service.resolve(withTimeout: 5)
    }
    func netServiceBrowser(_ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool) {
        services.removeValue(forKey: id(service))?.stop(); nearby.removeAll { $0.id == id(service) }
    }
    func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
        status = "Nearby discovery unavailable. Check Local Network permission or use AirDrop pairing directly."
    }
    func netServiceDidResolveAddress(_ sender: NetService) {
        guard sender.port == 18444, let host = sender.hostName?.trimmingCharacters(in: CharacterSet(charactersIn: ".")),
              let data = sender.txtRecordData(),
              let pinData = NetService.dictionary(fromTXTRecord: data)["fingerprint"],
              let pin = String(data: pinData, encoding: .utf8), pin.count == 64, pin.allSatisfy({ $0.isHexDigit }),
              sender.name.count <= 80 else { return }
        guard pin.lowercased() != publishedFingerprint.lowercased() else { return }
        let value = NearbyRelay(id: id(sender), name: sender.name, host: host, fingerprint: pin.lowercased())
        guard (try? Peer(name: value.name, alias: host, port: 18444, fingerprint: pin).validate()) != nil else { return }
        nearby.removeAll { $0.id == value.id }; nearby.append(value)
        nearby.sort { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        status = "\(nearby.count) nearby Mac(s). Discovery alone does not pair or trust a device."
    }
}

/// AirDrop owns the recipient picker and transfer; this app never sends automatically.
@MainActor final class PairingAirDrop: NSObject, NSSharingServiceDelegate {
    private var service: NSSharingService?
    private var file: URL?
    func share(_ data: Data) throws {
        guard service == nil else { throw RouteError.message("Finish the current AirDrop transfer first.") }
        guard let service = NSSharingService(named: .sendViaAirDrop) else { throw RouteError.message("AirDrop is unavailable. Use Export pairing instead.") }
        let directory = Files.directory.appendingPathComponent("pairing-transfer", isDirectory: true)
        try Files.prepare(directory)
        // Remove stale local copies from interrupted transfers before preparing a new one.
        for old in (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? [] {
            if old.pathExtension == "peerrelay" { try? FileManager.default.removeItem(at: old) }
        }
        let file = directory.appendingPathComponent("Peer Route-\(UUID().uuidString).peerrelay")
        try Files.privateWrite(data, to: file)
        self.file = file; self.service = service; service.delegate = self
        service.perform(withItems: [file])
    }
    private func cleanup() { if let file { try? FileManager.default.removeItem(at: file) }; file = nil; service = nil }
    func sharingService(_ sharingService: NSSharingService, didShareItems items: [Any]) { cleanup() }
    func sharingService(_ sharingService: NSSharingService, didFailToShareItems items: [Any], error: Error) { cleanup() }
}
