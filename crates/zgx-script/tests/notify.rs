//! What a script tells someone while they are away: whether its window is
//! the active one and shows (`ui.window.active`, `ui.window.shown`), a
//! system notification (`ui.notify`), and the app icon's badge
//! (`ui.badge`).
mod common;
use common::Harness;
use zgx_script::ScriptHost;

const APP: &str = r#"
local shown = ui.state("shown", "")
return ui.column {
    ui.text { id = "window", function()
        return `{ui.window.active:get()} {ui.window.shown:get()}`
    end },
    ui.text { id = "shown", "{shown}" },
    ui.button { id = "notify", w = 40, h = 20, "n", on_click = function()
        ui.notify({ title = "Ada", body = "Lunch?", tag = "general" }):after(
            function() shown:set("posted") end,
            function(err) shown:set(`{err.kind}: {err}`) end
        )
    end },
    ui.button { id = "clickable", w = 40, h = 20, "k", on_click = function()
        ui.notify({ title = "Grace", body = "Review?", on_click = function()
            shown:set("opened")
        end })
    end },
    ui.button { id = "badge", w = 40, h = 20, "b", on_click = function()
        shown:set(tostring(ui.badge(3)))
    end },
    ui.button { id = "clear", w = 40, h = 20, "c", on_click = function()
        ui.badge(0)
    end },
}
"#;

fn app() -> Harness {
    Harness::with_desktop(|runtime| ScriptHost::inline(runtime, "app", APP))
}

#[test]
fn the_window_says_whether_it_is_active_and_shows() {
    let mut h = app();
    h.host.window_state(&h.ui);
    h.frames(1);
    assert_eq!(h.text("window"), "true true");
    h.ui.set_window_active(false);
    h.frames(1);
    assert_eq!(h.text("window"), "false true");
    h.ui.set_presented(false);
    h.ui.set_window_active(true);
    h.frames(1);
    assert_eq!(h.text("window"), "true false");
}

#[test]
fn a_notification_is_posted_or_says_why_not() {
    let mut h = app();
    h.click("notify");
    h.settle();
    h.frames(1);
    assert_eq!(h.text("shown"), "posted");
    let posted = h.desktop.as_ref().unwrap().notifications();
    assert_eq!(posted.len(), 1);
    assert_eq!(
        (posted[0].title.as_str(), posted[0].body.as_str()),
        ("Ada", "Lunch?")
    );
    assert_eq!(posted[0].tag.as_deref(), Some("general"));
    h.desktop.as_ref().unwrap().deny_notifications();
    h.click("notify");
    h.settle();
    h.frames(1);
    assert_eq!(
        h.text("shown"),
        "denied: notifications are off for this app"
    );
}

#[test]
fn without_a_system_it_is_unsupported() {
    let mut h = Harness::inline(APP);
    h.click("notify");
    h.settle();
    h.frames(1);
    assert!(
        h.text("shown").starts_with("unsupported: "),
        "{}",
        h.text("shown")
    );
}

#[test]
fn the_badge_counts_and_clears() {
    let mut h = app();
    h.click("badge");
    h.frames(1);
    assert_eq!(h.text("shown"), "true");
    assert_eq!(h.desktop.as_ref().unwrap().badge().as_deref(), Some("3"));
    h.click("clear");
    h.frames(1);
    assert_eq!(h.desktop.as_ref().unwrap().badge(), None);
}

#[test]
fn a_click_on_a_notification_calls_its_on_click() {
    let mut h = app();
    h.click("clickable");
    h.settle();
    h.frames(1);
    let desktop = h.desktop.clone().unwrap();
    let posted = desktop.notifications();
    let tag = posted[0].tag.clone().expect("given a tag of its own");
    assert!(tag.starts_with("zgx-notification-"), "{tag}");
    desktop.click_notification(&tag);
    h.settle();
    h.frames(1);
    assert_eq!(h.text("shown"), "opened");
    // A click on what nothing listens for calls nothing.
    desktop.click_notification("elsewhere");
    h.settle();
    h.frames(1);
    assert_eq!(h.text("shown"), "opened");
    assert_eq!(h.error(), None);
}
