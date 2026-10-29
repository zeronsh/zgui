//! The inbox example (examples/inbox), run headless against its in-memory
//! API: routing with back and forward and a deep link, debounced search
//! with its states, pages loaded as the list scrolls, folders reordered by
//! dragging, and the settings modal route.
mod common;

#[path = "../examples/support/inbox_server.rs"]
mod inbox_server;

use common::Harness;
use inbox_server::InboxServer;
use std::time::Duration;
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/inbox/app.luau");

fn inbox(server: &InboxServer) -> Harness {
    let server = server.clone();
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, APP);
        server.serve(&host, Duration::from_millis(5));
        host
    });
    h.ui.scene.borrow_mut().resize(900., 700.);
    h.frames(2);
    h
}
fn ready(h: &mut zgx_script::testing::TestApp) -> bool {
    h.find("message-1").is_some()
}

#[test]
fn the_list_loads_and_a_thread_opens_and_goes_back() {
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert_eq!(h.error(), None);
    assert!(h.wait_until(ready));
    h.click("message-1");
    assert!(h.wait_until(|h| h.find("thread-subject").is_some()));
    assert!(h.wait_until(|h| h.text("thread-subject").contains("#1")), "{}", h.text("thread-subject"));
    h.click("back");
    assert!(h.wait_until(ready));
    h.click("forward");
    assert!(h.wait_until(|h| h.find("thread-subject").is_some()));
}

#[test]
fn a_deep_link_opens_its_thread() {
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert!(h.wait_until(ready));
    h.host.open(Vec::new(), vec!["mail://thread/42".into()]);
    assert!(h.wait_until(|h| h.find("thread-subject").is_some() && h.text("thread-subject").contains("#42")));
}

#[test]
fn search_waits_for_typing_to_pause() {
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert!(h.wait_until(ready));
    h.type_text("search", "lun");
    h.type_text("search", "ch");
    assert!(h.wait_until(|_| server.asked().iter().any(|a| a.contains("q=lunch"))));
    let asked = server.asked();
    assert!(!asked.iter().any(|a| a.contains("q=lun&")), "nothing sent mid-word: {asked:?}");
    h.fill("search", "zzzz");
    assert!(h.wait_until(|h| h.find("empty").is_some()));
}

#[test]
fn scrolling_down_loads_the_next_page() {
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert!(h.wait_until(ready));
    let list = h.bounds("messages");
    for _ in 0..20 {
        h.ui.dispatch(zgx::input::InputEvent::Scroll {
            x: list.x + 20.,
            y: list.y + 20.,
            delta_x: 0.,
            delta_y: 400.,
        });
        h.frames(1);
    }
    assert!(h.wait_until(|_| server.asked().iter().any(|a| a.contains("cursor=40"))), "{:?}", server.asked());
}

#[test]
fn settings_is_a_modal_route() {
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert!(h.wait_until(ready));
    h.click("open-settings");
    assert!(h.wait_until(|h| h.find("close-settings").is_some() && h.shown("close-settings")));
    h.click("close-settings");
    assert!(h.wait_until(|h| !h.shown("close-settings")), "closed");
    assert!(ready(&mut h), "back on the list");
}

#[test]
fn a_folder_dragged_onto_another_takes_its_place() {
    use zgx::input::{InputEvent, PointerButton};
    let server = InboxServer::new();
    let mut h = inbox(&server);
    assert!(h.wait_until(|h| h.find("folder-Receipts").is_some()));
    // Past the first frame of their fade-in: a node at opacity 0 takes no
    // pointer (unlike CSS), from its first visible frame it does.
    std::thread::sleep(Duration::from_millis(120));
    h.frames(2);
    let (from, to) = (h.bounds("folder-Receipts"), h.bounds("folder-Inbox"));
    h.ui.dispatch(InputEvent::PointerDown { x: from.x + 10., y: from.y + 5., button: PointerButton::Primary });
    h.ui.dispatch(InputEvent::PointerMove { x: from.x + 30., y: from.y + 15. });
    h.ui.dispatch(InputEvent::PointerMove { x: to.x + 10., y: to.y + 5. });
    h.ui.dispatch(InputEvent::PointerUp { x: to.x + 10., y: to.y + 5., button: PointerButton::Primary });
    h.settle();
    assert_eq!(h.error(), None);
    let moved = h.wait_until(|_| server.folders().first().map(String::as_str) == Some("Receipts"));
    assert!(moved, "{:?} asked {:?} texts {:?} err {:?}", server.folders(), server.asked(), h.texts(), h.error());
}
