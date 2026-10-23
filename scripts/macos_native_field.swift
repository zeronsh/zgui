import AppKit
// The AppKit baseline for scripts/macos_latency_probe.swift: a 600x240 window with an
// NSTextField where the input_latency example has its field.
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let window = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 600, height: 240), styleMask: [.titled, .closable], backing: .buffered, defer: false)
window.title = "native input latency"
let field = NSTextField(frame: NSRect(x: 24, y: 240 - 24 - 40, width: 520, height: 40))
field.font = NSFont.systemFont(ofSize: 16)
window.contentView!.addSubview(field)
window.makeKeyAndOrderFront(nil)
window.makeFirstResponder(field)
app.activate(ignoringOtherApps: true)
app.run()
