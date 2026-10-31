//! The channels example (examples/channels), run headless against its
//! in-memory server: the welcome loads the channels and their messages; a
//! message is shown at once and confirmed; a refused one is sent again; a
//! dropped connection shows the banner and reconnects; a message elsewhere
//! counts unread; and a mention opens at the caret and is chosen.
mod common;

#[path = "../examples/support/channels_server.rs"]
mod channels_server;

use channels_server::ChannelsServer;
use common::Harness;
use std::time::Duration;
use zgx::input::{InputEvent, Key, Modifiers};
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/channels/app.luau");

fn channels(server: &ChannelsServer) -> Harness {
    let server = server.clone();
    server.pace(Duration::from_millis(10));
    let h = Harness::with_desktop(move |runtime| {
        let host = ScriptHost::new(runtime, APP);
        server.serve(&host);
        host
    });
    h.ui.scene.borrow_mut().resize(1100., 760.);
    h.frames(2);
    h
}

fn key(h: &mut Harness, key: Key, shift: bool) {
    let modifiers = Modifiers {
        shift,
        ..Modifiers::default()
    };
    h.ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
}

fn type_in_composer(h: &mut Harness, text: &str) {
    let composer = h.find("composer").unwrap();
    assert!(h.ui.input.focus(&h.ui.scene, Some(composer)));
    h.ui.dispatch(InputEvent::Text(text.into()));
    h.frames(1);
}

#[test]
fn the_welcome_loads_and_a_message_is_sent_and_confirmed() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(
        h.wait_until(|h| h.find("channel-design").is_some()),
        "{:?}",
        h.error()
    );
    assert_eq!(h.text("title"), "# general");
    assert!(h.find("banner").is_none(), "connected: no banner");
    type_in_composer(&mut h, "Hello **team**");
    key(&mut h, Key::Enter, false);
    h.frames(1);
    // Shown at once, pending, then confirmed by the server.
    assert!(h.find("message-l1").is_some());
    std::thread::sleep(Duration::from_millis(40));
    h.settle();
    h.frames(2);
    assert!(
        h.find("message-l1").is_some(),
        "kept as the same row once stored"
    );
    assert!(h.find("failed-l1").is_none());
    assert_eq!(h.error(), None);
}

#[test]
fn a_refused_message_is_sent_again() {
    let server = ChannelsServer::new();
    server.fail(1);
    let mut h = channels(&server);
    assert!(h.wait_until(|h| h.find("channel-general").is_some()));
    type_in_composer(&mut h, "Will this go?");
    key(&mut h, Key::Enter, false);
    assert!(
        h.wait_until(|h| h.find("retry-l1").is_some()),
        "{:?}",
        h.error()
    );
    assert!(h.text("failed-l1").contains("the server is busy"));
    h.click("retry-l1");
    assert!(h.wait_until(|h| h.find("retry-l1").is_none()));
    assert_eq!(h.error(), None);
}

#[test]
fn a_dropped_connection_shows_the_banner_and_reconnects() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(h.wait_until(|_| server.connects() == 1));
    assert!(h.wait_until(|h| h.find("channel-general").is_some()));
    server.drop_all();
    assert!(
        h.wait_until(|h| h.find("banner").is_some()),
        "the banner shows"
    );
    assert!(
        h.text("banner").contains("Reconnecting"),
        "{}",
        h.text("banner")
    );
    assert!(
        h.wait_until(|h| h.find("banner").is_none()),
        "back: no banner"
    );
    assert_eq!(server.connects(), 2);
}

#[test]
fn a_message_elsewhere_counts_unread_until_seen() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(h.wait_until(|h| h.find("channel-design").is_some()));
    server.say("u-ada", "design", "New mockups are up.");
    assert!(h.wait_until(|h| h.find("unread-design").is_some()));
    assert_eq!(h.text("unread-design"), "1");
    h.click("channel-design");
    h.frames(2);
    assert!(h.find("unread-design").is_none());
    assert_eq!(h.text("title"), "# design");
}

#[test]
fn a_mention_opens_at_the_caret_and_is_chosen() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(h.wait_until(|h| h.find("composer").is_some()));
    type_in_composer(&mut h, "Thanks @");
    assert!(h.wait_until(|h| h.find("mention-u-ada").is_some()));
    let composer = h.find("composer").unwrap();
    assert_eq!(
        h.ui.input.focused(),
        Some(composer),
        "focus stays in the composer"
    );
    key(&mut h, Key::ArrowDown, false);
    key(&mut h, Key::Enter, false);
    h.frames(2);
    let text =
        h.ui.editor(composer)
            .unwrap()
            .editor
            .borrow()
            .text()
            .to_string();
    assert!(text.starts_with("Thanks @"), "{text}");
    assert!(text.ends_with(' '), "{text:?}");
    assert!(h.find("mention-u-ada").is_none(), "closed once chosen");
}

#[test]
fn an_earlier_page_loads_as_the_list_reaches_its_start() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(h.wait_until(|h| h.find("channel-general").is_some()));
    let to_top = |h: &mut Harness| {
        h.ui.dispatch(InputEvent::Scroll {
            x: 600.,
            y: 300.,
            delta_x: 0.,
            delta_y: -100_000.,
        });
        h.frames(2);
    };
    // Up to the start, again as each earlier page comes in above, until
    // the channel's first message shows: 90 messages, 30 a page.
    let mut found = false;
    for _ in 0..200 {
        to_top(&mut h);
        std::thread::sleep(Duration::from_millis(5));
        h.settle();
        if h.find("message-1").is_some() {
            found = true;
            break;
        }
    }
    assert!(found, "every earlier page came in");
    assert_eq!(h.error(), None);
}

#[test]
fn a_dropped_file_uploads_and_goes_with_the_message() {
    let server = ChannelsServer::new();
    let mut h = channels(&server);
    assert!(h.wait_until(|h| h.find("channel-general").is_some()));
    let dir = std::env::temp_dir().join(format!("zgx-channels-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, "the notes").unwrap();
    h.ui.dispatch(InputEvent::FileHover {
        x: 500.,
        y: 300.,
        path: file.clone(),
    });
    h.frames(1);
    assert!(
        h.find("drop-hint").is_some(),
        "the hint shows while files hover"
    );
    h.ui.dispatch(InputEvent::FilesDrop {
        x: 500.,
        y: 300.,
        paths: std::sync::Arc::from(vec![file]),
    });
    h.frames(1);
    assert!(h.find("drop-hint").is_none());
    assert!(h.wait_until(|h| h.find("upload-l1").is_some()));
    type_in_composer(&mut h, "Here they are");
    // Uploaded (read off the thread, then four steps of 10 ms), then sent
    // with the message.
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(10));
        h.settle();
        h.frames(1);
    }
    key(&mut h, Key::Enter, false);
    h.frames(2);
    assert!(
        h.find("upload-l1").is_none(),
        "the composer is cleared of it"
    );
    assert!(h.wait_until(|h| h.find("message-l2").is_some()));
    assert!(
        h.texts().iter().any(|text| text.contains("📎 notes.txt")),
        "the file goes with the message"
    );
}
