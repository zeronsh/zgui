//! Input, focus, pointer and drag. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
#[path = "../actions.rs"]
mod actions;
#[path = "../active_disable_lifecycle.rs"]
mod active_disable_lifecycle;
#[path = "../active_drag_back.rs"]
mod active_drag_back;
#[path = "../active_key_ownership.rs"]
mod active_key_ownership;
#[path = "../composition_event_defaults.rs"]
mod composition_event_defaults;
#[path = "../composition_events.rs"]
mod composition_events;
#[path = "../composition_focus_scroll.rs"]
mod composition_focus_scroll;
#[path = "../cursors.rs"]
mod cursors;
#[path = "../document_events.rs"]
mod document_events;
#[path = "../drag_drop.rs"]
mod drag_drop;
#[path = "../focus_groups.rs"]
mod focus_groups;
#[path = "../focus_removal.rs"]
mod focus_removal;
#[path = "../focus_visible.rs"]
mod focus_visible;
#[path = "../hit_targets.rs"]
mod hit_targets;
#[path = "../hover_nested.rs"]
mod hover_nested;
#[path = "../hover_refresh.rs"]
mod hover_refresh;
#[path = "../hover_removal.rs"]
mod hover_removal;
#[path = "../hover_subtree.rs"]
mod hover_subtree;
#[path = "../key_activation_target.rs"]
mod key_activation_target;
#[path = "../native_pointer.rs"]
mod native_pointer;
#[path = "../pointer_drag_buttons.rs"]
mod pointer_drag_buttons;
#[path = "../zoom.rs"]
mod zoom;
