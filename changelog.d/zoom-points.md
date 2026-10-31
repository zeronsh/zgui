---
section: changed
---
- **Wheels and drags in a zoomed box move it as far as the hand** (zgx, zgx-script): a scroll view's offset, `on_wheel`'s `dx` and `dy`, and `anime.createDraggable`'s position took the window's points, so under `zoom = 0.5` content moved half as far as the wheel or pointer did. They are in the box's own points now. A drag's `grab_x`, `grab_y` are from where the source shows. An app that scaled these for its own zoom now scales twice. docs/reference/events.md says which points each field is in.
