use std::time::Duration;
use zgx::{
    caret_blink::CaretBlink,
    compose::prelude::*,
    scene::{NodeId, NodeKind},
    widgets::{EditorHandle, Ui},
};

const BLINK: CaretBlink = CaretBlink {
    on: Duration::from_millis(500),
    off: Duration::from_millis(400),
    timeout: Some(Duration::from_secs(5)),
};

fn focused_editor(ui: &mut Ui) -> EditorHandle {
    let value = ui.signal(String::from("blink"));
    let mounted = ui.mount(column().child(text_input("Name", value).id("name").size(200., 30.)));
    ui.prepare_frame();
    let node = mounted.find("name").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    ui.prepare_frame();
    ui.focused_editor().unwrap()
}

fn alpha(ui: &Ui, caret: NodeId) -> u8 {
    match ui.scene.borrow().kind(caret) {
        NodeKind::Rect(color) => color.3,
        other => panic!("the caret is {other:?}"),
    }
}

#[test]
fn a_focused_caret_blinks_at_the_set_rate_damaging_only_itself() {
    let mut ui = Ui::new(400., 200.);
    let editor = focused_editor(&mut ui);
    assert_eq!(
        ui.next_interaction_deadline(),
        None,
        "solid, waking for nothing, until the host sets a blink"
    );
    ui.set_caret_blink(Some(BLINK));
    let first = ui.next_interaction_deadline().expect("a blink");
    ui.scene.borrow_mut().flush();
    assert!(ui.advance_interactions(first).unwrap());
    assert!(ui.caret_blinked_off());
    assert_eq!(alpha(&ui, editor.caret), 0);
    // One small repaint: the caret's rectangle, nothing laid out.
    let report = ui.scene.borrow_mut().flush();
    let caret = ui.scene.borrow().bounds(editor.caret);
    assert_eq!(report.layout_nodes, 0);
    assert!(!report.damage.is_empty());
    for damage in &report.damage {
        assert!(
            damage.x >= caret.x - 1.
                && damage.y >= caret.y - 1.
                && damage.x + damage.width <= caret.x + caret.width + 1.
                && damage.y + damage.height <= caret.y + caret.height + 1.,
            "{damage:?} past the caret {caret:?}"
        );
    }
    // Hidden for `off`, then shown for `on`.
    let second = ui.next_interaction_deadline().unwrap();
    assert_eq!(second - first, BLINK.off);
    assert!(ui.advance_interactions(second).unwrap());
    assert!(!ui.caret_blinked_off());
    assert!(alpha(&ui, editor.caret) > 0);
    assert_eq!(ui.next_interaction_deadline().unwrap() - second, BLINK.on);
}

#[test]
fn typing_shows_the_caret_solid_and_starts_its_blink_again() {
    let mut ui = Ui::new(400., 200.);
    let editor = focused_editor(&mut ui);
    ui.set_caret_blink(Some(BLINK));
    let first = ui.next_interaction_deadline().unwrap();
    ui.advance_interactions(first).unwrap();
    assert!(ui.caret_blinked_off());
    let typed = std::time::Instant::now();
    editor.editor.borrow_mut().insert("s");
    editor.refresh();
    ui.prepare_frame();
    assert!(!ui.caret_blinked_off());
    assert!(alpha(&ui, editor.caret) > 0);
    let next = ui.next_interaction_deadline().unwrap();
    assert!(next >= typed + BLINK.on, "a full phase shown after typing");
}

#[test]
fn the_caret_stops_solid_at_the_timeout_and_then_wakes_for_nothing() {
    let mut ui = Ui::new(400., 200.);
    let editor = focused_editor(&mut ui);
    let started = std::time::Instant::now();
    ui.set_caret_blink(Some(CaretBlink {
        on: Duration::from_millis(300),
        off: Duration::from_millis(200),
        timeout: Some(Duration::from_secs(2)),
    }));
    let mut blinks = 0;
    let mut last = started;
    while let Some(deadline) = ui.next_interaction_deadline() {
        assert!(blinks < 20, "still blinking past its timeout");
        ui.advance_interactions(deadline).unwrap();
        blinks += 1;
        last = deadline;
    }
    assert!(last >= started + Duration::from_secs(2));
    assert!(blinks >= 7, "{blinks} blinks in 2 s at 300 + 200 ms");
    assert!(!ui.caret_blinked_off());
    assert!(alpha(&ui, editor.caret) > 0);
}

#[test]
fn no_blink_while_the_window_is_not_presented_or_the_editor_blurred() {
    let mut ui = Ui::new(400., 200.);
    let _editor = focused_editor(&mut ui);
    ui.set_caret_blink(Some(BLINK));
    assert!(ui.next_interaction_deadline().is_some());
    ui.set_presented(false);
    assert_eq!(ui.next_interaction_deadline(), None, "hidden or occluded");
    ui.set_presented(true);
    assert!(ui.next_interaction_deadline().is_some());
    // The focus gone (its window deactivated, or another view took it).
    ui.input.focus(&ui.scene, None);
    ui.prepare_frame();
    assert_eq!(ui.next_interaction_deadline(), None);
}

#[test]
fn turning_the_blink_off_shows_a_hidden_caret() {
    let mut ui = Ui::new(400., 200.);
    let editor = focused_editor(&mut ui);
    ui.set_caret_blink(Some(BLINK));
    let first = ui.next_interaction_deadline().unwrap();
    ui.advance_interactions(first).unwrap();
    assert_eq!(alpha(&ui, editor.caret), 0);
    ui.set_caret_blink(None);
    assert!(alpha(&ui, editor.caret) > 0);
    assert_eq!(ui.next_interaction_deadline(), None);
}

/// Every node's box, in document order.
fn boxes(ui: &Ui) -> Vec<(NodeId, zgx::scene::Rect)> {
    let scene = ui.scene.borrow();
    let mut out = Vec::new();
    let mut stack = vec![scene.root()];
    while let Some(node) = stack.pop() {
        out.push((node, scene.bounds(node)));
        stack.extend(scene.children(node).iter().rev().copied());
    }
    out
}

/// A blinking caret never moves anything, by so much as a pixel: through
/// many blinks of a field, an area and a field sized by its text, beside
/// neighbours that would shift if any of them did, every box stays where it
/// was, nothing is laid out, and each blink damages the caret alone.
#[test]
fn a_blinking_caret_never_moves_a_box_or_damages_past_itself() {
    for multiline in [false, true] {
        for sized_by_text in [false, true] {
            let mut ui = Ui::new(500., 300.);
            let value = ui.signal(String::from("blink, and nothing moves"));
            let editor = if multiline {
                text_area("Notes", value.clone())
            } else {
                text_input("Name", value.clone())
            }
            .id("field");
            let editor = if sized_by_text {
                editor.h(60.)
            } else {
                editor.size(200., 60.)
            };
            let mounted = ui.mount(
                column().child(
                    row()
                        .child(text("before").id("before"))
                        .child(editor)
                        .child(text("after").id("after")),
                ),
            );
            ui.prepare_frame();
            let node = mounted.find("field").unwrap();
            ui.input.focus(&ui.scene, Some(node));
            ui.prepare_frame();
            let caret = ui.focused_editor().unwrap().caret;
            ui.set_caret_blink(Some(BLINK));
            ui.scene.borrow_mut().flush();
            let before = boxes(&ui);
            let case = format!("multiline {multiline}, sized by text {sized_by_text}");
            for blink in 0..6 {
                let at = ui.next_interaction_deadline().expect("blinking");
                assert!(ui.advance_interactions(at).unwrap());
                ui.prepare_frame();
                let report = ui.scene.borrow_mut().flush();
                assert_eq!(report.layout_nodes, 0, "{case}, blink {blink}: laid out");
                assert_eq!(boxes(&ui), before, "{case}, blink {blink}: a box moved");
                let shown = ui.scene.borrow().bounds(caret);
                for damage in &report.damage {
                    assert!(
                        damage.x >= shown.x - 1.
                            && damage.y >= shown.y - 1.
                            && damage.x + damage.width <= shown.x + shown.width + 1.
                            && damage.y + damage.height <= shown.y + shown.height + 1.,
                        "{case}, blink {blink}: {damage:?} past the caret {shown:?}"
                    );
                }
            }
        }
    }
}
