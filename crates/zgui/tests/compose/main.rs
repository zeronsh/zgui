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
#[path = "../composition_split.rs"]
mod composition_split;
#[path = "../deferred_mount_failure.rs"]
mod deferred_mount_failure;
#[path = "../lifecycle_resources.rs"]
mod lifecycle_resources;
#[path = "../native_views.rs"]
mod native_views;
#[path = "../panic_recovery.rs"]
mod panic_recovery;
#[path = "../reactive_dependencies.rs"]
mod reactive_dependencies;
#[path = "../widgets.rs"]
mod widgets;

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
