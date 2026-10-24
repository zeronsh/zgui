import Foundation
import CoreGraphics
// startup_probe <binary> [args...]: launch to first content, framework-neutral.
// Spawns the binary (with this process's environment), polls the window list
// until the process shows a window, then captures a strip of its top-left
// content until it is no longer flat (the first frame's text drew). Prints
// JSON: t0 (mach ms), ms to window, ms to content, and the process's
// instructions retired and CPU time at content. Wall times swing with load
// and the window server; instructions are the stable comparison. Needs
// Screen Recording permission. The capture is CGWindowListCreateImage,
// looked up at run time (newer SDKs no longer declare it): it is quick
// enough to poll, as ScreenCaptureKit's setup is not.
// Build: swiftc -O scripts/startup_probe.swift -o /tmp/zgx-startup-probe
typealias Capture = @convention(c) (CGRect, CGWindowListOption, CGWindowID, CGWindowImageOption) -> Unmanaged<CGImage>?
let capture = unsafeBitCast(dlsym(dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_NOW), "CGWindowListCreateImage"), to: Capture.self)
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func now() -> Double { Double(mach_absolute_time()) * Double(tb.numer) / Double(tb.denom) / 1e6 }
let args = Array(CommandLine.arguments.dropFirst())
var pid: pid_t = 0
let cargs = args.map { strdup($0) } + [nil]
let t0 = now()
guard posix_spawn(&pid, args[0], nil, nil, cargs, environ) == 0 else { fatalError("spawn") }
func usage() -> (UInt64, Double) {
  var info = rusage_info_v4()
  let ok = withUnsafeMutablePointer(to: &info) { $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) } }
  guard ok == 0 else { return (0, 0) }
  return (info.ri_instructions, Double(info.ri_user_time + info.ri_system_time) * Double(tb.numer) / Double(tb.denom) / 1e6)
}
var window: CGWindowID = 0, bounds = CGRect.zero, tWindow = 0.0
while now() - t0 < 20000 {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
  if let w = list.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }),
     let b = w[kCGWindowBounds as String] as? NSDictionary, let r = CGRect(dictionaryRepresentation: b), r.width > 100 {
    window = w[kCGWindowNumber as String] as! CGWindowID; bounds = r; tWindow = now() - t0; break
  }
  usleep(500)
}
guard window != 0 else { kill(pid, SIGKILL); fatalError("no window") }
// The header strip: below the title bar, the left 400 points.
let strip = CGRect(x: bounds.minX, y: bounds.minY + 28, width: 400, height: 50)
var tContent = 0.0
while now() - t0 < 20000 {
  if let image = capture(strip, .optionIncludingWindow, window, [.boundsIgnoreFraming, .nominalResolution])?.takeRetainedValue(),
     let data = image.dataProvider?.data, let p = CFDataGetBytePtr(data) {
    let n = CFDataGetLength(data) / 4
    var distinct = Set<UInt32>()
    p.withMemoryRebound(to: UInt32.self, capacity: n) { px in for i in Swift.stride(from: 0, to: n, by: 7) { distinct.insert(px[i]); if distinct.count > 8 { break } } }
    if distinct.count > 8 { tContent = now() - t0; break }
  }
  usleep(500)
}
let (instructions, cpu) = usage()
kill(pid, SIGKILL)
var status: Int32 = 0; waitpid(pid, &status, 0)
print(String(format: "{\"t0\":%.1f,\"window_ms\":%.1f,\"content_ms\":%.1f,\"instructions\":%llu,\"cpu_ms\":%.1f}", t0, tWindow, tContent, instructions, cpu))
