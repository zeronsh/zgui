---
section: fixed
---
- **A window's content fills it** (zgx-script): the content of a `ui.window` or an item's window of `ui.windows` took its own size, so a detail window's column sat in its top-left corner and a `ui.tabs` in it fell back to a scroll view's default 320 by 240. It now fills its window as the app's own window's root does: across, as a block fills its parent, and down with `h_full`.
