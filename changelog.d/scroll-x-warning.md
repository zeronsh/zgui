---
section: fixed
---
- **Tabs and horizontal scrolls no longer warn of an unresolved height** (zgx): with devtools recording, every `ui.tabs` (and any `scroll_x` with no height of its own in a row sized by its content) reported "height 100% has nothing to resolve against" for its viewport and content. Those take a definite height or their content's by design, so layout warnings now pass over a scroll's own viewport and content.
