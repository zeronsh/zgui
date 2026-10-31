---
section: added
---
- **`display = "contents"`** (zgx, zgx-script): an element's children lay out in its place, as its parent's own (CSS `display: contents`), and `ui.each(items, row, { display = "contents" })` makes its rows a grid's cells or a row's items rather than one box. Focus order and assistive technology keep the element where it is, with a role or label it has. In Rust, `.display_contents()` and `Scene::set_display_contents`.
