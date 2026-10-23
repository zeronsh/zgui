//! Composition, widgets and lifecycle. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
#[path = "../composition.rs"]
mod composition;
#[path = "../composition_checkbox.rs"]
mod composition_checkbox;
#[path = "../composition_initial_failure.rs"]
mod composition_initial_failure;
#[path = "../composition_progress.rs"]
mod composition_progress;
#[path = "../composition_slider.rs"]
mod composition_slider;
#[path = "../deferred_mount_failure.rs"]
mod deferred_mount_failure;
#[path = "../lifecycle_resources.rs"]
mod lifecycle_resources;
#[path = "../native_views.rs"]
mod native_views;
#[path = "../reactive_dependencies.rs"]
mod reactive_dependencies;
#[path = "../widgets.rs"]
mod widgets;
