// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SpinitronMenu",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "SpinitronMenu", targets: ["SpinitronMenu"])],
    targets: [
        .target(name: "SpinitronCore"),
        .executableTarget(name: "SpinitronMenu", dependencies: ["SpinitronCore"]),
        .testTarget(name: "SpinitronCoreTests", dependencies: ["SpinitronCore"]),
    ]
)
