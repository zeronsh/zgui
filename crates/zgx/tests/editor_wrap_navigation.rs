use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
/// Home and End as a visual line's edges: ⌘← and ⌘→ on macOS, where Home
/// and End scroll the text instead.
fn line_edge(key: Key) -> (Key, Modifiers) {
    match key {
        Key::Home | Key::End if cfg!(target_os = "macos") => (
            if key == Key::End {
                Key::ArrowRight
            } else {
                Key::ArrowLeft
            },
            Modifiers {
                meta: true,
                ..Modifiers::default()
            },
        ),
        key => (key, Modifiers::default()),
    }
}
fn key(ui: &mut Ui, key: Key) {
    let (key, modifiers) = line_edge(key);
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
}
#[test]
fn end_keeps_upstream_wrap_affinity_and_home_stays_on_that_visual_row() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        text_area("Wrapped", ui.signal("abcdefghijklmnopqrst".into()))
            .size(80., 120.)
            .p(10.)
            .text_size(10.)
            .text_wrap(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(2, 2);
    editor.refresh();
    ui.prepare_frame();
    let first_y = ui.scene.borrow().bounds(editor.caret).y;
    key(&mut ui, Key::End);
    ui.prepare_frame();
    assert_eq!(editor.editor.borrow().selection().focus, 10);
    let end = ui.scene.borrow().bounds(editor.caret);
    assert_eq!(end.y, first_y);
    assert_eq!(end.x, 69.); // One-pixel caret remains inside the 60-pixel viewport.
    key(&mut ui, Key::Home);
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    key(&mut ui, Key::End);
    key(&mut ui, Key::ArrowDown);
    assert_eq!(editor.editor.borrow().selection().focus, 20);
    key(&mut ui, Key::Home);
    assert_eq!(editor.editor.borrow().selection().focus, 10);
}
#[test]
fn vertical_navigation_remembers_visual_column_through_short_lines_and_resets_on_home() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        text_area("Wrapped", ui.signal("abcdefghij\nx\nabcdefghij".into()))
            .size(80., 120.)
            .p(10.)
            .text_size(10.)
            .text_wrap(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(8, 8);
    editor.refresh();
    key(&mut ui, Key::ArrowDown);
    assert_eq!(editor.editor.borrow().selection().focus, 12);
    key(&mut ui, Key::ArrowDown);
    assert_eq!(editor.editor.borrow().selection().focus, 21);
    key(&mut ui, Key::ArrowUp);
    assert_eq!(editor.editor.borrow().selection().focus, 12);
    key(&mut ui, Key::ArrowUp);
    assert_eq!(editor.editor.borrow().selection().focus, 8);
    key(&mut ui, Key::Home);
    key(&mut ui, Key::ArrowDown);
    assert_eq!(editor.editor.borrow().selection().focus, 11);
}
