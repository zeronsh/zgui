//! `display_contents`: a view's children lay out in its place, as its
//! parent's own, while the tree (paint, hits, focus, assistive technology)
//! keeps them under it. The numbers are worked out from CSS's rules.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    scene::Rect,
    style::Styles,
    widgets::Ui,
};

fn bounds(ui: &Ui, view: &zgx::compose::ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(view.find(id).unwrap())
}
fn cell(id: &str) -> View {
    div().id(id).h(20.)
}

#[test]
fn in_a_grid_each_child_is_a_cell() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div()
            .grid()
            .grid_cols(3)
            .w(300.)
            .child(cell("a"))
            .child(
                div()
                    .id("group")
                    .display_contents()
                    .child(cell("b"))
                    .child(cell("c"))
                    .child(cell("d")),
            )
            .child(cell("e")),
    );
    ui.prepare_frame();
    let at = |id| {
        let b = bounds(&ui, &view, id);
        (b.x, b.y, b.width)
    };
    assert_eq!(at("a"), (0., 0., 100.));
    assert_eq!(at("b"), (100., 0., 100.));
    assert_eq!(at("c"), (200., 0., 100.));
    assert_eq!(at("d"), (0., 20., 100.));
    assert_eq!(at("e"), (100., 20., 100.));
    // Its box spans what it holds.
    assert_eq!(bounds(&ui, &view, "group"), Rect::new(0., 0., 300., 40.));
    let scene = ui.scene.borrow();
    let group = view.find("group").unwrap();
    assert_eq!(
        scene.children(group).len(),
        3,
        "the tree keeps them under it"
    );
}

#[test]
fn in_a_row_each_child_is_an_item_with_the_rows_gap() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        row().gap(10.).child(div().id("a").w(30.).h(10.)).child(
            div()
                .display_contents()
                // Its own size and padding don't apply.
                .w(1000.)
                .p(50.)
                .child(div().id("b").w(40.).h(10.))
                .child(div().id("c").w(50.).h(10.)),
        ),
    );
    ui.prepare_frame();
    let x = |id| bounds(&ui, &view, id).x;
    assert_eq!((x("a"), x("b"), x("c")), (0., 40., 90.));
}

#[test]
fn a_child_that_changes_or_comes_and_goes_lays_out_again() {
    let mut ui = Ui::new(400., 300.);
    let (wide, many) = (ui.signal(40_f32), ui.signal(2_usize));
    let (read_wide, read_many) = (wide.clone(), many.clone());
    let view = ui.mount(
        row().gap(10.).child(
            div()
                .display_contents()
                .child(
                    div()
                        .id("first")
                        .h(10.)
                        .reactive_style(move || Styles::new().w(read_wide.get())),
                )
                .child(keyed(
                    move || (0..read_many.get()).collect::<Vec<_>>(),
                    |i, _| div().id(format!("k{i}")).w(20.).h(10.),
                )),
        ),
    );
    ui.prepare_frame();
    wide.set(100.);
    ui.prepare_frame();
    let x = |ui: &Ui, id: &str| view.find(id).map(|n| ui.scene.borrow().bounds(n).x);
    assert_eq!(
        x(&ui, "k0"),
        Some(110.),
        "after the wider first child and the gap"
    );
    many.set(3);
    ui.prepare_frame();
    assert!(x(&ui, "k2").is_some());
    let _ = Modifiers::default();
}

#[test]
fn focus_keeps_document_order() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div()
            .grid()
            .grid_cols(2)
            .w(200.)
            .child(div().id("one").h(20.).focusable(true).on_click(|| {}))
            .child(
                div()
                    .display_contents()
                    .child(div().id("two").h(20.).focusable(true).on_click(|| {}))
                    .child(div().id("three").h(20.).focusable(true).on_click(|| {})),
            ),
    );
    ui.prepare_frame();
    let mut order = Vec::new();
    for _ in 0..3 {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.prepare_frame();
        let focused = ui.input.focused().unwrap();
        order.push(
            ["one", "two", "three"]
                .into_iter()
                .find(|id| view.find(id) == Some(focused)),
        );
    }
    assert_eq!(order, [Some("one"), Some("two"), Some("three")]);
}

/// Each leaf of `build(true)`, children laid out in their place, where it is
/// in `build(false)`, the same leaves written as the parent's own.
fn lays_out_as_written_flat(ids: &[&str], build: impl Fn(bool) -> View) {
    let place = |flat: bool| {
        let mut ui = Ui::new(400., 300.);
        let view = ui.mount(build(flat));
        ui.prepare_frame();
        ids.iter()
            .map(|id| bounds(&ui, &view, id))
            .collect::<Vec<_>>()
    };
    assert_eq!(place(true), place(false));
}
/// `items` in a group laid out in its place, or as they are.
fn grouped(flat: bool, items: Vec<View>) -> Vec<View> {
    match flat {
        true => vec![div().id("group").display_contents().children(items)],
        false => items,
    }
}

#[test]
fn in_a_column_or_row_the_children_lay_out_as_its_own() {
    let leaves = ["a", "b", "c", "d"];
    let items = || {
        vec![
            div().id("b").h(20.).w(30.),
            div().id("c").grow().h(10.),
            div().id("d").h(15.).w_percent(50.),
        ]
    };
    let justifies: [fn(View) -> View; 3] = [
        |v| v.justify_start(),
        |v| v.justify_center(),
        |v| v.justify_between(),
    ];
    for justify in justifies {
        lays_out_as_written_flat(&leaves, |flat| {
            justify(column().h(200.))
                .child(div().id("a").h(10.))
                .children(grouped(flat, items()))
        });
        lays_out_as_written_flat(&leaves, |flat| {
            justify(row().w(300.).h(50.).items_center())
                .child(div().id("a").w(10.).h(10.))
                .children(grouped(flat, items()))
        });
    }
}

#[test]
fn nested_groups_and_absolute_children_lay_out_as_its_own() {
    lays_out_as_written_flat(&["a", "b", "c", "pinned"], |flat| {
        let inner = grouped(
            flat,
            vec![
                div().id("b").h(20.),
                div().id("pinned").absolute().right(4.).top(4.).w(8.).h(8.),
            ],
        );
        let items = grouped(
            flat,
            inner.into_iter().chain([div().id("c").h(30.)]).collect(),
        );
        column()
            .w(200.)
            .p(5.)
            .child(div().id("a").h(10.))
            .children(items)
    });
}

#[test]
fn in_a_column_a_child_that_changes_or_comes_and_goes_lays_out_again() {
    let mut ui = Ui::new(400., 300.);
    let (tall, many) = (ui.signal(10_f32), ui.signal(2_usize));
    let (read_tall, read_many) = (tall.clone(), many.clone());
    let view = ui.mount(
        column().child(
            div()
                .id("group")
                .display_contents()
                .child(
                    div()
                        .id("first")
                        .w(10.)
                        .reactive_style(move || Styles::new().h(read_tall.get())),
                )
                .child(keyed(
                    move || (0..read_many.get()).collect::<Vec<_>>(),
                    |i, _| div().id(format!("k{i}")).w(20.).h(10.),
                )),
        ),
    );
    ui.prepare_frame();
    tall.set(40.);
    ui.prepare_frame();
    let y = |ui: &Ui, id: &str| view.find(id).map(|n| ui.scene.borrow().bounds(n).y);
    assert_eq!(y(&ui, "k0"), Some(40.), "below the taller first child");
    many.set(3);
    ui.prepare_frame();
    assert_eq!(y(&ui, "k2"), Some(60.));
    assert_eq!(bounds(&ui, &view, "group"), Rect::new(0., 0., 20., 70.));
}

/// A group's box spans what it holds: as a child widens it, the child and
/// the group's box are repainted where they grow.
#[test]
fn a_child_that_widens_its_group_is_repainted_where_it_grows() {
    let mut ui = Ui::new(400., 300.);
    let wide = ui.signal(40_f32);
    let read = wide.clone();
    ui.mount(
        column().items_start().child(
            div()
                .display_contents()
                .bg(zgx::scene::Color(0, 0, 255, 255))
                .child(
                    div()
                        .h(20.)
                        .bg(zgx::scene::Color(255, 0, 0, 255))
                        .reactive_style(move || Styles::new().w(read.get())),
                )
                .child(div().w(30.).h(20.)),
        ),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    wide.set(120.);
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    let covered = |x: f32, y: f32| {
        damage
            .iter()
            .any(|r| r.x <= x && x < r.x + r.width && r.y <= y && y < r.y + r.height)
    };
    assert!(covered(100., 10.), "where the child grew: {damage:?}");
    assert!(covered(100., 30.), "where the group's box grew: {damage:?}");
}
