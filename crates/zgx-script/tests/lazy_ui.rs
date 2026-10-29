//! `ui`'s modules are made the first time a script names one of their
//! functions: each sets exactly the names it is registered under (checked
//! against what its maker does, under whichever features this build has),
//! a name read is there, and a script that names none of a module's makes
//! nothing of it.
mod common;

use common::Harness;

#[test]
fn every_module_sets_exactly_the_names_it_is_registered_under() {
    let h = Harness::inline("return ui.text { \"x\" }");
    assert_eq!(h.error(), None);
    h.host.check_lazy_modules().unwrap();
}

#[test]
fn a_name_read_first_makes_its_module() {
    let mut h = Harness::inline(
        "local missing = {}\n\
         for _, name in { \"theme\", \"call\", \"json\", \"window\", \"t\", \"path\", \"filter\" } do\n\
             if ui[name] == nil then table.insert(missing, name) end\n\
         end\n\
         if anime == nil or anime.animate == nil then table.insert(missing, \"anime\") end\n\
         return ui.text { table.concat(missing, \" \"), id = \"missing\" }\n",
    );
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
