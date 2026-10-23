//! Checking, diagnostics, types and the testing kit. One test binary for the area (see AGENTS.md):
//! a test file goes in the group for its area, listed here.
// Each file loads its own `common` (a Harness of its own, which files
// add methods to), as it did as a binary of its own.
#![allow(clippy::duplicate_mod)]
#[path = "../check.rs"]
mod check;
#[path = "../diagnostics.rs"]
mod diagnostics;
#[path = "../input_hints.rs"]
mod input_hints;
#[path = "../lint_cli.rs"]
mod lint_cli;
#[path = "../reference_docs.rs"]
mod reference_docs;
#[path = "../testing_kit.rs"]
mod testing_kit;
#[path = "../types.rs"]
mod types;
