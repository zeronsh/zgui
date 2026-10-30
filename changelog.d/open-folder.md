---
section: added
---
- **`ui.open_folder`** (zgx, zgx-script): a folder a person picks, as a handle only the dialog makes, which lists, reads and writes inside that folder and nowhere else: `..`, absolute paths and links out of it are refused, it works off the main thread, and a write replaces its file whole. A plugin needs the `folders` grant, and a Rust host can refuse it to every script (`ScriptHost::refuse_folders`, `Capability::Folders`, `Platform::open_folder`).
