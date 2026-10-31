---
section: performance
---
- **A field reads its code options with less work** (zgx-script): `highlight`, `decorations`, `underlines`, `tab`, `on_caret`, `on_top_line` and `find` are read straight from its table, rather than by a protected lookup each, which every `ui.input` and `ui.textarea` paid.
