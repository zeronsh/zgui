//! Developer diagnostics: the error banner over the window, and the layout
//! inspector's view of a script (where each node was created).
mod common;

use common::Harness;
use std::time::Duration;
use zgx::{inspect::Inspector, scene::NodeKind};
use zgx_script::ScriptHost;

const APP: &str = r#"local count = ui.state("count", 0)
local function bump()
    error("bump failed")
end
return ui.column {
    p = 12, gap = 8,
    ui.text { "Count: {count}", id = "label" },
    ui.button { id = "inc", w = 120, h = 40, on_click = ui.add(count), "Increment" },
    ui.button { id = "bump", w = 120, h = 40, on_click = bump, "Bump" },
}
"#;

fn temp_script(name: &str, code: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-diag-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.luau");
    std::fs::write(&path, code).unwrap();
    path
}
fn rewrite(path: &std::path::Path, code: &str) {
    std::fs::write(path, code).unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(later)
        .unwrap();
}
/// Every text under node `id`, in order.
fn texts(h: &mut Harness, id: &str) -> Vec<String> {
    let node = h.find(id).unwrap_or_else(|| panic!("no `{id}`"));
    let scene = h.ui.scene.borrow();
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if let NodeKind::Text { text, .. } = scene.kind(n) {
            out.push(text.to_string());
        }
        stack.extend(scene.children(n).iter().rev());
    }
    out
}

#[test]
fn no_error_means_no_banner_nodes() {
    let mut h = Harness::inline(APP);
    let nodes = h.ui.scene.borrow().len();
    let host_root = h.handle.node();
    assert_eq!(
        h.ui.scene.borrow().children(host_root).len(),
        1,
        "the interface alone: no banner region"
    );
    assert!(h.find("script-error-banner").is_none());

    h.host.error().set(Some("app: something broke".into()));
    h.settle();
    assert!(h.find("script-error-banner").is_some());
    assert!(h.ui.scene.borrow().len() > nodes);

    h.host.error().set(None);
    h.settle();
    assert!(h.find("script-error-banner").is_none());
    assert_eq!(h.ui.scene.borrow().len(), nodes, "nothing left behind");
    assert_eq!(h.ui.scene.borrow().children(host_root).len(), 1);
}

#[test]
fn a_failed_reload_shows_where_and_a_good_one_clears_it() {
    let path = temp_script("reload", APP);
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, &path));
    let layout = |h: &mut Harness| {
        let node = h.find("inc").unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    let before = layout(&mut h);
    // The user's case: a local function used before its definition.
    rewrite(
        &path,
        "local function render()\n    return ui.column { helper() }\nend\n\
         local function helper()\n    return ui.text 'hi'\nend\nreturn render()\n",
    );
    assert!(h.host.reload_if_changed());
    h.settle();
    let banner = texts(&mut h, "script-error-banner");
    assert_eq!(banner[0], "app.luau:2", "{banner:?}");
    assert!(
        banner[1].contains("attempt to call a nil value"),
        "{banner:?}"
    );
    assert!(
        banner[3..].iter().any(|line| line.contains("app.luau:7")),
        "the traceback follows: {banner:?}"
    );
    // Over the interface, not in its flow: the old interface stays put.
    assert_eq!(layout(&mut h), before, "the banner takes no room");
    let b = {
        let node = h.find("script-error-banner").unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    assert!(b.x >= 12. && b.x + b.width <= 388. + 0.5, "{b:?}");
    assert!(b.y + b.height <= 288. + 0.5 && b.height > 10., "{b:?}");
    assert_eq!(h.text("label"), "Count: 0", "the interface keeps running");

    rewrite(&path, APP);
    assert!(h.host.reload_if_changed());
    h.settle();
    assert_eq!(h.host.error().get(), None);
    assert!(
        h.find("script-error-banner").is_none(),
        "a good reload clears it"
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn handler_errors_show_and_the_banner_dismisses() {
    let mut h = Harness::inline(APP);
    h.click("bump");
    let banner = texts(&mut h, "script-error-banner");
    assert_eq!(banner[0], "app:3", "{banner:?}");
    assert!(banner[1].contains("bump failed"), "{banner:?}");
    h.click("script-error-dismiss");
    assert!(h.find("script-error-banner").is_none());
    assert_eq!(h.host.error().get(), None);
    h.click("inc");
    assert_eq!(h.text("label"), "Count: 1");
}

#[test]
fn the_inspector_names_nodes_by_the_script_line_that_made_them() {
    let inspector = Inspector::off();
    let code = r#"return ui.column {
    w = 400, h = 300,
    ui.column {
        id = "clip", class = "pane", w = 100, h = 40, overflow = "hidden",
        ui.div { w = 100, h = 120, shrink = 0 },
    },
    ui.text { "plain", id = "plain" },
}
"#;
    let mut h = Harness::inspected(
        |runtime| ScriptHost::inline(runtime, "app", code),
        &inspector,
    );
    let clip = h.find("clip").unwrap();
    assert_eq!(inspector.label_of(clip), None, "off: nothing recorded");

    // Turning it on records sources, reloading the script.
    inspector.set_enabled(true);
    h.settle();
    h.ui.prepare_frame();
    let clip = h.find("clip").unwrap();
    let label = inspector.label_of(clip).expect("labelled");
    assert_eq!(&*label.name, "column #clip .pane");
    assert_eq!(label.source.as_deref(), Some("app:3"));
    let plain = h.find("plain").unwrap();
    assert_eq!(
        inspector.label_of(plain).and_then(|l| l.source).as_deref(),
        Some("app:7")
    );

    inspector.update(&h.ui, None);
    let warnings = inspector.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    assert_eq!(
        warnings[0].to_string(),
        "app:3: column #clip .pane: content overflows its clip vertically by 80 (120 in 40) \
         and it does not scroll; use ui.scroll, or make room"
    );
    let b = h.ui.scene.borrow().bounds(clip);
    inspector.update(&h.ui, Some((b.x + b.width - 1., b.y + 1.)));
    // The pointer is over the clipped content: a `ui.div`, laid out as a column.
    let details = inspector.details().join("\n");
    assert!(details.contains("div · column"), "{details}");
    assert!(details.contains("created at app:5"), "{details}");
    assert!(details.contains("clipped to 100 × 40"), "{details}");

    inspector.set_enabled(false);
    inspector.update(&h.ui, None);
    assert_eq!(inspector.overlay(), None);
}

/// Watching, a reload follows a save without waiting out the poll: the
/// kernel says when a file changes (macOS), in place or replaced.
#[cfg(target_os = "macos")]
#[test]
fn a_watching_host_reloads_when_a_file_is_saved() {
    let path = temp_script("watch", APP);
    let h = Harness::new(|runtime| ScriptHost::new(runtime, &path));
    // A poll far longer than the test: only a file event reloads in time.
    let watching = h.host.watch(Duration::from_secs(600));
    h.executor.borrow_mut().spawn(watching);
    h.settle();
    let mut h = h;
    let wait_for = |h: &mut Harness, label: &str| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            h.settle();
            h.ui.prepare_frame();
            if h.text("label") == label {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    };
    // Written in place.
    rewrite(&path, &APP.replace("Count:", "Total:"));
    assert!(wait_for(&mut h, "Total: 0"), "{}", h.text("label"));
    // Replaced, as editors save: written beside it and renamed over it.
    let beside = path.with_extension("tmp");
    std::fs::write(&beside, APP.replace("Count:", "Sum:")).unwrap();
    std::fs::rename(&beside, &path).unwrap();
    assert!(wait_for(&mut h, "Sum: 0"), "{}", h.text("label"));
    // A burst of writes, as a save in several parts: what shows is the
    // last (a reload waits for the writes to stop).
    for word in ["One:", "Two:", "Three:"] {
        rewrite(&path, &APP.replace("Count:", word));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(wait_for(&mut h, "Three: 0"), "{}", h.text("label"));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn a_debug_build_lints_its_scripts_after_each_load() {
    let dir = std::env::temp_dir().join(format!("zgx-lint-on-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let app = dir.join("app.luau");
    std::fs::write(
        &app,
        "local n = ui.state(\"n\", 0)\nreturn ui.text { \"n: \" .. n:get() }\n",
    )
    .unwrap();
    let h = Harness::new(|runtime| ScriptHost::new(runtime, app.clone()));
    let findings = h.host.lint_findings().with_untracked(|f| f.clone());
    let text = findings.expect("the untracked read is found");
    assert!(
        text.contains("app.luau:2:") && text.contains("untracked-get"),
        "{text}"
    );
    assert!(
        h.host
            .warnings()
            .iter()
            .any(|w| w.contains("lint[untracked-get]")),
        "{:?}",
        h.host.warnings()
    );
    std::fs::write(
        &app,
        "local n = ui.state(\"n\", 0)\nreturn ui.text { \"n: {n}\" }\n",
    )
    .unwrap();
    h.host.reload().unwrap();
    assert_eq!(
        h.host.lint_findings().with_untracked(|f| f.clone()),
        None,
        "fixed, it goes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
