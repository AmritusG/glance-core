// Swift Glance's own code on the files named on the command line: the source of
// tests/data/swift-glance-*.tsv, which crates/glance-media/tests/swift_parity.rs and
// crates/glance-proof/src/hapq.rs compare the Rust code with.
//
// Build (needs .ref/glance, a clone of AmritusG/glance; made at a9dc601):
//   cd tools/swift-crosstest && swift build -c release
// Regenerate the tables (paths are printed as given, so run from the folder they are relative to):
//   cd tests/fixtures && find . -type f \( -name '*.mov' -o -name '*.mp4' \) | sed 's|^\./||' | sort \
//     | tr '\n' '\0' | xargs -0 ../../tools/swift-crosstest/.build/release/xtest > ../data/swift-glance-fixtures.tsv
//   (the same in Amrit's test folder, with mkv / webm / avi added, for swift-glance-testfiles.tsv)
//   cd tests/fixtures && ../../tools/swift-crosstest/.build/release/xtest --hapq-hash 1000 tiles_hapy.mov \
//     tiles_hapm.mov tiles_hapm_chunks4.mov tiles_hap5.mov tiles_hap1.mov tiles_hapa.mov > ../data/swift-glance-hap-rgba.tsv
//
// One tab-separated line per file:
//   path  route  message  [fourcc]  displayName  clipKey  kind  look  samples  exceeding
// "-" where Swift returns nil. route is PlayerModel.open's order (dxv / hap / av / unsupported);
// message is the line Swift Glance shows when nothing plays the file. kind "-" means probeDXV
// threw. With --hapq-hash <frames> <file>...: file, frame, SHA-256 of Swift Glance's CPU HAP
// decode of that frame. With --hapq <file> <frame> <out>: that decode as raw RGBA8.
import Foundation
import AVFoundation
import CryptoKit
import GlanceCore

// AVPlaybackController.matroskaCodec and the message of AVPlaybackController.inspect, copied from
// glance/Sources/Glance/AVPlaybackController.swift:84-123 (they live in the app target, which a
// package cannot import).
func matroskaCodec(_ url: URL) -> String? {
    guard ["mkv", "webm", "mk3d"].contains(url.pathExtension.lowercased()),
          let fh = try? FileHandle(forReadingFrom: url) else { return nil }
    defer { try? fh.close() }
    guard let head = try? fh.read(upToCount: 1 << 20),
          let range = head.range(of: Data("V_".utf8)) else { return nil }
    let id = String(decoding: head[range.lowerBound...].prefix(32).prefix { b in
        (b >= 0x41 && b <= 0x5A) || (b >= 0x30 && b <= 0x39) || b == 0x5F || b == 0x2F
    }, as: UTF8.self)
    let names = ["V_VP8": "VP8", "V_VP9": "VP9", "V_AV1": "AV1", "V_MPEGH/ISO/HEVC": "HEVC",
                 "V_MPEG4/ISO/AVC": "H.264", "V_PRORES": "ProRes", "V_THEORA": "Theora",
                 "V_MPEG2": "MPEG-2", "V_MS/VFW/FOURCC": "VfW"]
    return names[id] ?? (id.count > 2 ? String(id.dropFirst(2)) : nil)
}

func unsupportedMessage(_ url: URL) -> String {
    let container = url.pathExtension.isEmpty ? "this container" : url.pathExtension.uppercased()
    let codecName = DXVDetector.compressorFourCC(at: url)
        .map { DXVDetector.displayName(for: $0) ?? $0.trimmingCharacters(in: .whitespaces) }
        ?? matroskaCodec(url)
    return codecName.map { "\($0) in \(container) is not supported" } ?? "\(container) files are not supported"
}

/// PlayerModel.open's order (DXV, HAP, AVFoundation) and inspect's playable test.
func route(_ url: URL) async -> String {
    if (try? DXVDemuxer.demux(url: url)) != nil { return "dxv" }
    if (try? HAPDemuxer.demux(url: url)) != nil { return "hap" }
    let asset = AVURLAsset(url: url, options: [AVURLAssetPreferPreciseDurationAndTimingKey: true])
    do {
        let (playable, tracks) = try await asset.load(.isPlayable, .tracks)
        guard let video = tracks.first(where: { $0.mediaType == .video }) else { return "unsupported" }
        let decodable = try await video.load(.isDecodable)
        guard playable, decodable else { return "unsupported" }
        _ = try await FrameTiming.load(track: video)
        return "av"
    } catch {
        return "unsupported"
    }
}

let args = Array(CommandLine.arguments.dropFirst())
if args.first == "--hapq" {
    let url = URL(fileURLWithPath: args[1])
    let index = try HAPDemuxer.demux(url: url)
    let (rgba, w, h) = try HAPThumbnail.rgbaOfFrame(at: Int(args[2])!, in: index, url: url)
    try Data(rgba).write(to: URL(fileURLWithPath: args[3]))
    print("\(w)x\(h) \(rgba.count)")
    exit(0)
}
if args.first == "--hapq-hash" {
    // file, frame, SHA-256 of Swift Glance's CPU decode of that frame (HAPThumbnail.rgbaOfFrame).
    let frames = Int(args[1])!
    for path in args.dropFirst(2) {
        let url = URL(fileURLWithPath: path)
        let index = try HAPDemuxer.demux(url: url)
        for f in 0..<min(frames, index.frames.count) {
            let (rgba, _, _) = try HAPThumbnail.rgbaOfFrame(at: f, in: index, url: url)
            print("\(path)\t\(f)\t\(SHA256.hash(data: Data(rgba)).map { String(format: "%02x", $0) }.joined())")
        }
    }
    exit(0)
}
for path in args {
    let url = URL(fileURLWithPath: path)
    let cc = DXVDetector.compressorFourCC(at: url)
    let name = cc.flatMap { DXVDetector.displayName(for: $0) }
    let key = AlphaTypeProbe.clipKey(url)
    var kind = "-", look = "-", samples = 0, exceeding = 0
    if let r = try? AlphaTypeProbe.probeDXV(url) {
        switch r.kind {
        case .none: kind = "none"
        case .choosable: kind = "choosable"
        case .stated(let t, _): kind = "stated-\(t.rawValue)"
        }
        look = r.look?.label ?? "-"
        samples = r.samples
        exceeding = Int((r.share * Double(r.samples)).rounded())
    }
    let r = await route(url)
    print([path, r, unsupportedMessage(url), cc.map { "[\($0)]" } ?? "-", name ?? "-", key ?? "-", kind, look, String(samples), String(exceeding)].joined(separator: "\t"))
}
