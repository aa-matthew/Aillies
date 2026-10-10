import Foundation
import Security

struct Peer: Codable, Identifiable, Equatable {
    var id = UUID()
    var name: String
    var alias: String
    var port: Int = 18443
    var fingerprint: String = ""
    var accountLabel: String = ""

    func validate() throws {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              name.count <= 80,
              alias.range(of: "^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$", options: .regularExpression) != nil,
              (1024...65535).contains(port),
              fingerprint.count == 64, fingerprint.allSatisfy({ $0.isHexDigit }) else {
            throw RouteError.message("Enter a name, a valid hostname or IPv4 address, a port from 1024 to 65535, and a 64-character certificate fingerprint.")
        }
    }
}

enum RouteError: LocalizedError {
    case message(String)
    var errorDescription: String? { if case let .message(value) = self { return value }; return nil }
}

enum Vault {
    static let service = "io.peerroute.mac.client-key"
    static func query(_ id: UUID) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service, kSecAttrAccount as String: id.uuidString]
    }
    static func save(_ value: String, for id: UUID) throws {
        let data = Data(value.utf8)
        let result = SecItemUpdate(query(id) as CFDictionary, [kSecValueData: data] as CFDictionary)
        if result == errSecItemNotFound {
            var item = query(id)
            item[kSecValueData as String] = data
            item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            let status = SecItemAdd(item as CFDictionary, nil)
            guard status == errSecSuccess else { throw RouteError.message("Could not save client key in Keychain (\(status)).") }
        } else if result != errSecSuccess {
            throw RouteError.message("Could not update Keychain (\(result)).")
        }
    }
    static func read(_ id: UUID) throws -> String {
        var item = query(id)
        item[kSecReturnData as String] = true
        item[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(item as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data, let value = String(data: data, encoding: .utf8) else {
            throw RouteError.message("Client key unavailable. Edit this machine and save its CLIProxyAPI client key again.")
        }
        return value
    }
    static func delete(_ id: UUID) { SecItemDelete(query(id) as CFDictionary) }
}

struct SavedState: Codable {
    var peers: [Peer] = []
    var selected: UUID?
    var model = ""
}

enum Files {
    static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Peer Route", isDirectory: true)
    }
    static func prepare(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
    }
    static func privateWrite(_ data: Data, to url: URL) throws {
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
    static func load() throws -> SavedState {
        let path = directory.appendingPathComponent("machines.json")
        if !FileManager.default.fileExists(atPath: path.path) { return SavedState() }
        let state = try JSONDecoder().decode(SavedState.self, from: Data(contentsOf: path))
        for peer in state.peers { try peer.validate() }
        return state
    }
    static func save(_ state: SavedState) throws {
        try prepare(directory)
        try privateWrite(JSONEncoder().encode(state), to: directory.appendingPathComponent("machines.json"))
    }
}
