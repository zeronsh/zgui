//! State, calls, I/O, plugins and the host API. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
// Each file loads its own `common` (a Harness of its own, which files
// add methods to), as it did as a binary of its own.
#![allow(clippy::duplicate_mod)]
#[path = "../app_api.rs"]
mod app_api;
#[path = "../calls.rs"]
mod calls;
#[path = "../io.rs"]
mod io;
#[path = "../plugins.rs"]
mod plugins;
#[path = "../script.rs"]
mod script;
#[path = "../state.rs"]
mod state;
#[path = "../streaming.rs"]
mod streaming;
#[path = "../windows.rs"]
mod windows;
