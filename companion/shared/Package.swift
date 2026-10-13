// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SpinitronShared",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "SpinitronCore", targets: ["SpinitronCore"]),
        .library(name: "SpinitronUI", targets: ["SpinitronUI"]),
    ],
    targets: [
        .target(name: "SpinitronCore"),
        .target(name: "SpinitronUI", dependencies: ["SpinitronCore"], resources: [.process("Resources")]),
        .testTarget(name: "SpinitronCoreTests", dependencies: ["SpinitronCore"]),
    ]
)
