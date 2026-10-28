# zgx patch to taffy 0.9.0

Vendored from crates.io `taffy` 0.9.0 (MIT, `LICENSE.md` from the
repository's v0.9.0 tag, which the published crate leaves out) and
published as `zgx-taffy`, its library still `taffy`, so zgx's crates build
from crates.io without a `[patch]` section. The changes are marked `zgx:` in
the source.

## Why

A flex line with an auto margin (`margin_left = auto` to push items to the
end, a toolbar's spacer) lost its gaps: where free space goes to auto
margins, upstream set each item's main-axis offset to the margin alone,
dropping the `gap` between items that every other path keeps. CSS keeps
the gaps.

A child kept a size from its parent's old layout mode: a grid's child given
an aspect ratio, its parent then made flex, kept the height the grid had
sized it to (122 where flex gives it its own 36). A node's cache is keyed
by known dimensions, available space and run mode, not by which algorithm
its parent sizes it with, and changing a node's display left its children's
caches as they were.

## What

1. In `src/compute/flexbox.rs`, distributing free space to auto margins
   sets each item's offset to the gap before it (none before the first;
   the last in a reversed line), then adds its auto margins as before.
2. In `src/tree/taffy_tree.rs`, `TaffyTree::set_style` clears the caches
   of a node's children when its `display` changes (rare: a few nodes'
   caches, which the change dirties anyway).

Drop the fork when a taffy release keeps the gaps and keys children's
results by their parent's layout mode (the zgx tests are
`crates/zgx/tests/layout_gaps.rs` and `layout_history.rs`).
