---
section: fixed
---
- **A declared window's own state follows it** (zgx-desktop, zgx-run, zgx-script): `win.active`, `win.shown` and `win.size` stopped following a declared window for two reasons. `WindowContext::on_closed` replaced any earlier handler, so `zgx run`'s second handler for a window dropped the first, and with it the effect keeping the state current. And a reload (a hot one, or devtools starting to record) built the window's content with a new state that no host set. Each `on_closed` handler now runs, in the order given, and a window keeps its own state across loads while it is published.
