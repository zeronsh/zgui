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
#[path = "../each_reorder.rs"]
mod each_reorder;
#[path = "../editor_props.rs"]
mod editor_props;
#[path = "../exiting_rows.rs"]
mod exiting_rows;
#[path = "../failures.rs"]
mod failures;
#[path = "../file_drop.rs"]
mod file_drop;
#[path = "../grid_tracks.rs"]
mod grid_tracks;
#[path = "../hit_target.rs"]
mod hit_target;
#[path = "../i18n_script.rs"]
mod i18n_script;
#[path = "../idle_gc.rs"]
mod idle_gc;
#[path = "../image_cache.rs"]
mod image_cache;
#[path = "../intrinsic_sizes.rs"]
mod intrinsic_sizes;
#[path = "../layout_gaps.rs"]
mod layout_gaps;
#[path = "../layout_motion.rs"]
mod layout_motion;
#[path = "../lazy_ui.rs"]
mod lazy_ui;
#[path = "../list_edges.rs"]
mod list_edges;
#[path = "../mentions.rs"]
mod mentions;
#[path = "../regions.rs"]
mod regions;
#[path = "../rtl.rs"]
mod rtl;
#[cfg(feature = "scene3d")]
#[path = "../scene3d.rs"]
mod scene3d;
#[cfg(not(feature = "scene3d"))]
#[path = "../scene3d_off.rs"]
mod scene3d_off;
#[path = "../script_crashes.rs"]
mod script_crashes;
#[path = "../selection.rs"]
mod selection;
#[path = "../size_calc.rs"]
mod size_calc;
#[path = "../stack.rs"]
mod stack;
#[path = "../sticky.rs"]
mod sticky;
#[path = "../sweep.rs"]
mod sweep;
#[path = "../text_props.rs"]
mod text_props;
#[path = "../theme.rs"]
mod theme;
#[path = "../ui.rs"]
mod ui;
#[path = "../variants.rs"]
mod variants;
#[path = "../wheel_phase.rs"]
mod wheel_phase;
#[path = "../z_order.rs"]
mod z_order;
