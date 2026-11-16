// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "A21sMirror",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "A21sMirror", targets: ["A21sMirror"])
    ],
    targets: [
        .executableTarget(
            name: "A21sMirror",
            path: "Sources/A21sMirror"
        )
    ],
    swiftLanguageVersions: [.v5]
)
