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
