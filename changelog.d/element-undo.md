---
section: added
---
- **A field's undo and text position from a script** (zgx, zgx-script): `ui.editor.undo(el)`, `redo`, `can_undo` and `can_redo`, so an app can make one undo stack across panes, and `ui.editor.offset_at(el, x, y)`, the byte under a point, so a drop goes where the pointer is (`EditorHandle::undo`, `redo`, `offset_at`).
