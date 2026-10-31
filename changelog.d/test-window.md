---
section: added
---
- **A test acts in the app's other windows** (zgx-script): `app:window(id)` in `zgx test` is a window the app declares (`ui.window`, or one of `ui.windows`) with an app's methods, nil while it is closed. It shares the app's state, tasks and frames as a desktop app's windows do, and `close()` closes it as a person does. In Rust, `TestApp::window(id)` mounts one as a `TestApp` of its own.
