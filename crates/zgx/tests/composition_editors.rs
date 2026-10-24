use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    scene::{NodeId, NodeKind},
    semantics::Role,
    text::TextValue,
    text_layout::FontFamily,
    widgets::{EditorHandle, Ui},
};

fn descendants(ui: &Ui, root: NodeId) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let mut result = vec![root];
    let mut cursor = 0;
    while cursor < result.len() {
        result.extend_from_slice(scene.children(result[cursor]));
        cursor += 1;
    }
    result
}

#[test]
fn bound_editors_preserve_roles_model_sync_ime_and_unmount() {
    let mut ui = Ui::new(500., 400.);
    let value = ui.signal(String::from("start"));
    let multiline = ui.signal(String::new());
    let mounted = ui.mount(
        column().children([
            text_input("Name", value.clone()).id("name").w(240.),
            text_area("Notes", multiline.clone())
                .id("notes")
                .size(300., 180.),
        ]),
    );
    let name = mounted.find("name").unwrap();
    let notes = mounted.find("notes").unwrap();
    assert_eq!(
        ui.semantics.borrow().get(name).unwrap().role,
        Role::TextInput
    );
    assert_eq!(
        ui.semantics.borrow().get(notes).unwrap().role,
        Role::MultilineTextInput
    );
    assert!(ui.input.focus(&ui.scene, Some(name)));
    let editor = ui.focused_editor().unwrap();
    value.set(String::from("model"));
    assert_eq!(editor.editor.borrow().text(), "model");
    editor.editor.borrow_mut().set_selection(0, 5);
    ui.dispatch(InputEvent::ImePreedit {
        text: "日本".into(),
        cursor: Some((6, 6)),
    });
    assert_eq!(value.get(), "model", "preedit must not commit the model");
    ui.dispatch(InputEvent::ImeCommit("日本".into()));
    assert_eq!(value.get(), "日本");
    editor.paste("\nnext");
    assert!(!value.get().contains('\n'));
    assert!(ui.input.focus(&ui.scene, Some(notes)));
    ui.dispatch(InputEvent::Text("first\nsecond".into()));
    assert_eq!(multiline.get(), "first\nsecond");
    let last_editor_value = editor.editor.borrow().text().into_owned();
    let old_nodes = descendants(&ui, mounted.node());
    mounted.unmount();
    assert!(ui.focused_editor().is_none());
    assert!(old_nodes.iter().all(|id| !ui.scene.borrow().contains(*id)));
    assert!(old_nodes.iter().all(|id| !ui.input.has_listeners(*id)));
    value.set("after removal".into());
    assert_eq!(
        editor.editor.borrow().text(),
        last_editor_value,
        "unmount disposes model subscription"
    );
}

/// A long document bound as a `TextValue` is held once: the editor, its
/// binding and its semantics share one text, an edit's new value is the
/// editor's own (not a copy), and a document the binding is given is taken
/// as it is, its change undoable.
#[test]
fn a_long_bound_document_is_shared_with_its_editor_not_copied() {
    let mut ui = Ui::new(500., 400.);
    let lines = |word: &str| -> String {
        (0..10_000)
            .map(|i| format!("{word} {i} of the document\n"))
            .collect()
    };
    let document = lines("line");
    let value = ui.signal(TextValue::from(document.as_str()));
    assert!(value.with_untracked(|v| v.as_text().is_some()));
    let mounted = ui.mount(
        text_area_value("Document", value.clone())
            .id("doc")
            .size(300., 180.),
    );
    let doc = mounted.find("doc").unwrap();
    let shared = |editor: &EditorHandle| {
        let ours = editor.editor.borrow().value();
        value.with_untracked(|v| v.ptr_eq(&ours))
    };
    assert!(ui.input.focus(&ui.scene, Some(doc)));
    let editor = ui.focused_editor().unwrap();
    assert!(shared(&editor), "mounting takes the bound text as it is");
    editor.editor.borrow_mut().set_selection(0, 0);
    ui.dispatch(InputEvent::Text("x".into()));
    assert!(shared(&editor), "an edit's value is the editor's text");
    assert_eq!(value.with_untracked(|v| v.len()), document.len() + 1);
    assert!(value.with_untracked(|v| *v == *format!("x{document}")));
    ui.prepare_frame();
    let semantic = ui.semantics.borrow().long_value(doc).cloned();
    let ours = editor.editor.borrow().long_text().cloned();
    assert!(semantic.zip(ours).is_some_and(|(a, b)| a.ptr_eq(&b)));
    // Another document, taken as given; undo goes back to the edited one.
    let next = TextValue::from(lines("row").as_str());
    value.set(next.clone());
    assert!(shared(&editor) && value.with_untracked(|v| v.ptr_eq(&next)));
    assert!(editor.editor.borrow_mut().undo());
    assert_eq!(editor.editor.borrow().len(), document.len() + 1);
    mounted.unmount();
}

#[test]
fn editor_focus_and_disabled_styles_preserve_editor_semantics_and_input() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(String::new());
    let disabled = ui.signal(false);
    let read_disabled = disabled.clone();
    let mounted = ui.mount(
        text_input("Search", value.clone())
            .bg(rgb(0x123456))
            .focus(|style| style.bg(rgb(0xabcdef)))
            .disabled_when(move || read_disabled.get())
            .disabled_style(|style| style.opacity(0.4)),
    );
    let root = mounted.node();
    assert!(ui.input.focus(&ui.scene, Some(root)));
    assert!(
        matches!(ui.scene.borrow().kind(root), NodeKind::Panel { quad, .. } if quad.fill == rgb(0xabcdef))
    );
    ui.dispatch(InputEvent::Text("a".into()));
    disabled.set(true);
    assert_eq!(ui.input.focused(), None);
    assert!(ui.semantics.borrow().get(root).unwrap().disabled);
    assert_eq!(ui.scene.borrow().effects(root).opacity, 0.4);
    assert!(!ui.input.focus(&ui.scene, Some(root)));
    ui.dispatch(InputEvent::Text("ignored".into()));
    assert_eq!(value.get(), "a");
    disabled.set(false);
    assert!(ui.input.focus(&ui.scene, Some(root)));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.dispatch(InputEvent::Text("b".into()));
    assert_eq!(value.get(), "ab");
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().role,
        Role::TextInput
    );
}

#[test]
fn inherited_editor_font_and_reactive_dimensions_update_retained_content() {
    let mut ui = Ui::new(500., 300.);
    let large = ui.signal(false);
    let parent_read = large.clone();
    let width_read = large.clone();
    let value = ui.signal("wide text".to_owned());
    let mounted = ui.mount(
        column()
            .text_color(rgb(0x123456))
            .font_family(FontFamily::Monospace)
            .font_bold()
            .reactive_style(move || {
                Styles::new().text_size(if parent_read.get() { 24. } else { 16. })
            })
            .child(
                text_input("Name", value)
                    .id("input")
                    .reactive_style(move || {
                        Styles::new().w(if width_read.get() { 360. } else { 180. })
                    }),
            ),
    );
    let root = mounted.find("input").unwrap();
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let nodes = descendants(&ui, root);
    let text = *nodes
        .iter()
        .find(|id| matches!(ui.scene.borrow().kind(**id), NodeKind::Text { .. }))
        .unwrap();
    assert_eq!(ui.scene.borrow().font(text).family, FontFamily::Monospace);
    assert_eq!(ui.scene.borrow().font(text).weight, 700);
    assert!(
        matches!(ui.scene.borrow().kind(text), NodeKind::Text { color, font_size, .. } if *color == rgb(0x123456) && *font_size == 16.)
    );
    large.set(true);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    assert_eq!(descendants(&ui, root), nodes);
    assert_eq!(ui.scene.borrow().bounds(root).width, 360.);
    assert!(
        matches!(ui.scene.borrow().kind(text), NodeKind::Text { font_size, .. } if *font_size == 24.)
    );
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
}

#[test]
fn editor_viewport_follows_flex_allocation_and_padding_without_a_model_write() {
    let mut ui = Ui::new(600., 300.);
    let wide = ui.signal(false);
    let read = wide.clone();
    let value = ui.signal("keep text and selection".to_owned());
    let mounted = ui.mount(
        row()
            .reactive_style(move || Styles::new().w(if read.get() { 500. } else { 300. }))
            .child(
                text_input("Editable", value.clone())
                    .grow()
                    .p(10.)
                    .id("editor"),
            )
            .child(column().w(100.)),
    );
    let root = mounted.find("editor").unwrap();
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(root)));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(2, 5);
    editor.refresh();
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let viewport = ui.scene.borrow().children(root)[0];
    let before = ui.scene.borrow().bounds(viewport);
    wide.set(true);
    ui.prepare_frame();
    let after = ui.scene.borrow().bounds(viewport);
    assert!(after.width > before.width);
    assert_eq!(after.width, ui.scene.borrow().bounds(root).width - 20.);
    assert_eq!(after.x - ui.scene.borrow().bounds(root).x, 10.);
    assert_eq!(after.y - ui.scene.borrow().bounds(root).y, 10.);
    assert_eq!(editor.editor.borrow().selection().range(), 2..5);
    assert_eq!(value.get(), "keep text and selection");
    ui.scene.borrow_mut().flush();
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
}

#[test]
fn paste_allows_model_subscribers_to_normalize_the_value() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal(String::new());
    let normalize = value.clone();
    let _normalizer = ui.runtime.effect(move || {
        let next = normalize.get().to_uppercase();
        normalize.set(next);
    });
    let mounted = ui.mount(text_input("Name", value.clone()));
    assert!(ui.input.focus(&ui.scene, Some(mounted.node())));
    let editor = ui.focused_editor().unwrap();
    editor.paste("lowercase");
    assert_eq!(value.get(), "LOWERCASE");
    assert_eq!(editor.editor.borrow().text(), "LOWERCASE");
}

#[test]
fn multiline_wheel_scroll_preserves_selection_clamps_and_bubbles_at_edges() {
    use std::{cell::Cell, rc::Rc};
    use zgx::input::EventPhase;
    let mut ui = Ui::new(400., 300.);
    let initial = (0..30).map(|n| format!("line {n}\n")).collect::<String>();
    let value = ui.signal(initial.clone());
    let mounted = ui.mount(
        column().child(
            text_area("Notes", value.clone())
                .size(250., 90.)
                .id("notes"),
        ),
    );
    let root = mounted.find("notes").unwrap();
    let bubbled = Rc::new(Cell::new(0));
    let count = bubbled.clone();
    ui.on_event(mounted.node(), false, move |event| {
        if event.phase == EventPhase::Bubble && matches!(event.event, InputEvent::Scroll { .. }) {
            count.set(count.get() + 1);
        }
    });
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(root)));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    let text = descendants(&ui, root)
        .into_iter()
        .find(|id| matches!(ui.scene.borrow().kind(*id), NodeKind::Text { .. }))
        .unwrap();
    let bounds = ui.scene.borrow().bounds(root);
    let scroll = |delta_y| InputEvent::Scroll {
        x: bounds.x + 20.,
        y: bounds.y + 20.,
        delta_x: 0.,
        delta_y,
    };
    let before = ui.scene.borrow().transform(text);
    assert!(ui.dispatch(scroll(35.)).default_prevented);
    let after = ui.scene.borrow().transform(text);
    assert_eq!(after.y, before.y - 35.);
    assert_eq!(value.get(), initial);
    assert_eq!(editor.editor.borrow().selection().range(), 0..0);
    assert_eq!(bubbled.get(), 0);
    assert!(ui.dispatch(scroll(100_000.)).default_prevented);
    let at_bottom = ui.scene.borrow().transform(text);
    let viewport = ui.scene.borrow().children(root)[0];
    let extent = ui.scene.borrow().bounds(text).height - ui.scene.borrow().bounds(viewport).height;
    assert_eq!(
        at_bottom.y, -extent,
        "wheel scrolling clamps to content extent"
    );
    // Within the gesture the editor keeps the wheel at its edge (latched,
    // as macOS's views are); a new gesture after a pause bubbles out.
    assert!(!ui.dispatch(scroll(100_000.)).default_prevented);
    assert_eq!(bubbled.get(), 0);
    std::thread::sleep(std::time::Duration::from_millis(350));
    assert!(!ui.dispatch(scroll(100_000.)).default_prevented);
    assert_eq!(ui.scene.borrow().transform(text), at_bottom);
    assert_eq!(bubbled.get(), 1);
    ui.dispatch(InputEvent::Text("X".into()));
    assert_eq!(
        ui.scene.borrow().transform(text).y,
        0.,
        "editing reveals the caret after wheel scrolling"
    );
    assert!(value.get().starts_with('X'));
    assert!(!ui.dispatch(scroll(-100_000.)).default_prevented);
    assert_eq!(bubbled.get(), 2);
}

#[test]
fn singleline_vertical_wheel_bubbles_without_moving_text() {
    use std::{cell::Cell, rc::Rc};
    use zgx::input::EventPhase;
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("one line".to_owned());
    let mounted = ui.mount(column().child(text_input("Name", value.clone()).id("name")));
    let root = mounted.find("name").unwrap();
    let bubbled = Rc::new(Cell::new(0));
    let count = bubbled.clone();
    ui.on_event(mounted.node(), false, move |event| {
        if event.phase == EventPhase::Bubble && matches!(event.event, InputEvent::Scroll { .. }) {
            count.set(count.get() + 1);
        }
    });
    ui.prepare_frame();
    let text = descendants(&ui, root)
        .into_iter()
        .find(|id| matches!(ui.scene.borrow().kind(*id), NodeKind::Text { .. }))
        .unwrap();
    let before = ui.scene.borrow().transform(text);
    let bounds = ui.scene.borrow().bounds(root);
    assert!(
        !ui.dispatch(InputEvent::Scroll {
            x: bounds.x + 20.,
            y: bounds.y + 20.,
            delta_x: 0.,
            delta_y: 40.
        })
        .default_prevented
    );
    assert_eq!(ui.scene.borrow().transform(text), before);
    assert_eq!(bubbled.get(), 1);
    assert_eq!(value.get(), "one line");
}

#[test]
fn reactive_editor_text_color_only_damages_paint() {
    let mut ui = Ui::new(400., 200.);
    let color = ui.signal(rgb(0x123456));
    let read = color.clone();
    let value = ui.signal("unchanged metrics".to_owned());
    let mounted = ui.mount(
        text_input("Name", value).reactive_style(move || Styles::new().text_color(read.get())),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let text = descendants(&ui, mounted.node())
        .into_iter()
        .find(|id| matches!(ui.scene.borrow().kind(*id), NodeKind::Text { .. }))
        .unwrap();
    color.set(rgb(0xabcdef));
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(!report.damage.is_empty());
    assert!(
        matches!(ui.scene.borrow().kind(text), NodeKind::Text { color, .. } if *color == rgb(0xabcdef))
    );
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
}
