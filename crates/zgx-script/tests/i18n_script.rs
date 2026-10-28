//! Localized messages in scripts: `ui.i18n` loads a directory of locale
//! files, `ui.t` shows a message that follows its locale, its arguments and
//! its file, `ui.locale` asks for a locale and says which is shown.
mod common;

use common::Harness;
use std::path::{Path, PathBuf};
use zgx_script::ScriptHost;

const EN: &str = r#"{
  "inbox": { "count": "{n, plural, one {# message} other {# messages}}" },
  "greeting": "Hello, {name}",
  "search": "Search"
}"#;
const FR: &str = r#"{
  "inbox": { "count": "{n, plural, one {# message} other {# messages}}" },
  "greeting": "Bonjour, {name}",
  "search": "Rechercher"
}"#;
const AR: &str = r#"{ "greeting": "مرحبا، {name}" }"#;

/// A script directory: `app.luau` with `code`, and `locales/` with these.
fn app(name: &str, code: &str, locales: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-i18n-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("locales")).unwrap();
    for (locale, text) in locales {
        std::fs::write(dir.join("locales").join(format!("{locale}.json")), text).unwrap();
    }
    std::fs::write(dir.join("app.luau"), code).unwrap();
    dir.join("app.luau")
}
fn harness(path: &Path) -> Harness {
    let path = path.to_path_buf();
    let h = Harness::new(|runtime| ScriptHost::new(runtime, path));
    h.settle();
    h
}

const COUNTER: &str = r#"
ui.i18n { dir = "locales", source = "en" }
local n = ui.state("n", 1)
local name = ui.state("name", "Ada")
return ui.column {
    ui.text { ui.t("inbox.count", { n = n }), id = "count" },
    ui.text { ui.t("greeting", { name = name }), id = "greeting" },
    ui.input { value = ui.state("q", ""), placeholder = ui.t("search"), id = "search" },
    ui.button { ui.t("greeting", { name = "button" }), id = "button", on_click = function() n:set(n:get() + 1) end },
}
"#;

#[test]
fn a_message_shows_its_text_and_follows_its_arguments() {
    let path = app("args", COUNTER, &[("en", EN), ("fr", FR)]);
    let mut h = harness(&path);
    h.host.inspect(false);
    assert_eq!(h.error(), None);
    h.ui.prepare_frame();
    h.0.host.set_state("n", zgx_script::Value::Num(1.)).unwrap();
    h.settle();
    assert_eq!(h.text("count"), "1 message");
    assert_eq!(h.text("greeting"), "Hello, Ada");
    h.0.host.set_state("n", zgx_script::Value::Num(3.)).unwrap();
    h.settle();
    assert_eq!(h.text("count"), "3 messages");
    h.0.host
        .set_state("name", zgx_script::Value::Str("Grace".into()))
        .unwrap();
    h.settle();
    assert_eq!(h.text("greeting"), "Hello, Grace");
}

#[test]
fn a_locale_asked_for_changes_the_text_and_what_is_current() {
    let code = format!(
        "{COUNTER}\n",
    )
    .replace(
        "return ui.column {",
        "ui.command { id = \"fr\", run = function() ui.locale:set(\"fr\") end }\n\
         ui.command { id = \"system\", run = function() ui.locale:set(nil) end }\n\
         return ui.column {\n    ui.text { function() return ui.locale.current end, id = \"current\" },",
    );
    let path = app("switch", &code, &[("en", EN), ("fr", FR)]);
    let mut h = harness(&path);
    zgx::i18n::set_system_languages(|| vec!["en-US".into()]);
    assert!(h.host.run_command("system"));
    h.settle();
    assert_eq!(h.text("greeting"), "Hello, Ada");
    assert!(h.host.run_command("fr"));
    h.settle();
    assert_eq!(h.text("greeting"), "Bonjour, Ada");
    assert_eq!(h.text("current"), "fr");
    assert!(h.host.run_command("system"));
    h.settle();
    assert_eq!(h.text("greeting"), "Hello, Ada");
}

#[test]
fn a_locale_file_edited_changes_its_text_without_reloading_the_script() {
    let path = app("reload", COUNTER, &[("en", EN)]);
    let mut h = harness(&path);
    assert_eq!(h.text("greeting"), "Hello, Ada");
    // Edited later than it was read (mtime granularity).
    let file = path.parent().unwrap().join("locales/en.json");
    std::fs::write(&file, EN.replace("Hello, {name}", "Hi there, {name}")).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let calls = h.calls();
    assert!(h.host.reload_if_changed());
    h.settle();
    assert_eq!(h.text("greeting"), "Hi there, Ada");
    assert_eq!(h.calls(), calls, "the script didn't run again");
}

#[test]
fn an_arabic_locale_runs_the_script_right_to_left() {
    let code = COUNTER.replace(
        "return ui.column {",
        "ui.command { id = \"ar\", run = function() ui.locale:set(\"ar\") end }\nreturn ui.column { id = \"root\",",
    );
    let path = app("rtl", &code, &[("en", EN), ("ar", AR)]);
    let mut h = harness(&path);
    let direction = |h: &mut Harness| {
        let root = h.find("root").unwrap();
        h.ui.scene.borrow().direction(root)
    };
    assert_eq!(direction(&mut h), zgx::scene::Direction::Ltr);
    assert!(h.host.run_command("ar"));
    h.settle();
    assert_eq!(direction(&mut h), zgx::scene::Direction::Rtl);
    assert_eq!(h.text("greeting"), "مرحبا، Ada");
}

#[test]
fn messages_used_wrongly_are_errors_or_warnings_naming_what() {
    // `ui.t` before `ui.i18n`.
    let error = ScriptHost::new(
        &zgx::reactive::Runtime::new(),
        app(
            "early",
            "return ui.text { ui.t(\"greeting\") }",
            &[("en", EN)],
        ),
    )
    .reload()
    .unwrap_err();
    assert!(
        error.contains("app.luau:1") && error.contains("call ui.i18n"),
        "{error}"
    );
    // A directory with no locale files.
    let error = ScriptHost::new(
        &zgx::reactive::Runtime::new(),
        app(
            "empty",
            "ui.i18n { dir = \"nowhere\" } return ui.text 'x'",
            &[],
        ),
    )
    .reload()
    .unwrap_err();
    assert!(error.contains("no locale files"), "{error}");
    // A key no locale has shows as itself, and is told once.
    let path = app(
        "missing",
        "ui.i18n {}\nreturn ui.text { ui.t(\"nope.key\"), id = \"t\" }",
        &[("en", EN), ("fr", "{ \"broken\": \"{n, plural\" }")],
    );
    let mut h = harness(&path);
    assert_eq!(h.text("t"), "nope.key");
    let warnings = h.host.warnings().join("\n");
    assert!(warnings.contains("nope.key"), "{warnings}");
    assert!(
        warnings.contains("fr.json"),
        "a parse error names its file: {warnings}"
    );
}

#[test]
fn the_lint_compares_each_locale_with_the_source() {
    let path = app(
        "lint",
        "ui.i18n { dir = \"locales\", source = \"en\" }\nreturn ui.text { ui.t(\"no.such\") }\n",
        &[
            ("en", EN),
            (
                "fr",
                "{\n  \"greeting\": \"Bonjour, {nom}\",\n  \"extra\": \"en trop\",\n  \"search\": \"{n, plural\"\n}",
            ),
        ],
    );
    let report = zgx_script::lint::lint(&[path.parent().unwrap().to_path_buf()]).unwrap();
    let found: Vec<(String, String)> = report
        .diagnostics
        .iter()
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect();
    let has = |code: &str, text: &str| found.iter().any(|(c, m)| c == code && m.contains(text));
    assert!(has("i18n-args", "greeting names {nom}"), "{found:?}");
    assert!(has("i18n-extra", "extra"), "{found:?}");
    assert!(has("i18n-missing", "inbox.count"), "{found:?}");
    assert!(has("i18n-parse", "search"), "{found:?}");
    assert!(has("i18n-key", "no.such"), "{found:?}");
    assert!(!has("i18n-key", "greeting"), "{found:?}");
}

/// The labels the semantics give nodes of `role`, in no set order.
fn labels(h: &mut Harness, role: zgx::semantics::Role) -> Vec<String> {
    h.ui.semantics
        .borrow()
        .iter()
        .filter(|(_, node)| node.role == role)
        .map(|(_, node)| node.label.clone())
        .collect()
}

#[test]
fn widget_labels_follow_the_locale_and_their_state() {
    use zgx::semantics::Role;
    let code = r#"
ui.i18n { dir = "locales", source = "en" }
ui.command { id = "fr", run = function() ui.locale:set("fr") end }
ui.command { id = "rename", run = function() ui.state("title", "Draft"):set("Final") end }
local title = ui.state("title", "Draft")
return ui.column {
    ui.menu_item { label = ui.t("search"), id = "item" },
    ui.modal { label = ui.t("greeting", { name = "Ada" }), open = ui.state("open", true), ui.text { "In the dialog" } },
    ui.modal { label = title, open = ui.state("open2", true), layer = true, ui.text { "Another" } },
    ui.checkbox { label = ui.t("search"), value = ui.state("on", false) },
    ui.input { label = title, value = ui.state("q", "") },
}
"#;
    let path = app("labels", code, &[("en", EN), ("fr", FR)]);
    let mut h = harness(&path);
    zgx::i18n::set_system_languages(|| vec!["en-US".into()]);
    h.ui.set_assistive_technology(true);
    h.settle();
    assert_eq!(h.error(), None);
    assert_eq!(h.text("item"), "Search");
    assert_eq!(labels(&mut h, Role::MenuItem), vec!["Search".to_string()]);
    let mut dialogs = labels(&mut h, Role::Dialog);
    dialogs.sort();
    assert_eq!(dialogs, vec!["Draft".to_string(), "Hello, Ada".to_string()]);
    assert_eq!(labels(&mut h, Role::CheckBox), vec!["Search".to_string()]);
    assert_eq!(labels(&mut h, Role::TextInput), vec!["Draft".to_string()]);
    assert!(h.host.run_command("fr"));
    assert!(h.host.run_command("rename"));
    h.settle();
    assert_eq!(
        labels(&mut h, Role::CheckBox),
        vec!["Rechercher".to_string()]
    );
    assert_eq!(labels(&mut h, Role::TextInput), vec!["Final".to_string()]);
    assert_eq!(h.text("item"), "Rechercher", "drawn as it is named");
    assert_eq!(
        labels(&mut h, Role::MenuItem),
        vec!["Rechercher".to_string()]
    );
    let mut dialogs = labels(&mut h, Role::Dialog);
    dialogs.sort();
    assert_eq!(
        dialogs,
        vec!["Bonjour, Ada".to_string(), "Final".to_string()]
    );
}

#[test]
fn tabs_and_selects_name_and_title_themselves_in_the_locale() {
    use zgx::semantics::Role;
    // A title follows the locale through a tracked read: a computed list
    // whose labels read `ui.t(...):get()`.
    let code = r#"
ui.i18n { dir = "locales", source = "en" }
ui.command { id = "fr", run = function() ui.locale:set("fr") end }
local tabs = ui.computed(function()
    return { { id = "g", label = ui.t("greeting", { name = "Ada" }):get() } }
end)
return ui.column {
    ui.tabs {
        label = ui.t("search"),
        items = tabs,
        value = ui.state("tab", "g"),
        panel = function(id) return ui.text { id } end,
    },
    ui.select { label = ui.t("search"), value = ui.state("pick", "a"), options = { "a", "b" } },
}
"#;
    let path = app("tabs", code, &[("en", EN), ("fr", FR)]);
    let mut h = harness(&path);
    zgx::i18n::set_system_languages(|| vec!["en-US".into()]);
    h.ui.set_assistive_technology(true);
    h.settle();
    assert_eq!(h.error(), None);
    assert!(
        h.texts().contains(&"Hello, Ada".to_string()),
        "{:?}",
        h.texts()
    );
    assert_eq!(labels(&mut h, Role::TabList), vec!["Search".to_string()]);
    assert_eq!(labels(&mut h, Role::ComboBox), vec!["Search".to_string()]);
    assert!(h.host.run_command("fr"));
    h.settle();
    assert!(
        h.texts().contains(&"Bonjour, Ada".to_string()),
        "{:?}",
        h.texts()
    );
    assert_eq!(
        labels(&mut h, Role::TabList),
        vec!["Rechercher".to_string()]
    );
    assert_eq!(
        labels(&mut h, Role::ComboBox),
        vec!["Rechercher".to_string()]
    );
}
