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
fn text_aligned_to_its_end_follows_the_direction() {
    let (ui, view) = mount(
        column()
            .id("column")
            .w(300.)
            .child(text("12.50").id("end").text_align(TextAlign::End)),
    );
    let end = view.find("end").unwrap();
    assert_eq!(ui.scene.borrow().font(end).align, TextAlign::Right);
    let column = view.find("column").unwrap();
    ui.scene
        .borrow_mut()
        .set_direction(column, Some(Direction::Rtl));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().font(end).align, TextAlign::Left);
    ui.scene
        .borrow_mut()
        .set_direction(column, Some(Direction::Ltr));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().font(end).align, TextAlign::Right);
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

#[test]
fn a_languages_way_from_its_tag() {
    for tag in ["ar", "he-IL", "fa_IR.UTF-8", "ur", "ku-Arab", "ckb"] {
        assert_eq!(Direction::of_language(tag), Direction::Rtl, "{tag}");
    }
    for tag in ["en-US", "ja", "zh-Hans", "C", "", "ku-Latn", "az-Latn"] {
        assert_eq!(Direction::of_language(tag), Direction::Ltr, "{tag}");
    }
}

#[test]
fn a_slider_right_to_left_fills_from_the_right_and_its_keys_go_the_way_it_shows() {
    use zgui::input::{InputEvent, Key, Modifiers, PointerButton};
    for rtl in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let value = ui.signal(25.);
        let view = ui.mount(
            column()
                .direction(if rtl { Direction::Rtl } else { Direction::Ltr })
                .p(20.)
                .child(
                    slider("Volume", value.clone(), 0.0..=100.)
                        .id("slider")
                        .w(200.)
                        .h(20.),
                ),
        );
        ui.prepare_frame();
        let root = view.find("slider").unwrap();
        let (bounds, rail, fill, thumb) = {
            let scene = ui.scene.borrow();
            let parts = scene.children(root).to_vec();
            // The rail, the fill's clip (and the fill in it), the thumb.
            let fill = scene.children(parts[1])[0];
            (
                scene.bounds(root),
                scene.bounds(parts[0]),
                scene.visible_bounds(fill).unwrap(),
                scene.bounds(parts[2]),
            )
        };
        let inside =
            |r: Rect| r.x >= bounds.x - 0.01 && r.x + r.width <= bounds.x + bounds.width + 0.01;
        assert!(
            inside(rail) && inside(fill) && inside(thumb),
            "{rtl}: {bounds:?} {rail:?} {fill:?} {thumb:?}"
        );
        assert!(
            (fill.width - rail.width * 0.25).abs() < 0.5,
            "{rtl}: a quarter full: {fill:?}"
        );
        let thumb_centre = thumb.x + thumb.width / 2.;
        if rtl {
            assert!(
                (fill.x + fill.width - (rail.x + rail.width)).abs() < 0.01,
                "fills from the right: {fill:?}"
            );
            assert!(
                (thumb_centre - (fill.x)).abs() < 0.5,
                "the thumb at the fill's end"
            );
        } else {
            assert!((fill.x - rail.x).abs() < 0.01);
            assert!((thumb_centre - (fill.x + fill.width)).abs() < 0.5);
        }
        // Left is more right to left, less left to right.
        ui.input.focus(&ui.scene, Some(root));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowLeft,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert_eq!(value.get(), if rtl { 26. } else { 24. });
        // A press a quarter of the way from the start (its right, right to left).
        let x = if rtl {
            rail.x + rail.width * 0.75
        } else {
            rail.x + rail.width * 0.25
        };
        let y = rail.y;
        ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
        assert!((value.get() - 25.).abs() < 0.5, "{rtl}: {}", value.get());
    }
}

#[test]
fn the_retained_slider_fills_from_the_right_right_to_left() {
    use zgui::input::{InputEvent, Key, Modifiers};
    for rtl in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let parent = ui.container(ui.root(), zgui::scene::Layout::Column, Default::default());
        if rtl {
            ui.scene
                .borrow_mut()
                .set_direction(parent, Some(Direction::Rtl));
        }
        let value = ui.signal(50.);
        let root = ui.slider(parent, "Volume", value.clone(), 0.0..=100., 200.);
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let bounds = scene.bounds(root);
        // The background, then the fill and the knob.
        let parts = scene.children(root).to_vec();
        let (fill, knob) = (scene.bounds(parts[1]), scene.bounds(parts[2]));
        drop(scene);
        let inside =
            |r: Rect| r.x >= bounds.x - 0.01 && r.x + r.width <= bounds.x + bounds.width + 0.01;
        assert!(
            inside(fill) && inside(knob),
            "{rtl}: {bounds:?} {fill:?} {knob:?}"
        );
        if rtl {
            assert_eq!(
                fill.x + fill.width,
                bounds.x + bounds.width - 8.,
                "fills from the right"
            );
            assert_eq!(knob.x + knob.width, fill.x, "the knob at the fill's end");
        } else {
            assert_eq!(fill.x, bounds.x + 8.);
            assert_eq!(knob.x, fill.x + fill.width);
        }
        ui.input.focus(&ui.scene, Some(root));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowLeft,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert_eq!(value.get(), if rtl { 51. } else { 49. });
    }
}

#[test]
fn a_progress_bar_fills_from_the_right_right_to_left() {
    for rtl in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let view = ui.mount(
            column()
                .direction(if rtl { Direction::Rtl } else { Direction::Ltr })
                .child(progress("Upload", ui.signal(0.25)).id("bar").w(200.).h(8.)),
        );
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let root = view.find("bar").unwrap();
        let bounds = scene.bounds(root);
        let clip = scene.children(root)[0];
        let fill = scene.visible_bounds(scene.children(clip)[0]).unwrap();
        assert_eq!(fill.width, 50., "{rtl}");
        let expected = if rtl { bounds.x + 150. } else { bounds.x };
        assert_eq!(fill.x, expected, "{rtl}: {fill:?} in {bounds:?}");
    }
}
