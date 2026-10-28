//! The ui library: nodes, regions, layout and themes. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
// Each file loads its own `common` (a Harness of its own, which files
// add methods to), as it did as a binary of its own.
#![allow(clippy::duplicate_mod)]
#[path = "../components.rs"]
mod components;
#[path = "../container_queries.rs"]
mod container_queries;
#[path = "../driver.rs"]
mod driver;
#[path = "../failures.rs"]
mod failures;
#[path = "../layout_gaps.rs"]
mod layout_gaps;
#[path = "../layout_motion.rs"]
mod layout_motion;
#[path = "../regions.rs"]
mod regions;
#[path = "../rtl.rs"]
mod rtl;
#[path = "../scene3d.rs"]
mod scene3d;
#[path = "../selection.rs"]
mod selection;
#[path = "../stack.rs"]
mod stack;
#[path = "../sweep.rs"]
mod sweep;
#[path = "../theme.rs"]
mod theme;
#[path = "../ui.rs"]
mod ui;
#[path = "../variants.rs"]
mod variants;
