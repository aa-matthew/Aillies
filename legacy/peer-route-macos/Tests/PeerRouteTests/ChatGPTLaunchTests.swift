import XCTest
@testable import PeerRoute

final class ChatGPTLaunchTests: XCTestCase {
    func testNodeRouteOverridesBothEnvironmentCasesAndKeepsCloudEndpoint() {
        let env = ChatGPTLaunch.proxyEnvironment(port: 18319)
        XCTAssertEqual(env["NODE_USE_ENV_PROXY"], "1")
        for key in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy"] {
            XCTAssertEqual(env[key], "http://127.0.0.1:18319")
        }
        XCTAssertEqual(env["NO_PROXY"], env["no_proxy"])
        XCTAssertFalse(env.values.contains("*"))
        let command = ChatGPTLaunch.command(port: 18320, includeNode: true)
        XCTAssertTrue(command.contains("HTTPS_PROXY=http://127.0.0.1:18320"))
        XCTAssertFalse(command.contains("CODEX_APP_SERVER_WS_URL"))
        XCTAssertFalse(command.contains("NODE_TLS_REJECT_UNAUTHORIZED"))
        XCTAssertTrue(command.contains("--proxy-server=http://127.0.0.1:18320"))
        XCTAssertFalse(ChatGPTLaunch.command(port: 18319, includeNode: false).contains("NODE_USE_ENV_PROXY"))
    }
}
