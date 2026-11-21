// cover <pid> <wait-s> <cover-s>: once the window of process <pid> is on screen, wait <wait-s>, then lay an opaque
// window of this (other) process over it, 40 pt larger on every side, for <cover-s>; then take it away and exit.
// Prints "seen|on|off <unix-ms>" lines. It posts no input events and does not activate itself.
import AppKit
let a = CommandLine.arguments
guard a.count == 4, let pid = Int32(a[1]), let wait = Double(a[2]), let hold = Double(a[3]) else { print("usage: cover <pid> <wait-s> <cover-s>"); exit(2) }
func ms() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }
func bounds() -> CGRect? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else { return nil }
    var r: CGRect?
    for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 {
        if let d = w[kCGWindowBounds as String] as? NSDictionary, let b = CGRect(dictionaryRepresentation: d) { r = r.map { $0.union(b) } ?? b }
    }
    return r
}
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
var b: CGRect?
for _ in 0..<200 { b = bounds(); if b != nil { break }; Thread.sleep(forTimeInterval: 0.05) }
guard let cg = b, let screen = NSScreen.screens.first else { print("no window for pid \(pid)"); exit(1) }
print("seen \(ms())"); fflush(stdout)
let H = screen.frame.height
let rect = NSRect(x: cg.minX - 40, y: H - cg.maxY - 40, width: cg.width + 80, height: cg.height + 80)
let win = NSWindow(contentRect: rect, styleMask: .borderless, backing: .buffered, defer: false)
win.backgroundColor = NSColor(white: 0.3, alpha: 1); win.isOpaque = true; win.hasShadow = false
DispatchQueue.main.asyncAfter(deadline: .now() + wait) {
    win.orderFrontRegardless(); print("on \(ms())"); fflush(stdout)
    DispatchQueue.main.asyncAfter(deadline: .now() + hold) {
        win.orderOut(nil); print("off \(ms())"); fflush(stdout)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { exit(0) }
    }
}
app.run()
