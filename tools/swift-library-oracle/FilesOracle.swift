// FilesOracle — Swift MaCrate's own code (CrateKit / CrateCore of MaCrate 0.16.4, GlanceCore f0b917a)
// run on files, paths, names and numbers, printing what glance-library's `clip_id`, `header`,
// `enumerate`, `roots` and `thumbkey` modules are compared with. Built by files-oracle.sh inside a
// scratch copy of the MaCrate source as an extra executable target (with -enable-testing, so the
// internal `ThumbnailCache.cacheKey`, `LibraryIndexer.isDangerousRoot` and
// `ClipMetadata.fileSizeFromClipID` are called as they are, not re-typed). Not part of the Rust
// workspace and not built by CI.
//
// Doubles print as their IEEE-754 bit pattern (16 lower-case hex digits) so nothing is lost; `-` is
// Swift's nil. Every mode reads its input lines from a file and writes one line per input line.
import CoreGraphics
import Darwin
import Foundation
@testable import CrateCore
@testable import CrateKit

func bits(_ d: Double?) -> String {
    guard let d else { return "-" }
    let s = String(d.bitPattern, radix: 16)
    return String(repeating: "0", count: 16 - s.count) + s
}

func int(_ v: Int?) -> String { v.map { String($0) } ?? "-" }

func lines(_ path: String) -> [String] {
    guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
        FileHandle.standardError.write("cannot read \(path)\n".data(using: .utf8)!)
        exit(2)
    }
    return text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).filter { !$0.isEmpty }
}

func double(hex: String) -> Double { Double(bitPattern: UInt64(hex, radix: 16) ?? 0) }

func info(_ i: ContainerHeaderInfo) -> String {
    "\(bits(i.duration))\t\(bits(i.fps))\t\(int(i.width))\t\(int(i.height))"
}

/// `files <dir>`: every non-directory entry under `dir` (hidden ones and symlinks included, nothing
/// followed), sorted by byte order of the relative path: relative path, kind, clip_id,
/// ContainerDurationReader.info, DurationExtractor.extractInfo, the thumbnail key, lstat's mtime
/// (sec, nsec), Foundation's modificationDate (attributesOfItem) and contentModificationDate (URL
/// resource value) as seconds since 1970, and the size attributesOfItem reports.
func files(_ dir: String) async {
    let base = URL(fileURLWithPath: dir)
    var rel: [String] = []
    if let e = FileManager.default.enumerator(atPath: dir) {
        while let p = e.nextObject() as? String { rel.append(p) }
    }
    rel.sort { Array($0.utf8).lexicographicallyPrecedes(Array($1.utf8)) }
    for r in rel {
        let url = base.appendingPathComponent(r)
        var st = stat()
        guard lstat(url.path, &st) == 0 else { print("\(r)\tgone"); continue }
        let mode = st.st_mode & S_IFMT
        if mode == S_IFDIR { continue }
        let kind = mode == S_IFLNK ? "link" : (mode == S_IFREG ? "file" : "other")
        let id = (try? ClipHasher.computeClipID(at: url)) ?? "-"
        let cdr = ContainerDurationReader.info(url: url)
        let de = await DurationExtractor.extractInfo(url: url)
        let key = ThumbnailCache.cacheKey(for: url) ?? "-"
        let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
        let attrMtime = (attrs?[.modificationDate] as? Date)?.timeIntervalSince1970
        let attrSize = (attrs?[.size] as? NSNumber)?.int64Value
        let rv = try? url.resourceValues(forKeys: [.contentModificationDateKey])
        let rvMtime = rv?.contentModificationDate?.timeIntervalSince1970
        print([r, kind, id, info(cdr), info(de), key,
               "\(st.st_mtimespec.tv_sec)", "\(st.st_mtimespec.tv_nsec)",
               bits(attrMtime), bits(rvMtime), attrSize.map { String($0) } ?? "-"].joined(separator: "\t"))
    }
}

/// `enum <root> [excluded…]`: FileEnumerator.directVideos(in:) then recursiveVideos(in:excludedPaths:)
/// (the excluded folders given as paths, stored as ExclusionStore.standardizedPath of them): path,
/// size, mtime (seconds since 1970), the thumbnail key of the listed URL (`ThumbnailCache.cacheKey(for: clip.url)`).
func enumerate(_ root: String, excluded: [String]) async {
    let url = URL(fileURLWithPath: root)
    do {
        for c in try FileEnumerator.directVideos(in: url) {
            print("direct\t\(c.url.path)\t\(c.sizeBytes)\t\(bits(c.mtime.timeIntervalSince1970))\t\(ThumbnailCache.cacheKey(for: c.url) ?? "-")")
        }
    } catch {
        print("direct-error")
    }
    let set = Set(excluded.map { ExclusionStore.standardizedPath(URL(fileURLWithPath: $0)) })
    do {
        for c in try await FileEnumerator.recursiveVideos(in: url, excludedPaths: set) {
            print("recursive\t\(c.url.path)\t\(c.sizeBytes)\t\(bits(c.mtime.timeIntervalSince1970))\t\(ThumbnailCache.cacheKey(for: c.url) ?? "-")")
        }
    } catch {
        print("recursive-error")
    }
}

/// `walk <root>`: what FileManager's enumerator with MaCrate's options yields, before any filter:
/// path, isDirectory, isPackage, isHidden, isSymbolicLink.
func walk(_ root: String) {
    let keys: [URLResourceKey] = [.isDirectoryKey, .isPackageKey, .isHiddenKey, .isSymbolicLinkKey]
    guard let e = FileManager.default.enumerator(at: URL(fileURLWithPath: root), includingPropertiesForKeys: keys,
                                                 options: [.skipsHiddenFiles, .skipsPackageDescendants]) else {
        print("nil-enumerator"); return
    }
    while let u = e.nextObject() as? URL {
        let v = try? u.resourceValues(forKeys: Set(keys))
        print("\(u.path)\t\(v?.isDirectory == true)\t\(v?.isPackage == true)\t\(v?.isHidden == true)\t\(v?.isSymbolicLink == true)")
    }
}

/// `ispkg <file>`: one directory path per line → URLResourceValues.isPackage.
func ispkg(_ file: String) {
    for p in lines(file) {
        let v = try? URL(fileURLWithPath: p).resourceValues(forKeys: [.isPackageKey])
        print("\(p)\t\(v?.isPackage.map { String($0) } ?? "-")")
    }
}

/// `doubles <file>`: one bit pattern per line → Swift's `"\(Double)"`.
func doubles(_ file: String) {
    for l in lines(file) { print("\(double(hex: l))") }
}

/// `compare <file>`: `a<TAB>b` per line → a.localizedCaseInsensitiveCompare(b) as -1 / 0 / 1.
func compare(_ file: String) {
    for l in lines(file) {
        let p = l.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard p.count == 2 else { print("bad"); continue }
        print(p[0].localizedCaseInsensitiveCompare(p[1]).rawValue)
    }
}

/// `sort <file>`: the lines sorted as FileEnumerator sorts display names.
func sortNames(_ file: String) {
    for l in lines(file).sorted(by: { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }) { print(l) }
}

/// `paths <file>`: one path per line → standardizedFileURL.path, LibraryRootStore.canonicalPath,
/// LibraryIndexer.isDangerousRoot(standardized), lastPathComponent.
@MainActor
func paths(_ file: String) {
    for p in lines(file) {
        let u = URL(fileURLWithPath: p)
        let std = u.standardizedFileURL.path
        print("\(std)\t\(LibraryRootStore.canonicalPath(u))\t\(LibraryIndexer.isDangerousRoot(std))\t\(u.lastPathComponent)")
    }
}

/// `contained <file>`: `candidate<TAB>root` → LibraryRootStore.isContained(candidate, in: root).
func contained(_ file: String) {
    for l in lines(file) {
        let p = l.split(separator: "\t").map(String.init)
        guard p.count == 2 else { print("bad"); continue }
        print(LibraryRootStore.isContained(URL(fileURLWithPath: p[0]), in: URL(fileURLWithPath: p[1])))
    }
}

/// `clipsize <file>`: one clip id per line → ClipMetadata.fileSizeFromClipID.
func clipsize(_ file: String) {
    for l in lines(file) {
        print(ClipMetadata.makeDefault(clipID: l).fileSizeFromClipID.map { String($0) } ?? "-")
    }
}

/// `json <file>`: `dateAdded bits<TAB>displayName<TAB>bookmark hex` per line → `[LibraryRoot]` as
/// plain JSONEncoder writes library-roots.json (one root, a fixed id). Then the other files' shapes.
func json(_ file: String) {
    let id = UUID(uuidString: "6F1C2A4E-9B1D-4C55-8E3A-0123456789AB")!
    for l in lines(file) {
        let p = l.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard p.count == 3 else { print("bad"); continue }
        var data = Data()
        var i = p[2].startIndex
        while i < p[2].endIndex, let j = p[2].index(i, offsetBy: 2, limitedBy: p[2].endIndex) {
            data.append(UInt8(p[2][i..<j], radix: 16) ?? 0)
            i = j
        }
        let root = LibraryRoot(id: id, displayName: p[1], bookmarkData: data,
                               dateAdded: Date(timeIntervalSinceReferenceDate: double(hex: p[0])))
        print(String(data: try! JSONEncoder().encode([root]), encoding: .utf8)!)
    }
    for mode in RootSortMode.allCases {
        print("root-sort\t" + String(data: try! JSONEncoder().encode(mode), encoding: .utf8)!)
    }
    for b in [true, false] {
        print("root-size-display\t" + String(data: try! JSONEncoder().encode(b), encoding: .utf8)!)
    }
    let blobs: [Data] = [Data(), Data([0]), Data([0xff, 0xfe]), Data((0..<200).map { UInt8($0 & 0xff) })]
    print("bookmark-blobs\t" + String(data: try! JSONEncoder().encode(blobs), encoding: .utf8)!)
    print("bookmark-blobs-empty\t" + String(data: try! JSONEncoder().encode([Data]()), encoding: .utf8)!)
}

/// `downscale <file>`: `w h` per line → ThumbnailDownscale.downscaleIfNeeded's output size.
func downscale(_ file: String) {
    for l in lines(file) {
        let p = l.split(separator: " ").compactMap { Int($0) }
        guard p.count == 2, let ctx = CGContext(data: nil, width: p[0], height: p[1], bitsPerComponent: 8,
                                                bytesPerRow: p[0] * 4, space: CGColorSpaceCreateDeviceRGB(),
                                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue),
              let image = ctx.makeImage() else { print("bad"); continue }
        let out = ThumbnailDownscale.downscaleIfNeeded(image)
        print("\(out.width) \(out.height)")
    }
}

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: FilesOracle files|enum|walk|ispkg|doubles|compare|sort|paths|contained|clipsize|json|downscale <arg> …\n".data(using: .utf8)!)
    exit(2)
}
switch args[1] {
case "files": await files(args[2])
case "enum": await enumerate(args[2], excluded: Array(args.dropFirst(3)))
case "walk": walk(args[2])
case "ispkg": ispkg(args[2])
case "doubles": doubles(args[2])
case "compare": compare(args[2])
case "sort": sortNames(args[2])
case "paths": await paths(args[2])
case "contained": contained(args[2])
case "clipsize": clipsize(args[2])
case "json": json(args[2])
case "downscale": downscale(args[2])
default:
    FileHandle.standardError.write("unknown mode \(args[1])\n".data(using: .utf8)!)
    exit(2)
}
