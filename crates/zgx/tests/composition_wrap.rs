use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{NodeId, NodeKind},
    widgets::Ui,
};
fn text_node(ui: &Ui, root: NodeId) -> NodeId {
    let scene = ui.scene.borrow();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if matches!(scene.kind(node), NodeKind::Text { .. }) {
            return node;
        }
        pending.extend(scene.children(node));
    }
    panic!("editor text node missing")
}
fn key(ui: &mut Ui, key: Key, shift: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
        repeat: false,
    });
}
#[test]
fn inherited_wrap_tracks_allocated_width_without_changing_model_or_selection() {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("abcdefghijabcdefghijabcdefghij".to_owned());
    let width = ui.signal(100.);
    let read = width.clone();
    let mounted = ui.mount(
        column().text_wrap(true).text_size(10.).child(
            text_area("Wrapped", value.clone())
                .id("editor")
                .h(160.)
                .p(10.)
                .reactive_style(move || Styles::new().w(read.get())),
        ),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(3, 20);
    editor.refresh();
    ui.prepare_frame();
    let text = text_node(&ui, node);
    let before = ui.scene.borrow().bounds(text).height;
    let selection = editor.editor.borrow().selection();
    assert!(ui.scene.borrow().style(text).text_wrap);
    width.set(220.);
    ui.prepare_frame();
    let after = ui.scene.borrow().bounds(text).height;
    assert!(after < before, "wider allocation reduces visual lines");
    assert_eq!(editor.editor.borrow().selection(), selection);
    assert_eq!(value.get(), "abcdefghijabcdefghijabcdefghij");
    assert_eq!(text_node(&ui, node), text);
    mounted.unmount();
    value.set("gone".into());
    assert!(!ui.scene.borrow().contains(text));
}
#[test]
fn wrapped_pointer_hit_and_vertical_selection_use_visual_lines() {
    let mut ui = Ui::new(300., 300.);
    let value = ui.signal("abcdefghijklmnopqrstuvwxyz".to_owned());
    let mounted = ui.mount(
        text_area("Wrapped", value.clone())
            .size(80., 150.)
            .p(10.)
            .text_size(10.)
            .text_wrap(true),
    );
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(mounted.node());
    ui.dispatch(InputEvent::PointerDown {
        x: bounds.x + 11.,
        y: bounds.y + 31.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: bounds.x + 11.,
        y: bounds.y + 31.,
        button: PointerButton::Primary,
    });
    let editor = ui.focused_editor().unwrap();
    let hit = editor.editor.borrow().selection().focus;
    assert!(hit >= 9, "second visual line maps past first ten glyphs");
    assert!(hit < 20);
    editor.editor.borrow_mut().set_selection(2, 2);
    editor.refresh();
    key(&mut ui, Key::ArrowDown, true);
    let selection = editor.editor.borrow().selection();
    assert_eq!(selection.anchor, 2);
    assert!(selection.focus > selection.anchor + 5);
    assert_eq!(value.get(), "abcdefghijklmnopqrstuvwxyz");
    let caret = ui.scene.borrow().bounds(editor.caret);
    assert!(caret.y > bounds.y + 20.);
}
#[test]
fn single_line_stays_unwrapped_and_reactive_wrap_toggle_preserves_editor() {
    let mut ui = Ui::new(400., 300.);
    let wrap = ui.signal(true);
    let read = wrap.clone();
    let value = ui.signal("abcdefghijabcdefghij".to_owned());
    let mounted = ui.mount(
        column()
            .text_wrap(true)
            .child(
                text_input("Single", value.clone())
                    .id("single")
                    .size(80., 40.),
            )
            .child(
                text_area("Multi", value.clone())
                    .id("multi")
                    .size(80., 120.)
                    .reactive_style(move || Styles::new().text_wrap(read.get())),
            ),
    );
    ui.prepare_frame();
    let single = text_node(&ui, mounted.find("single").unwrap());
    let multi = text_node(&ui, mounted.find("multi").unwrap());
    assert!(!ui.scene.borrow().style(single).text_wrap);
    assert!(ui.scene.borrow().style(multi).text_wrap);
    let height = ui.scene.borrow().bounds(multi).height;
    wrap.set(false);
    ui.prepare_frame();
    assert!(!ui.scene.borrow().style(multi).text_wrap);
    assert!(ui.scene.borrow().bounds(multi).height < height);
    wrap.set(true);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(multi).height, height);
}

#[test]
fn equal_wrap_and_idle_geometry_do_not_relayout_or_damage() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("wrapped text across several visual lines".to_owned());
    let mounted = ui.mount(text_area("Wrapped", value).size(100., 100.).text_wrap(true));
    ui.input.focus(&ui.scene, Some(mounted.node()));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let editor = ui.focused_editor().unwrap();
    editor.set_wrap(true);
    ui.prepare_frame();
    let equal = ui.scene.borrow_mut().flush();
    assert_eq!(equal.layout_nodes, 0);
    assert!(equal.damage.is_empty());
    ui.prepare_frame();
    let idle = ui.scene.borrow_mut().flush();
    assert_eq!(idle.layout_nodes, 0);
    assert!(idle.damage.is_empty());
}

#[test]
fn wrapped_wheel_scrolling_preserves_selection_and_keyboard_reveals_caret() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("abcdefghij ".repeat(30));
    let mounted = ui.mount(
        text_area("Wrapped", value.clone())
            .size(100., 80.)
            .p(8.)
            .text_size(10.)
            .text_wrap(true),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    let text = text_node(&ui, mounted.node());
    let before = ui.scene.borrow().bounds(text).y;
    ui.dispatch(InputEvent::Scroll {
        x: 20.,
        y: 20.,
        delta_x: 0.,
        delta_y: 100.,
    });
    let after = ui.scene.borrow().bounds(text).y;
    assert!(after < before);
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    assert_eq!(value.get(), "abcdefghij ".repeat(30));
    key(&mut ui, Key::ArrowRight, false);
    ui.prepare_frame();
    let caret = ui.scene.borrow().bounds(editor.caret);
    assert!(caret.y >= 8. && caret.y + caret.height <= 72.);
    assert_eq!(editor.editor.borrow().selection().focus, 1);
}
