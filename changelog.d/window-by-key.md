---
section: changed
---
- **A test names an item's window by its key** (zgx-script): `app:window(key)` in `zgx test`, and `TestApp::window`, take the key of an item of `ui.windows` (its `id` field, or `key(item)`), whose window's own id a test can't know. A `ui.window`'s id still names it, and a key two lists share fails, saying so.
