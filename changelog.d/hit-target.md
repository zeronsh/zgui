---
section: added
---
- **Clicks land where an animation is going** (zgx, zgx-script): `hit = "target"` hits an element, and what it holds, where its `ui.tween`, `ui.spring` or layout animation is taking it, not where it shows: clicking fast on a stack that re-fans after each removal removes the photo under the pointer each time. Assistive technology reads it at its goal too. In Rust, `Scene::set_hit_target` gives any node a `HitTarget`; `Scene::hit_bounds` is where it takes hits. Also: a node removed with size math's content widths no longer leaves them behind.
