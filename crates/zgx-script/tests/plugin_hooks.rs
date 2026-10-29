//! Plugins hooking into the app's own interface (`ui.contribute { target,
//! before | after | replace }`): any element by its id, a declared hook or
//! its role and name, matched as the interface is built. Adding before or
//! after an element is open; replacing one takes `Capability::Override`; an
//! element marked `hook = false` or `private = true` is out of reach. What a
//! plugin adds is told apart as the plugin's.
mod common;
use common::Harness;
use std::path::PathBuf;
use zgx_script::{Capability, Plugin, ScriptHost};

fn plugin(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-hooks-{}-{name}", std::process::id()));
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

const APP: &str = r##"
local shown = ui.state("shown", true)
return ui.column {
    ui.row {
        id = "toolbar",
        ui.button { id = "open", w = 60, h = 24, "Open" },
        ui.button { id = "save", hook = "toolbar.save", w = 60, h = 24, "Save" },
        ui.button { id = "delete", hook = false, w = 60, h = 24, "Delete" },
        ui.button { id = "secret", private = true, w = 60, h = 24, "Secret" },
    },
    ui.show(shown, ui.column { ui.text { id = "later", "Later" } }),
    ui.button { id = "toggle", w = 60, h = 24, on_click = ui.toggle(shown), "Toggle" },
}
"##;

#[test]
fn a_plugin_adds_before_and_after_an_element_by_id() {
    let mut h = with(
        APP,
        "extra",
        r##"
ui.contribute { target = "#save", before = ui.text { id = "before", "Before" } }
ui.contribute { target = "#save", after = ui.text { id = "after", "After" } }
"##,
        vec![],
    );
    assert_eq!(h.error(), None);
    let texts = h.texts();
    let at = |t: &str| {
        texts
            .iter()
            .position(|x| x == t)
            .unwrap_or_else(|| panic!("{t} in {texts:?}"))
    };
    assert!(at("Open") < at("Before") && at("Before") < at("Save") && at("Save") < at("After"));
    assert!(at("After") < at("Delete"), "{texts:?}");
}

#[test]
fn a_selector_names_a_target_by_role() {
    let mut h = with(
        APP,
        "selector",
        r##"local target = ui.selector('role:button "Open"')
assert(target == 'role:button "Open"', "the text itself")
ui.contribute { target = target, after = ui.text { "After open" } }"##,
        vec![],
    );
    assert_eq!(h.error(), None);
    let texts = h.texts();
    let at = |t: &str| texts.iter().position(|x| x == t);
    assert_eq!(
        at("After open"),
        at("Open").map(|at| at + 1),
        "the selector found the button: {texts:?}"
    );
}

#[test]
fn a_script_naming_no_selector_makes_none() {
    let h = with(
        APP,
        "plain",
        r##"assert(rawget(ui, "selector") == nil, "made only for a script that names it")
ui.contribute { target = "#open", after = ui.text { "After open" } }"##,
        vec![],
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_declared_hook_and_a_role_and_name_find_the_element() {
    let mut h = with(
        APP,
        "finder",
        r##"
ui.contribute { target = "hook:toolbar.save", after = ui.text { "By hook" } }
ui.contribute { target = 'role:button "Open"', after = ui.text { "By name" } }
"##,
        vec![],
    );
    assert_eq!(h.error(), None);
    let texts = h.texts();
    let at = |t: &str| texts.iter().position(|x| x == t).unwrap();
    assert_eq!(at("By name"), at("Open") + 1, "{texts:?}");
    assert_eq!(at("By hook"), at("Save") + 1, "{texts:?}");
}

#[test]
fn a_private_element_is_out_of_reach() {
    let mut h = with(
        APP,
        "nosy",
        r##"
ui.contribute { target = "#delete", after = ui.text { "Next to delete" } }
ui.contribute { target = '"Secret"', before = ui.text { "Next to secret" } }
"##,
        vec![],
    );
    let texts = h.texts();
    assert!(!texts.iter().any(|t| t.starts_with("Next to")), "{texts:?}");
}

#[test]
fn replacing_an_element_takes_the_override_capability() {
    const REPLACE: &str = r##"ui.contribute { target = "#save", replace = ui.button { id = "mine", w = 60, h = 24, "Mine" } }"##;
    let mut denied = with(APP, "swap", REPLACE, vec![]);
    let error = denied.error().unwrap_or_default();
    assert!(error.contains("Override"), "{error}");
    assert!(denied.find("save").is_some(), "the app's own stays");
    assert!(denied.find("mine").is_none());

    let mut granted = with(
        APP,
        "swap",
        REPLACE,
        vec![Capability::Override("#save".into())],
    );
    assert_eq!(granted.error(), None);
    assert!(granted.find("save").is_none());
    assert!(granted.find("mine").is_some());

    let mut any = with(APP, "swap", REPLACE, vec![Capability::Override("*".into())]);
    assert!(any.find("mine").is_some());
}

#[test]
fn what_a_plugin_adds_is_told_apart_as_its_own() {
    let mut h = with(
        APP,
        "lookalike",
        r##"ui.contribute { target = "#save", after = ui.button { id = "confirm", w = 60, h = 24, "Confirm" } }"##,
        vec![],
    );
    let node = h.find("confirm").expect("contributed");
    let semantics = h.ui.semantics.borrow();
    let described = semantics.get(node).and_then(|s| s.description.clone());
    assert_eq!(described.as_deref(), Some("From the plugin lookalike"));
}

#[test]
fn the_host_can_ask_a_capability_to_add_at_a_target() {
    const ADD: &str = r##"ui.contribute { target = "#open", after = ui.text { "Added" } }"##;
    let entry = plugin("guarded", ADD);
    let mut h = Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.expose_hook("#open", Some(Capability::Named("toolbar".into())));
        host.set_plugins(vec![Plugin {
            name: "guarded".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    let error = h.error().unwrap_or_default();
    assert!(error.contains("toolbar"), "{error}");
    assert!(!h.texts().contains(&"Added".to_string()));
}

#[test]
fn a_contribution_goes_and_comes_back_with_its_element() {
    let mut h = with(
        APP,
        "follower",
        r##"ui.contribute { target = "#later", after = ui.text { id = "tag", "Tag" } }"##,
        vec![],
    );
    assert!(h.find("tag").is_some());
    h.click("toggle");
    assert!(h.find("later").is_none());
    assert!(h.find("tag").is_none(), "gone with its element");
    h.click("toggle");
    assert!(h.find("tag").is_some(), "back with it");
}

#[test]
fn a_target_known_only_once_shown_is_refused() {
    let h = with(
        APP,
        "late",
        r##"ui.contribute { target = "#toolbar row 2", after = ui.text { "x" } }"##,
        vec![],
    );
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("matched as the interface is built"),
        "{error}"
    );
}

#[test]
fn what_a_private_element_holds_is_sealed_however_it_builds() {
    const SEALED: &str = r##"
return ui.column {
    ui.column {
        private = true,
        ui.scroll { h = 80, ui.button { id = "inner", w = 60, h = 24, "Inner" } },
        ui.input { id = "password", value = ui.state("password", "") },
    },
}
"##;
    let mut h = with(
        SEALED,
        "prying",
        r##"
ui.contribute { target = "#inner", after = ui.text { "Next to inner" } }
ui.contribute { target = "#password", after = ui.text { "Next to password" } }
"##,
        vec![],
    );
    assert!(h.find("inner").is_some() && h.find("password").is_some());
    let texts = h.texts();
    assert!(!texts.iter().any(|t| t.starts_with("Next to")), "{texts:?}");
}

#[test]
fn what_a_show_or_a_switch_shows_is_replaced_or_filtered() {
    const SHOWS: &str = r##"
local empty = ui.state("empty", true)
local mode = ui.state("mode", "a")
return ui.column {
    ui.show(empty, ui.text { id = "none", "Nothing yet" }),
    ui.switch(mode, { a = ui.text { id = "mode-a", "Mode A" } }),
}
"##;
    let mut h = with(
        SHOWS,
        "shown",
        r##"
ui.contribute { target = "#none", replace = ui.text { id = "mine", "Mine instead" } }
ui.filter { target = "#mode-a", text = function(t) return t .. "!" end }
"##,
        vec![Capability::Override("*".into())],
    );
    assert_eq!(h.error(), None);
    assert!(h.find("none").is_none() && h.find("mine").is_some());
    assert!(
        h.texts().contains(&"Mode A!".to_string()),
        "{:?}",
        h.texts()
    );
}
