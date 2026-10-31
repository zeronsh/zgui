---
section: changed
---
- **`zgx_script::driver::Line` and `zgx_script::Window` have new fields** (zgx-script): `Line.hidden` (#1121), whether a node shows, and `Window.under_pointer` (#1139), `Window.visible` and `Window.state` (#1141), where a window opens, whether it shows and its own active, shown and size. They're snapshots zgx-script fills in for its host and tools, so a struct literal of either, written out field by field, stops compiling. *Migrate:* read them as zgx-script gives them rather than building them; a test building one adds `hidden: false`, or `under_pointer: None, visible: None, state: None`.
