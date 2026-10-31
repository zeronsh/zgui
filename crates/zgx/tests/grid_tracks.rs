//! Grid tracks of any size (`grid_template_columns`, `grid_template_rows`).
//! The numbers are worked out from CSS Grid's rules, in the comments where
//! they aren't plain.
use zgx::{
    compose::prelude::*, grid_tracks::GridTracks, reactive::Signal, scene::Rect, style::Styles,
    widgets::Ui,
};

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}
fn bounds(ui: &Ui, view: &zgx::compose::ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(view.find(id).unwrap())
}
fn tracks(text: &str) -> GridTracks {
    text.parse().unwrap()
}
/// A grid `w` wide of `count` cells named `c0`, `c1`…
fn cells(list: &str, w: f32, count: usize) -> View {
    div()
        .grid()
        .w(w)
        .grid_template_columns(tracks(list))
        .children((0..count).map(|i| div().id(format!("c{i}")).h(20.)))
}

#[test]
fn fixed_shares_and_percentages_take_their_room() {
    let mut ui = Ui::new(800., 400.);
    let view = ui.mount(cells("220 1fr 2fr 10%", 620., 4));
    ui.prepare_frame();
    // 620 less 220 and 62 leaves 338: 1fr is 112.67.
    for (cell, x, w) in [
        ("c0", 0., 220.),
        ("c1", 220., 112.67),
        ("c2", 332.67, 225.33),
        ("c3", 558., 62.),
    ] {
        let b = bounds(&ui, &view, cell);
        near(b.x, x);
        near(b.width, w);
    }
}

#[test]
fn an_auto_track_is_the_size_of_what_it_holds() {
    let mut ui = Ui::new(800., 400.);
    let view = ui.mount(
        div()
            .grid()
            .w(500.)
            .grid_template_columns(tracks("auto 1fr"))
            .child(div().id("label").w(120.).h(20.))
            .child(div().id("field").h(20.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "field").x, 120.);
    near(bounds(&ui, &view, "field").width, 380.);
}

#[test]
fn minmax_holds_a_track_between_its_limits() {
    let mut ui = Ui::new(800., 400.);
    let view = ui.mount(cells("minmax(100, 200) 1fr", 500., 2));
    ui.prepare_frame();
    near(bounds(&ui, &view, "c0").width, 200.);
    near(bounds(&ui, &view, "c1").width, 300.);
}

#[test]
fn auto_fill_fits_what_it_can_and_follows_the_width() {
    let mut ui = Ui::new(350., 400.);
    let view = ui.mount(
        div()
            .grid()
            .w_full()
            .grid_template_columns(tracks("repeat(auto-fill, minmax(100, 1fr))"))
            .children((0..5).map(|i| div().id(format!("c{i}")).h(20.))),
    );
    ui.prepare_frame();
    // Three tracks of 116.67 fit 350; the fourth cell wraps.
    near(bounds(&ui, &view, "c1").x, 116.67);
    near(bounds(&ui, &view, "c3").x, 0.);
    near(bounds(&ui, &view, "c3").y, 20.);
    ui.scene.borrow_mut().resize(520., 400.);
    ui.prepare_frame();
    // Five fit now, with room for all of them on one line.
    near(bounds(&ui, &view, "c4").x, 416.);
    near(bounds(&ui, &view, "c4").y, 0.);
}

#[test]
fn auto_fit_collapses_the_tracks_it_leaves_empty() {
    for (list, width) in [
        ("repeat(auto-fit, minmax(100, 1fr))", 300.),
        ("repeat(auto-fill, minmax(100, 1fr))", 100.),
    ] {
        let mut ui = Ui::new(800., 400.);
        let view = ui.mount(cells(list, 600., 2));
        ui.prepare_frame();
        near(bounds(&ui, &view, "c0").width, width);
    }
}

#[test]
fn rows_take_track_lists_too() {
    let mut ui = Ui::new(800., 400.);
    let view = ui.mount(
        div()
            .grid()
            .size(300., 300.)
            .grid_template_rows(tracks("50 1fr 2fr"))
            .children((0..3).map(|i| div().id(format!("c{i}")))),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "c0").height, 50.);
    near(bounds(&ui, &view, "c1").height, 83.33);
    near(bounds(&ui, &view, "c2").y, 133.33);
}

#[test]
fn a_scroll_views_tracks_lay_its_content_out() {
    let mut ui = Ui::new(800., 400.);
    let offset = ui.runtime.signal(0.);
    let view = ui.mount(
        scroll(offset)
            .grid()
            .size(400., 100.)
            .grid_template_columns(tracks("100 1fr"))
            .child(div().id("a").h(300.))
            .child(div().id("b").h(300.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "b").x, 100.);
    near(bounds(&ui, &view, "b").width, 300.);
}

/// A sidebar dragged: a track list per pointer move. A list lasts as long
/// as the node showing it, however many there have been.
#[test]
fn thousands_of_track_lists_keep_only_what_nodes_show() {
    let mut ui = Ui::new(800., 400.);
    let width: Signal<f32> = ui.runtime.signal(100.);
    let read = width.clone();
    let view = ui.mount(
        div()
            .grid()
            .w(800.)
            .reactive_style(move || {
                Styles::new().grid_template_columns(tracks(&format!("{} 1fr", read.get())))
            })
            .child(div().id("side").h(20.))
            .child(div().id("main").h(20.)),
    );
    ui.prepare_frame();
    for step in 0..5_000 {
        let at = 100. + step as f32 * 0.1;
        width.set(at);
        ui.prepare_frame();
        if step % 997 == 0 {
            near(bounds(&ui, &view, "side").width, at);
        }
        assert_eq!(ui.scene.borrow().grid_template_count(), 1);
    }
    near(bounds(&ui, &view, "main").x, 599.9);
}

#[test]
fn a_nodes_track_lists_go_with_it() {
    let mut ui = Ui::new(800., 400.);
    let shown = ui.runtime.signal(true);
    let read = shown.clone();
    let _view = ui.mount(switch(
        move || read.get(),
        |shown, _| {
            if shown {
                cells("100 1fr", 400., 2)
            } else {
                div()
            }
        },
    ));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().grid_template_count(), 1);
    shown.set(false);
    for _ in 0..4 {
        ui.prepare_frame();
        ui.reclaim(std::time::Duration::from_secs(1));
    }
    assert_eq!(ui.scene.borrow().grid_template_count(), 0);
}

/// A style dropping its lists goes back to equal tracks.
#[test]
fn a_style_without_track_lists_clears_them() {
    let mut ui = Ui::new(800., 400.);
    let custom = ui.runtime.signal(true);
    let read = custom.clone();
    let view = ui.mount(
        div()
            .grid()
            .w(400.)
            .grid_cols(2)
            .reactive_style(move || {
                if read.get() {
                    Styles::new().grid_template_columns(tracks("100 1fr"))
                } else {
                    Styles::new()
                }
            })
            .child(div().id("a").h(20.))
            .child(div().id("b").h(20.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "b").x, 100.);
    custom.set(false);
    ui.prepare_frame();
    near(bounds(&ui, &view, "b").x, 200.);
    assert_eq!(ui.scene.borrow().grid_template_count(), 0);
}
