//! The plugins example (examples/plugins), run headless with its host and
//! both plugins: what each adds, filters and replaces shows, the app's
//! own element marked `hook = false` stays its own, and the host's actions
//! change the tasks both the app and the plugins show.
mod common;

#[path = "../examples/support/plugins_host.rs"]
mod plugins_host;

use common::Harness;

fn app() -> Harness {
    let h = Harness::new(plugins_host::host);
    h.ui.scene.borrow_mut().resize(560., 480.);
    h.frames(2);
    assert_eq!(h.error(), None);
    h
}

#[test]
fn both_plugins_load_and_add_where_they_target() {
    let mut h = app();
    assert!(h.find("stamp").is_some(), "before the title");
    assert!(
        h.find("focus").is_some(),
        "after the declared hook toolbar.add"
    );
    assert!(
        h.find("clear").is_some(),
        "the hook = false button is the app's"
    );
    let texts = h.texts();
    assert!(texts.contains(&"Write the guide".to_string()), "{texts:?}");
    assert!(
        texts.contains(&"done".to_string()),
        "the overriding row module's badge"
    );
}

#[test]
fn the_focus_filter_follows_the_shared_state_it_reads() {
    let mut h = app();
    assert!(h.texts().contains(&"Review the plugins".to_string()));
    assert!(
        h.texts().contains(&"Focus (2)".to_string()),
        "it reads the tasks: {:?}",
        h.texts()
    );
    h.click("focus");
    h.frames(1);
    let texts = h.texts();
    assert!(
        !texts.contains(&"Review the plugins".to_string()),
        "done tasks hidden: {texts:?}"
    );
    assert!(texts.contains(&"Show all".to_string()), "{texts:?}");
    h.click("focus");
    h.frames(1);
    assert!(h.texts().contains(&"Review the plugins".to_string()));
}

#[test]
fn the_hosts_actions_change_what_everyone_shows() {
    let mut h = app();
    h.click("done-1");
    h.frames(1);
    assert!(h.find("done-1").is_none(), "completed");
    h.click("clear");
    h.frames(1);
    let texts = h.texts();
    assert!(!texts.contains(&"Write the guide".to_string()), "{texts:?}");
    assert!(texts.contains(&"Ship it".to_string()), "{texts:?}");
}

#[test]
fn the_empty_state_is_the_plugins() {
    let mut h = app();
    h.click("done-1");
    h.click("done-3");
    h.click("clear");
    h.frames(1);
    assert!(h.find("empty").is_none(), "replaced");
    assert!(
        h.texts()
            .contains(&"All clear: add a task above.".to_string()),
        "{:?}",
        h.texts()
    );
}
