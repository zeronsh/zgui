//! Where a range of an editor's text shows (`EditorHandle::text_area`,
//! `Scene::text_range_area`), as Windows' input methods ask for it
//! (`GetTextExt`): in the window's points, through any zoom and the
//! editor's scroll, across every line the range runs on.
use zgx::input::InputEvent;
use zgx::scene::{Layout, Style, Transform};
use zgx::widgets::{EditorHandle, Ui};

fn editor(text: &str, width: f32, multiline: bool, zoom: f32) -> (Ui, EditorHandle) {
    let mut ui = Ui::new(500., 300.);
    let parent = ui.container(
        ui.root(),
        Layout::Column,
        Style {
            width: Some(400.),
            height: Some(200.),
            ..Default::default()
        },
    );
    ui.scene
        .borrow_mut()
        .set_transform(parent, Transform { x: 30., y: 20. });
    ui.scene.borrow_mut().set_zoom(parent, zoom);
    let value = ui.signal(text.to_string());
    let editor = ui.text_input(parent, "Input", value, width, multiline);
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(editor.node)));
    ui.prepare_frame();
    (ui, editor)
}

/// The clause an input method converts, asked for by its range, is where
/// the editor underlines it and puts the candidates (`composition_area`),
/// zoomed or not.
#[test]
fn a_clauses_range_is_where_its_underline_shows() {
    for zoom in [1., 2.] {
        let (mut ui, editor) = editor("ab", 300., false, zoom);
        editor.set_selection(1, 1);
        ui.dispatch(InputEvent::ImePreedit {
            text: "日本語".into(),
            cursor: Some((3, 6)),
        });
        ui.prepare_frame();
        let clause = editor.composition_area().unwrap();
        // "a" then the preedit: the clause is bytes 4..7 of what shows.
        let area = editor.text_area(4..7).unwrap();
        for (got, want) in [
            (area.x, clause.x),
            (area.y, clause.y),
            (area.width, clause.width),
            (area.height, clause.height),
        ] {
            assert!(
                (got - want).abs() < 0.01,
                "zoom {zoom}: {area:?} vs {clause:?}"
            );
        }
    }
}

/// An empty range is a caret's place: no width, the line's height, where
/// the characters before it end.
#[test]
fn an_empty_range_is_a_caret() {
    let (_ui, editor) = editor("abcd", 300., false, 1.);
    let whole = editor.text_area(0..4).unwrap();
    let start = editor.text_area(0..0).unwrap();
    let end = editor.text_area(4..4).unwrap();
    let two = editor.text_area(0..2).unwrap();
    assert_eq!((start.width, end.width), (0., 0.));
    assert_eq!((start.x, end.x), (whole.x, whole.x + whole.width));
    assert_eq!(editor.text_area(2..2).unwrap().x, two.x + two.width);
    assert_eq!(start.height, whole.height);
}

/// A range over a line break is the box around both lines' pieces.
#[test]
fn a_range_across_lines_holds_each_lines_piece() {
    let (_ui, editor) = editor("abc\ndefgh", 300., true, 1.);
    let first = editor.text_area(1..3).unwrap();
    let second = editor.text_area(4..6).unwrap();
    let both = editor.text_area(1..6).unwrap();
    assert!(second.y >= first.y + first.height - 0.01);
    assert_eq!(both, first.union(second));
}

/// Scrolled to its end, a long single line's last characters are where
/// they show, inside the field.
#[test]
fn a_scrolled_fields_range_is_where_it_shows() {
    let text = "a long line of text that runs well past the field's width";
    let (ui, editor) = editor(text, 120., false, 1.);
    let n = text.len();
    editor.set_selection(n, n);
    ui.prepare_frame();
    let field = ui.scene.borrow().transformed_bounds(editor.node);
    assert!(
        editor.text_area(0..n).unwrap().width > field.width,
        "the line is longer than the field"
    );
    let last = editor.text_area(n - 5..n).unwrap();
    assert!(
        last.x >= field.x && last.x + last.width <= field.x + field.width + 0.01,
        "{last:?} within {field:?}"
    );
    let caret = ui.scene.borrow().transformed_bounds(editor.caret);
    assert!((editor.text_area(n..n).unwrap().x - caret.x).abs() < 2.);
}
