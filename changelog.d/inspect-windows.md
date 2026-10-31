---
section: added
---
- **Devtools reach every window** (zgx-script, zgx-run): `zgx inspect windows` lists the app's windows (the main one, `ui.window`'s and `ui.windows`'s), each with its id, size, title and popups open. Any command takes `window` (`--window` on the CLI, `window` on every MCP tool, `zgx_windows` to list) and answers for that window: an id, or an item's key in `ui.windows`. zgx-run registers each window it opens while devtools run (`inspect::register_window`), and forgets it when it closes.
