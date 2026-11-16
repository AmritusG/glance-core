// tools/window-id.swift <pid> [layer] [small] — the on-screen window of <pid> at <layer> (default 0, a normal
// window; 8 is an app-modal alert's): the largest, or with "small" the smallest (an alert at layer 0, as Swift
// Glance's first-launch alert is): "<window-number> <x> <y> <width> <height>" in global points, top-left origin,
// for `screencapture -l` and for clicking at known offsets. Read-only.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
let pid = Int32(args.count > 1 ? args[1] : "") ?? -1
let layer = Int(args.count > 2 ? args[2] : "0") ?? 0
let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let mine = all.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == layer }
func area(_ w: [String: Any]) -> Double { let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]; return (b["Width"] ?? 0) * (b["Height"] ?? 0) }
// The largest (in full screen the title bar is a window of its own).
let small = args.count > 3 && args[3] == "small"
let pick = small ? mine.filter { area($0) > 10_000 }.min(by: { area($0) < area($1) }) : mine.max(by: { area($0) < area($1) })
guard let w = pick, let b = w[kCGWindowBounds as String] as? [String: Double] else { exit(1) }
print("\(w[kCGWindowNumber as String] ?? 0) \(Int(b["X"] ?? 0)) \(Int(b["Y"] ?? 0)) \(Int(b["Width"] ?? 0)) \(Int(b["Height"] ?? 0))")
