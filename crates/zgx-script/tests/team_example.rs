//! The team example (examples/team), run headless against its in-memory
//! API: loading and a failed load's retry, adding and editing a person in
//! the modal form with its checks and the server's answer, and removing one
//! after confirming.
mod common;

#[path = "../examples/support/team_server.rs"]
mod team_server;

use common::Harness;
use std::time::Duration;
use team_server::TeamServer;
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/team/app.luau");

fn team(server: &TeamServer) -> Harness {
    let server = server.clone();
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, APP);
        // Slow enough that loading shows between a click and its answer.
        server.serve(&host, Duration::from_millis(30));
        host
    });
    h.ui.scene.borrow_mut().resize(720., 640.);
    h.frames(2);
    h
}
fn people() -> TeamServer {
    TeamServer::new(&[
        ("Ada Lovelace", "ada@example.com", "Engineer"),
        ("Grace Hopper", "grace@example.com", "Manager"),
    ])
}

#[test]
fn the_team_loads_and_a_failed_load_retries() {
    let server = people();
    server.fail(1);
    let mut h = team(&server);
    assert_eq!(h.error(), None);
    assert!(h.wait_until(|h| h.find("load-error").is_some()));
    assert!(
        h.text("load-error").contains("The network is unreachable"),
        "{}",
        h.text("load-error")
    );
    // Held: a loaded machine could take the latency within the click,
    // answered before the loading state was looked for.
    server.hold(true);
    h.click("retry");
    assert!(h.find("loading").is_some(), "loading again");
    server.hold(false);
    assert!(h.wait_until(|h| h.find("member-m1").is_some()));
    assert!(h.find("member-m2").is_some());
    assert_eq!(h.error(), None);
}

#[test]
fn a_new_person_is_checked_then_saved() {
    let server = people();
    let mut h = team(&server);
    assert!(h.wait_until(|h| h.find("member-m1").is_some()));
    h.click("add");
    h.frames(2);
    // Nothing shows wrong until a field is left, or a save is tried.
    assert!(h.find("name-error").is_none());
    h.click("save");
    h.frames(1);
    assert_eq!(h.text("name-error"), "A name is required");
    assert_eq!(h.text("email-error"), "An email is required");
    assert_eq!(server.asked(), ["GET /members"], "nothing sent");
    h.type_text("name", "Katherine Johnson");
    h.type_text("email", "ADA");
    h.frames(1);
    assert!(h.find("name-error").is_none());
    assert_eq!(h.text("email-error"), "That is not an email address");
    // Typed after the text, case aside: Ada's.
    h.type_text("email", "@example.com");
    h.frames(1);
    assert_eq!(h.text("email-error"), "Someone already has that email");
    h.click("cancel");
    h.frames(2);
    assert!(!h.shown("save"));
    // A new form, blank, each time it opens.
    h.click("add");
    h.frames(2);
    assert!(h.shown("save"));
    assert!(h.find("name-error").is_none());
    assert_eq!(h.text("name"), "");
    h.type_text("name", "Katherine Johnson");
    h.type_text("email", "katherine@example.com");
    h.click("save");
    assert!(h.wait_until(|h| h.find("member-m3").is_some()));
    assert!(!h.shown("save"), "the modal closed");
    assert_eq!(server.len(), 3);
    assert_eq!(h.error(), None);
}

#[test]
fn an_edit_shows_what_the_server_refused() {
    let server = people();
    let mut h = team(&server);
    assert!(h.wait_until(|h| h.find("member-m2").is_some()));
    h.click("edit-m1");
    h.frames(2);
    assert_eq!(h.text("name"), "Ada Lovelace");
    h.type_text("name", " King");
    h.click("cancel");
    h.frames(2);
    // Each opening starts from the person edited.
    h.click("edit-m2");
    h.frames(2);
    assert_eq!(h.text("name"), "Grace Hopper");
    server.fail(1);
    h.click("save");
    assert!(h.wait_until(|h| h.find("save-error").is_some()));
    assert_eq!(h.text("save-error"), "The network is unreachable");
    h.click("save");
    assert!(h.wait_until(|h| !h.shown("save")));
    assert_eq!(h.error(), None);
}

#[test]
fn a_person_is_removed_after_confirming() {
    let server = people();
    let mut h = team(&server);
    assert!(h.wait_until(|h| h.find("member-m1").is_some()));
    h.click("remove-m1");
    h.frames(2);
    h.click("keep");
    h.frames(2);
    assert!(h.find("member-m1").is_some());
    h.click("remove-m1");
    h.frames(2);
    h.click("confirm-remove");
    assert!(h.wait_until(|h| h.find("member-m1").is_none()));
    assert_eq!(server.len(), 1);
    assert_eq!(h.error(), None);
}
