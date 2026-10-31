---
section: added
---
- **A folder can be kept past the run** (zgx-script, zgx, zgx-desktop): `ui.open_folder { remember = "library" }` asks the host to keep the grant under that key, and `ui.remembered_folder("library")` settles with the handle on a later launch if the folder still resolves, or nil. `folder:forget()` drops it. The host keeps it where no script reads, in its storage's `.folders`: on macOS a security-scoped bookmark, which works sandboxed or not and follows the folder moved or renamed, elsewhere its path. Plugins keep none, and `ScriptHost::refuse_remembered_folders()` keeps none. `Platform` gains `bookmark_folder`, `resolve_folder` and `release_folder`, with defaults that keep a path.
