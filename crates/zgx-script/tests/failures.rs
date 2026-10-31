//! Failures a running app meets go on: a handler's runtime error is
//! reported (logged, and shown over the window in debug builds) while the
//! rest of the app keeps working; a missing image draws empty with its
//! error reported; a font that isn't installed falls back.
mod common;

use common::Harness;

/// A failing text computed reports its error without stopping the other
/// texts, and can return text again on its next update.
#[test]
fn a_computed_text_that_fails_reports_and_the_rest_update() {
    let mut h = Harness::inline(
        r#"local tick = ui.shared("tick", 0)
        return ui.column {
            ui.text { id = "first", function() tick:get(); return "first", "ignored" end },
            ui.text { id = "bad", function()
                if tick:get() == 1 then error("computed text failure marker") end
                return "recovered"
            end },
            ui.text { id = "last", function() return "last " .. tick:get() end },
            ui.text { id = "none", function() return end },
        }"#,
    );
    assert_eq!(h.text("first"), "first");
    assert_eq!(h.text("none"), "");
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(1.));
    h.settle();
    assert!(h.error().unwrap().contains("computed text failure marker"));
    assert_eq!(h.text("bad"), "⚠");
    assert_eq!(h.text("last"), "last 1");
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(2.));
    h.settle();
    assert_eq!(h.text("bad"), "recovered");
    assert_eq!(h.text("last"), "last 2");
}

#[test]
fn a_handlers_runtime_error_is_reported_and_the_app_keeps_working() {
    let mut h = Harness::inline(
        r#"
local n = ui.state("n", 0)
return ui.column {
	ui.button { id = "broken", on_click = function() local t = nil; t.x = 1 end, "Broken" },
	ui.button { id = "more", on_click = ui.add(n), "More" },
	ui.text { "{n}", id = "n" },
}
"#,
    );
    h.click("broken");
    h.frame();
    let error = h.error().expect("the error is reported");
    assert!(error.contains("attempt to index nil"), "{error}");
    h.click("more");
    h.frame();
    assert_eq!(h.text("n"), "1", "the app still answers");
}

#[test]
fn a_missing_image_draws_empty_and_a_missing_font_falls_back() {
    let mut h = Harness::inline(
        r#"
return ui.column {
	ui.image { src = "no/such/picture.png", id = "picture", w = 40, h = 40 },
	ui.text { "Still here", id = "text", font = "No Such Font Family" },
}
"#,
    );
    h.frame();
    assert_eq!(h.text("text"), "Still here");
    assert!(h.find("picture").is_some(), "its node keeps its place");
    let error = h.error().expect("the image's error is reported");
    assert!(error.contains("picture.png"), "{error}");
}

/// A missing image is reported with the script line that named it, in
/// the file that names it: it loads once the node mounts, when no script
/// line runs.
#[test]
fn a_missing_image_names_the_line_that_asked_for_it() {
    let error_of = |name: &str, files: &[(&str, &str)]| {
        let dir =
            std::env::temp_dir().join(format!("zgx-image-site-{}-{name}", std::process::id()));
        for (file, code) in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, code).unwrap();
        }
        let path = dir.join("app.luau");
        let h = Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, path));
        h.frame();
        h.error().expect("the image's error is reported")
    };
    let error = error_of(
        "bg",
        &[(
            "app.luau",
            "return ui.column {\n\n\tui.div { bg_image = 'no-such.png', w = 40, h = 40 },\n}\n",
        )],
    );
    assert!(
        error.contains("app.luau:3: ") && error.contains("no-such.png"),
        "{error}"
    );
    let error = error_of(
        "module",
        &[
            (
                "app.luau",
                "local card = require('./components/card')\nreturn card()\n",
            ),
            (
                "components/card.luau",
                "return function()\n\treturn ui.image { src = \"no/such/picture.png\", w = 40, h = 40 }\nend\n",
            ),
        ],
    );
    assert!(error.contains("components/card.luau:2: "), "{error}");
    // Not a line of tools' files or declarations, which may name the
    // same text (and sort first).
    let error = error_of(
        "types",
        &[
            (".tool/notes.luau", "-- see \"gone.png\"\n"),
            ("a/extra.d.luau", "-- see \"gone.png\"\n"),
            (
                "app.luau",
                "return ui.image { src = \"gone.png\", w = 40, h = 40 }\n",
            ),
        ],
    );
    assert!(error.contains("app.luau:1: "), "{error}");
}

/// An empty `src` is no picture yet, as the web's: an image, a
/// background or an SVG shows nothing and is no error, and a missing file
/// is still one once `src` names it.
#[test]
fn an_empty_image_src_shows_nothing_and_is_no_error() {
    let mut h = Harness::inline(
        r#"
local src = ui.state("src", "")
return ui.column {
	ui.image { src = src, id = "picture", w = 40, h = 40 },
	ui.div { bg_image = src, id = "backdrop", w = 40, h = 40 },
	ui.svg { src = src, id = "icon", w = 20, h = 20 },
}
"#,
    );
    h.frame();
    assert!(h.find("picture").is_some(), "its node keeps its place");
    assert_eq!(h.error(), None);
    h.0.host
        .set_state("src", zgx_script::Value::Str("no/such/picture.png".into()))
        .unwrap();
    h.frame();
    let error = h.error().expect("a missing file is still reported");
    assert!(error.contains("picture.png"), "{error}");
}
