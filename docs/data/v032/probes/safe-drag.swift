// tools/drag-file.swift <x> <y> <file>... — drag files onto the screen point (x, y) (global, top-left origin,
// points) the way Finder does: a small window at the screen's top-left starts an AppKit drag session with the
// files' URLs (NSDraggingSession, file URLs on the drag pasteboard), and the mouse is moved there and released
// by posted HID events (CGEvent). The receiving app sees an ordinary file drop. Prints the drag's operation.
// Needs the Accessibility permission for the process that runs it (posting mouse events).
// SAFE variant (glance-core v0.3.2, after drops landed on other apps and on crash dialogs): TARGET_PID names the
// only process allowed to receive the drop. Before the button goes down, the topmost window under the start point
// must be this tool's own; before release, the topmost window under the drop point must be TARGET_PID's. Otherwise
// the drag is carried back to this tool's window and released there (a drop on its own non-destination view, which
// AppKit refuses), "ABORT: ..." is printed and the exit code is 3. Nothing is ever dropped on another app.
import AppKit

let args = Array(CommandLine.arguments.dropFirst())
guard args.count >= 3, let tx = Double(args[0]), let ty = Double(args[1]) else {
    print("usage: drag-file <x> <y> <file>..."); exit(2)
}
let urls = args.dropFirst(2).map { URL(fileURLWithPath: $0) }

final class Source: NSView, NSDraggingSource {
    override func mouseDown(with event: NSEvent) {
        let items = urls.enumerated().map { i, url -> NSDraggingItem in
            let item = NSDraggingItem(pasteboardWriter: url as NSURL)
            let icon = NSWorkspace.shared.icon(forFile: url.path)
            item.setDraggingFrame(NSRect(x: 10 + i * 8, y: 10, width: 64, height: 64), contents: icon)
            return item
        }
        beginDraggingSession(with: items, event: event, source: self)
    }
    func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation { .copy }
    func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
        print("drag ended: operation \(operation.rawValue) (\(operation.contains(.copy) ? "copy" : "none"))")
        // Stay alive a moment after the drop, as Finder does: the receiver still talks to the source.
        let linger = Double(ProcessInfo.processInfo.environment["DRAG_LINGER"] ?? "2") ?? 2
        DispatchQueue.main.asyncAfter(deadline: .now() + linger) { NSApp.terminate(nil) }
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let screen = NSScreen.screens[0].frame
let size = NSSize(width: 100, height: 100)
let window = NSWindow(contentRect: NSRect(x: 40, y: screen.height - size.height - 80, width: size.width, height: size.height),
                      styleMask: [.titled], backing: .buffered, defer: false)
window.title = "drag"
window.contentView = Source(frame: NSRect(origin: .zero, size: size))
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)

let me = ProcessInfo.processInfo.processIdentifier
let targetPid = Int32(ProcessInfo.processInfo.environment["TARGET_PID"] ?? "") ?? -1
if targetPid <= 0 { print("ABORT: TARGET_PID not set"); exit(3) }
/// The owner of the topmost ordinary window (layers below the menu bar, any alpha above 0) under a global
/// top-left point; this tool's own windows are skipped only when `skipMine` (its drag image follows the cursor).
func ownerAt(_ x: Double, _ y: Double, skipMine: Bool) -> (Int32, String) {
    let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in all {
        let layer = w[kCGWindowLayer as String] as? Int ?? 0
        let alpha = w[kCGWindowAlpha as String] as? Double ?? 1
        let pid = w[kCGWindowOwnerPID as String] as? Int32 ?? -1
        guard layer < 24, alpha > 0, let b = w[kCGWindowBounds as String] as? [String: Double] else { continue }
        let r = CGRect(x: b["X"] ?? 0, y: b["Y"] ?? 0, width: b["Width"] ?? 0, height: b["Height"] ?? 0)
        guard r.contains(CGPoint(x: x, y: y)) else { continue }
        // The Dock's screen-sized overlay (layer 20, always present) is not a window anything drops on.
        let full = CGDisplayBounds(CGMainDisplayID())
        if (w[kCGWindowOwnerName as String] as? String) == "Dock" && r == full { continue }
        if skipMine && pid == me { continue }
        return (pid, "\(w[kCGWindowOwnerName as String] as? String ?? "?") layer \(layer) \(r)")
    }
    return (-1, "nothing")
}
func post(_ type: CGEventType, _ x: Double, _ y: Double) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: CGPoint(x: x, y: y), mouseButton: .left)?.post(tap: .cghidEventTap)
}
DispatchQueue.global().asyncAfter(deadline: .now() + 1.0) {
    // The window's content centre in global top-left coordinates.
    let f = window.frame
    let sx = f.midX, sy = screen.height - (f.minY + size.height / 2)
    let start = ownerAt(sx, sy, skipMine: false)
    guard start.0 == me else { print("ABORT: start point is under \(start.1) (pid \(start.0)), not this tool"); exit(3) }
    let drop = ownerAt(tx, ty, skipMine: true)
    guard drop.0 == targetPid else { print("ABORT: drop point is under \(drop.1) (pid \(drop.0)), not pid \(targetPid)"); exit(3) }
    post(.mouseMoved, sx, sy); usleep(150_000)
    post(.leftMouseDown, sx, sy); usleep(150_000)
    let steps = 40
    for i in 1...steps {
        let t = Double(i) / Double(steps)
        post(.leftMouseDragged, sx + (tx - sx) * t, sy + (ty - sy) * t)
        usleep(25_000)
    }
    usleep(400_000)
    post(.leftMouseDragged, tx, ty); usleep(300_000)
    let last = ownerAt(tx, ty, skipMine: true)
    guard last.0 == targetPid else {
        // Carry the drag home and release it over this tool's own window.
        for i in 1...steps { let t = Double(i) / Double(steps); post(.leftMouseDragged, tx + (sx - tx) * t, ty + (sy - ty) * t); usleep(10_000) }
        post(.leftMouseUp, sx, sy)
        print("ABORT: before release the drop point is under \(last.1) (pid \(last.0)), not pid \(targetPid); released over this tool")
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { exit(3) }
        return
    }
    post(.leftMouseUp, tx, ty)
    DispatchQueue.main.asyncAfter(deadline: .now() + 15) { print("drag did not end in 15 s"); exit(1) }
}
app.run()
