//! Plugins passing an app's element's props through functions of their own
//! (`ui.filter { target, text = fn, … }`): its text, name, whether it is
//! hidden or disabled, its tooltip and a `ui.each`'s items. A filter takes
//! `Capability::Override`, as replacing does, and follows what it reads; one
//! that fails shows the value as it was.
mod common;
use common::Harness;
use std::path::PathBuf;
use zgx_script::{Capability, Plugin, ScriptHost};

fn plugin(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-filters-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("init.luau");
    std::fs::write(&entry, code).unwrap();
    entry
}
fn with(app: &'static str, name: &str, code: &str, grants: Vec<Capability>) -> Harness {
    let entry = plugin(name, code);
    let name = name.to_string();
    Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", app);
        host.set_plugins(vec![Plugin {
            name,
            entry,
            grants,
        }]);
        host
    })
}
fn any() -> Vec<Capability> {
    vec![Capability::Override("*".into())]
}

const APP: &str = r##"
local count = ui.state("count", 0)
local fruit = ui.state("fruit", { "apple", "banana", "cherry" })
return ui.column {
    ui.text { id = "title", "Count: {count}" },
    ui.button {
        id = "add",
        w = 60,
        h = 24,
        on_click = function() count:update(function(n) return n + 1 end) end,
        "Add",
    },
    ui.text { id = "note", "A note" },
    ui.each(fruit, function(name) return ui.text { name } end, { id = "fruit" }),
}
"##;

#[test]
fn filtering_takes_the_override_capability() {
    const UPPER: &str =
        r##"ui.filter { target = "#title", text = function(t) return t:upper() end }"##;
    let mut denied = with(APP, "shout", UPPER, vec![]);
    let error = denied.error().unwrap_or_default();
    assert!(error.contains("Override"), "{error}");
    assert!(denied.texts().contains(&"Count: 0".to_string()));

    let mut granted = with(
        APP,
        "shout",
        UPPER,
        vec![Capability::Override("#title".into())],
    );
    assert_eq!(granted.error(), None);
    assert!(granted.texts().contains(&"COUNT: 0".to_string()));
}

#[test]
fn a_filtered_text_follows_the_app_state_it_shows() {
    let mut h = with(
        APP,
        "bang",
        r##"ui.filter { target = "#title", text = function(t) return t .. "!" end }"##,
        any(),
    );
    assert!(
        h.texts().contains(&"Count: 0!".to_string()),
        "{:?}",
        h.texts()
    );
    h.click("add");
    assert!(
        h.texts().contains(&"Count: 1!".to_string()),
        "{:?}",
        h.texts()
    );
}

#[test]
fn a_buttons_text_is_its_label_child() {
    let mut h = with(
        APP,
        "plus",
        r##"ui.filter { target = "#add", text = function(t) return t .. " one" end }"##,
        any(),
    );
    assert!(
        h.texts().contains(&"Add one".to_string()),
        "{:?}",
        h.texts()
    );
    h.click("add");
    assert!(
        h.texts().contains(&"Count: 1".to_string()),
        "its handler stays the app's"
    );
}

#[test]
fn a_disabled_filter_stops_the_handler() {
    let mut h = with(
        APP,
        "lock",
        r##"ui.filter { target = "#add", disabled = function(was) return true end }"##,
        any(),
    );
    h.click("add");
    assert!(
        h.texts().contains(&"Count: 0".to_string()),
        "{:?}",
        h.texts()
    );
}

#[test]
fn a_hidden_filter_hides_the_element() {
    let mut h = with(
        APP,
        "quiet",
        r##"ui.filter { target = "#note", hidden = function(was) return not was end }"##,
        any(),
    );
    assert_eq!(h.error(), None);
    let note = h.find("note").expect("still there, hidden");
    let shown = h.ui.scene.borrow().bounds(note).height;
    assert_eq!(shown, 0., "hidden takes no room");
}

#[test]
fn an_items_filter_changes_what_a_list_shows() {
    let mut h = with(
        APP,
        "nobanana",
        r##"
ui.filter {
    target = "#fruit",
    items = function(items)
        local kept = {}
        for _, name in items do
            if name ~= "banana" then table.insert(kept, name) end
        end
        return kept
    end,
}
"##,
        any(),
    );
    assert_eq!(h.error(), None);
    let texts = h.texts();
    assert!(
        texts.contains(&"apple".to_string()) && texts.contains(&"cherry".to_string()),
        "{texts:?}"
    );
    assert!(!texts.contains(&"banana".to_string()), "{texts:?}");
}

#[test]
fn filtered_items_follow_the_list() {
    const GROWS: &str = r##"
local fruit = ui.state("fruit", { "apple" })
return ui.column {
    ui.each(fruit, function(name) return ui.text { name } end, { id = "fruit" }),
    ui.button { id = "more", w = 60, h = 24, on_click = function() fruit:push("plum") end, "More" },
}
"##;
    let mut h = with(
        GROWS,
        "loud",
        r##"
ui.filter {
    target = "#fruit",
    items = function(items)
        local out = {}
        for i, name in items do out[i] = string.upper(name) end
        return out
    end,
}
"##,
        any(),
    );
    assert!(h.texts().contains(&"APPLE".to_string()), "{:?}", h.texts());
    h.click("more");
    assert!(h.texts().contains(&"PLUM".to_string()), "{:?}", h.texts());
}

#[test]
fn a_failing_filter_shows_the_value_as_it_was() {
    let mut h = with(
        APP,
        "broken",
        r##"ui.filter { target = "#note", text = function(t) error("no") end }"##,
        any(),
    );
    assert!(h.texts().contains(&"A note".to_string()), "{:?}", h.texts());
    let error = h.error().unwrap_or_default();
    assert!(error.contains("no"), "{error}");
}

#[test]
fn only_the_listed_props_are_filtered() {
    let h = with(
        APP,
        "greedy",
        r##"ui.filter { target = "#add", on_click = function() end }"##,
        any(),
    );
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("on_click") && error.contains("not filtered"),
        "{error}"
    );
}
