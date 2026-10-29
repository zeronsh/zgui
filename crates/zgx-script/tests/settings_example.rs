//! The settings example (examples/settings), run headless against its
//! in-memory API: the form filled from the server, a save with an undo, a
//! refused save's message, and the notifications switch saving at once and
//! going back when the server fails.
mod common;

#[path = "../examples/support/settings_server.rs"]
mod settings_server;

use common::Harness;
use settings_server::SettingsServer;
use std::time::Duration;
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/settings/app.luau");

fn settings(server: &SettingsServer) -> Harness {
    let server = server.clone();
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, APP);
        server.serve(&host, Duration::from_millis(20));
        host
    });
    h.ui.scene.borrow_mut().resize(520., 560.);
    h.frames(2);
    h
}
/// Whether checkbox `id` is on, as assistive technology hears it.
fn checked(h: &mut zgx_script::testing::TestApp, id: &str) -> Option<bool> {
    let node = h.find(id)?;
    h.ui.semantics.borrow().get(node)?.checked
}
/// Whether `id` shows `text`.
fn shows(h: &mut zgx_script::testing::TestApp, id: &str, text: &str) -> bool {
    h.find(id).is_some() && h.text(id) == text
}

#[test]
fn the_form_takes_the_servers_settings() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| h.find("name").is_some()));
    assert_eq!(h.error(), None);
    assert!(
        h.wait_until(|h| shows(h, "name", "Ada Lovelace")),
        "{}",
        h.text("name")
    );
    assert_eq!(h.text("email"), "ada@example.com");
}

#[test]
fn a_save_shows_an_undo_that_puts_it_back() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| shows(h, "name", "Ada Lovelace")));
    h.fill("name", "Ada King");
    h.click("save");
    assert!(
        h.wait_until(|_| server.field("name") == "Ada King"),
        "{}",
        server.field("name")
    );
    assert!(h.wait_until(|h| h.texts().contains(&"Settings saved".to_string())));
    assert_eq!(h.text("name"), "Ada King");
    h.click(r#"role:button "Undo""#);
    assert!(
        h.wait_until(|_| server.field("name") == "Ada Lovelace"),
        "{}",
        server.field("name")
    );
    assert!(
        h.wait_until(|h| shows(h, "name", "Ada Lovelace")),
        "{}",
        h.text("name")
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_refused_save_says_why() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| shows(h, "name", "Ada Lovelace")));
    h.fill("name", "taken");
    h.click("save");
    assert!(h.wait_until(|h| h.find("failure").is_some()));
    assert_eq!(h.text("failure"), "Someone already has that name");
}

#[test]
fn the_switch_saves_at_once_and_goes_back_if_refused() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| shows(h, "name", "Ada Lovelace")));
    server.fail(1);
    h.click("notifications");
    h.frames(1);
    assert!(
        h.wait_until(|h| h
            .texts()
            .contains(&"Couldn't change notifications".to_string())),
        "{:?}",
        h.texts()
    );
    assert_eq!(server.field("notifications"), "true");
    // Saved for real this time.
    h.click("notifications");
    assert!(
        h.wait_until(|_| server.field("notifications") == "false"),
        "{}",
        server.field("notifications")
    );
}

#[test]
fn what_is_typed_survives_the_switch_saving() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| shows(h, "name", "Ada Lovelace")));
    h.fill("name", "Ada K");
    // The switch's save writes the loaded settings the form follows.
    h.click("notifications");
    assert!(h.wait_until(|_| server.field("notifications") == "false"));
    h.frames(2);
    assert_eq!(
        h.text("name"),
        "Ada K",
        "an edited field keeps what was typed"
    );
    assert_eq!(h.text("email"), "ada@example.com");
}

#[test]
fn save_waits_for_a_change_and_a_refused_switch_goes_back() {
    let server = SettingsServer::new();
    let mut h = settings(&server);
    assert!(h.wait_until(|h| shows(h, "name", "Ada Lovelace")));
    h.click("save");
    h.frames(2);
    assert_eq!(server.saves(), 0, "nothing changed: Save is disabled");
    h.fill("name", "Ada K");
    h.fill("name", "Ada Lovelace");
    h.click("save");
    h.frames(2);
    assert_eq!(server.saves(), 0, "changed back: not dirty");
    server.hold(true);
    server.fail(1);
    h.click("notifications");
    h.frames(1);
    assert_eq!(
        checked(&mut h, "notifications"),
        Some(false),
        "shown off at once"
    );
    server.hold(false);
    assert!(
        h.wait_until(|h| checked(h, "notifications") == Some(true)),
        "back on after the refusal"
    );
}
