# zgx patch to alacritty_terminal 0.26.0

Vendored from crates.io `alacritty_terminal` 0.26.0 (Apache-2.0,
`LICENSE-APACHE` as published) and published as `zgx-alacritty-terminal`, its
library still `alacritty_terminal`, so zgx's crates build from crates.io
without a `[patch]` section. The changes are marked `zgx:` in the source. The
crate's reference tests (`tests/`) are left out.

## Why

`index.rs` compared `usize` with a `Column` and `i32` with a `Line` from the
primitive's side (`impl PartialOrd<Column> for usize`, and `PartialEq`). Such
an impl is seen by every crate whose dependency graph holds this one: there
`usize` compares with two types, and a comparison whose type was inferred
from `usize` having one no longer compiles. An app turning on zgx-desktop's
`terminal` feature found code of its own broken (`range.contains(i)`, with
`i: &&usize`, in zgx's comparisons demo).

## What

The macro making the index types no longer implements `PartialEq` and
`PartialOrd` on the primitive for the index type. A `Column` or `Line` still
compares with its primitive (`column < 5`); the primitive on the left takes
the index type's (`Line(1) == line`). Three assertions in the grid's tests
compared that way and now name the index type.

## Checked

The crate's 132 unit tests pass. zgx-terminal's
`usize_compares_only_with_itself` compiles only while no crate in its graph
implements a comparison of `usize` with another type.
