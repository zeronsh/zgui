//! The caret a host draws itself (`Ui::set_native_caret`): only where the
//! editor's would show exactly, nothing over it, marking a bare insertion
//! point; the editor's painting nothing and waking for nothing meanwhile.
use std::time::Duration;
use zgx::{
    caret_blink::{CaretBlink, NativeCaret},
    compose::prelude::*,
    scene::{NodeId, Rect},
    widgets::{EditorHandle, Ui},
};

/// macOS's: no timeout.
const BLINK: CaretBlink = CaretBlink {
    on: Duration::from_millis(650),
    off: Duration::from_millis(350),
    timeout: None,
};

fn native_ui() -> Ui {
    let ui = Ui::new(400., 300.);
    ui.set_caret_blink(Some(BLINK));
    ui.set_native_caret(true);
    ui
}

fn focus(ui: &mut Ui, mounted: &zgx::compose::ViewHandle, id: &str) -> EditorHandle {
    ui.prepare_frame();
    let node = mounted.find(id).unwrap();
    ui.input.focus(&ui.scene, Some(node));
    ui.prepare_frame();
    ui.focused_editor().unwrap()
}

fn field(ui: &mut Ui) -> EditorHandle {
    let value = ui.signal(String::from("native"));
    let mounted = ui.mount(
        column()
            .p(20.)
            .child(text_input("Name", value).id("name").size(200., 30.)),
    );
    focus(ui, &mounted, "name")
}

fn opacity(ui: &Ui, caret: NodeId) -> f32 {
    ui.scene.borrow().effects(caret).opacity
}

fn drawn(ui: &Ui) -> Option<NativeCaret> {
    ui.native_caret().1
}

/// Where the editor's caret shows: its box, cut by its viewport's clip.
fn shows(ui: &Ui, caret: NodeId) -> Rect {
    let scene = ui.scene.borrow();
    let viewport = scene.parent(caret).unwrap();
    scene
        .bounds(caret)
        .intersection(scene.visible_bounds(viewport).unwrap())
        .unwrap()
}

#[test]
fn a_plain_focused_caret_is_the_hosts_where_the_editors_would_show() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    let caret = drawn(&ui).expect("the host draws it");
    assert_eq!(caret.rect, shows(&ui, editor.caret));
    assert!(caret.rect.width > 0. && caret.rect.height > 0.);
    assert_eq!((caret.on, caret.off), (BLINK.on, BLINK.off));
    assert_eq!(
        opacity(&ui, editor.caret),
        0.,
        "the editor's paints nothing"
    );
    assert_eq!(
        ui.next_interaction_deadline(),
        None,
        "the host blinks it: no wake"
    );
    assert!(!ui.caret_blinked_off(), "shown at first");
    // Nothing to draw while it blinks.
    ui.scene.borrow_mut().flush();
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().damage.is_empty());
}

#[test]
fn without_a_host_that_draws_it_the_caret_blinks_through_frames_as_before() {
    let mut ui = Ui::new(400., 300.);
    ui.set_caret_blink(Some(BLINK));
    let editor = field(&mut ui);
    assert_eq!(drawn(&ui), None);
    assert_eq!(opacity(&ui, editor.caret), 1.);
    assert!(ui.next_interaction_deadline().is_some());
}

#[test]
fn moving_the_caret_moves_the_hosts_and_paints_nothing() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    let (serial, before) = ui.native_caret();
    ui.scene.borrow_mut().flush();
    editor.editor.borrow_mut().set_selection(2, 2);
    editor.refresh();
    ui.prepare_frame();
    let (moved, after) = ui.native_caret();
    let after = after.expect("still the host's");
    assert_ne!(moved, serial, "a change for the host to apply");
    assert!(
        after.rect.x < before.unwrap().rect.x,
        "moved left, to after \"na\""
    );
    assert_eq!(after.rect, shows(&ui, editor.caret));
    assert!(
        after.since >= before.unwrap().since,
        "its blink starts again"
    );
    let report = ui.scene.borrow_mut().flush();
    assert!(
        report.damage.is_empty(),
        "no frame for a moved caret: {:?}",
        report.damage
    );
}

#[test]
fn typing_restarts_the_hosts_blink() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    let before = drawn(&ui).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    editor.editor.borrow_mut().insert("s");
    editor.refresh();
    ui.prepare_frame();
    let after = drawn(&ui).unwrap();
    assert!(after.since > before.since);
    assert!(after.rect.x > before.rect.x);
    assert_eq!(opacity(&ui, editor.caret), 0.);
}

#[test]
fn a_selection_gives_the_caret_back_to_the_editor_and_it_blinks_as_before() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    editor.editor.borrow_mut().set_selection(0, 3);
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None);
    // Collapsed again: the host's once more.
    editor.editor.borrow_mut().set_selection(3, 3);
    editor.refresh();
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
    assert_eq!(opacity(&ui, editor.caret), 0.);
}

#[test]
fn composing_gives_the_caret_back_to_the_editor() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    editor.editor.borrow_mut().set_preedit("ka", Some((2, 2)));
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None);
    // The marked text's caret, if it shows, is the editor's.
    let shows = opacity(&ui, editor.caret);
    assert!(shows == 1. || shows == 0., "{shows}");
    editor.editor.borrow_mut().set_preedit("", None);
    editor.refresh();
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
}

#[test]
fn losing_the_focus_or_the_window_ends_the_hosts_caret() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    assert!(drawn(&ui).is_some());
    ui.set_presented(false);
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None, "a window not presented shows none");
    ui.set_presented(true);
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
    ui.input.focus(&ui.scene, None);
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None);
    assert_eq!(opacity(&ui, editor.caret), 0., "blurred: no caret at all");
    assert_eq!(ui.next_interaction_deadline(), None);
}

#[test]
fn turning_the_host_off_gives_back_a_shown_blinking_caret() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    ui.set_native_caret(false);
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None);
    assert_eq!(opacity(&ui, editor.caret), 1.);
    assert!(
        ui.next_interaction_deadline().is_some(),
        "blinking through frames"
    );
}

#[test]
fn a_caret_that_stops_after_a_timeout_stays_the_editors() {
    let mut ui = Ui::new(400., 300.);
    ui.set_caret_blink(Some(CaretBlink {
        timeout: Some(Duration::from_secs(5)),
        ..BLINK
    }));
    ui.set_native_caret(true);
    let editor = field(&mut ui);
    assert_eq!(drawn(&ui), None);
    assert_eq!(opacity(&ui, editor.caret), 1.);
}

#[test]
fn anything_painted_over_the_caret_keeps_it_the_editors() {
    let mut ui = native_ui();
    let value = ui.signal(String::from("covered"));
    let cover = ui.signal(false);
    let shown = cover.clone();
    let mounted = ui.mount(
        overlay()
            .size(400., 300.)
            .child(
                column()
                    .p(20.)
                    .child(text_input("Name", value).id("name").size(200., 30.)),
            )
            .child(
                div()
                    .absolute()
                    .size(400., 300.)
                    .bg(rgba(0x00000080))
                    .reactive_style(move || {
                        Styles::new().opacity(if shown.get() { 1. } else { 0. })
                    }),
            ),
    );
    let editor = focus(&mut ui, &mounted, "name");
    assert!(drawn(&ui).is_some(), "a transparent cover paints nothing");
    cover.set(true);
    ui.prepare_frame();
    assert_eq!(drawn(&ui), None, "a scrim over the field");
    assert_eq!(opacity(&ui, editor.caret), 1.);
    cover.set(false);
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
}

#[test]
fn a_scaled_faded_rotated_or_zoomed_ancestor_keeps_it_the_editors() {
    type Wrap = fn(View) -> View;
    let wrap: [(&str, Wrap); 4] = [
        ("scaled", |field| div().scale(1.5).child(field)),
        ("faded", |field| div().opacity(0.5).child(field)),
        ("rotated", |field| div().rotate(0.17).child(field)),
        ("zoomed", |field| div().zoom(1.5).child(field)),
    ];
    for (name, wrap) in wrap {
        let mut ui = native_ui();
        let value = ui.signal(String::from("styled"));
        let mounted = ui.mount(
            column()
                .p(20.)
                .child(wrap(text_input("Name", value).id("name").size(200., 30.))),
        );
        let editor = focus(&mut ui, &mounted, "name");
        assert_eq!(drawn(&ui), None, "{name}");
        assert_eq!(opacity(&ui, editor.caret), 1., "{name}");
    }
}

#[test]
fn a_caret_partly_out_of_its_clip_is_drawn_as_far_as_the_editors_would_show() {
    let mut ui = native_ui();
    let value = ui.signal(String::from("clipped"));
    let mounted = ui.mount(
        column().p(20.).child(
            div().size(200., 12.).overflow_hidden().child(
                text_input("Name", value)
                    .id("name")
                    .size(200., 30.)
                    .shrink_0(),
            ),
        ),
    );
    let editor = focus(&mut ui, &mounted, "name");
    let caret = ui.scene.borrow().bounds(editor.caret);
    let shown = drawn(&ui).expect("the part that shows").rect;
    assert!(
        shown.y >= 20. && shown.y + shown.height <= 32. + 0.001,
        "{shown:?} within the 12-point clip"
    );
    assert!(shown.height < caret.height, "cut: {shown:?} of {caret:?}");
    assert_eq!((shown.x, shown.width), (caret.x, caret.width));
    assert_eq!(opacity(&ui, editor.caret), 0.);
}

#[test]
fn a_caret_scrolled_out_of_sight_draws_nothing() {
    let mut ui = native_ui();
    let value = ui.signal(String::from("gone"));
    let mounted = ui.mount(
        column().p(20.).child(
            div()
                .size(200., 30.)
                .overflow_hidden()
                .child(div().h(80.).shrink_0())
                .child(
                    text_input("Name", value)
                        .id("name")
                        .size(200., 30.)
                        .shrink_0(),
                ),
        ),
    );
    let _editor = focus(&mut ui, &mounted, "name");
    assert_eq!(drawn(&ui), None, "nothing of it shows");
}

#[test]
fn right_to_left_text_puts_the_hosts_caret_where_the_editors_is() {
    let mut ui = native_ui();
    let value = ui.signal(String::from("abc אבג def"));
    let mounted = ui.mount(
        column()
            .p(20.)
            .child(text_input("Name", value).id("name").size(240., 30.)),
    );
    let editor = focus(&mut ui, &mounted, "name");
    for at in [0, 4, 6, 8, 12] {
        editor.editor.borrow_mut().set_selection(at, at);
        editor.refresh();
        ui.prepare_frame();
        let caret = drawn(&ui).unwrap_or_else(|| panic!("at {at}"));
        assert_eq!(caret.rect, shows(&ui, editor.caret), "at {at}");
    }
}

#[test]
fn the_hosts_caret_takes_the_editors_colour_as_it_changes() {
    let mut ui = native_ui();
    let value = ui.signal(String::from("colour"));
    let ink = ui.signal(0xe5edf7_u32);
    let shade = ink.clone();
    let mounted = ui.mount(
        column().p(20.).child(
            text_input("Name", value)
                .id("name")
                .size(200., 30.)
                .reactive_style(move || Styles::new().text_color(rgb(shade.get()))),
        ),
    );
    let editor = focus(&mut ui, &mounted, "name");
    let painted = |ui: &Ui| match ui.scene.borrow().kind(editor.caret) {
        zgx::scene::NodeKind::Rect(color) => *color,
        other => panic!("the caret is {other:?}"),
    };
    assert_eq!(drawn(&ui).unwrap().color, painted(&ui));
    ink.set(0xff3366);
    ui.prepare_frame();
    assert_eq!(drawn(&ui).expect("still the host's").color, painted(&ui));
}

#[test]
fn the_hosts_caret_is_in_window_points_whatever_the_zoom() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    ui.set_zoom(2.);
    ui.prepare_frame();
    let caret = drawn(&ui).expect("zoomed, still plain");
    assert_eq!(caret.rect, shows(&ui, editor.caret));
    let _: Rect = caret.rect;
}

#[test]
fn taking_the_caret_over_repaints_where_the_editors_showed() {
    let mut ui = Ui::new(400., 300.);
    ui.set_caret_blink(Some(BLINK));
    ui.set_native_caret(true);
    // Not shown yet (a window opening): the editor paints its caret.
    ui.set_presented(false);
    let editor = field(&mut ui);
    assert_eq!(drawn(&ui), None);
    assert_eq!(opacity(&ui, editor.caret), 1.);
    ui.scene.borrow_mut().flush();
    ui.set_presented(true);
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
    let caret = shows(&ui, editor.caret);
    let damage = ui.scene.borrow_mut().flush().damage;
    assert!(
        damage.iter().any(|d| d.intersection(caret).is_some()),
        "the editor's caret painted over: {damage:?} misses {caret:?}"
    );
}

#[test]
fn blurred_and_focused_again_between_frames_the_editors_caret_stays_unpainted() {
    let mut ui = native_ui();
    let editor = field(&mut ui);
    assert!(drawn(&ui).is_some());
    // Its window deactivated and activated again before the next frame:
    // the editor draws its caret hidden, then shown, with no frame between.
    let node = ui.input.focused().unwrap();
    ui.input.focus(&ui.scene, None);
    editor.refresh();
    ui.input.focus(&ui.scene, Some(node));
    editor.refresh();
    ui.prepare_frame();
    assert!(drawn(&ui).is_some());
    assert_eq!(opacity(&ui, editor.caret), 0., "painted under the host's");
}
