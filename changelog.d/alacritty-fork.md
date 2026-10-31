---
section: changed
---
- **zgx-terminal's screen model is zgx's fork, `zgx-alacritty-terminal`** (zgx-terminal): `alacritty_terminal` 0.26.0 without its comparisons of `usize` and `i32` with its grid indices from the primitive's side. Those impls reach every crate linking it, where an inferred comparison (`range.contains(i)` with `i: &&usize`) stopped compiling: an app turning on zgx-desktop's `terminal` feature would have found code of its own broken. Its default `serde` feature is off.
