import AppKit

enum ChatGPTLaunch {
    static let app = URL(fileURLWithPath: "/Applications/ChatGPT.app", isDirectory: true)
    static func proxyEnvironment(port: Int) -> [String: String] {
        let proxy = "http://127.0.0.1:\(port)"
        return ["NODE_USE_ENV_PROXY": "1", "HTTP_PROXY": proxy, "HTTPS_PROXY": proxy,
                "http_proxy": proxy, "https_proxy": proxy,
                "NO_PROXY": "localhost,127.0.0.1,::1", "no_proxy": "localhost,127.0.0.1,::1"]
    }
    static func command(port: Int, includeNode: Bool) -> String {
        let prefix = includeNode ? "/usr/bin/env " + proxyEnvironment(port: port).sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: " ") + " /Applications/ChatGPT.app/Contents/MacOS/ChatGPT" : "open -a ChatGPT --args"
        return prefix + " --proxy-server=http://127.0.0.1:\(port)"
    }
    @MainActor static func launch(port: Int, includeNode: Bool) throws {
        guard FileManager.default.fileExists(atPath: app.path) else { throw RouteError.message("Install ChatGPT in Applications first.") }
        guard !NSWorkspace.shared.runningApplications.contains(where: { $0.bundleURL?.standardizedFileURL == app.standardizedFileURL }) else {
            throw RouteError.message("Save your work and fully quit ChatGPT with ⌘Q, then click Launch again. Peer Route will not interrupt your current chat.")
        }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.arguments = ["--proxy-server=http://127.0.0.1:\(port)"]
        configuration.environment = includeNode ? proxyEnvironment(port: port) : [:]
        configuration.activates = true
        NSWorkspace.shared.openApplication(at: app, configuration: configuration) { _, error in
            if error != nil { Task { @MainActor in
                let alert = NSAlert(); alert.messageText = "ChatGPT could not be launched"
                alert.informativeText = "Use the copyable launch command in Terminal. No account or profile settings were changed."
                alert.runModal()
            } }
        }
    }
}
