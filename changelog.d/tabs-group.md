---
section: added
---
- **Tabs move between strips** (zgx-components, zgx-script): `ui.tabs { group = "panes", reorderable = true }`, a tab dragged over another strip or panel of the group goes there, as in an editor's split panes; `on_move` hears it (`TabsMore::group`, `on_move`, through `tabs_with`).
