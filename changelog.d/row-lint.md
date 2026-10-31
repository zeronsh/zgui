---
section: added
---
- **Lint `list-row-get`** (zgx-script): `state:get()` straight in a positional `ui.list`'s `row` builder, which is read once per index and goes stale as the list changes; `data` with a `key` makes each row follow its item.
