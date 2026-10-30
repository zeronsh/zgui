---
section: added
---
- **A `ui.textarea` scrolls by line and says where it is** (zgx, zgx-script): `el:scroll_to_line(n, { align })` puts a line at the top, middle or bottom of the view, and `on_top_line` hears the first line in view: an outline can jump to a heading and follow the text (`EditorHandle::scroll_to_line`, `View::on_top_line`).
