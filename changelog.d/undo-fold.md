---
section: fixed
---
- **⌘Z no longer sticks in a field the app normalises** (zgx, zgx-script): undo went back to what was typed, the app normalised it again, and that became a new step. What the app makes of an edit now joins that edit, so undo goes past both (`TextEditor::answer_next`).
