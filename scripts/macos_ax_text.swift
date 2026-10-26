// A running app's text field as VoiceOver reads and moves through it, as
// JSON: its text, then a selection set (as VoiceOver sets one, through
// AXSelectedTextRange), read back with where its characters show
// (AXBoundsForRange), and a caret on another line. No input is sent: the
// field's selection is set through accessibility alone.
//
//     swiftc -O scripts/macos_ax_text.swift -o /tmp/macos_ax_text
//     /tmp/macos_ax_text <pid> <field title>
//
// Exits 3 when this process may not use other apps' accessibility, 4 when
// the field is not found.
import ApplicationServices
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 3, let pid = pid_t(arguments[1]) else {
    FileHandle.standardError.write("usage: macos_ax_text <pid> <field title>\n".data(using: .utf8)!)
    exit(2)
}
let title = arguments[2]
guard AXIsProcessTrusted() else { exit(3) }
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 5)

func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}
func find(_ element: AXUIElement, depth: Int = 0) -> AXUIElement? {
    let role = attribute(element, kAXRoleAttribute) as? String
    if role == "AXTextField" || role == "AXTextArea",
       (attribute(element, kAXTitleAttribute) as? String) == title
           || (attribute(element, kAXDescriptionAttribute) as? String) == title {
        return element
    }
    guard depth < 40, let children = attribute(element, kAXChildrenAttribute) as? [AXUIElement] else {
        return nil
    }
    for child in children {
        if let found = find(child, depth: depth + 1) { return found }
    }
    return nil
}
func rect(_ value: AnyObject?) -> [Double]? {
    guard let value = value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var r = CGRect.zero
    guard AXValueGetValue(value as! AXValue, .cgRect, &r) else { return nil }
    return [r.origin.x, r.origin.y, r.size.width, r.size.height]
}
func range(_ value: AnyObject?) -> [Int]? {
    guard let value = value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var r = CFRange()
    guard AXValueGetValue(value as! AXValue, .cfRange, &r) else { return nil }
    return [r.location, r.length]
}
func select(_ field: AXUIElement, _ location: Int, _ length: Int) {
    var r = CFRange(location: location, length: length)
    let value = AXValueCreate(.cfRange, &r)!
    AXUIElementSetAttributeValue(field, kAXSelectedTextRangeAttribute as CFString, value)
}
func bounds(_ field: AXUIElement, _ location: Int, _ length: Int) -> [Double]? {
    var r = CFRange(location: location, length: length)
    let value = AXValueCreate(.cfRange, &r)!
    var out: AnyObject?
    let status = AXUIElementCopyParameterizedAttributeValue(
        field, kAXBoundsForRangeParameterizedAttribute as CFString, value, &out)
    return status == .success ? rect(out) : nil
}

var field: AXUIElement?
for _ in 0..<40 {
    field = find(app)
    if field != nil { break }
    Thread.sleep(forTimeInterval: 0.25)
}
guard let field = field else { exit(4) }
var out: [String: Any] = [:]
out["value"] = attribute(field, kAXValueAttribute) as? String ?? ""
out["frame"] = rect(attribute(field, "AXFrame"))
// "two": characters 4 to 7.
select(field, 4, 3)
Thread.sleep(forTimeInterval: 0.3)
out["selected_range"] = range(attribute(field, kAXSelectedTextRangeAttribute))
out["selected_text"] = attribute(field, kAXSelectedTextAttribute) as? String ?? ""
out["selection_bounds"] = bounds(field, 4, 3)
// A caret at "three" (the second line's start).
select(field, 8, 0)
Thread.sleep(forTimeInterval: 0.3)
out["caret_range"] = range(attribute(field, kAXSelectedTextRangeAttribute))
out["caret_line"] = (attribute(field, kAXInsertionPointLineNumberAttribute) as? NSNumber)?.intValue
out["first_line_bounds"] = bounds(field, 0, 1)
out["second_line_bounds"] = bounds(field, 8, 1)
let data = try! JSONSerialization.data(withJSONObject: out, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
