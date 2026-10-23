import AppKit
import CoreMedia
import Foundation
import ScreenCaptureKit
// macos_latency_probe <pid> <samples> <clickX> <clickY> <rx> <ry> <rw> <rh> [--hid] [--tab]:
// keystroke to composited frame on macOS, as scripts/windows_latency_probe.py is on Windows.
// Points are window-relative from its top-left (the title bar included). Each sample types
// one character and times the first frame ScreenCaptureKit sees with the region changed,
// by its display time: input to composition (scan-out adds up to a refresh).
// --hid posts through the HID tap, as a keyboard does, and only while the app is
// frontmost (so a key never lands elsewhere); otherwise keys go to the process alone.
// --tab presses Tab first to focus the field. Needs Accessibility and Screen Recording.
//   swiftc -O scripts/macos_latency_probe.swift -o /tmp/zgui-latency-probe
//   swiftc -O scripts/macos_native_field.swift -o /tmp/zgui-native-field   # AppKit baseline
//   cargo build --release -p zgui-desktop --example input_latency
//   target/release/examples/input_latency & /tmp/zgui-latency-probe $! 60 64 72 24 52 520 40 --hid --tab
_ = NSApplication.shared  // a window-server connection for AppKit and CGEvent
let a = CommandLine.arguments
let pid = pid_t(a[1])!, samples = Int(a[2])!
let click = CGPoint(x: Double(a[3])!, y: Double(a[4])!)
let region = CGRect(x: Double(a[5])!, y: Double(a[6])!, width: Double(a[7])!, height: Double(a[8])!)
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func ms(_ ticks: UInt64) -> Double { Double(ticks) * Double(tb.numer) / Double(tb.denom) / 1e6 }

final class Watch: NSObject, SCStreamOutput {
  let lock = NSLock()
  var frames: [(UInt64, UInt64)] = []  // (display time, region hash)
  var scale = 1.0
  func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen,
          let att = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
          let raw = att.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
          let display = att.first?[.displayTime] as? UInt64,
          let px = CMSampleBufferGetImageBuffer(buffer) else { return }
    CVPixelBufferLockBaseAddress(px, .readOnly); defer { CVPixelBufferUnlockBaseAddress(px, .readOnly) }
    guard let base = CVPixelBufferGetBaseAddress(px) else { return }
    let stride = CVPixelBufferGetBytesPerRow(px), w = CVPixelBufferGetWidth(px), h = CVPixelBufferGetHeight(px)
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    let x0 = Int(region.minX * scale), x1 = min(w, Int(region.maxX * scale))
    let y0 = Int(region.minY * scale), y1 = min(h, Int(region.maxY * scale))
    for y in Swift.stride(from: y0, to: y1, by: 1) {
      let row = base.advanced(by: y * stride).assumingMemoryBound(to: UInt32.self)
      for x in x0..<x1 { hash = (hash ^ UInt64(row[x])) &* 0x100_0000_01b3 }
    }
    lock.lock(); frames.append((display, hash)); lock.unlock()
  }
  func last() -> (UInt64, UInt64)? { lock.lock(); defer { lock.unlock() }; return frames.last }
  func after(_ t: UInt64, differing hash: UInt64) -> UInt64? {
    lock.lock(); defer { lock.unlock() }
    return frames.first(where: { $0.0 > t && $0.1 != hash })?.0
  }
}

let hid = CommandLine.arguments.contains("--hid")
/// The owner of the topmost ordinary window, from the window server itself
/// (NSWorkspace's answer goes stale without a running main loop).
func frontmostOwner() -> pid_t? {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
  return list.first(where: { ($0[kCGWindowLayer as String] as? Int) == 0 })?[kCGWindowOwnerPID as String] as? pid_t
}
/// Posts a key to the app under test: to its process (`postToPid`), or with
/// `--hid` through the HID tap, only while it is the frontmost app, so a
/// key never lands anywhere else. False if it was not posted.
@discardableResult
func key(_ code: CGKeyCode, flags: CGEventFlags = []) -> Bool {
  if hid && frontmostOwner() != pid { return false }
  let src = CGEventSource(stateID: .hidSystemState)
  for down in [true, false] {
    let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)!
    e.flags = flags
    if hid { e.post(tap: .cghidEventTap) } else { e.postToPid(pid) }
  }
  return true
}

let sem = DispatchSemaphore(value: 0)
Task {
  let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
  guard let window = content.windows.filter({ $0.owningApplication?.processID == pid && $0.windowLayer == 0 })
          .max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) else {
    print("{\"error\":\"no window\"}"); exit(1)
  }
  let frame = window.frame
  let config = SCStreamConfiguration()
  config.width = Int(frame.width) * 2; config.height = Int(frame.height) * 2
  config.minimumFrameInterval = CMTime(value: 1, timescale: 240)
  config.queueDepth = 8; config.showsCursor = false
  let watch = Watch(); watch.scale = 2
  let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: nil)
  try stream.addStreamOutput(watch, type: .screen, sampleHandlerQueue: DispatchQueue(label: "f"))
  try await stream.startCapture()
  NSRunningApplication(processIdentifier: pid)?.activate()
  try await Task.sleep(nanoseconds: 600_000_000)
  // Click the field to focus it.
  let p = CGPoint(x: frame.minX + click.x, y: frame.minY + click.y)
  for t in [CGEventType.leftMouseDown, .leftMouseUp] {
    CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: .left)!.postToPid(pid)
  }
  try await Task.sleep(nanoseconds: 400_000_000)
  if CommandLine.arguments.contains("--tab") { key(48); try await Task.sleep(nanoseconds: 300_000_000) }
  var lat: [Double] = []

  for i in 0..<samples {
    if i % 20 == 0 { key(0, flags: .maskCommand); key(51); try await Task.sleep(nanoseconds: 300_000_000) }  // cmd-A, delete
    guard let (_, before) = watch.last() else { continue }
    let t0 = mach_absolute_time()
    if !key(0) { continue }  // 'a'
    var seen: UInt64? = nil
    for _ in 0..<100 { if let t = watch.after(t0, differing: before) { seen = t; break }; try await Task.sleep(nanoseconds: 2_000_000) }
    if let t = seen { lat.append(ms(t - t0)) }
    try await Task.sleep(nanoseconds: UInt64.random(in: 120_000_000...200_000_000))
  }
  try await stream.stopCapture()
  lat.sort()
  let med = lat.isEmpty ? -1 : lat[lat.count / 2], p90 = lat.isEmpty ? -1 : lat[min(lat.count - 1, lat.count * 9 / 10)]
  print(String(format: "{\"samples\":%d,\"median_ms\":%.2f,\"p90_ms\":%.2f,\"min_ms\":%.2f,\"max_ms\":%.2f}", lat.count, med, p90, lat.first ?? -1, lat.last ?? -1))
  sem.signal()
}
sem.wait()
