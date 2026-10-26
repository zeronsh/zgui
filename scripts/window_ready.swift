import AppKit
import CoreGraphics
import Foundation

// window_ready <pid> <timeout>: wait for this benchmark's window to be
// frontmost and uncovered. Only activates the benchmark process; no clicks,
// keys, permission requests, or actions on system dialogs.
let pid = pid_t(CommandLine.arguments[1])!
let timeout = Double(CommandLine.arguments[2])!
let began = Date()
let dialogOwners: Set<String> = ["UserNotificationCenter", "CoreServicesUIAgent", "Problem Reporter", "universalAccessAuthWarn", "SecurityAgent"]
func emit(_ value: [String: Any], _ code: Int32) -> Never {
    print(String(data: try! JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), encoding: .utf8)!)
    exit(code)
}
func bounds(_ window: [String: Any]) -> CGRect {
    guard let raw = window[kCGWindowBounds as String] as? NSDictionary else { return .zero }
    return CGRect(dictionaryRepresentation: raw) ?? .zero
}
var last: [String: Any] = [:]
while Date().timeIntervalSince(began) < timeout {
    guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated else {
        emit(["error": "app exited before its window was ready", "pid": pid], 1)
    }
    guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
        emit(["error": "cannot read on-screen windows", "pid": pid], 1)
    }
    let dialogs = windows.filter { dialogOwners.contains($0[kCGWindowOwnerName as String] as? String ?? "") }
        .map { $0[kCGWindowOwnerName as String] as? String ?? "unknown" }
    if !dialogs.isEmpty { emit(["error": "system dialog on screen; not touching it", "dialogs": dialogs], 1) }
    let normal = windows.filter { ($0[kCGWindowLayer as String] as? Int) == 0 && ($0[kCGWindowAlpha as String] as? Double ?? 1) > 0 }
    if let index = normal.firstIndex(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && bounds($0).width > 100 && bounds($0).height > 100 }) {
        let frame = bounds(normal[index])
        let covering = normal.prefix(index).filter {
            let overlap = frame.intersection(bounds($0))
            return !overlap.isNull && overlap.width > 1 && overlap.height > 1
        }.map { $0[kCGWindowOwnerName as String] as? String ?? "unknown" }
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier == pid
        last = ["window_exists": true, "frontmost": front, "covering": covering]
        if front && covering.isEmpty {
            emit(["ready": true, "pid": pid, "window_id": normal[index][kCGWindowNumber as String] ?? 0,
                  "ready_ms": Date().timeIntervalSince(began) * 1000], 0)
        }
        app.activate()
    } else {
        last = ["window_exists": false]
    }
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
}
last["error"] = "window did not become frontmost and uncovered before timeout"
last["pid"] = pid
emit(last, 1)
