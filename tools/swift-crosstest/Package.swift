// swift-tools-version:5.9
// The cross-test program: Swift Glance's own code (GlanceCore from the read-only clone in
// .ref/glance, which pins DXVKit 1.0.0) run on files, printing what the Rust tests compare with.
// Not part of the Rust workspace and not built by CI. See Sources/xtest/main.swift.
import PackageDescription
let package = Package(
    name: "xtest",
    platforms: [.macOS(.v14)],
    dependencies: [.package(path: "../../.ref/glance")],
    targets: [.executableTarget(name: "xtest", dependencies: [.product(name: "GlanceCore", package: "glance")])]
)
