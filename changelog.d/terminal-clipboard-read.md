---
section: added
---
- **An app answers a terminal program's request to read the clipboard** (zgx, zgx-terminal, zgx-desktop, zgx-script): `Terminal::on_clipboard_read` (Luau: `ui.term`'s `on_clipboard_read`) gets each OSC 52 read as a `ClipboardRead` to `allow()` or `deny()`, at once or after a prompt the app shows; a request let go is denied. Without one, Ghostty still asks with its own dialog and the engine-drawn terminal denies, now with an empty answer the program can read rather than none.
