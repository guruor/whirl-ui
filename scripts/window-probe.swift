// Print the on-screen windows of one process, one per line.
//
// Window *metadata* (owner, pid, layer, bounds, the on-screen flag) needs no
// permission. The window *title* needs Screen Recording permission, so it can
// come back empty; nothing here depends on it, and nothing here asks for a
// permission or reads a pixel.
//
// usage: window-probe <pid>
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 2, let wanted = Int(args[1]) else {
    FileHandle.standardError.write("usage: window-probe <pid>\n".data(using: .utf8)!)
    exit(2)
}

let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    print("no-window-list")
    exit(1)
}

for info in list {
    let ownerPid = info[kCGWindowOwnerPID as String] as? Int ?? -1
    guard ownerPid == wanted else { continue }
    let id = info[kCGWindowNumber as String] as? Int ?? -1
    let owner = info[kCGWindowOwnerName as String] as? String ?? "?"
    let title = info[kCGWindowName as String] as? String ?? ""
    let layer = info[kCGWindowLayer as String] as? Int ?? -1
    let onscreen = info[kCGWindowIsOnscreen as String] as? Bool ?? false
    let alpha = info[kCGWindowAlpha as String] as? Double ?? -1
    let bounds = info[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let w = bounds["Width"] as? Double ?? -1
    let h = bounds["Height"] as? Double ?? -1
    print("window id=\(id) owner=\(owner) pid=\(ownerPid) layer=\(layer) onscreen=\(onscreen) alpha=\(alpha) size=\(Int(w))x\(Int(h)) title=\(title.isEmpty ? "(empty)" : title)")
}
print("probe-done pid=\(wanted)")
