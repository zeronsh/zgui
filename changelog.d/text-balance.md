---
section: added
---
- **Balanced text: `wrap = "balance"`, `Styled::text_balance`** (zgx, zgx-script): wrapped text breaks at the narrowest width that keeps as many lines, so a heading's last line isn't left short, as CSS's `text-wrap: balance` does, for up to six lines. Inherited, as CSS's is; boxes and line counts are as they were. `Scene::set_text_balance` sets it, and `Scene::text_wrap` is what renderers read.
