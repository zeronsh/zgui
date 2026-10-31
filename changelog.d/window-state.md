---
section: added
---
- **A window reads its own state** (zgx-script, zgx-run): `ui.windows`' `build(item, win)`, and `ui.window` content given as `function(win)`, get `win.active`, `win.shown` and `win.size`, that window's, where `ui.window.active` is the main window's: a command can act on the window in front.
