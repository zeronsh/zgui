import Foundation
import CoreGraphics
// hidden_window_probe <binary>: a window made hidden shows whole (the
// hidden_window example). Spawns the binary, which prints SHOW as it asks its
// window to show; then polls the window list for its window and, from the
// first poll it is on screen, captures a strip of its content each 0.5 ms
// until that strip holds the content (not flat). Prints JSON: whether the
// window was on screen before SHOW (it must not be), ms from SHOW to the
// window on screen and to content there, the flat captures seen on screen
// before content (0: no blank frame), and the process's CPU over the second
// before SHOW (hidden, after its first frame: about none). Needs Screen
// Recording permission; CGWindowListCreateImage is looked up at run time.
// Build: swiftc -O scripts/hidden_window_probe.swift -o /tmp/zgx-hidden-probe
typealias Capture = @convention(c) (CGRect, CGWindowListOption, CGWindowID, CGWindowImageOption) -> Unmanaged<CGImage>?
let capture = unsafeBitCast(dlsym(dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_NOW), "CGWindowListCreateImage"), to: Capture.self)
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func now() -> Double { Double(mach_absolute_time()) * Double(tb.numer) / Double(tb.denom) / 1e6 }
let binary = CommandLine.arguments[1]
var pid: pid_t = 0
var actions: posix_spawn_file_actions_t? = nil
posix_spawn_file_actions_init(&actions)
var out: [Int32] = [0, 0]
pipe(&out)
posix_spawn_file_actions_adddup2(&actions, out[1], 1)
let cargs = [strdup(binary), nil]
guard posix_spawn(&pid, binary, &actions, nil, cargs, environ) == 0 else { fatalError("spawn") }
close(out[1])
func cpu() -> Double {
  var info = rusage_info_v4()
  let ok = withUnsafeMutablePointer(to: &info) { $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) } }
  guard ok == 0 else { return 0 }
  return Double(info.ri_user_time + info.ri_system_time) * Double(tb.numer) / Double(tb.denom) / 1e6
}
func onScreen() -> (CGWindowID, CGRect)? {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
  guard let w = list.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }),
        let b = w[kCGWindowBounds as String] as? NSDictionary, let r = CGRect(dictionaryRepresentation: b), r.width > 100 else { return nil }
  return (w[kCGWindowNumber as String] as! CGWindowID, r)
}
// Read the app's lines on a thread of its own; SHOW's time is when it's read.
var shownAt = 0.0
let lock = NSLock()
Thread.detachNewThread {
  var buffer = [UInt8](repeating: 0, count: 4096), text = ""
  while true {
    let n = read(out[0], &buffer, buffer.count)
    if n <= 0 { break }
    text += String(decoding: buffer[0..<n], as: UTF8.self)
    if text.contains("SHOW") { lock.lock(); if shownAt == 0 { shownAt = now() }; lock.unlock() }
  }
}
func shown() -> Double { lock.lock(); defer { lock.unlock() }; return shownAt }
// The capture's first call is slow (~150 ms): made now, of a strip of the
// screen, so the window's first is as quick as the rest.
_ = capture(CGRect(x: 0, y: 0, width: 64, height: 64), .optionOnScreenOnly, kCGNullWindowID, [.nominalResolution])
// Hidden: never on screen before SHOW; CPU over the last second of it.
let t0 = now()
var earlyOnScreen = false, cpuStart = 0.0, cpuStartAt = 0.0, hiddenCpu = -1.0
while shown() == 0 && now() - t0 < 20000 {
  if onScreen() != nil { earlyOnScreen = true }
  let t = now() - t0
  if t > 300 && cpuStartAt == 0 { cpuStart = cpu(); cpuStartAt = now() }
  usleep(2000)
}
let show = shown()
guard show != 0 else { kill(pid, SIGKILL); fatalError("no SHOW") }
if cpuStartAt != 0 { hiddenCpu = (cpu() - cpuStart) / (show - cpuStartAt) }
var window: CGWindowID = 0, bounds = CGRect.zero, tWindow = 0.0
while now() - show < 5000 {
  if let (w, r) = onScreen() { window = w; bounds = r; tWindow = now() - show; break }
  usleep(500)
}
guard window != 0 else { kill(pid, SIGKILL); fatalError("never shown") }
let strip = CGRect(x: bounds.minX, y: bounds.minY + 28, width: 400, height: 50)
var tContent = 0.0, flat = 0, missing = 0, firstImage = 0.0
while now() - show < 5000 {
  if let image = capture(strip, .optionIncludingWindow, window, [.boundsIgnoreFraming, .nominalResolution])?.takeRetainedValue(),
     let data = image.dataProvider?.data, let p = CFDataGetBytePtr(data) {
    if firstImage == 0 { firstImage = now() - show }
    let n = CFDataGetLength(data) / 4
    var distinct = Set<UInt32>()
    p.withMemoryRebound(to: UInt32.self, capacity: n) { px in for i in Swift.stride(from: 0, to: n, by: 7) { distinct.insert(px[i]); if distinct.count > 8 { break } } }
    if distinct.count > 8 { tContent = now() - show; break }
    flat += 1
  } else {
    missing += 1
  }
  usleep(500)
}
kill(pid, SIGKILL)
var status: Int32 = 0; waitpid(pid, &status, 0)
print(String(format: "{\"on_screen_before_show\":%@,\"window_ms\":%.1f,\"first_image_ms\":%.1f,\"content_ms\":%.1f,\"flat_captures\":%d,\"no_image_captures\":%d,\"hidden_cpu_percent\":%.2f}", earlyOnScreen ? "true" : "false", tWindow, firstImage, tContent, flat, missing, hiddenCpu * 100))
