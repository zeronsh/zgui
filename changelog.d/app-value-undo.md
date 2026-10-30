---
section: changed
---
- **A value the app sets on a `ui.input` or `ui.textarea` leaves nothing to undo** (zgx-script), as setting an HTML field's value does: ⌘Z after showing another note could write the old note over it, and `on_change` would save that. The field's own edits still undo. A value answering one keeps the history: with `value = function`, the function's next change after `on_change` or `on_commit`, however late; with `value = state`, a different value within 100 ms of the edit. Rust's bindings keep their history (`TextEditor::load_next` asks for this).
