---
section: added
---
- **`TextEditor::move_visually_at`** (zgx) moves left or right as the text shows it from a caret place (`TextPosition`, with its affinity), stopping on each side where bidirectional runs meet, and returns the place it went to draw the caret there. `move_visually`, 0.1.6's form, keeps its signature and moves from the caret's byte.
