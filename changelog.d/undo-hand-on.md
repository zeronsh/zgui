---
section: changed
---
- **A field with nothing to undo hands ⌘Z on** (zgx), and ⌘⇧Z or Ctrl+Y with nothing to redo, to the app's commands, as AppKit's responder chain does: a `ui.command { key = "cmd+z" }` reaches the app's own undo from inside a field without `reserved = true`. A field with a step still takes the key.
