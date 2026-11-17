// swift-tools-version:5.9
// SPDX-License-Identifier: MIT
// The Send to Arena cross-test oracle: SendEhrKit's own Swift code (1.1.2, relicensed MIT by its
// author for the Glance family) encoding and decoding the kit's files, decoding compositions,
// encoding file URLs and comparing names, so glance-arena's tests compare with Swift itself.
// Not part of the Rust workspace and not built by CI.
//
//   SENDEHR_PATH=<SendEhr checkout at v1.1.2> swift build -c release
//   .build/release/arena-oracle encode <dir> | decode <dir> | composition <file> | fileurl <file>
//                               | compare <file> | settings <file>
import Foundation
import PackageDescription

let sendehr = ProcessInfo.processInfo.environment["SENDEHR_PATH"] ?? "../../.ref/SendEhr"

let package = Package(
    name: "arena-oracle",
    platforms: [.macOS(.v14)],
    dependencies: [.package(path: sendehr)],
    targets: [
        .executableTarget(name: "arena-oracle", dependencies: [.product(name: "SendEhrKit", package: "SendEhr")]),
    ]
)
