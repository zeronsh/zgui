//! The components' tests, as one binary (see AGENTS.md): a test file goes
//! beside the others (`tests/name.rs`) and is listed here.
#[path = "../combobox.rs"]
mod combobox;
#[path = "../controls.rs"]
mod controls;
#[path = "../date.rs"]
mod date;
#[path = "../grid.rs"]
mod grid;
#[path = "../tabs.rs"]
mod tabs;
#[path = "../toast.rs"]
mod toast;
#[path = "../tree.rs"]
mod tree;

/// A test file no area lists builds into nothing and its tests never run,
/// silently.
#[test]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let listed = std::fs::read_to_string(dir.join("components/main.rs")).unwrap();
    let missing: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .filter(|f| !listed.contains(&format!("\"../{f}.rs\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "test files that never run: {missing:?}. List each in tests/components/main.rs"
    );
}
