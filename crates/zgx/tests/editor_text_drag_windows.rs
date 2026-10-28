//! Text dragged between windows: each window's scene numbers its nodes its
//! own way, so the first field of two windows has the same node id. A drop
//! moves text within an editor only from that very editor (its window's),
//! and the editor it came from lets go of only what is still there.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    widgets::{EditorHandle, Ui},
};

fn field(ui: &mut Ui, text: &str) -> EditorHandle {
    let value = ui.signal(text.to_owned());
    let v = ui.mount(
        text_area("Editor", value)
            .size(300., 60.)
            .p(10.)
            .text_size(10.)
            .line_height(20.),
    );
    ui.prepare_frame();
    ui.editor(v.node()).unwrap()
}

/// "alpha" selected and dragged out of `editor`'s window.
fn begin(ui: &mut Ui, editor: &EditorHandle) {
    ui.input.focus(&ui.scene, Some(editor.node));
    editor.set_selection(0, 5);
    ui.dispatch(InputEvent::PointerDown {
        x: 17.,
        y: 20.,
        button: PointerButton::Primary,
    });
    ui.prepare_frame();
    ui.dispatch(InputEvent::PointerMove { x: 25., y: 20. });
    ui.prepare_frame();
    assert!(ui.input.is_dragging());
    assert_eq!(
        ui.input
            .drag_state()
            .unwrap()
            .payload
            .downcast_ref::<zgx::input::DraggedText>()
            .unwrap()
            .text(),
        "alpha"
    );
    ui.input.set_drag_away(&ui.scene, true);
}

/// "alpha" of "alpha beta" dragged to the end of `target_text` in another
/// window (its field's node id another than the source's if `pad`), the
/// source's text changed to "x" on the way if `edit_source`: the source's
/// text and the destination's after.
fn transfer(pad: bool, target_text: &str, edit_source: bool) -> (String, String) {
    let mut source = Ui::new(400., 200.);
    let mut target = Ui::new(400., 200.);
    let from = field(&mut source, "alpha beta");
    if pad {
        target.mount(div().size(0., 0.));
    }
    let to = field(&mut target, target_text);
    assert_eq!(from.node == to.node, !pad);
    begin(&mut source, &from);
    let payload = source.input.drag_state().unwrap().payload;
    if edit_source {
        from.value().set("x".to_owned());
        source.prepare_frame();
    }
    let b = target.scene.borrow().bounds(to.node);
    let (x, y) = (b.x + 220., b.y + 20.);
    assert!(target.input.drag_over(&target.scene, &payload, x, y));
    let accepted = target.input.drag_drop(&target.scene, &payload, x, y);
    assert!(accepted);
    source
        .input
        .end_drag_elsewhere(&source.scene, accepted, false);
    source.prepare_frame();
    target.prepare_frame();
    (from.value().get(), to.value().get())
}

#[test]
fn separate_windows_with_equal_node_ids_move_between_editors() {
    assert_eq!(
        transfer(false, "alpha omega", false),
        (" beta".to_owned(), "alpha omegaalpha".to_owned())
    );
}

#[test]
fn different_node_ids_control() {
    assert_eq!(
        transfer(true, "alpha omega", false),
        (" beta".to_owned(), "alpha omegaalpha".to_owned())
    );
}

#[test]
fn short_destination_in_another_window_must_not_use_source_range() {
    assert_eq!(
        transfer(false, "x", false),
        (" beta".to_owned(), "xalpha".to_owned())
    );
}

#[test]
fn unicode_destination_in_another_window_must_not_use_source_boundaries() {
    assert_eq!(
        transfer(false, "🙂🙂", false),
        (" beta".to_owned(), "🙂🙂alpha".to_owned())
    );
}

#[test]
fn shorter_source_model_during_drag_must_not_panic_at_end() {
    assert_eq!(
        transfer(true, "target", true),
        ("x".to_owned(), "targetalpha".to_owned())
    );
}
