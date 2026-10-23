//! The mail example (examples/mail), run headless: opening a message,
//! searching, composing, the settings window's appearance, and how it
//! folds up in narrower windows.
mod common;

use common::Harness;
use std::time::Instant;
use zgui::{scene::NodeKind, widgets::Ui};
use zgui_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/mail/app.luau");

/// The app in a window `width` wide, laid out.
fn mail(width: f32) -> Harness {
    let h = Harness::new(|runtime| ScriptHost::new(runtime, APP));
    h.ui.scene.borrow_mut().resize(width, 760.);
    h.frames(2);
    h
}
/// A click at the middle of the text `label` shows in `ui`.
fn click_text(ui: &mut Ui, label: &str) {
    let at = {
        let scene = ui.scene.borrow();
        let found = scene
            .paint_items()
            .find(|i| matches!(scene.kind(i.id), NodeKind::Text { text, .. } if &**text == label))
            .map(|i| scene.bounds(i.id))
            .unwrap_or_else(|| panic!("no {label:?}"));
        (found.x + found.width / 2., found.y + found.height / 2.)
    };
    use zgui::input::{InputEvent, PointerButton};
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown {
        x: at.0,
        y: at.1,
        button,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: at.0,
        y: at.1,
        button,
    });
}

fn texts(ui: &Ui) -> Vec<String> {
    let scene = ui.scene.borrow();
    scene
        .paint_items()
        .filter_map(|item| match scene.kind(item.id) {
            NodeKind::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_mail_example_opens_searches_composes_and_chooses_its_appearance() {
    let start = Instant::now();
    let mut h = mail(1180.);
    let loaded = start.elapsed();
    assert!(loaded.as_secs_f64() < 2., "loads in {loaded:?}");
    // A row, then its message.
    h.click("row-1");
    h.frames(1);
    let first = h.text("subject");
    assert!(!first.is_empty(), "the message opens");
    // Down opens the next one, as a mail app's list does.
    let before = h.texts();
    h.key(zgui::input::Key::ArrowDown);
    h.frames(1);
    assert_ne!(h.texts(), before, "another message shows");
    h.key(zgui::input::Key::ArrowUp);
    h.frames(1);
    assert_eq!(h.text("subject"), first, "and Up goes back");
    // Searching narrows the list.
    h.type_text("search", "invoice");
    h.frames(1);
    let shown: Vec<String> = (1..=3)
        .filter_map(|i| {
            h.find(&format!("row-{i}"))
                .map(|_| h.text(&format!("row-{i}")))
        })
        .collect();
    assert!(!shown.is_empty());
    // ⌘N composes; the body is spellchecked (a password field is not).
    assert!(h.host.run_command("new"));
    h.frames(1);
    assert!(h.find("body").is_some(), "the compose sheet opens");
    // ⌘, opens the settings window. The appearance follows the system
    // until a choice there sets it, in every window.
    assert!(h.host.run_command("settings"));
    h.frames(1);
    let windows = h.host.windows().get();
    assert_eq!(windows.len(), 1);
    assert!(windows[0].open.get().truthy());
    let mut settings = Ui::with_runtime(h.ui.runtime.clone(), 420., 300.);
    settings.mount(h.host.window_view("settings"));
    settings.prepare_frame();
    assert!(texts(&settings).contains(&"Appearance".to_owned()));
    for (choice, theme) in [("Light", "zgui-light"), ("Dark", "zgui-dark")] {
        click_text(&mut settings, choice);
        h.frames(1);
        assert_eq!(&*h.ui.theme().current(), theme, "{choice}");
    }
}

#[test]
fn narrower_windows_fold_the_mailboxes_then_show_one_pane() {
    // Wide: the mailboxes beside the list and the message.
    let mut h = mail(1180.);
    assert!(h.texts().contains(&"Inbox".to_owned()));
    // Medium: the mailboxes in a drawer, opened from the list's header and
    // closed by choosing one.
    h.ui.scene.borrow_mut().resize(800., 760.);
    h.frames(1);
    assert!(!h.texts().contains(&"Inbox".to_owned()));
    click_text(&mut h.ui, "☰");
    h.frames(1);
    click_text(&mut h.ui, "Starred");
    h.frames(1);
    assert!(
        !h.texts().contains(&"Starred".to_owned()),
        "the drawer closes"
    );
    click_text(&mut h.ui, "☰");
    h.frames(1);
    click_text(&mut h.ui, "Inbox");
    h.frames(1);
    // The message pane is what is left, its text wrapped within it.
    h.click("row-1");
    h.frames(1);
    let subject = h.bounds("subject");
    assert!(subject.x + subject.width <= 800., "{subject:?}");
    // Narrow: the list alone, then the message alone, with a way back.
    h.ui.scene.borrow_mut().resize(420., 760.);
    h.frames(1);
    assert!(h.find("subject").is_some(), "the open message fills it");
    assert!(h.find("search").is_none());
    click_text(&mut h.ui, "‹ Back");
    h.frames(1);
    assert!(h.find("subject").is_none());
    assert!(h.find("search").is_some());
    let search = h.bounds("search");
    assert!(search.x + search.width <= 420., "{search:?}");
}

#[test]
#[ignore = "a timing probe: cargo test --release -- --ignored --nocapture"]
fn how_long_opening_a_message_takes() {
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, APP));
    h.frames(2);
    let start = Instant::now();
    for i in 0..10 {
        h.click(&format!("row-{}", i % 2 + 1));
        h.frames(1);
    }
    eprintln!("open: {:?} per message", start.elapsed() / 10);
    let start = Instant::now();
    for i in 0..10 {
        h.type_text("search", if i % 2 == 0 { "r" } else { "e" });
        h.frames(1);
    }
    eprintln!("search keystroke: {:?}", start.elapsed() / 10);
}

/// What has keyboard focus, as assistive technology names it.
fn focus_name(h: &Harness) -> String {
    let id = h.ui.input.focused().expect("something has focus");
    let semantics = h.ui.semantics.borrow();
    let node = semantics.get(id).expect("focus has semantics");
    let label: String = node.label.chars().take(24).collect();
    format!("{:?} {label}", node.role)
}

#[test]
fn the_keyboard_alone_reaches_every_part_once() {
    use zgui::input::Key;
    let mut h = mail(1180.);
    h.click("row-1");
    h.frames(1);
    h.ui.input.focus(&h.ui.scene, None);
    let mut stops = Vec::new();
    for _ in 0..5 {
        h.key(Key::Tab);
        h.frames(1);
        stops.push(focus_name(&h));
    }
    // A list box, the message list and the toolbar are one stop each: the
    // selected mailbox, the open message, the toolbar's first action.
    let expected = [
        "Button New Message",
        "ListBoxOption Inbox 426",
        "TextInput Search",
        "ListBoxOption Donald Knuth, Quarterly ",
    ];
    assert_eq!(stops[..4], expected.map(str::to_owned), "{stops:#?}");
    assert!(stops[4].starts_with("Button Reply"), "{stops:#?}");
    // Right moves along the toolbar.
    h.key(Key::ArrowRight);
    h.frames(1);
    assert!(
        focus_name(&h).starts_with("Button Star"),
        "{}",
        focus_name(&h)
    );
    // Back in the list, Down opens the next message as it moves.
    h.key_with(
        Key::Tab,
        zgui::input::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    h.frames(1);
    let subject = h.text("subject");
    h.key(Key::ArrowDown);
    h.frames(1);
    assert!(
        focus_name(&h).starts_with("ListBoxOption Margaret"),
        "{}",
        focus_name(&h)
    );
    assert_ne!(h.text("subject"), subject, "the focused message opens");
    // In the mailboxes, Down shows the next mailbox.
    for _ in 0..2 {
        h.key_with(
            Key::Tab,
            zgui::input::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        h.frames(1);
    }
    assert!(
        focus_name(&h).starts_with("ListBoxOption Inbox"),
        "{}",
        focus_name(&h)
    );
    h.key(Key::ArrowDown);
    h.frames(1);
    assert!(
        focus_name(&h).starts_with("ListBoxOption Starred"),
        "{}",
        focus_name(&h)
    );
}

#[test]
fn the_compose_sheet_holds_focus_until_escape_returns_it() {
    use zgui::input::Key;
    let mut h = mail(1180.);
    h.click("row-1");
    h.frames(1);
    let before = h.ui.input.focused();
    assert!(h.host.run_command("new"));
    h.frames(2);
    // Tab goes round the sheet's fields and buttons, and nowhere else.
    let body = h.find("body").unwrap();
    let mut seen = Vec::new();
    for _ in 0..8 {
        h.key(Key::Tab);
        h.frames(1);
        let id = h.ui.input.focused().expect("focus stays in the sheet");
        let scene = h.ui.scene.borrow();
        let sheet = scene.ancestors(body).nth(1).unwrap();
        assert!(scene.is_within(id, sheet), "{}", focus_name(&h));
        drop(scene);
        seen.push(focus_name(&h));
    }
    assert!(
        seen.iter().any(|s| s.starts_with("TextInput To")),
        "{seen:#?}"
    );
    assert!(
        seen.iter().any(|s| s.starts_with("Button Send")),
        "{seen:#?}"
    );
    assert!(h.ui.input.focus_scope().is_some());
    h.key(Key::Escape);
    h.frames(2);
    assert!(
        h.ui.input.focus_scope().is_none(),
        "Escape closes it: {seen:#?}"
    );
    assert_eq!(h.ui.input.focused(), before, "and focus goes back");
}

#[test]
fn reading_a_message_costs_what_changed() {
    // Opening one marks it read: a key of its own and its mailbox's count,
    // not the 4000 messages (whose list and filter ran again, over CI's
    // 50 ms budget). A tighter budget than the default holds it.
    use zgui::input::Key;
    let limits = zgui_script::Limits {
        call: std::time::Duration::from_millis(15),
        ..Default::default()
    };
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, APP).limits(limits));
    h.ui.scene.borrow_mut().resize(1180., 760.);
    h.frames(2);
    let inbox = |h: &mut Harness| {
        h.texts()
            .iter()
            .skip_while(|t| t.as_str() != "Inbox")
            .nth(1)
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let before = inbox(&mut h);
    h.click("row-3");
    h.frames(1);
    for _ in 0..6 {
        h.key(Key::ArrowDown);
        h.frames(1);
    }
    assert_eq!(h.error(), None);
    let read = before - inbox(&mut h);
    assert!(
        (1..=7).contains(&read),
        "{before} -> {} unread",
        before - read
    );
}

#[test]
fn what_an_action_did_is_said_in_a_live_status() {
    let mut h = mail(1180.);
    let status = h.find("status").expect("the status line");
    assert_eq!(
        h.ui.semantics.borrow().get(status).map(|n| n.role),
        Some(zgui::semantics::Role::Status),
        "politely live, as a status is"
    );
    // The text in it, which is what a screen reader announces.
    let said = |h: &Harness| {
        let scene = h.ui.scene.borrow();
        let semantics = h.ui.semantics.borrow();
        let mut pending = vec![status];
        let mut text = String::new();
        while let Some(id) = pending.pop() {
            if let Some(node) = semantics
                .get(id)
                .filter(|n| n.role == zgui::semantics::Role::Label)
            {
                text.push_str(node.value.as_deref().unwrap_or(&node.label));
            }
            pending.extend(scene.children(id).iter().rev().copied());
        }
        text
    };
    assert!(h.host.run_command("receive"));
    h.frames(1);
    assert_eq!(
        said(&h),
        "New message from Grace Hopper: The nanosecond, again"
    );
    assert!(h.texts().contains(&"The nanosecond, again".to_owned()));
    h.click("row-1");
    h.frames(1);
    assert!(h.host.run_command("new"));
    h.frames(1);
    h.click("send");
    h.frames(1);
    assert_eq!(said(&h), "Message sent");
}

#[test]
fn rows_have_one_name_and_the_body_is_a_document() {
    use zgui::semantics::Role;
    let mut h = mail(1180.);
    let row = h.find("row-3").unwrap();
    let name = h.ui.semantics.borrow().get(row).unwrap().label.clone();
    assert!(name.starts_with("Unread, "), "{name}");
    assert!(!name.contains('●'), "{name}");
    h.click("row-3");
    h.frames(1);
    let name = h.ui.semantics.borrow().get(row).unwrap().label.clone();
    assert!(!name.starts_with("Unread"), "read now: {name}");
    let documents = {
        let semantics = h.ui.semantics.borrow();
        let scene = h.ui.scene.borrow();
        let mut found = Vec::new();
        let mut pending = vec![scene.root()];
        while let Some(id) = pending.pop() {
            if let Some(node) = semantics.get(id).filter(|n| n.role == Role::Document) {
                found.push(node.label.clone());
            }
            pending.extend(scene.children(id).iter().copied());
        }
        found
    };
    assert_eq!(documents, ["Message body"]);
}
