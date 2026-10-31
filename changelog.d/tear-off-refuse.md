---
section: changed
---
- **A tab can stay home when torn off** (zgx-components, zgx-script): `on_tear_off` returns whether it takes the tab. `false` keeps it in `items`, its preview sliding home, as an app's "All" tab needs; Luau's returning nothing still tears it off. `TabsOptions::on_tear_off`'s `TearOffTab` now returns a `bool`.
