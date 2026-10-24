use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::Color,
    text_layout::FontFamily,
    widgets::Ui,
};
#[test]
fn inherited_pitch_updates_labels_and_boxes_without_rebuilding_components() {
    let mut ui = Ui::new(600., 500.);
    let pitch = ui.signal(30.);
    let read = pitch.clone();
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let mounted = ui.mount(
        column()
            .text_size(10.)
            .reactive_style(move || Styles::new().line_height(read.get()))
            .child(component(move |_| {
                count.set(count.get() + 1);
                column().children([
                    text("a\nb").id("plain"),
                    text("a\nb").p(5.).id("boxed"),
                    text("a\nb").line_height_normal().id("normal"),
                    // All other typography is explicit; line height still inherits.
                    text("a\nb")
                        .text_size(10.)
                        .text_color(Color(1, 2, 3, 255))
                        .text_wrap(false)
                        .font_family(FontFamily::SansSerif)
                        .font_weight(400)
                        .italic(false)
                        .id("independent"),
                ])
            })),
    );
    ui.prepare_frame();
    let plain = mounted.find("plain").unwrap();
    let boxed = mounted.find("boxed").unwrap();
    let normal = mounted.find("normal").unwrap();
    let independent = mounted.find("independent").unwrap();
    assert_eq!(ui.scene.borrow().bounds(plain).height, 60.);
    assert_eq!(ui.scene.borrow().bounds(boxed).height, 70.);
    assert_eq!(ui.scene.borrow().bounds(normal).height, 28.);
    pitch.set(20.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(plain).height, 40.);
    assert_eq!(ui.scene.borrow().bounds(boxed).height, 50.);
    assert_eq!(ui.scene.borrow().bounds(independent).height, 40.);
    assert_eq!(ui.scene.borrow().bounds(normal).height, 28.);
    assert_eq!(builds.get(), 1);
    assert_eq!(mounted.find("plain"), Some(plain));
}
#[test]
fn sparse_reset_and_wrapper_override_preserve_equality_and_idle_damage() {
    let mut ui = Ui::new(600., 500.);
    let custom = ui.signal(true);
    let read = custom.clone();
    let mounted = ui.mount(
        column().text_size(10.).line_height(24.).children([
            text("a\nb").id("dynamic").reactive_style(move || {
                if read.get() {
                    Styles::new().line_height(40.)
                } else {
                    Styles::new()
                }
            }),
            component(|_| text("a\nb").line_height(70.))
                .line_height(36.)
                .id("wrapper"),
            text("a\nb")
                .line_height(70.)
                .line_height_normal()
                .id("reset"),
            text("a\nb").line_height(f32::NAN).id("invalid"),
            text("a\nb").line_height(2.5).id("tight"),
        ]),
    );
    ui.prepare_frame();
    let dynamic = mounted.find("dynamic").unwrap();
    assert_eq!(ui.scene.borrow().bounds(dynamic).height, 80.);
    assert_eq!(
        ui.scene
            .borrow()
            .bounds(mounted.find("wrapper").unwrap())
            .height,
        72.
    );
    for id in ["reset", "invalid"] {
        assert_eq!(
            ui.scene.borrow().bounds(mounted.find(id).unwrap()).height,
            28.
        );
    }
    custom.set(false);
    ui.prepare_frame();
    assert_eq!(
        ui.scene
            .borrow()
            .bounds(mounted.find("tight").unwrap())
            .height,
        5.
    );
    assert_eq!(ui.scene.borrow().bounds(dynamic).height, 48.);
    assert_eq!(
        Styles::new().line_height(f32::NAN),
        Styles::new().line_height_normal()
    );
    ui.scene.borrow_mut().flush();
    custom.set(false);
    ui.prepare_frame();
    let idle = ui.scene.borrow_mut().flush();
    assert_eq!(idle.layout_nodes, 0);
    assert!(idle.damage.is_empty());
}
#[test]
fn editor_pitch_tracks_inheritance_for_caret_pointer_and_vertical_navigation() {
    let mut ui = Ui::new(400., 400.);
    let pitch = ui.signal(28.);
    let read = pitch.clone();
    let value = ui.signal("abc\ndef".to_owned());
    let mounted = ui.mount(
        column()
            .text_size(10.)
            .reactive_style(move || Styles::new().line_height(read.get()))
            .child(
                text_area("Editor", value.clone())
                    .id("editor")
                    .size(160., 160.)
                    .p(10.)
                    .text_wrap(true),
            ),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(1, 1);
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).height, 28.);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(editor.editor.borrow().selection().focus, 5);
    let before = ui.scene.borrow().bounds(editor.caret);
    pitch.set(42.);
    ui.prepare_frame();
    let after = ui.scene.borrow().bounds(editor.caret);
    assert_eq!(after.height, 42.);
    assert_eq!(after.y - before.y, 14.);
    assert_eq!(value.get(), "abc\ndef");
    assert_eq!(editor.editor.borrow().selection().focus, 5);
    ui.dispatch(InputEvent::PointerDown {
        x: 11.,
        y: 56.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 11.,
        y: 56.,
        button: PointerButton::Primary,
    });
    assert_eq!(editor.editor.borrow().selection().focus, 4);
}
