---
section: added
---
- **An app's main window reopens where it was left** (zgx-run): `"frame_autosave": true` in `zgx.json` saves the window's frame under the app's `app_id`, so each app zgx-run runs keeps its own, and a string saves it under that name, as `ui.window { frame_autosave }` does for an app's other windows. `zgx.json` rejected the key.
