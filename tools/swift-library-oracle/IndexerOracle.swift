// IndexerOracle — Swift MaCrate 0.16.4's own LibraryIndexer (CrateCore) and ThumbnailExtractor (CrateKit) driven
// from a script, for glance-library's indexer and thumbnail cross-test (docs/data/m3/indexer-crosstest.txt). Built
// by indexer-oracle.sh inside a scratch copy of the MaCrate source as an extra executable target (with
// -enable-testing, so the internal `FSEventsWatcher.ascendWithinRoot` is called as it is). Not part of the Rust
// workspace and not built by CI. Run it with HOME and CFFIXED_USER_HOME in a scratch folder: MaCrate's code logs
// to ~/Library/Logs/<bundle id>/diag.log under HOME.
//
//   IndexerOracle run <library.sqlite> <script>
//       One command per line, fields separated by tabs, lists inside a field by `|`:
//         indexRoot  <root>  <excluded…>             LibraryIndexer.indexRoot(_:excluded:)
//         fsevent    <dirs…> <roots…> <excluded…>    LibraryIndexer.handleFSEvent(changedDirs:roots:excluded:)
//         sidecar    <sidecar path>                  LibraryIndexer.handleExternalSidecar(_:)
//         settle     <excluded…>                     what ContentView runs when a pass settles: retireOrphansWithLiveTwin,
//                                                    then deleteRows(underPathPrefix:) per excluded folder
//         progress                                   prints the indexer's progress (phase, total, completed, root name)
//       Prints each external rename / external sidecar change the indexer sends, and the settle counts.
//   IndexerOracle ascend <input>
//       Each line `<missing path>\t<root>|<root>…`: FSEventsWatcher.ascendWithinRoot, sorted paths.
//   IndexerOracle thumb <clip> <premultiplied|straight> <out.rgba>
//       ThumbnailExtractor.extract(url:alphaType:) (the walk-forward, the decode, the checkerboard): writes the
//       picture as RGBA (premultipliedLast, DeviceRGB, as the extractor's context holds it) and prints `w h`.
//   IndexerOracle stats <in.rgba> <w> <h> <premultiplied|straight>
//       The extractor's 32×32 contentful-frame statistics (its own code, copied: the function is private) of an
//       RGBA picture labelled as given; prints `mean max` as bit patterns and decimals.
import Combine
import CoreGraphics
import Foundation
import GlanceCore
@testable import CrateCore
@testable import CrateKit

func bits(_ d: Double) -> String {
    let s = String(d.bitPattern, radix: 16)
    return String(repeating: "0", count: 16 - s.count) + s
}

func fail(_ s: String) -> Never {
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
    exit(2)
}

func list(_ field: String?) -> [String] {
    guard let field, !field.isEmpty else { return [] }
    return field.split(separator: "|").map(String.init)
}

@MainActor
func run(db: String, script: String) async {
    guard let text = try? String(contentsOfFile: script, encoding: .utf8) else { fail("cannot read \(script)") }
    let store: MetadataStore
    do {
        store = try MetadataStore(location: .file(URL(fileURLWithPath: db)), mountedVolumes: { [] })
    } catch {
        fail("cannot open \(db): \(error)")
    }
    let indexer = LibraryIndexer(metadataStore: store, sidecarStore: SidecarStore(), hashingService: ClipHashingService())
    var subs: [AnyCancellable] = []
    subs.append(indexer.externalRenames.sink { c in
        print("rename\t\(c.clipID)\t\(c.oldURL.path)\t\(c.newURL.path)")
    })
    subs.append(indexer.externalSidecarChanges.sink { u in print("sidecar-change\t\(u.path)") })
    for raw in text.split(separator: "\n") {
        let f = raw.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard let cmd = f.first else { continue }
        let field = { (i: Int) -> String? in i < f.count ? f[i] : nil }
        print("> \(raw)")
        switch cmd {
        case "indexRoot":
            await indexer.indexRoot(URL(fileURLWithPath: field(1) ?? ""), excluded: Set(list(field(2))))
        case "fsevent":
            let dirs = Set(list(field(1)).map { URL(fileURLWithPath: $0) })
            let roots = Set(list(field(2)).map { URL(fileURLWithPath: $0) })
            await indexer.handleFSEvent(changedDirs: dirs, roots: roots, excluded: Set(list(field(3))))
        case "sidecar":
            await indexer.handleExternalSidecar(URL(fileURLWithPath: field(1) ?? ""))
        case "settle":
            let retired = (try? await store.retireOrphansWithLiveTwin()) ?? []
            var purged = 0
            for prefix in list(field(1)) {
                purged += ((try? await store.deleteRows(underPathPrefix: prefix)) ?? []).count
            }
            let dups = ((try? await store.clipIDsWithDuplicates()) ?? []).sorted()
            print("settle\tretired=\(retired.sorted())\tpurged=\(purged)\tduplicates=\(dups)")
        case "progress":
            let p = indexer.progress
            print("progress\t\(p.phase)\t\(p.total)\t\(p.completed)\t\(p.rootName ?? "-")")
        default:
            fail("unknown command \(cmd)")
        }
    }
    _ = subs
}

func ascend(_ input: String) {
    guard let text = try? String(contentsOfFile: input, encoding: .utf8) else { fail("cannot read \(input)") }
    for line in text.split(separator: "\n") {
        let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        let out = FSEventsWatcher.ascendWithinRoot(missing: URL(fileURLWithPath: f[0]), rootPaths: list(f.count > 1 ? f[1] : nil))
        print("\(f[0])\t\(out.map(\.path).sorted().joined(separator: "|"))")
    }
}

func rgbaBytes(_ image: CGImage) -> (Data, Int, Int) {
    let w = image.width, h = image.height
    var data = Data(count: w * h * 4)
    data.withUnsafeMutableBytes { raw in
        guard let ctx = CGContext(data: raw.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
    }
    return (data, w, h)
}

func thumb(_ clip: String, _ alpha: String, _ out: String) async {
    let extractor = ThumbnailExtractor()
    do {
        let image = try await extractor.extract(url: URL(fileURLWithPath: clip), alphaType: alpha == "straight" ? .straight : .premultiplied)
        let (data, w, h) = rgbaBytes(image)
        try data.write(to: URL(fileURLWithPath: out))
        print("\(w) \(h)")
    } catch {
        print("error \(error)")
    }
}

/// `ThumbnailExtractor.imageStats`, verbatim (private there).
func imageStats(_ image: CGImage) -> (mean: Double, max: Double) {
    let w = 32, h = 32
    let bytesPerRow = w * 4
    var buffer = [UInt8](repeating: 0, count: bytesPerRow * h)
    return buffer.withUnsafeMutableBytes { bytes -> (mean: Double, max: Double) in
        guard let baseAddress = bytes.baseAddress,
              let ctx = CGContext(
                data: baseAddress,
                width: w, height: h,
                bitsPerComponent: 8,
                bytesPerRow: bytesPerRow,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              )
        else { return (1.0, 1.0) }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        var sum: UInt64 = 0
        var maxVal: UInt8 = 0
        let count = w * h * 4
        for i in stride(from: 0, to: count, by: 4) {
            let r = bytes[i]
            let g = bytes[i + 1]
            let b = bytes[i + 2]
            sum += UInt64(r) + UInt64(g) + UInt64(b)
            let local = Swift.max(r, Swift.max(g, b))
            if local > maxVal { maxVal = local }
        }
        let pixelCount = UInt64(w * h) * 3
        let mean = Double(sum) / (Double(pixelCount) * 255.0)
        let maxLum = Double(maxVal) / 255.0
        return (mean, maxLum)
    }
}

func stats(_ input: String, _ w: Int, _ h: Int, _ alpha: String) {
    guard let data = FileManager.default.contents(atPath: input), data.count == w * h * 4,
          let provider = CGDataProvider(data: data as CFData),
          let image = CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                              space: CGColorSpaceCreateDeviceRGB(),
                              bitmapInfo: CGBitmapInfo(rawValue: (alpha == "straight" ? CGImageAlphaInfo.last
                                                                    : CGImageAlphaInfo.premultipliedLast).rawValue),
                              provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    else { fail("bad stats input \(input)") }
    let s = imageStats(image)
    print("\(bits(s.mean)) \(bits(s.max)) \(s.mean) \(s.max)")
}

/// `reduce <in.rgba> <w> <h> <alpha> <out>`: the 32×32 premultiplied buffer `imageStats` sums, as bytes.
func reduce(_ input: String, _ w: Int, _ h: Int, _ alpha: String, _ out: String) {
    guard let data = FileManager.default.contents(atPath: input), data.count == w * h * 4,
          let provider = CGDataProvider(data: data as CFData),
          let image = CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                              space: CGColorSpaceCreateDeviceRGB(),
                              bitmapInfo: CGBitmapInfo(rawValue: (alpha == "straight" ? CGImageAlphaInfo.last
                                                                    : CGImageAlphaInfo.premultipliedLast).rawValue),
                              provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    else { fail("bad reduce input \(input)") }
    var buffer = [UInt8](repeating: 0, count: 32 * 32 * 4)
    buffer.withUnsafeMutableBytes { bytes in
        guard let ctx = CGContext(data: bytes.baseAddress, width: 32, height: 32, bitsPerComponent: 8, bytesPerRow: 128,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: 32, height: 32))
    }
    try? Data(buffer).write(to: URL(fileURLWithPath: out))
}

let args = CommandLine.arguments
switch args.count > 1 ? args[1] : "" {
case "run" where args.count == 4:
    await run(db: args[2], script: args[3])
case "ascend" where args.count == 3:
    ascend(args[2])
case "thumb" where args.count == 5:
    await thumb(args[2], args[3], args[4])
case "reduce" where args.count == 7:
    reduce(args[2], Int(args[3]) ?? 0, Int(args[4]) ?? 0, args[5], args[6])
case "stats" where args.count == 6:
    stats(args[2], Int(args[3]) ?? 0, Int(args[4]) ?? 0, args[5])
default:
    fail("usage: IndexerOracle run <db> <script> | ascend <input> | thumb <clip> <alpha> <out> | stats <rgba> <w> <h> <alpha>")
}
