//! `ui.form { values }` filling a form from what is loaded, a field taking
//! the new value unless it has been edited; `dirty`; the form handed to
//! `submit` and `on_success`; and controls controlled as React's are
//! (`value = function` with `on_change`).
mod common;
use common::Harness;

const FOLLOW: &str = r##"
local loaded = ui.state("loaded", { name = "Ada", email = "ada@example.com" })
local seen = ui.state("seen", "")
local form = ui.form {
    fields = { name = "", email = "" },
    values = loaded,
    submit = function(values, form)
        return form.dirty:peek()
    end,
    on_success = function(was_dirty, values, form)
        seen:set(`{tostring(was_dirty)} {values.name}`)
        form.reset(values)
    end,
}
return ui.column {
    ui.input { id = "name", label = "Name", field = form.fields.name },
    ui.input { id = "email", label = "Email", field = form.fields.email },
    ui.text { id = "dirty", function() return tostring(form.dirty:get()) end },
    ui.text { id = "seen", "{seen}" },
    ui.button { id = "send", w = 40, h = 20, "s", on_click = form.submit },
    ui.button {
        id = "reload",
        w = 40,
        h = 20,
        "r",
        on_click = function() loaded:set({ name = "Ada L", email = "lovelace@example.com" }) end,
    },
}
"##;

#[test]
fn a_form_follows_its_values_keeping_what_was_typed() {
    let mut h = Harness::inline(FOLLOW);
    assert_eq!(h.error(), None);
    assert_eq!(h.text("name"), "Ada", "filled from `values`");
    assert_eq!(h.text("dirty"), "false");
    h.fill("name", "Countess");
    assert_eq!(h.text("dirty"), "true");
    h.click("reload");
    h.frames(1);
    assert_eq!(
        h.text("name"),
        "Countess",
        "an edited field keeps what was typed"
    );
    assert_eq!(
        h.text("email"),
        "lovelace@example.com",
        "an untouched one follows"
    );
    assert_eq!(h.text("dirty"), "true", "still differs from the new value");
    h.fill("name", "Ada L");
    assert_eq!(h.text("dirty"), "false", "typed back to what was loaded");
}

#[test]
fn submit_and_on_success_are_handed_the_form() {
    let mut h = Harness::inline(FOLLOW);
    h.fill("name", "Countess");
    h.click("send");
    h.frames(2);
    assert_eq!(h.text("seen"), "true Countess");
    assert_eq!(
        h.text("dirty"),
        "false",
        "on_success reset it through the form it was given"
    );
    assert_eq!(h.error(), None);
}

const CONTROLLED: &str = r##"
local on = ui.state("on", false)
local refusing = ui.state("refusing", false)
local mode = ui.state("mode", "week")
return ui.column {
    ui.checkbox {
        id = "box",
        label = "On",
        value = function() return on:get() end,
        on_change = function(next)
            if not refusing:peek() then on:set(next) end
        end,
    },
    ui.segmented {
        id = "mode",
        options = { "day", "week" },
        value = function() return mode:get() end,
        on_change = function(next) mode:set(next) end,
    },
    ui.text { id = "on", "{on}" },
    ui.text { id = "mode-now", "{mode}" },
    ui.button { id = "refuse", w = 40, h = 20, "x", on_click = function() refusing:set(true) end },
    ui.button { id = "outside", w = 40, h = 20, "o", on_click = function() on:set(false) end },
}
"##;

fn checked(h: &mut Harness, id: &str) -> Option<bool> {
    let node = h.find(id)?;
    h.ui.prepare_frame();
    h.ui.semantics.borrow().get(node)?.checked
}

#[test]
fn a_controlled_checkbox_shows_its_function_and_asks_on_change() {
    let mut h = Harness::inline(CONTROLLED);
    assert_eq!(h.error(), None);
    assert_eq!(checked(&mut h, "box"), Some(false));
    h.click("box");
    h.frames(1);
    assert_eq!(h.text("on"), "true", "the change went to on_change");
    assert_eq!(checked(&mut h, "box"), Some(true));
    h.click("outside");
    h.frames(1);
    assert_eq!(
        checked(&mut h, "box"),
        Some(false),
        "it follows what its function reads"
    );
    h.click("refuse");
    h.click("box");
    h.frames(1);
    assert_eq!(h.text("on"), "false");
    assert_eq!(
        checked(&mut h, "box"),
        Some(false),
        "a change refused shows what the function says"
    );
}

#[test]
fn a_controlled_segmented_control_asks_on_change() {
    let mut h = Harness::inline(CONTROLLED);
    h.click(r#"role:radio "day""#);
    h.frames(1);
    assert_eq!(h.text("mode-now"), "day");
}
