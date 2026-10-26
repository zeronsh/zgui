import CoreImage
import CoreMedia
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers
// frameseq <pid> <delay> <seconds> [dir]: the sequence of distinct images a
// process's window shows, as the window server composites it (ScreenCaptureKit,
// as framepace.swift captures it). A frame counts when its pixels differ from
// the one before; it is a *revisit* when they equal one of the few distinct
// images shown just before (A, B, A): content older than the last frame shown
// again, as a surface presented with stale or half-updated pixels would show.
// A correct app's frames move forward, so its revisits are none (an
// animation that returns to an earlier state exactly can revisit, a few times
// a second at most). With `dir`, the first 120 changed frames are written
// there as PNGs, named by their order and short hash. JSON on stdout.
let a = CommandLine.arguments
let pid = pid_t(a[1])!, delay = Double(a[2])!, seconds = Double(a[3])!
let dir = a.count > 4 ? URL(fileURLWithPath: a[4]) : nil
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
/// How many distinct images back a repeat counts as a revisit.
let lookback = 4
/// FRAMESEQ_STAMP=1: also decode each capture's frame stamp (the workloads'
/// ZGX_FRAME_STAMP: 12 black or white 8-point cells from 8 points in, 8
/// down, then a white and a black marker), so every capture says which
/// frame it shows.
let stamping = ProcessInfo.processInfo.environment["FRAMESEQ_STAMP"] != nil

/// The stamp in a capture (half the window's points wide, from its content's
/// top), searched over the first rows; nil where none reads cleanly.
func stamp(_ base: UnsafeMutableRawPointer, _ width: Int, _ height: Int, _ stride: Int) -> Int? {
  func level(_ x: Int, _ y: Int) -> Int? {
    guard x < width, y < height else { return nil }
    let pixel = base.advanced(by: y * stride).assumingMemoryBound(to: UInt32.self)[x]
    let (b, g, r) = (pixel & 0xff, pixel >> 8 & 0xff, pixel >> 16 & 0xff)
    if r < 40 && g < 40 && b < 40 { return 0 }
    if r > 215 && g > 215 && b > 215 { return 1 }
    return nil
  }
  // Cells are 4 pixels wide here (8 points at half scale), the first from x 4.
  for y in 0..<min(24, height) {
    var value = 0
    var clean = true
    for bit in 0..<12 {
      guard let on = level(4 + bit * 4 + 2, y) else { clean = false; break }
      value |= on << bit
    }
    // The markers after the bits: white, then black.
    if clean && level(4 + 12 * 4 + 2, y) == 1 && level(4 + 13 * 4 + 2, y) == 0 { return value }
  }
  return nil
}

final class Recorder: NSObject, SCStreamOutput {
  var changes: [(Double, UInt64)] = []
  /// Every capture while recording: its display time and decoded stamp.
  var stamps: [(Double, Int?)] = []
  var revisits: [(Int, Int)] = []  // (change index, how many images back)
  var recent: [UInt64] = []
  var recording = false
  var last: UInt64 = 0
  var saved = 0
  let lock = NSLock()
  let context = CIContext()
  func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen,
          let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
          let raw = attachments.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
          let display = attachments.first?[.displayTime] as? UInt64,
          let pixels = CMSampleBufferGetImageBuffer(buffer)
    else { return }
    CVPixelBufferLockBaseAddress(pixels, .readOnly)
    guard let base = CVPixelBufferGetBaseAddress(pixels) else {
      CVPixelBufferUnlockBaseAddress(pixels, .readOnly); return
    }
    let (width, height, stride) = (CVPixelBufferGetWidth(pixels), CVPixelBufferGetHeight(pixels), CVPixelBufferGetBytesPerRow(pixels))
    // Every pixel: a revisit must be the same image, not a near one.
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    for y in 0..<height {
      let row = base.advanced(by: y * stride).assumingMemoryBound(to: UInt32.self)
      for x in 0..<width {
        hash = (hash ^ UInt64(row[x])) &* 0x100_0000_01b3
      }
    }
    let decoded = stamping ? stamp(base, width, height, stride) : nil
    CVPixelBufferUnlockBaseAddress(pixels, .readOnly)
    lock.withLock {
      defer { last = hash }
      if recording && stamping {
        stamps.append((Double(display) * Double(tb.numer) / Double(tb.denom) / 1e9, decoded))
      }
      guard recording, hash != last else { return }
      let index = changes.count
      changes.append((Double(display) * Double(tb.numer) / Double(tb.denom) / 1e9, hash))
      // `recent` holds the distinct images before this one, newest last;
      // the one just before is `last`, so a repeat of it is no change.
      // How many distinct images back this one was last shown, if lately.
      let before = Array(recent.dropLast())
      if let at = before.lastIndex(of: hash) {
        revisits.append((index, before.count - at + 1))
      }
      recent.append(hash)
      if recent.count > lookback + 1 { recent.removeFirst() }
      if let dir, saved < 120 {
        saved += 1
        let image = CIImage(cvPixelBuffer: pixels)
        if let cg = context.createCGImage(image, from: image.extent) {
          let url = dir.appendingPathComponent(String(format: "%03d-%08llx.png", index, hash & 0xffff_ffff))
          if let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) {
            CGImageDestinationAddImage(dest, cg, nil)
            CGImageDestinationFinalize(dest)
          }
        }
      }
    }
  }
}

func fail(_ message: String) -> Never {
  print("{\"error\":\"\(message)\"}"); exit(1)
}

let recorder = Recorder()
if let dir { try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true) }
Task {
  do {
    let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
    guard let window = content.windows
      .filter({ $0.owningApplication?.processID == pid })
      .max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height })
    else { fail("no window") }
    let rate = NSScreen.screens.map(\.maximumFramesPerSecond).max() ?? 60
    let config = SCStreamConfiguration()
    let titleBar: CGFloat = 32
    config.sourceRect = CGRect(x: 0, y: titleBar, width: window.frame.width, height: window.frame.height - titleBar)
    config.width = Int(window.frame.width / 2)
    config.height = Int((window.frame.height - titleBar) / 2)
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(rate))
    config.queueDepth = 8
    config.showsCursor = false
    let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: nil)
    try stream.addStreamOutput(recorder, type: .screen, sampleHandlerQueue: DispatchQueue(label: "frames"))
    try await stream.startCapture()
    try await Task.sleep(nanoseconds: UInt64(delay * 1e9))
    recorder.lock.withLock { recorder.recording = true }
    try await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
    let (changes, revisits, stamps) = recorder.lock.withLock {
      recorder.recording = false
      return (recorder.changes, recorder.revisits, recorder.stamps)
    }
    try await stream.stopCapture()
    let back2 = revisits.filter { $0.1 == 2 }.count
    // Intervals between changed images, as framepace reports them.
    let intervals = zip(changes.dropFirst(), changes).map { ($0.0 - $1.0) * 1000 }.sorted()
    let at = { (q: Double) in intervals.isEmpty ? 0 : intervals[min(intervals.count - 1, Int(q * Double(intervals.count)))] }
    let long = intervals.filter { $0 > 25 }.count
    let first = revisits.prefix(12).map { "\($0.0):\($0.1)" }.joined(separator: ",")
    print(String(format: "{\"display_hz\":%d,\"changes\":%d,\"changes_per_second\":%.3f,\"revisits\":%d,\"revisit_fraction\":%.5f,\"revisits_two_back\":%d,\"first_revisits\":\"%@\",\"interval_p50_ms\":%.3f,\"interval_p95_ms\":%.3f,\"interval_p99_ms\":%.3f,\"interval_max_ms\":%.3f,\"long_intervals\":%d,\"long_interval_fraction\":%.5f}",
      rate, changes.count, Double(changes.count) / seconds, revisits.count,
      changes.isEmpty ? 0 : Double(revisits.count) / Double(changes.count), back2, first,
      at(0.5), at(0.95), at(0.99), intervals.last ?? 0, long, intervals.isEmpty ? 0 : Double(long) / Double(intervals.count)))
    if stamping {
      // Captures: how often, and gaps that could hide a frame (16.7 ms or
      // more at 120 Hz). Frames: the stamps seen in order; a forward jump
      // of n misses n - 1 frames, the same stamp again is a duplicate
      // capture, a lower one a stale frame shown again (wrapping at 4096).
      let gaps = zip(stamps.dropFirst(), stamps).map { ($0.0 - $1.0) * 1000 }
      let captureGaps = gaps.filter { $0 >= 1000.0 / Double(rate) * 2 - 0.5 }.count
      let read = stamps.compactMap { $0.1 }
      var missing = 0, duplicates = 0, backwards = 0, distinct = 0
      for (next, prev) in zip(read.dropFirst(), read) {
        let step = (next - prev + 4096) % 4096
        if step == 0 { duplicates += 1 } else if step < 2048 { distinct += 1; missing += step - 1 } else { backwards += 1 }
      }
      let span = read.count > 1 ? (read.last! - read.first! + 4096) % 4096 : 0
      print(String(format: "{\"stamp_captures\":%d,\"captures_per_second\":%.3f,\"capture_gaps\":%d,\"stamps_read\":%d,\"frames_seen\":%d,\"frames_spanned\":%d,\"frames_missing\":%d,\"duplicate_captures\":%d,\"backwards\":%d}",
        stamps.count, Double(stamps.count) / seconds, captureGaps, read.count, distinct + (read.isEmpty ? 0 : 1), span, missing, duplicates, backwards))
    }
    exit(0)
  } catch {
    fail("\(error)".replacingOccurrences(of: "\"", with: "'"))
  }
}
RunLoop.main.run()
