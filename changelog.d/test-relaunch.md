---
section: changed
---
- **A second `mount` in a test is the next launch** (zgx-script): a test's apps in `zgx test` share a storage directory of the test's own, each saved before the next starts, so a second `mount()` finds what the first kept (`ui.persist`, folders kept with `ui.open_folder { remember }`), as a relaunch would. Each test starts with nothing kept. A headless app's `ui.persist` kept state in memory only.
