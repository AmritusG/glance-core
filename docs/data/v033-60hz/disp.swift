import CoreGraphics
var ids = [CGDirectDisplayID](repeating: 0, count: 8); var n: UInt32 = 0
CGGetActiveDisplayList(8, &ids, &n)
for id in ids.prefix(Int(n)) { let m = CGDisplayCopyDisplayMode(id)!; print(id, CGDisplayIsBuiltin(id) != 0 ? "built-in" : "external", CGDisplayVendorNumber(id), CGDisplayModelNumber(id), m.width, "x", m.height, "@", m.refreshRate, "Hz", "pixels", m.pixelWidth, "x", m.pixelHeight, "bounds", CGDisplayBounds(id)) }
