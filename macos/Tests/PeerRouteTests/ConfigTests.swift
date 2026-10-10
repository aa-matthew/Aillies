import XCTest
@testable import PeerRoute

final class ConfigTests: XCTestCase {
    func testMergePreservesSandboxAndExistingTables() throws {
        let input = "model = \"old-model\"\nmodel_provider = \"openai\"\nsandbox_mode = \"read-only\"\n\n[features]\nweb_search = true\napi_key_model_discovery = false\n\n[mcp_servers.example]\ncommand = \"example\"\n"
        let output = try CodexConfig.render(original: input, port: 18317, key: "test-key", model: "peer-model")
        XCTAssertTrue(output.contains("model_provider = \"peer_route\""))
        XCTAssertTrue(output.contains("sandbox_mode = \"read-only\""))
        XCTAssertTrue(output.contains("[mcp_servers.example]\ncommand = \"example\""))
        XCTAssertTrue(output.contains("api_key_model_discovery = true"))
        XCTAssertEqual(output.components(separatedBy: "[features]").count, 2)
        XCTAssertFalse(output.contains("danger-full-access"))
    }

    func testAmbiguousTOMLIsRejected() {
        for input in ["features = { web_search = true }", "model_provider = \"a\"\nmodel_provider = \"b\"", "x = '''multiline'''", "[model_providers.peer_route]"] {
            XCTAssertThrowsError(try CodexConfig.render(original: input, port: 18317, key: "key", model: "model"))
        }
    }

    func testQuotedTablesAndMultilineArraysArePreserved() throws {
        let input = """
        "model" = "old"
        # Brackets inside values must not be mistaken for headers.
        startup = [
          ["[features]", "#value"],
          ["other", "value"],
        ]
        [ 'features' ] # existing quoted table
        'api_key_model_discovery' = true
        options = [
          "one", # comment
          "two",
        ]
        [projects."/Users/example/project # [one]"]
        trust_level = "trusted"
        [mcp_servers.'a.b']
        command = "example"
        [[custom.items]]
        name = "first"
        """
        let output = try CodexConfig.render(original: input, port: 18322, key: "fake-key", model: "test", turnEndpoint: true)
        XCTAssertTrue(output.contains("model = \"test\""))
        XCTAssertTrue(output.contains("[ 'features' ] # existing quoted table\napi_key_model_discovery = false"))
        XCTAssertTrue(output.contains("startup = [\n  [\"[features]\", \"#value\"],"))
        XCTAssertTrue(output.contains("options = [\n  \"one\", # comment\n  \"two\",\n]"))
        XCTAssertTrue(output.contains("[projects.\"/Users/example/project # [one]\"]\ntrust_level = \"trusted\""))
        XCTAssertTrue(output.contains("[mcp_servers.'a.b']\ncommand = \"example\""))
        XCTAssertTrue(output.contains("[[custom.items]]\nname = \"first\""))
    }

    func testQuotedProviderConflictsAndMalformedStatementsAreRejected() {
        for input in [
            "[model_providers.\"peer_route\"]", "[model_providers]\n'peer_route' = {}",
            "[features]\n[features.api_key_model_discovery]", "\"features\" = {}",
            "[features]\n['features']", "model.child = \"x\"",
            "x = [\n  \"unfinished\"", "[projects.\"unfinished]", "[features] trailing",
            "\"model\" = \"x\"\nmodel = \"y\""
        ] {
            XCTAssertThrowsError(try CodexConfig.render(original: input, port: 18322, key: "key", model: "model"))
        }
        let input = "[model_providers.other]\nmodel_catalog_url = \"https://example.invalid/models\""
        let output = try? CodexConfig.render(original: input, port: 18322, key: "key", model: "model", turnEndpoint: true)
        XCTAssertTrue(output?.contains("model_catalog_url = \"https://example.invalid/models\"") == true)
    }

    func testInstallRestoreAndConcurrentEditProtection() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let home = root.appendingPathComponent("home")
        let receipts = root.appendingPathComponent("receipts")
        let folder = home.appendingPathComponent(".codex")
        try Files.prepare(folder)
        let path = folder.appendingPathComponent("config.toml")
        let original = Data("sandbox_mode = \"read-only\"\n".utf8)
        try original.write(to: path)
        try CodexConfig.install(port: 18317, key: "fake-key", model: "model", home: home, receiptDirectory: receipts)
        let installed = try Data(contentsOf: path)
        let mode = try FileManager.default.attributesOfItem(atPath: path.path)[.posixPermissions] as? Int
        XCTAssertEqual(mode, 0o600)
        XCTAssertThrowsError(try CodexConfig.install(port: 18317, key: "key", model: "model", home: home, receiptDirectory: receipts))
        try Data("# independent user edit\n".utf8).write(to: path)
        XCTAssertThrowsError(try CodexConfig.restore(home: home, receiptDirectory: receipts))
        XCTAssertEqual(try String(contentsOf: path), "# independent user edit\n")
        try installed.write(to: path)
        try CodexConfig.restore(home: home, receiptDirectory: receipts)
        XCTAssertEqual(try Data(contentsOf: path), original)
    }

    func testFreshConfigRestoresAbsenceAndLinkedConfigIsRejected() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let home = root.appendingPathComponent("home")
        let receipts = root.appendingPathComponent("receipts")
        try CodexConfig.install(port: 18317, key: "key", model: "model", home: home, receiptDirectory: receipts)
        try CodexConfig.restore(home: home, receiptDirectory: receipts)
        let path = home.appendingPathComponent(".codex/config.toml")
        XCTAssertFalse(FileManager.default.fileExists(atPath: path.path))
        let other = root.appendingPathComponent("other.toml")
        try Data("# untouched".utf8).write(to: other)
        try FileManager.default.createSymbolicLink(at: path, withDestinationURL: other)
        XCTAssertThrowsError(try CodexConfig.install(port: 18317, key: "key", model: "model", home: home, receiptDirectory: receipts))
        XCTAssertEqual(try String(contentsOf: other), "# untouched")
    }

    func testHostnameAndFingerprintValidation() {
        for alias in ["-oProxyCommand=evil", "a;echo", "a b", "$(whoami)", ""] {
            XCTAssertThrowsError(try Peer(name: "peer", alias: alias, fingerprint: String(repeating: "a", count: 64)).validate())
        }
        XCTAssertNoThrow(try Peer(name: "Home Mac", alias: "mac-home.local", fingerprint: String(repeating: "a", count: 64)).validate())
    }

    func testCommentedFeatureHeaderAndEscaping() throws {
        let output = try CodexConfig.render(original: "[features] # user features\nweb_search = true\n", port: 18317, key: "key/with/slash", model: "model")
        XCTAssertEqual(output.components(separatedBy: "[features]").count, 2)
        XCTAssertTrue(output.contains("api_key_model_discovery = true"))
        XCTAssertTrue(output.contains("\"key/with/slash\""))
        XCTAssertFalse(output.contains("\\/"))
    }
}
