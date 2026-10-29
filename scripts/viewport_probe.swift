// viewport_probe PID WARMUP SECONDS RATE PNG: what the viewport scene's
// window shows (examples/viewport_workload.rs). After WARMUP seconds it
// captures the window of PID RATE times a second for SECONDS
// (ScreenCaptureKit, at the window's points), writes the first capture to
// PNG, and counts captures in which magenta, the colour under the GPU view in
// `ZGX_MODE=switch`, shows anywhere in the window's middle: a frame where
// neither the view's own layer nor its composited image covered it, as it
// moves between the two. Prints one JSON line. It needs Screen Recording
// permission, which it only checks (CGPreflightScreenCaptureAccess): it
// never asks.
//
//   swiftc -O scripts/viewport_probe.swift -o /tmp/zgx-viewport-probe
import AppKit
import CoreGraphics
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count == 6, let pid = pid_t(args[1]), let warmup = Double(args[2]),
      let seconds = Double(args[3]), let rate = Double(args[4]) else {
    FileHandle.standardError.write("usage: viewport_probe PID WARMUP SECONDS RATE PNG\n".data(using: .utf8)!)
    exit(2)
}
let png = URL(fileURLWithPath: args[5])
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
guard CGPreflightScreenCaptureAccess() else {
    print("{\"error\":\"no screen recording permission\"}")
    exit(1)
}

/// Magenta pixels in the middle half of `image` (a stride of 4 points).
func magenta(_ image: CGImage) -> Int {
    guard let data = image.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { return 0 }
    let perRow = image.bytesPerRow, perPixel = image.bitsPerPixel / 8
    let bgra = image.bitmapInfo.contains(.byteOrder32Little)
    var found = 0
    for y in stride(from: image.height / 4, to: image.height * 3 / 4, by: 4) {
        for x in stride(from: image.width / 4, to: image.width * 3 / 4, by: 4) {
            let p = bytes + y * perRow + x * perPixel
            let (r, g, b) = bgra ? (p[2], p[1], p[0]) : (p[0], p[1], p[2])
            if r > 200 && g < 70 && b > 200 { found += 1 }
        }
    }
    return found
}

/// The frame count the viewport scene's painter draws (support/cubes.rs):
/// 8 blocks along the window's bottom left, white a 1, from bit 0.
func count(_ image: CGImage) -> Int? {
    guard let data = image.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { return nil }
    let perRow = image.bytesPerRow, perPixel = image.bitsPerPixel / 8
    let bgra = image.bitmapInfo.contains(.byteOrder32Little)
    var value = 0
    for bit in 0..<8 {
        let x = 2 + bit * 12 + 5, y = image.height - 7
        guard x < image.width, y >= 0 else { return nil }
        let p = bytes + y * perRow + x * perPixel
        let (r, g, b) = bgra ? (p[2], p[1], p[0]) : (p[0], p[1], p[2])
        let level = (Int(r) + Int(g) + Int(b)) / 3
        if level > 200 { value |= 1 << bit } else if level > 55 { return nil }
    }
    return value
}

Task {
    try await Task.sleep(nanoseconds: UInt64(warmup * 1e9))
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard let window = content.windows.first(where: { $0.owningApplication?.processID == pid && $0.frame.width > 100 }) else {
        print("{\"error\":\"no window for pid \(pid)\"}")
        exit(1)
    }
    let filter = SCContentFilter(desktopIndependentWindow: window)
    let config = SCStreamConfiguration()
    config.width = Int(window.frame.width)
    config.height = Int(window.frame.height)
    config.showsCursor = false
    config.colorSpaceName = CGColorSpace.sRGB
    var captures = 0, uncovered = 0, most = 0, written = false, trace = "", wroteUncovered = false
    var last: Int? = nil, backwards = 0, unread = 0, steps = ""
    var phase = "" 
    let end = Date().addingTimeInterval(seconds)
    while Date() < end {
        let started = Date()
        if let image = try? await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) {
            captures += 1
            if !written, let out = CGImageDestinationCreateWithURL(png as CFURL, UTType.png.identifier as CFString, 1, nil) {
                CGImageDestinationAddImage(out, image, nil)
                written = CGImageDestinationFinalize(out)
            }
            let shown = magenta(image)
            most = max(most, shown)
            if shown > 0 { uncovered += 1 }
            trace += shown > 0 ? "x" : "."
            // Frames never show out of order: each capture's count is the
            // last one's or later (mod 256, a few frames a capture).
            if let n = count(image) {
                if let l = last, (n - l + 256) % 256 > 128 {
                    backwards += 1
                    steps += "\(l)>\(n)@\(captures) "
                }
                last = n
            } else {
                unread += 1
            }
            // The first capture showing magenta, beside the PNG.
            if shown > 0 && !wroteUncovered,
               let out = CGImageDestinationCreateWithURL(png.deletingPathExtension().appendingPathExtension("uncovered.png") as CFURL, UTType.png.identifier as CFString, 1, nil) {
                CGImageDestinationAddImage(out, image, nil)
                wroteUncovered = CGImageDestinationFinalize(out)
            }
        }
        let wait = 1 / rate - Date().timeIntervalSince(started)
        if wait > 0 { try await Task.sleep(nanoseconds: UInt64(wait * 1e9)) }
    }
    print(String(format: "{\"captures\":%d,\"uncovered_captures\":%d,\"most_magenta_samples\":%d,\"backwards\":%d,\"unread\":%d,\"steps\":\"%@\",\"trace\":\"%@\"}", captures, uncovered, most, backwards, unread, steps, trace))
    exit(0)
}
RunLoop.main.run()
