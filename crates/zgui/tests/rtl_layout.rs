//! Right-to-left layout (CSS's `direction: rtl`): a subtree's rows run from
//! the right, a column's children start at its right, text starts at the
//! right, and the logical `start`/`end` edges follow, while padding, margins
//! and insets given as left and right stay where they were given.
use zgui::{
    compose::{ViewHandle, prelude::*},
    scene::{Direction, Rect},
    text_layout::TextAlign,
    widgets::Ui,
};

fn mount(view: View) -> (Ui, ViewHandle) {
    let mut ui = Ui::new(400., 300.);
    let handle = ui.mount(view);
    ui.prepare_frame();
    (ui, handle)
}
fn rect(ui: &Ui, view: &ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(view.find(id).unwrap())
}
fn chip(id: &str, w: f32) -> View {
    div().id(id).w(w).h(10.).shrink_0()
}

#[test]
fn a_row_runs_from_the_right_and_back() {
    let (ui, view) = mount(
        row()
            .id("row")
            .w(300.)
            .gap(10.)
            .rtl()
            .child(chip("a", 50.))
            .child(chip("b", 30.)),
    );
    assert_eq!(rect(&ui, &view, "a").x, 250.);
    assert_eq!(rect(&ui, &view, "b").x, 210.);
    let row = view.find("row").unwrap();
    ui.scene
        .borrow_mut()
        .set_direction(row, Some(Direction::Ltr));
    ui.prepare_frame();
    assert_eq!(rect(&ui, &view, "a").x, 0.);
    assert_eq!(rect(&ui, &view, "b").x, 60.);
}

#[test]
fn left_and_right_stay_physical_and_start_and_end_follow() {
    let (ui, view) = mount(
        column()
            .w(300.)
            .rtl()
            // Physical: 20 on the left, nothing on the right.
            .child(row().w(300.).pl(20.).child(chip("physical", 50.)))
            // Logical: 20 at the start, the right.
            .child(row().w(300.).ps(20.).child(chip("logical", 50.)))
            // A margin at the end of a line: on the left.
            .child(
                row()
                    .w(300.)
                    .child(chip("margin", 50.).me(15.))
                    .child(chip("after", 10.)),
            ),
    );
    assert_eq!(rect(&ui, &view, "physical").x, 250.);
    assert_eq!(rect(&ui, &view, "logical").x, 230.);
    assert_eq!(rect(&ui, &view, "margin").x, 250.);
    assert_eq!(rect(&ui, &view, "after").x, 225.);
}

#[test]
fn a_columns_children_start_at_its_right_and_nested_rows_inherit() {
    let (ui, view) = mount(
        column()
            .w(300.)
            .items_start()
            .rtl()
            .child(chip("start", 40.))
            .child(row().w(200.).child(chip("nested", 20.))),
    );
    assert_eq!(rect(&ui, &view, "start").x, 260.);
    // The row sits at the column's start (its right), and runs from its own.
    assert_eq!(rect(&ui, &view, "nested").x, 280.);
}

#[test]
fn text_starts_at_the_right() {
    let (ui, view) = mount(
        column()
            .w(300.)
            .rtl()
            .child(text("مرحبا").id("start"))
            .child(text("left").id("left").text_align(TextAlign::Left)),
    );
    let scene = ui.scene.borrow();
    assert_eq!(
        scene.font(view.find("start").unwrap()).align,
        TextAlign::Right
    );
    assert_eq!(
        scene.font(view.find("left").unwrap()).align,
        TextAlign::Left
    );
}

#[test]
fn a_wrapping_row_mirrors_its_lines_and_absolute_insets_are_physical() {
    let (ui, view) = mount(
        overlay()
            .w(300.)
            .h(100.)
            .relative()
            .rtl()
            .child(
                row()
                    .w(300.)
                    .flex_wrap()
                    .gap_x(10.)
                    .child(chip("one", 140.))
                    .child(chip("two", 140.))
                    .child(chip("three", 140.)),
            )
            .child(div().id("pinned").absolute().left(5.).top(0.).w(10.).h(10.))
            .child(
                div()
                    .id("logical")
                    .absolute()
                    .start(5.)
                    .top(0.)
                    .w(10.)
                    .h(10.),
            ),
    );
    assert_eq!(rect(&ui, &view, "one").x, 160.);
    assert_eq!(rect(&ui, &view, "two").x, 10.);
    assert_eq!(rect(&ui, &view, "three").x, 160.);
    assert_eq!(rect(&ui, &view, "pinned").x, 5.);
    assert_eq!(rect(&ui, &view, "logical").x, 285.);
}

#[test]
fn the_style_read_back_is_the_one_given() {
    let (ui, view) = mount(column().w(300.).rtl().child(row().id("row").pl(20.).mr(4.)));
    let style = ui.scene.borrow().style(view.find("row").unwrap());
    assert_eq!(
        style.padding_edges.map(|p| (p.left, p.right)),
        Some((20., 0.))
    );
    assert_eq!((style.margin.left, style.margin.right), (0., 4.));
}

#[test]
fn a_left_to_right_part_keeps_its_margins_where_given() {
    // A phone number's row turned back left to right inside a right-to-left
    // column: placed from the column's start (its right), its left margin
    // on its left, its own children from its left.
    let (ui, view) = mount(
        column().w(300.).items_start().rtl().child(
            row()
                .id("ltr")
                .w(200.)
                .ml(10.)
                .mr(4.)
                .direction(Direction::Ltr)
                .child(chip("digit", 20.)),
        ),
    );
    assert_eq!(rect(&ui, &view, "ltr").x, 96.);
    assert_eq!(rect(&ui, &view, "digit").x, 96.);
}
