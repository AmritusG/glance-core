// The settings round-trip harness (glance-core C3): Swift Glance's own settings code, run as a
// script against a throw-away preferences domain, so the Rust store is checked against what the
// Swift app writes and reads — never against Swift Glance's real domain (com.amritus.glance).
//
//   xcrun swift harness.swift write <domain>   Swift Glance writes its three settings
//   xcrun swift harness.swift read <domain>    Swift Glance reads them back; prints JSON
//
// Copied from Swift Glance a9dc601 (MIT, Amrit's own): AlphaTypeMemory.swift:10-26 (with the
// `defaults:` parameter it already has), PlayerModel.swift:63-71 (backgroundMode),
// DefaultHandlerControl.swift:25,43-50 (the prompt flag); AlphaType / BackgroundMode raw values
// from GlanceCore/AlphaType.swift:32-36 and GlancePlayback/DXVRenderer.swift:75-78.
import Foundation

enum AlphaType: String { case premultiplied, straight }
enum BackgroundMode: String { case black, checkerboard }

enum AlphaTypeMemory {
    static let defaultsKey = "Glance.alphaTypeByClip"
    static let capacity = 2000

    static func choice(for key: String, defaults: UserDefaults) -> AlphaType? {
        let prefix = key + "="
        guard let line = (defaults.stringArray(forKey: defaultsKey) ?? []).last(where: { $0.hasPrefix(prefix) }) else { return nil }
        return AlphaType(rawValue: String(line.dropFirst(prefix.count)))
    }

    static func remember(_ type: AlphaType, for key: String, defaults: UserDefaults) {
        let prefix = key + "="
        var lines = (defaults.stringArray(forKey: defaultsKey) ?? []).filter { !$0.hasPrefix(prefix) }
        lines.append(prefix + type.rawValue)
        if lines.count > capacity { lines.removeFirst(lines.count - capacity) }
        defaults.set(lines, forKey: defaultsKey)
    }
}

func key(_ i: Int) -> String { String(format: "%064x", i) }

let args = CommandLine.arguments
guard args.count == 3, let defaults = UserDefaults(suiteName: args[2]) else {
    FileHandle.standardError.write("usage: harness.swift write|read <domain>\n".data(using: .utf8)!)
    exit(2)
}

switch args[1] {
case "write":
    // PlayerModel.backgroundMode didSet
    defaults.set(BackgroundMode.checkerboard.rawValue, forKey: "Glance.backgroundMode")
    // DefaultHandlerControl.markPromptShown
    defaults.set(true, forKey: "Glance.defaultHandlerPromptShown")
    // AlphaTypeMemory: 2,003 choices (the cap drops the 3 oldest), then clip 5 changes its mind.
    for i in 0..<2003 { AlphaTypeMemory.remember(i % 2 == 0 ? .straight : .premultiplied, for: key(i), defaults: defaults) }
    AlphaTypeMemory.remember(.premultiplied, for: key(4), defaults: defaults)
    // Store-design probes (CueGlance / GlEnc / MaCrate shapes): nested string arrays, Data, Int,
    // Double, Float.
    defaults.set([[key(1), "straight"], [key(2), "premultiplied"]], forKey: "Probe.pairs")
    defaults.set(try! JSONEncoder().encode(["quality": 0.6]), forKey: "Probe.json")
    defaults.set(64, forKey: "Probe.int")
    defaults.set(180.0, forKey: "Probe.double")
    defaults.set(Float(0.5), forKey: "Probe.float")
    defaults.synchronize()
case "read":
    var out: [String: Any] = [:]
    out["background"] = (BackgroundMode(rawValue: defaults.string(forKey: "Glance.backgroundMode") ?? "") ?? .black).rawValue
    out["promptShown"] = defaults.bool(forKey: "Glance.defaultHandlerPromptShown")
    let lines = defaults.stringArray(forKey: AlphaTypeMemory.defaultsKey)
    out["alphaLines"] = lines?.count ?? -1
    var choices: [String: String] = [:]
    for i in [0, 1, 2, 3, 4, 7, 2002, 9000, 9001] { choices[String(i)] = AlphaTypeMemory.choice(for: key(i), defaults: defaults)?.rawValue ?? "none" }
    out["choices"] = choices
    out["probeDouble"] = defaults.double(forKey: "Probe.rustDouble")
    out["probeInt"] = defaults.integer(forKey: "Probe.rustInt")
    out["probeData"] = (defaults.data(forKey: "Probe.rustData") ?? Data()).map { String(format: "%02x", $0) }.joined()
    out["probePairs"] = (defaults.array(forKey: "Probe.rustPairs") as? [[String]]) ?? []
    let json = try! JSONSerialization.data(withJSONObject: out, options: [.sortedKeys])
    print(String(data: json, encoding: .utf8)!)
default:
    exit(2)
}
