// tools/menu-dump.swift <pid> — every item of an app's menu bar through Accessibility (the Apple menu and the
// system's Services submenu left out): "menu | item | key | modifiers | enabled", submenu items as
// "menu | item > sub". Modifiers are AXMenuItemCmdModifiers: 0 = ⌘, 1 = ⇧⌘, 2 = ⌥⌘, 4 = ⌃⌘, 8 = no ⌘. Read-only.
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else { print("usage: menu-dump <pid>"); exit(2) }
func attr(_ e: AXUIElement, _ name: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}
func children(_ e: AXUIElement) -> [AXUIElement] { attr(e, kAXChildrenAttribute) as? [AXUIElement] ?? [] }
func title(_ e: AXUIElement) -> String {
    let t = attr(e, kAXTitleAttribute) as? String ?? ""
    return t.isEmpty ? "---" : t
}
let app = AXUIElementCreateApplication(pid)
guard let bar = attr(app, kAXMenuBarAttribute) else { print("no menu bar (Accessibility permission?)"); exit(1) }
for (i, top) in children(bar as! AXUIElement).enumerated() where i > 0 {
    let menuName = title(top)
    for menu in children(top) {
        for item in children(menu) {
            let name = title(item)
            let key = attr(item, "AXMenuItemCmdChar") as? String ?? ""
            let mods = key.isEmpty ? "" : "\(attr(item, "AXMenuItemCmdModifiers") as? Int ?? 0)"
            let enabled = (attr(item, kAXEnabledAttribute) as? Bool ?? false) ? "on" : "off"
            print("\(menuName) | \(name) | \(key) | \(mods) | \(enabled)")
            guard name != "Services" else { continue }
            for sub in children(item) {
                for s in children(sub) {
                    let sk = attr(s, "AXMenuItemCmdChar") as? String ?? ""
                    print("\(menuName) | \(name) > \(title(s)) | \(sk) | \(sk.isEmpty ? "" : "\(attr(s, "AXMenuItemCmdModifiers") as? Int ?? 0)") | \((attr(s, kAXEnabledAttribute) as? Bool ?? false) ? "on" : "off")")
                }
            }
        }
    }
}
