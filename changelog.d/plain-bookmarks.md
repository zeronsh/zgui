---
section: fixed
---
- **A remembered folder survives an upgrade outside the sandbox** (zgx-desktop): on macOS, a folder kept with `ui.open_folder { remember }` is now a plain bookmark unless the app is sandboxed. A security-scoped bookmark resolves only under the code-signing identifier that made it, and an unbundled app's identifier changes with its build (zgx-run's is cargo's metadata hash), so every remembered folder was forgotten at an upgrade. A sandboxed app still makes security-scoped ones. A grant of the other kind still resolves, and is renewed as the kind the app makes.
