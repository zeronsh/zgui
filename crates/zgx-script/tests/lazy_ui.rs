//! `ui`'s modules are made the first time a script names one of their
//! functions: every name is there when read, and a script that names none
//! of a module's makes nothing of it. (In debug builds a module that sets a
//! name it isn't registered under fails as it is made.)
mod common;

use common::Harness;

/// Every name a module is registered under (`ScriptHost`'s setup).
const NAMES: &[&str] = &[
    "path",
    "linear",
    "json",
    "number",
    "sort",
    "term",
    "web",
    "command",
    "shortcut_text",
    "i18n",
    "t",
    "locale",
    "window",
    "call",
    "fetch",
    "resource",
    "task",
    "try",
    "clipboard",
    "open_files",
    "read_file",
    "save_file",
    "write_file",
    "prefers",
    "theme",
    "tokens",
    "form",
    "selector",
    "contribute",
    "filter",
    "override",
    "args",
    "on_open",
];

#[test]
fn every_name_of_every_module_is_there_when_read() {
    let mut names: Vec<&str> = NAMES.to_vec();
    if cfg!(feature = "scene3d") {
        names.extend(["scene3d", "mesh", "light", "group"]);
    }
    let list = names
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut h = Harness::inline(&format!(
        "local missing = {{}}\n\
         for _, name in {{ {list} }} do\n\
             if ui[name] == nil then table.insert(missing, name) end\n\
         end\n\
         if anime == nil or anime.animate == nil then table.insert(missing, \"anime\") end\n\
         return ui.text {{ table.concat(missing, \" \"), id = \"missing\" }}\n"
    ));
    assert_eq!(h.error(), None);
    assert_eq!(h.text("missing"), "");
}

#[test]
fn a_script_naming_no_module_makes_none() {
    let mut h = Harness::inline(
        "local made = {}\n\
         for _, name in { \"theme\", \"call\", \"json\", \"window\", \"i18n\", \"path\" } do\n\
             if rawget(ui, name) ~= nil then table.insert(made, name) end\n\
         end\n\
         return ui.text { table.concat(made, \" \"), id = \"made\" }\n",
    );
    assert_eq!(h.error(), None);
    assert_eq!(h.text("made"), "");
}

#[test]
fn a_module_named_once_keeps_its_functions_and_state() {
    // `ui.theme.current` is state made with the module: read twice, one.
    let mut h = Harness::inline(
        "local a = ui.theme.current\n\
         local b = ui.theme.current\n\
         return ui.text { tostring(rawequal(a, b)), id = \"same\" }\n",
    );
    assert_eq!(h.error(), None);
    assert_eq!(h.text("same"), "true");
}
