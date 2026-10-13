// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SpinitronMenu",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "SpinitronMenu", targets: ["SpinitronMenu"])],
    dependencies: [.package(path: "../shared")],
    targets: [
        .executableTarget(name: "SpinitronMenu", dependencies: [
            .product(name: "SpinitronCore", package: "shared"),
            .product(name: "SpinitronUI", package: "shared"),
        ]),
    ]
)
