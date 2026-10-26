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
