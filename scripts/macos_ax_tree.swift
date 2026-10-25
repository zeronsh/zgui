// A running app's accessibility tree as VoiceOver reads it, as JSON: each
// element's role, name, value, state and (tables, lists, outlines) how many
// rows it gives. Reads attributes only: no actions, no input.
//
//     swiftc -O scripts/macos_ax_tree.swift -o /tmp/macos_ax_tree
//     /tmp/macos_ax_tree <pid>
//
// Exits 3 when this process may not read other apps' accessibility
// (System Settings, Privacy & Security, Accessibility).
import ApplicationServices
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 2, let pid = pid_t(arguments[1]) else {
    FileHandle.standardError.write("usage: macos_ax_tree <pid>\n".data(using: .utf8)!)
    exit(2)
}
guard AXIsProcessTrusted() else {
    FileHandle.standardError.write("not trusted for accessibility\n".data(using: .utf8)!)
    exit(3)
}
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 5)

func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}
func string(_ value: AnyObject?) -> String? {
    if let text = value as? String { return text }
    if let number = value as? NSNumber { return number.stringValue }
    return nil
}
var count = 0
func node(_ element: AXUIElement) -> [String: Any] {
    count += 1
    var out: [String: Any] = [:]
    let names: [(String, String)] = [
        ("role", kAXRoleAttribute), ("subrole", kAXSubroleAttribute),
        ("title", kAXTitleAttribute), ("description", kAXDescriptionAttribute),
        ("value", kAXValueAttribute), ("help", kAXHelpAttribute),
    ]
    for (key, name) in names {
        if let text = string(attribute(element, name)), !text.isEmpty { out[key] = text }
    }
    if string(attribute(element, "AXInvalid")) == "true" { out["invalid"] = true }
    if let selected = attribute(element, kAXSelectedAttribute) as? Bool { out["selected"] = selected }
    if let enabled = attribute(element, kAXEnabledAttribute) as? Bool, !enabled { out["disabled"] = true }
    if let rows = attribute(element, kAXRowsAttribute) as? [AXUIElement] { out["rows"] = rows.count }
    var actions: CFArray?
    if AXUIElementCopyActionNames(element, &actions) == .success, let list = actions as? [String], !list.isEmpty {
        out["actions"] = list
    }
    if count < 6000, let children = attribute(element, kAXChildrenAttribute) as? [AXUIElement], !children.isEmpty {
        out["children"] = children.map(node)
    }
    return out
}
let tree = node(app)
let data = try! JSONSerialization.data(withJSONObject: tree, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
