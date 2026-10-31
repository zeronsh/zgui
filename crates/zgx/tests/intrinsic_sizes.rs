//! Widths from what a box holds: `min-content`, `max-content` and
//! `fit-content`. The expected numbers are worked out from CSS's rules.
use zgx::{compose::prelude::*, size_calc::SizeCalc, style::Styles, widgets::Ui};

/// Three 40-wide boxes in a row that wraps: 40 at its narrowest, 120 laid
/// out on one line.
fn boxes(count: usize) -> View {
    let mut row = row().id("a").flex_wrap().h(40.);
    for _ in 0..count {
        row = row.child(div().size(40., 10.).shrink_0());
    }
    row
}
fn width_in(parent: f32, sized: impl FnOnce(View) -> View) -> f32 {
    let mut ui = Ui::new(600., 400.);
    let view = ui.mount(column().w(parent).h(200.).child(sized(boxes(3))));
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    ui.scene.borrow().bounds(a).width
}
fn size(written: &str) -> SizeCalc {
    written.parse().unwrap()
}

#[test]
fn max_content_is_its_content_on_one_line_and_does_not_stretch() {
    assert_eq!(width_in(300., |view| view), 300., "a column stretches it");
    assert_eq!(
        width_in(300., |view| view.w_calc(size("max-content"))),
        120.
    );
    // Wider than the column, as CSS lets it overflow.
    assert_eq!(
        width_in(100., |view| view.w_calc(size("max-content"))),
        120.
    );
}

#[test]
fn min_content_is_its_narrowest() {
    assert_eq!(width_in(300., |view| view.w_calc(size("min-content"))), 40.);
}

#[test]
fn fit_content_takes_the_room_between_the_two() {
    let fit = |parent| width_in(parent, |view| view.w_calc(size("fit-content")));
    assert_eq!(fit(300.), 120.);
    assert_eq!(fit(90.), 90.);
    assert_eq!(fit(20.), 40.);
    let limited = |parent| width_in(parent, |view| view.w_calc(size("fit-content(100px)")));
    assert_eq!(limited(300.), 100.);
    assert_eq!(limited(50.), 100.);
}

#[test]
fn limits_take_them_too() {
    assert_eq!(
        width_in(300., |view| view.max_w_calc(size("max-content"))),
        120.,
        "stretched no further than its content"
    );
    assert_eq!(
        width_in(300., |view| view.w(10.).min_w_calc(size("min-content"))),
        40.
    );
}

#[test]
fn it_follows_what_it_holds() {
    let mut ui = Ui::new(600., 400.);
    let wide = ui.signal(120_f32);
    let read = wide.clone();
    let view = ui.mount(
        column().w(300.).h(200.).child(
            row().id("a").w_calc(size("max-content")).child(
                div()
                    .h(10.)
                    .reactive_style(move || Styles::new().w(read.get())),
            ),
        ),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    assert_eq!(ui.scene.borrow().bounds(a).width, 120.);
    for width in [200., 40., 400.] {
        wide.set(width);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow().bounds(a).width, width);
    }
}

#[test]
fn on_a_height_it_lays_out_as_auto() {
    let mut ui = Ui::new(600., 400.);
    let view = ui.mount(
        column().w(300.).h(200.).child(
            row()
                .id("a")
                .w(50.)
                .h_calc(size("max-content"))
                .child(div().size(40., 30.)),
        ),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    assert_eq!(ui.scene.borrow().bounds(a).height, 30.);
}

/// A child that takes a content size once its parent has laid out, and
/// one added later, are measured as one there from the start.
#[test]
fn a_content_size_given_later_or_to_a_new_child_takes_hold() {
    let mut ui = Ui::new(600., 400.);
    let sized = ui.signal(false);
    let read = sized.clone();
    let view = ui.mount(
        column().w(300.).h(200.).gap_x(1.).child(
            row()
                .id("a")
                .h(10.)
                .child(div().size(120., 10.).shrink_0())
                .reactive_style(move || match read.get() {
                    true => Styles::new().w_calc(size("max-content")),
                    false => Styles::new(),
                }),
        ),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    assert_eq!(ui.scene.borrow().bounds(a).width, 300.);
    sized.set(true);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(a).width, 120.);
    sized.set(false);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(a).width, 300.);
}
