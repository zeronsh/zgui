# Retained component layout

Build children with `div`, `row`, `column`, and reusable functions or components. Layout is a style refinement; it does not require appending scene commands.

**Children stretch across their container, as in CSS's flexbox.** A `column`'s children are as wide as the column and a `row`'s as tall as the row, unless a child sizes itself on that axis (`w`, `h`, `w_percent`, `aspect_ratio`), which it keeps. `div` is a column and does the same, and so do zgx-script's `ui.column`, `ui.row` and `ui.div`. `.items_start()` (`items = "start"`) keeps each child as wide or tall as it is, and `.items_center()` and `.items_end()` align them. Until October 2026 the default was `start`: an app written for that which shows buttons or chips spanning a column, or boxes taller than their content in a row, states `.items_start()` where it relied on it.

```rust
use zgx::compose::prelude::*;
// The button spans the column; the chip keeps its own width.
let stretched = column().w(240.).child(button().child(text("Save"))).child(div().w(60.).h(20.));
// Each child as wide as it is.
let hugging = column().items_start().child(button().child(text("Save")));
```

**Children shrink to fit, as in CSS's flexbox (`flex-shrink: 1`).** In a row too narrow for its children every child gives way, in proportion to its size, and a column does the same with heights. `.shrink_0()` (`shrink = 0`) holds a child at its size: a fixed sidebar, a chip that must not narrow. Until October 2026 the default was 0, so nothing shrank: an app written for that whose sidebar, icon or fixed-width box now narrows in a small window states `.shrink_0()` on it. Scroll content is the exception: along the axis it scrolls it keeps its extent, and scrolls rather than shrinking what is in it.

**A child that shrinks stops at its content's narrowest, as CSS's `min-width: auto` does.** In a row too narrow for its children, a child gives way only down to its min-content width: the longest word of a text, an unwrappable label's whole width, a box's children's needs plus its padding. Its siblings shrink further. A child that clips (`scroll`, `overflow_hidden`) may shrink to nothing, as a CSS scroll container does. A minimum the child sets is its own: `.min_w(0.)` (`min_w = 0`) lets a truncated label or a pane shrink past its content, as CSS needs `min-width: 0`. Columns do the same with heights.

```rust
use zgx::compose::prelude::*;
// The empty box gives way; the title keeps its words whole.
let bar = row().w(200.).child(div().w(180.)).child(text("Unbreakable"));
// A fixed icon holds; a truncated title shrinks past its text.
let truncated = row().w(200.).child(div().w(24.).shrink_0()).child(text("A long title").truncate().min_w(0.));
```

**Right to left.** `.rtl()` (`dir = "rtl"`) runs a subtree's lines from the right, as CSS's `direction: rtl` does, for Arabic, Hebrew, Persian and Urdu:
- rows run from the right;
- a column's and an overlay's children start at the right;
- text starts at the right;
- wrapping rows mirror their lines.

Padding, margins and insets given as left and right (`pl`, `ml`, `left`) stay on that side, as CSS keeps them. The logical ones follow the direction: `ps`/`pe` (padding at a line's start and end), `ms`/`me` and `.start()`/`.end()` insets (`inset_start`/`inset_end` in scripts; CSS's `*-inline-start` and `*-inline-end`). Write the start and end ones for anything that should mirror. A subtree inherits its parent's direction; `.direction(Direction::Ltr)` (`dir = "ltr"`) turns part of it back (a phone number, code). A desktop window starts the way the user's language runs: right to left under Arabic, Hebrew, Persian or Urdu (macOS's first preferred language, Windows' display locale, Linux's `LANG`; `ZGX_DIRECTION=rtl` or `ltr` overrides it to try a layout). A layout that sets no direction pays nothing.

```rust
use zgx::compose::prelude::*;
// "مرحبا" at the right, the avatar at the start (the right), 12 points in.
let header = row().rtl().gap(8.).ps(12.).child(div().size(32., 32.)).child(text("مرحبا"));
```

Grids and wrapping rows:

```rust
use zgx::{compose::prelude::*, layout::ContentAlign};
let dashboard = div().grid().grid_cols(4).grid_rows(3)
    .gap_x(12.).gap_y(8.).w_full().h(240.)
    .child(div().col_span_full().child(text("Header")))
    .child(div().row_span(2).child(text("Sidebar")))
    .child(div().col_span(3).row_span(2).child(text("Main")));
let cards = row().w_full().flex_wrap()
    .gap_x(10.).gap_y(16.).align_content(ContentAlign::Start)
    .children((0..30).map(|i| div().w(140.).h(44.).child(text(format!("Item {i}")))));
```

`.grid_cols(n)` makes equal `minmax(0, 1fr)` tracks. `.grid_template_columns(tracks)` and `.grid_template_rows(tracks)` take a list of any size, as CSS's `grid-template-columns` writes it: `"220 1fr auto".parse()?`, `"repeat(auto-fill, minmax(240, 1fr))".parse()?` (`zgx::grid_tracks::GridTracks`). A list that CSS would reject fails to parse, saying why. Starts use CSS-style one-based grid lines; negative starts count backward. `col_span`, `row_span`, and their `_full` variants compose with starts. Flex supports wrapping/reverse wrapping, reverse direction, basis, grow/shrink, item self-alignment and line distribution. Insets accept `Length::Px`, `Length::Percent`, or `Length::Auto`; a plain float converts to pixels. `Length::Percent(1.)` means 100%, whereas convenience methods such as `p_percent(10.)` take percentages from 0 to 100.

Sizes are border-box (CSS `box-sizing: border-box`, as GPUI): a container's border sits inside its width and height, its padding and children inside the border, and fit-content sizes include it; `border_over`, outlines and strokes only paint. Overlays stack children in one grid-like area, each aligned on its own (`items` across, `justify` down) and stretched by default. `absolute` children position against the nearest `relative` ancestor (not past an `absolute` one), else their parent; insets count from that block's padding box. See [the styling guide](styling.md#box-model) for the full rules.

`.w_calc("max-content".parse()?)` sizes a view by what it holds: `min-content`, `max-content`, `fit-content` and `fit-content(320px)`, as CSS has them, on widths. `.w_calc("calc(100% - 24px)".parse()?)` and its `h`, `min_` and `max_` siblings work a size out from the containing block: CSS's `calc()`, `min()`, `max()` and `clamp()`.

`.sticky_top(n)` holds a view `n` below the top of the scroll view around it as it scrolls past, inside its parent: CSS's `position: sticky` with `top`, and painted over its siblings. It sticks to the top of a vertical scroll view only.

`.z(n)` paints and hits a view over siblings with a lower `z`, among its siblings only: CSS's `z-index` without stacking contexts. Layout, focus and accessibility keep document order.

Percent padding uses the containing block's width on every edge. Auto margins absorb remaining space. `aspect_ratio` supplies the missing dimension. Pixel refinements override earlier percentage spacing, gaps and size constraints on the corresponding properties. `.hidden()` removes a subtree from layout and interaction while retaining its component identity; `.invisible()` preserves allocation. `.flex()` restores display and `.visible()` restores visibility. Hidden focused controls blur, and visibility observers stop hidden media deadlines.

`.contents()` makes a container lay out as its last child would in its place, like CSS `display: contents` for a box holding one view: the child's size, flex, self-alignment, percentages and absolute insets resolve against the container's parent, and the child fills the container. Of the container's own styles only margins (added to the child's), paint and clipping apply; earlier children lie over the last at its origin (content leaving). `switch(select, build).contents()` swaps views without a layout box of its own. The derived style is recomputed when the child's style or the child changes, never per frame, and measuring a contents node measures its child under the same constraints, so it costs no extra probes.

Scrollable views forward container layout to their retained content, so `scroll(offset).grid().grid_cols(2)` and `scroll(offset).flex_row().flex_wrap()` use the same child architecture. The viewport retains its own padding, dimensions and clipping. `overflow_x_hidden` and `overflow_y_hidden` independently clip paint, hit testing and visibility; the corresponding `_visible` methods restore overflow on that axis.

Ordinary row/column/overlay layouts keep the existing small cached layout path. Advanced containers allocate one retained Taffy 0.9 tree for their immediate children. Stable child IDs and Taffy caches survive resize and content updates; removed children release cache entries. Layout options use an optional shared allocation, so default styles do not allocate an extended options block. An unchanged scene performs no layout work. See [the feature cost audit](performance-feature-audit.md) for default-path and cache bounds.

Run `cargo run -p zgx-desktop --example layout` for the resizable gallery. `cargo test -p zgx --test layout advanced_layout::` covers geometry, refinement precedence, visibility and scrolling; the core retained-cache regression checks allocation reuse and stale-child removal. These tests establish the documented cases, not universal browser CSS conformance.

`View::hollow()` lets the pointer through a box while nothing it holds is displayed (`hidden` or `invisible`): a page kept mounted but hidden takes no presses meant for what's under it.
