import AppKit
import CoreGraphics
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers
// window_pixels capture <pid> <out.png>: one capture of a process's largest
// window at its full pixel size, without its shadow (ScreenCaptureKit's
// screenshot; Screen Recording permission, which it only checks).
// window_pixels diff <a.png> <b.png>: the pixels that differ, their bounding
// box and the largest difference, as JSON.
let a = CommandLine.arguments

func fail(_ message: String) -> Never {
  print("{\"error\":\"\(message)\"}"); exit(1)
}

func pixels(_ path: String) -> (Int, Int, [UInt8]) {
  guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
  else { fail("can't read \(path)") }
  let (w, h) = (image.width, image.height)
  var data = [UInt8](repeating: 0, count: w * h * 4)
  let context = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                          space: image.colorSpace ?? CGColorSpace(name: CGColorSpace.sRGB)!,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
  return (w, h, data)
}

switch a.count > 1 ? a[1] : "" {
case "diff":
  let (w, h, x) = pixels(a[2]), (w2, h2, y) = pixels(a[3])
  guard w == w2 && h == h2 else { fail("sizes \(w)x\(h) and \(w2)x\(h2)") }
  var count = 0, most = 0
  var box = (Int.max, Int.max, -1, -1)
  for row in 0..<h {
    for col in 0..<w {
      let i = (row * w + col) * 4
      var d = 0
      for c in 0..<4 { d = max(d, abs(Int(x[i + c]) - Int(y[i + c]))) }
      if d > 0 {
        count += 1
        most = max(most, d)
        box = (min(box.0, col), min(box.1, row), max(box.2, col), max(box.3, row))
      }
    }
  }
  print("{\"differing\":\(count),\"max\":\(most),\"box\":[\(box.0),\(box.1),\(box.2),\(box.3)]}")
case "capture":
  // The window server's connection, which ScreenCaptureKit's image needs.
  _ = NSApplication.shared
  let pid = pid_t(a[2])!, out = URL(fileURLWithPath: a[3])
  Task {
    do {
      let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
      guard let window = content.windows
        .filter({ $0.owningApplication?.processID == pid })
        .max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height })
      else { fail("no window") }
      let filter = SCContentFilter(desktopIndependentWindow: window)
      let config = SCStreamConfiguration()
      let scale = CGFloat(filter.pointPixelScale)
      config.width = Int(window.frame.width * scale)
      config.height = Int(window.frame.height * scale)
      config.showsCursor = false
      config.ignoreShadowsSingleWindow = true
      let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
      guard let dest = CGImageDestinationCreateWithURL(out as CFURL, UTType.png.identifier as CFString, 1, nil)
      else { fail("can't write \(out.path)") }
      CGImageDestinationAddImage(dest, image, nil)
      CGImageDestinationFinalize(dest)
      print("{\"width\":\(image.width),\"height\":\(image.height)}")
      exit(0)
    } catch {
      fail("\(error)".replacingOccurrences(of: "\"", with: "'"))
    }
  }
  RunLoop.main.run()
default:
  fail("usage: window_pixels capture <pid> <out.png> | diff <a.png> <b.png>")
}
