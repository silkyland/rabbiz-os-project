// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "RabbizMirror",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "RabbizMirror", targets: ["RabbizMirror"])
    ],
    targets: [
        .executableTarget(
            name: "RabbizMirror",
            path: "Sources/RabbizMirror"
        )
    ],
    swiftLanguageVersions: [.v5]
)
