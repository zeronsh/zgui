//! Sizes from the containing block (`calc()`, `min()`, `max()`, `clamp()`).
//! The expected numbers are worked out from CSS's rules.
use zgx::{compose::prelude::*, scene::Rect, size_calc::SizeCalc, style::Styles, widgets::Ui};

fn calc(written: &str) -> SizeCalc {
    written.parse().unwrap()
}
fn laid(ui: &mut Ui, view: View, id: &str) -> Rect {
    let view = ui.mount(view);
    ui.prepare_frame();
    let node = view.find(id).unwrap();
    ui.scene.borrow().bounds(node)
}

#[test]
fn a_width_takes_the_parents_less_a_gutter() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        column()
            .w(300.)
            .h(200.)
            .child(div().id("a").w_calc(calc("calc(100% - 24px)")).h(10.)),
        "a",
    );
    assert_eq!((rect.width, rect.height), (276., 10.));
}

#[test]
fn a_height_takes_a_definite_parents() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        row()
            .w(300.)
            .h(200.)
            .child(div().id("a").w(10.).h_calc(calc("calc(50% + 10px)"))),
        "a",
    );
    assert_eq!(rect.height, 110.);
}

#[test]
fn its_base_is_the_content_box() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        column()
            .w(300.)
            .h(200.)
            .p(10.)
            .child(div().id("a").w_calc(calc("calc(50% + 1px)")).h(10.)),
        "a",
    );
    assert_eq!(rect.width, 141.);
}

#[test]
fn limits_bound_a_size_from_either_side() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        column()
            .w(300.)
            .h(200.)
            .child(
                div()
                    .id("a")
                    .w(1000.)
                    .max_w_calc(calc("min(90%, 400px)"))
                    .h(10.),
            )
            .child(
                div()
                    .id("b")
                    .w(10.)
                    .min_w_calc(calc("calc(50% - 10px)"))
                    .h(10.),
            ),
        "a",
    );
    assert_eq!(rect.width, 270.);
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        column().w(300.).h(200.).child(
            div()
                .id("b")
                .w(10.)
                .min_w_calc(calc("calc(50% - 10px)"))
                .h(10.),
        ),
        "b",
    );
    assert_eq!(rect.width, 140.);
}

#[test]
fn clamp_follows_the_window() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().w_percent(100.).h(100.).child(
            div()
                .id("a")
                .w_calc(calc("clamp(100px, 50%, 300px)"))
                .h(10.),
        ),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    assert_eq!(ui.scene.borrow().bounds(a).width, 200.);
    for (window, width) in [(100., 100.), (1000., 300.), (400., 200.)] {
        ui.scene.borrow_mut().resize(window, 300.);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow().bounds(a).width, width, "{window}");
    }
}

/// A percentage of a parent with no size of its own is `auto`, as in CSS:
/// the box takes its content's size.
#[test]
fn under_a_parent_with_no_size_it_sizes_as_auto() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        row().child(
            column()
                .id("a")
                .h_calc(calc("calc(50% + 0px)"))
                .child(div().w(30.).h(20.)),
        ),
        "a",
    );
    assert_eq!(rect.height, 20.);
}

#[test]
fn it_follows_a_parent_that_changes_size() {
    let mut ui = Ui::new(600., 400.);
    let wide = ui.signal(300_f32);
    let state = wide.clone();
    let view = ui.mount(
        column()
            .h(200.)
            .reactive_style(move || Styles::new().w(state.get()))
            .child(div().id("a").h(10.).w_calc(calc("calc(50% - 10px)"))),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    for (parent, width) in [(300., 140.), (500., 240.), (100., 40.)] {
        wide.set(parent);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow().bounds(a).width, width, "{parent}");
    }
}

/// A parent as wide as what it holds is sized without the expression, which
/// then takes its share of that, as a percentage does in CSS.
#[test]
fn a_parent_sized_by_its_content_gives_it_a_base() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        row().child(
            column()
                .child(div().w(100.).h(10.))
                .child(div().id("a").w_calc(calc("calc(100% - 10px)")).h(10.)),
        ),
        "a",
    );
    assert_eq!(rect.width, 90.);
}

/// Unlike CSS, whose base is the cell (the limit docs/reference states).
#[test]
fn in_a_grid_its_base_is_the_grid() {
    let mut ui = Ui::new(400., 300.);
    let rect = laid(
        &mut ui,
        div()
            .grid()
            .grid_cols(2)
            .w(200.)
            .h(100.)
            .child(div().id("a").w_calc(calc("calc(50% + 0px)")).h(10.)),
        "a",
    );
    assert_eq!(rect.width, 100.);
}

#[test]
fn it_follows_a_style_and_lets_go() {
    let mut ui = Ui::new(400., 300.);
    let on = ui.signal(0);
    let state = on.clone();
    let view = ui.mount(
        column()
            .w(300.)
            .h(200.)
            .child(
                div()
                    .id("a")
                    .h(10.)
                    .reactive_style(move || match state.get() {
                        0 => Styles::new().w_calc(calc("calc(100% - 24px)")),
                        1 => Styles::new().w_calc(calc("calc(100% - 48px)")),
                        _ => Styles::new().w(30.),
                    }),
            ),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    assert_eq!(ui.scene.borrow().bounds(a).width, 276.);
    on.set(1);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(a).width, 252.);
    on.set(2);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(a).width, 30.);
}

#[test]
fn a_removed_node_leaves_no_size_behind() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let state = shown.clone();
    ui.mount(column().w(300.).h(200.).child(switch(
        move || state.get(),
        |shown, _| match shown {
            true => div().w_calc(calc("calc(100% - 24px)")).h(10.),
            false => div(),
        },
    )));
    ui.prepare_frame();
    for at in [false, true, false, true] {
        shown.set(at);
        ui.prepare_frame();
    }
}
