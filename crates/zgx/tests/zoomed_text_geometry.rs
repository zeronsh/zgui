//! Text geometry given to platform services (assistive technology, Look
//! Up, input methods' candidates) is where the text shows, through any
//! zoom: in the window's points, not the space it is laid out in.
use zgx::scene::{Color, Layout, NodeKind, Rect, Scene, Style, Transform};

fn make(zoom: f32) -> (Scene, zgx::scene::NodeId) {
    let mut s = Scene::new(400., 300.);
    let parent = s.append(
        s.root(),
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(100.),
            height: Some(60.),
            ..Default::default()
        },
    );
    s.set_transform(parent, Transform { x: 30., y: 20. });
    s.set_zoom(parent, zoom);
    let node = s.append(
        parent,
        NodeKind::Text {
            text: "abcd".into(),
            color: Color(0, 0, 0, 255),
            font_size: 10.,
        },
        Style {
            width: Some(80.),
            ..Default::default()
        },
    );
    s.flush();
    (s, node)
}

#[test]
fn zoomed_text_lines_are_in_window_points() {
    let (base, a) = make(1.);
    let (zoom, b) = make(2.);
    let old = base.text_lines(a).unwrap();
    let now = zoom.text_lines(b).unwrap();
    let plain = base.bounds(a);
    let shown = zoom.transformed_bounds(b);
    let expected = Rect::new(
        shown.x + (old[0].rect.x - plain.x) * 2.,
        shown.y + (old[0].rect.y - plain.y) * 2.,
        old[0].rect.width * 2.,
        old[0].rect.height * 2.,
    );
    assert_eq!(now[0].rect, expected);
    for (a, b) in old[0].carets.iter().zip(&now[0].carets) {
        assert_eq!(*b, shown.x + (*a - plain.x) * 2.);
    }
}

#[test]
fn zoomed_look_up_baseline_is_in_window_points() {
    let (base, a) = make(1.);
    let (zoom, b) = make(2.);
    let ((x, y), size) = base.text_baseline(a, 2).unwrap();
    let plain = base.bounds(a);
    let shown = zoom.transformed_bounds(b);
    assert_eq!(
        zoom.text_baseline(b, 2),
        Some((
            (shown.x + (x - plain.x) * 2., shown.y + (y - plain.y) * 2.),
            size * 2.
        ))
    );
}

fn editor(zoom: f32) -> (zgx::widgets::Ui, zgx::widgets::EditorHandle) {
    let mut ui = zgx::widgets::Ui::new(500., 300.);
    let parent = ui.container(
        ui.root(),
        Layout::Column,
        Style {
            width: Some(200.),
            height: Some(80.),
            ..Default::default()
        },
    );
    ui.scene
        .borrow_mut()
        .set_transform(parent, Transform { x: 30., y: 20. });
    ui.scene.borrow_mut().set_zoom(parent, zoom);
    let value = ui.signal("abcd".to_string());
    let editor = ui.text_input(parent, "Input", value, 100., false);
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(editor.node)));
    editor.set_selection(2, 2);
    ui.prepare_frame();
    (ui, editor)
}

#[test]
fn zoomed_ime_caret_area_is_in_window_points() {
    let (ui, editor) = editor(2.);
    let shown = ui.scene.borrow().transformed_bounds(editor.caret);
    assert_eq!(editor.input_method_area(), shown);
}

#[test]
fn zoomed_ime_clause_area_is_in_window_points() {
    use zgx::input::InputEvent;
    let (mut base, a) = editor(1.);
    let (mut zoom, b) = editor(2.);
    for ui in [&mut base, &mut zoom] {
        ui.dispatch(InputEvent::ImePreedit {
            text: "日本語".into(),
            cursor: Some((0, 6)),
        });
        ui.prepare_frame();
    }
    let old = a.composition_area().unwrap();
    let now = b.composition_area().unwrap();
    let expected = Rect::new(
        30. + (old.x - 30.) * 2.,
        20. + (old.y - 20.) * 2.,
        old.width * 2.,
        old.height * 2.,
    );
    assert_eq!(now, expected);
}
