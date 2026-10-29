use zgx::{compose::prelude::*, widgets::Ui};

#[test]
fn the_caret_takes_the_set_width_in_editors_already_shown() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::from("wide"));
    let mounted = ui.mount(column().child(text_input("Name", value).id("name").size(200., 30.)));
    ui.prepare_frame();
    let node = mounted.find("name").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 1.);
    // A thicker text cursor, as Windows' accessibility settings set it.
    ui.set_caret_width(3.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 3.);
    ui.set_caret_width(f32::NAN);
    assert_eq!(ui.caret_width(), 3.);
    ui.set_caret_width(50.);
    assert_eq!(ui.caret_width(), 20.);
    ui.set_caret_width(0.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(editor.caret).width, 1.);
}

#[test]
fn an_editor_takes_its_own_caret_colour_and_width() {
    use zgx::scene::{Color, NodeKind};
    const ACCENT: Color = Color(255, 64, 0, 255);
    let mut ui = Ui::new(400., 200.);
    let color = ui.signal(ACCENT);
    let shade = color.clone();
    let mounted = ui.mount(
        column()
            .child(
                text_area("Draft", ui.signal(String::from("wide")))
                    .id("draft")
                    .size(200., 60.)
                    .caret_color_when(move || Some(shade.get()))
                    .caret_width(2.),
            )
            .child(
                text_input("Name", ui.signal(String::new()))
                    .id("name")
                    .size(200., 30.),
            ),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, mounted.find("draft"));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    let caret = |ui: &Ui| {
        let scene = ui.scene.borrow();
        (
            scene.bounds(editor.caret).width,
            scene.kind(editor.caret).clone(),
        )
    };
    assert_eq!(caret(&ui), (2., NodeKind::Rect(ACCENT)));
    // Its colour follows what it reads; its width stays its own.
    color.set(Color(0, 128, 255, 255));
    ui.set_caret_width(3.);
    ui.prepare_frame();
    assert_eq!(caret(&ui), (2., NodeKind::Rect(Color(0, 128, 255, 255))));
    // Another editor keeps the text's colour and the app's width.
    ui.input.focus(&ui.scene, mounted.find("name"));
    ui.prepare_frame();
    let other = ui.focused_editor().unwrap();
    let scene = ui.scene.borrow();
    assert_eq!(scene.bounds(other.caret).width, 3.);
    assert_ne!(
        scene.kind(other.caret),
        &NodeKind::Rect(Color(0, 128, 255, 255))
    );
}
