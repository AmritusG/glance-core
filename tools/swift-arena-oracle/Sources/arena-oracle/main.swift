// SPDX-License-Identifier: MIT
// The oracle's commands. The samples here are built the same way in
// crates/glance-arena/tests/swift_oracle.rs; keep the two lists in step.
import Foundation
import SendEhrKit

func date(_ s: Double) -> Date { Date(timeIntervalSince1970: s) }
let uuidA = UUID(uuidString: "0A1B2C3D-0000-4000-8000-00000000ABCD")!
let uuidB = UUID(uuidString: "5E6F7A8B-1111-4222-9333-0123456789AB")!

let encoder: JSONEncoder = { let e = JSONEncoder(); e.dateEncodingStrategy = .secondsSince1970; return e }()
let sortedEncoder: JSONEncoder = {
    let e = JSONEncoder(); e.dateEncodingStrategy = .secondsSince1970; e.outputFormatting = [.sortedKeys]; return e
}()
let decoder: JSONDecoder = { let d = JSONDecoder(); d.dateDecodingStrategy = .secondsSince1970; return d }()

struct Sample {
    let name: String
    let encode: (JSONEncoder) throws -> Data
    /// Decodes `data`; (equal to the sample, re-encoded with sorted keys).
    let decode: (Data) throws -> (Bool, Data)
}

func sample<T: Codable & Equatable>(_ name: String, _ v: T) -> Sample {
    Sample(name: name, encode: { try $0.encode(v) }, decode: { data in
        let back = try decoder.decode(T.self, from: data)
        return (back == v, try sortedEncoder.encode(back))
    })
}

let snapshot = CompositionSnapshot(
    compositionName: "Show / Night \"1\"", deckName: "Main",
    layers: [LayerInfo(index: 1, id: 500, rawName: "Layer #", nameParamID: 501,
                       slots: [SlotInfo(column: 1, clipID: 500000, clipName: "Intro.mov", state: "Connected"),
                               SlotInfo(column: 2, clipID: 500001, clipName: nil, state: "Empty")]),
             LayerInfo(index: 2, id: 9_007_199_254_740_993, rawName: "Bass \u{1F3B5}", nameParamID: nil, slots: [])],
    columns: [ColumnInfo(index: 1, id: 9000, rawName: "Column #", nameParamID: 9500),
              ColumnInfo(index: 2, id: 9001, rawName: "Drop", nameParamID: nil)])

let samples: [Sample] = [
    sample("settings-default", SendEhrSettings()),
    sample("settings-full", SendEhrSettings(
        endpoint: ArenaEndpoint(host: "10.0.0.5", port: 8081),
        remaps: [PathRemap(id: uuidA, localPrefix: "/Volumes/Media \u{00D6}/clips", remotePrefix: "D:/Clips & #1"),
                 PathRemap(id: uuidB, localPrefix: "/", remotePrefix: "/Volumes/Other")],
        occupiedSlotMode: .replaceItem, triggerAfterLoad: true, triggerScope: .topLayerOnly, extraEmptyColumns: 3,
        sortKey: .dateAdded, sortAscending: false, includeSubfolders: true, largeFolderThreshold: 120,
        updateMode: .interval, updateInterval: 30, updateModeChosen: true)),
    sample("recents", [
        RecentTarget(destination: .slot(layerID: 1_234_567_890_123, column: 7),
                     label: "Layer 3 \u{201C}Bass\u{201D} \u{203A} Column 7", date: date(1_763_380_000.123456)),
        RecentTarget(destination: .nextFree(layerID: 5), label: "Layer 1 \u{203A} Next Free Column", date: date(1_763_380_123.5)),
        RecentTarget(destination: .appendColumns(layerID: -2), label: "x/y \"q\" \\ \t", date: date(0.1)),
        RecentTarget(destination: .newLayer, label: "New Layer", date: date(1e9)),
        RecentTarget(destination: .newNamedLayer(name: "Warm Up / Set"), label: "Folder", date: date(1_763_380_000)),
        RecentTarget(destination: .layerPerFolder, label: "Folders", date: date(1_763_380_000.75)),
    ]),
    sample("request-load", LoadRequest(id: uuidA, paths: ["/Volumes/A B/c.mov", "/x/\u{00FC}.mov"], destination: .layerPerFolder,
                                       replaceConfirmed: true, label: "Folders as New Layers (2)",
                                       createdAt: date(1_763_380_000.25), expectedLayers: 24...26)),
    sample("request-setting", LoadRequest(id: uuidB, paths: [], destination: .newLayer, replaceConfirmed: false,
                                          label: "Arena Updates every 15 s", createdAt: date(1_763_380_001),
                                          setting: .updateInterval(15))),
    sample("setting-changes", [SettingChange.sortKey(.dateModified), .sortAscending(false), .triggerAfterLoad(true),
                               .triggerScope(.topLayerOnly), .includeSubfolders(true), .updateMode(.onDemand),
                               .updateInterval(300)]),
    sample("statuses", [ArenaStatus.connected(product: "Arena 7.28.1 (26535)"), .unknown, .failed(.notRunning),
                        .failed(.webserverOff(host: "localhost", port: 8080)),
                        .failed(.unreachable(host: "10.0.0.5", port: 8080, detail: "timed out")),
                        .failed(.http(status: 412, path: "/composition", body: "busy")),
                        .failed(.badResponse("x")), .failed(.layerGone(layerID: 77)), .failed(.noFiles),
                        .failed(.cancelled)]),
    sample("cache", CacheState(status: .connected(product: "Arena 7.28.1 (26535)"), snapshot: snapshot, hostPID: 4242,
                               writtenAt: date(1_763_380_000.5), updatedAt: date(1_763_379_990))),
    sample("cache-empty", CacheState(status: .unknown, snapshot: nil, hostPID: 1, writtenAt: date(1_763_380_000))),
]

func stringArray(_ path: String) -> [String] {
    (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))) as? [String] ?? []
}

func printJSON(_ v: Any) {
    let d = try! JSONSerialization.data(withJSONObject: v, options: [.sortedKeys])
    print(String(decoding: d, as: UTF8.self))
}

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write(Data("usage: arena-oracle encode|decode <dir> | composition|fileurl|compare|settings <file>\n".utf8))
    exit(2)
}
let target = args[2]

switch args[1] {
case "encode":
    try? FileManager.default.createDirectory(atPath: target, withIntermediateDirectories: true)
    for s in samples {
        try s.encode(encoder).write(to: URL(fileURLWithPath: target).appendingPathComponent("\(s.name).json"))
        try s.encode(sortedEncoder).write(to: URL(fileURLWithPath: target).appendingPathComponent("\(s.name).sorted.json"))
    }
    print("encoded \(samples.count) samples")
case "decode":
    var failed = 0
    for s in samples {
        let url = URL(fileURLWithPath: target).appendingPathComponent("\(s.name).json")
        do {
            let (equal, sorted) = try s.decode(Data(contentsOf: url))
            try sorted.write(to: URL(fileURLWithPath: target).appendingPathComponent("\(s.name).swift-reencoded.sorted.json"))
            print("decode \(s.name): ok, equal to the Swift sample: \(equal)")
            if !equal { failed += 1 }
        } catch {
            print("decode \(s.name): FAILED \(error)")
            failed += 1
        }
    }
    exit(failed == 0 ? 0 : 1)
case "composition":
    printJSON(stringArray(target).map { json -> String in
        do {
            let s = try CompositionDecoder.decode(Data(json.utf8))
            return "ok " + String(decoding: try sortedEncoder.encode(s), as: UTF8.self)
        } catch let e as ArenaError {
            return "error " + e.plainLine
        } catch {
            return "error \(error)"
        }
    })
case "fileurl":
    printJSON(stringArray(target).map { p -> [String] in
        let e = ArenaFileURL.encode(path: p)
        return [e, ArenaFileURL.decode(e) ?? "(nil)"]
    })
case "compare":
    let pairs = (try? JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: target)))) as? [[String]] ?? []
    printJSON(pairs.map { p -> String in
        switch p[0].localizedStandardCompare(p[1]) {
        case .orderedAscending: return "<"
        case .orderedDescending: return ">"
        case .orderedSame: return "="
        }
    })
case "settings":
    printJSON(stringArray(target).map { json -> String in
        guard let s = try? decoder.decode(SendEhrSettings.self, from: Data(json.utf8)) else { return "error" }
        return String(decoding: try! sortedEncoder.encode(s), as: UTF8.self)
    })
default:
    FileHandle.standardError.write(Data("unknown command \(args[1])\n".utf8))
    exit(2)
}
