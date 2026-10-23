# Retained component layout

Build children with `div`, `row`, `column`, and reusable functions or components. Layout is a style refinement; it does not require appending scene commands.

**Children stretch across their container, as in CSS's flexbox.** A `column`'s children are as wide as the column and a `row`'s as tall as the row, unless a child sizes itself on that axis (`w`, `h`, `w_percent`, `aspect_ratio`), which it keeps. `div` is a column and does the same, and so do zgui-script's `ui.column`, `ui.row` and `ui.div`. `.items_start()` (`items = "start"`) keeps each child as wide or tall as it is, and `.items_center()` and `.items_end()` align them. Until October 2026 the default was `start`: an app written for that which shows buttons or chips spanning a column, or boxes taller than their content in a row, states `.items_start()` where it relied on it.

```rust
use zgui::compose::prelude::*;
// The button spans the column; the chip keeps its own width.
let stretched = column().w(240.).child(button().child(text("Save"))).child(div().w(60.).h(20.));
// Each child as wide as it is.
let hugging = column().items_start().child(button().child(text("Save")));
```

Grids and wrapping rows:

```rust
use zgui::{compose::prelude::*, layout::ContentAlign};
let dashboard = div().grid().grid_cols(4).grid_rows(3)
    .gap_x(12.).gap_y(8.).w_full().h(240.)
    .child(div().col_span_full().child(text("Header")))
    .child(div().row_span(2).child(text("Sidebar")))
    .child(div().col_span(3).row_span(2).child(text("Main")));
let cards = row().w_full().flex_wrap()
    .gap_x(10.).gap_y(16.).align_content(ContentAlign::Start)
    .children((0..30).map(|i| div().w(140.).h(44.).child(text(format!("Item {i}")))));
```

Grid tracks are equal `minmax(0, 1fr)` shares. Starts use CSS-style one-based grid lines; negative starts count backward. `col_span`, `row_span`, and their `_full` variants compose with starts. Flex supports wrapping/reverse wrapping, reverse direction, basis, grow/shrink, item self-alignment and line distribution. Insets accept `Length::Px`, `Length::Percent`, or `Length::Auto`; a plain float converts to pixels. `Length::Percent(1.)` means 100%, whereas convenience methods such as `p_percent(10.)` take percentages from 0 to 100.

Sizes are border-box (CSS `box-sizing: border-box`, as GPUI): a container's border sits inside its width and height, its padding and children inside the border, and fit-content sizes include it; `border_over`, outlines and strokes only paint. Overlays stack children in one grid-like area, each aligned on its own (`items` across, `justify` down) and stretched by default. `absolute` children position against the nearest `relative` ancestor (not past an `absolute` one), else their parent; insets count from that block's padding box. See [the styling guide](styling.md#box-model) for the full rules.

Percent padding uses the containing block's width on every edge. Auto margins absorb remaining space. `aspect_ratio` supplies the missing dimension. Pixel refinements override earlier percentage spacing, gaps and size constraints on the corresponding properties. `.hidden()` removes a subtree from layout and interaction while retaining its component identity; `.invisible()` preserves allocation. `.flex()` restores display and `.visible()` restores visibility. Hidden focused controls blur, and visibility observers stop hidden media deadlines.

`.contents()` makes a container lay out as its last child would in its place, like CSS `display: contents` for a box holding one view: the child's size, flex, self-alignment, percentages and absolute insets resolve against the container's parent, and the child fills the container. Of the container's own styles only margins (added to the child's), paint and clipping apply; earlier children lie over the last at its origin (content leaving). `switch(select, build).contents()` swaps views without a layout box of its own. The derived style is recomputed when the child's style or the child changes, never per frame, and measuring a contents node measures its child under the same constraints, so it costs no extra probes.

Scrollable views forward container layout to their retained content, so `scroll(offset).grid().grid_cols(2)` and `scroll(offset).flex_row().flex_wrap()` use the same child architecture. The viewport retains its own padding, dimensions and clipping. `overflow_x_hidden` and `overflow_y_hidden` independently clip paint, hit testing and visibility; the corresponding `_visible` methods restore overflow on that axis.

Ordinary row/column/overlay layouts keep the existing small cached layout path. Advanced containers allocate one retained Taffy 0.9 tree for their immediate children. Stable child IDs and Taffy caches survive resize and content updates; removed children release cache entries. Layout options use an optional shared allocation, so default styles do not allocate an extended options block. An unchanged scene performs no layout work. See [the feature cost audit](performance-feature-audit.md) for default-path and cache bounds.

Run `cargo run -p zgui-desktop --example layout` for the resizable gallery. `cargo test -p zgui --test advanced_layout` covers geometry, refinement precedence, visibility and scrolling; the core retained-cache regression checks allocation reuse and stale-child removal. These tests establish the documented cases, not universal browser CSS conformance.
