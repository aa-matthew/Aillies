// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "PeerRoute",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "PeerRoute", targets: ["PeerRoute"])],
    targets: [
        .executableTarget(name: "PeerRoute"),
        .testTarget(name: "PeerRouteTests", dependencies: ["PeerRoute"])
    ]
)
