//! State, calls, I/O, plugins and the host API. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
// Each file loads its own `common` (a Harness of its own, which files
// add methods to), as it did as a binary of its own.
#![allow(clippy::duplicate_mod)]
#[path = "../app_api.rs"]
mod app_api;
#[path = "../async_handlers.rs"]
mod async_handlers;
#[path = "../calls.rs"]
mod calls;
#[path = "../form_values.rs"]
mod form_values;
#[path = "../io.rs"]
mod io;
#[path = "../large_data.rs"]
mod large_data;
#[path = "../module_overrides.rs"]
mod module_overrides;
#[path = "../packages.rs"]
mod packages;
#[path = "../plugin_filters.rs"]
mod plugin_filters;
#[path = "../plugin_hooks.rs"]
mod plugin_hooks;
#[path = "../plugin_shared.rs"]
mod plugin_shared;
#[path = "../plugins.rs"]
mod plugins;
#[path = "../router.rs"]
mod router;
#[path = "../script.rs"]
mod script;
#[path = "../state.rs"]
mod state;
#[path = "../streaming.rs"]
mod streaming;
#[path = "../ui_errors.rs"]
mod ui_errors;
#[path = "../windows.rs"]
mod windows;
