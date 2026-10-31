---
section: added
---
- **Windows made hidden, shown fully drawn** (zgx-desktop, zgx-script, zgx-run): `WindowOptions { visible: false, .. }`, `ui.window { visible = false }` (or a state) and zgx.json's `"visible": false` make a window hidden, its content laid out, its script running and its first frame drawn meanwhile; hidden, it draws nothing more. `WindowHandle::set_visible(true)`, or `ui.window.visible:set(true)` for zgx-run's window, draws what changed since before it appears, so it shows whole at once, with no blank frame. On Wayland, where a window appears as it first draws, its first frame is drawn as it shows; on X11 a window keeps no picture while unmapped, so its theme's background (not black) shows for the few milliseconds between its mapping and its first frame.
