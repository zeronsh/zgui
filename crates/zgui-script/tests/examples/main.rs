//! The example apps, run as tests. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
// Each file loads its own `common` (a Harness of its own, which files
// add methods to), as it did as a binary of its own.
#![allow(clippy::duplicate_mod)]
#[path = "../catalog_example.rs"]
mod catalog_example;
#[path = "../dashboard_example.rs"]
mod dashboard_example;
#[path = "../example_scripts.rs"]
mod example_scripts;
#[path = "../examples.rs"]
mod examples;
#[path = "../frame_profiler.rs"]
mod frame_profiler;
#[path = "../mail_example.rs"]
mod mail_example;
#[path = "../monitor_example.rs"]
mod monitor_example;
#[path = "../sketch_example.rs"]
mod sketch_example;
#[path = "../terminal_example.rs"]
mod terminal_example;
