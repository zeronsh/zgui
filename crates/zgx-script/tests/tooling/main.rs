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
#[path = "../inspect_cli.rs"]
mod inspect_cli;
#[path = "../lint_cli.rs"]
mod lint_cli;
#[path = "../reference_docs.rs"]
mod reference_docs;
#[path = "../run_cli.rs"]
mod run_cli;
#[path = "../test_cli.rs"]
mod test_cli;
#[path = "../testing_kit.rs"]
mod testing_kit;
#[path = "../types.rs"]
mod types;

/// Every test file runs. With `autotests = false`, a file in `tests/` that
/// no area's `main.rs` lists (and no `[[test]]` names) builds into nothing
/// and its tests never run, silently.
#[test]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut listed = std::fs::read_to_string(dir.join("../Cargo.toml")).unwrap();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.join("main.rs").exists() {
            listed += &std::fs::read_to_string(path.join("main.rs")).unwrap();
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    let missing: Vec<_> = files
        .into_iter()
        .filter(|f| {
            !listed.contains(&format!("\"../{f}.rs\""))
                && !listed.contains(&format!("tests/{f}.rs"))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "test files that never run: {missing:?}. List each in its area's tests/<area>/main.rs \
         as `#[path = \"../name.rs\"] mod name;` (see AGENTS.md)"
    );
}
