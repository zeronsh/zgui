---
section: added
---
- **Tabs tear off into windows** (zgx-components, zgx-script): with `TabsOptions::on_tear_off` (Luau `ui.tabs { on_tear_off }`), a reorderable tab dragged off its strip is carried as a drag, its chip under the pointer. Dropped on a strip of its group in any of the app's windows, it goes there and `on_move` is told. Dropped back on its own strip, it stays. Released outside every window, it leaves `items` and `on_tear_off` is told its id and where it was grabbed, to open it in a window of its own (`ui.windows`, `under_pointer`).
