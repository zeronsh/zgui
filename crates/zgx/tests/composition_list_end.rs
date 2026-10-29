//! A virtual list's end coming in reach (`View::on_end`): called once each
//! time, again only once its content has grown, and while its content is
//! shorter than its viewport, until it fills or a call adds nothing.
use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, reactive::Signal, widgets::Ui};

/// A 300-point list of 20-point rows, `count` long, its end 50 points out:
/// each time it is reached, `adds` more rows (while there are more to add).
fn list(
    ui: &mut Ui,
    count: Signal<usize>,
    adds: usize,
    more: usize,
) -> (Signal<f32>, Rc<Cell<usize>>) {
    let offset = ui.signal(0.0_f32);
    let reached = Rc::new(Cell::new(0));
    let (calls, rows) = (reached.clone(), count.clone());
    let added = Rc::new(Cell::new(0));
    let read = count.clone();
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            move || read.get(),
            |index| index,
            |_, _, _| div().h(20.),
        )
        .on_end(50., move || {
            calls.set(calls.get() + 1);
            if added.get() < more {
                added.set(added.get() + adds);
                rows.update(|n| *n += adds);
            }
        })
        .size(200., 300.),
    );
    (offset, reached)
}

#[test]
fn a_short_list_asks_until_it_fills() {
    let mut ui = Ui::new(400., 400.);
    let count = ui.signal(5_usize);
    // 5 rows (100 points) in 300: asked, given 5 more each time, until the
    // rows pass the viewport and its end is out of reach.
    let (_, reached) = list(&mut ui, count.clone(), 5, 100);
    for _ in 0..10 {
        ui.prepare_frame();
    }
    // 20 rows (400 points) leave 100 past the viewport, more than 50.
    assert_eq!(count.get(), 20);
    assert_eq!(
        reached.get(),
        3,
        "100, 200 and 300 points were in reach; 400 isn't"
    );
}

#[test]
fn scrolling_into_reach_asks_once_until_it_grows() {
    let mut ui = Ui::new(400., 400.);
    let count = ui.signal(30_usize);
    let (offset, reached) = list(&mut ui, count.clone(), 10, 10);
    ui.prepare_frame();
    assert_eq!(reached.get(), 0, "600 points: the end is 300 out");
    offset.set(240.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 0, "60 out: not in reach");
    offset.set(260.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 1, "40 out: in reach");
    assert_eq!(count.get(), 40);
    // Scrolling about the same content asks no more.
    offset.set(200.);
    ui.prepare_frame();
    offset.set(280.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 1);
    // Grown, its new end reached: asked again; this time nothing comes.
    offset.set(460.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 2);
    assert_eq!(count.get(), 40, "no more to add");
    // Nothing came: it doesn't ask again however it scrolls.
    offset.set(400.);
    ui.prepare_frame();
    offset.set(500.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 2);
}

#[test]
fn a_measured_list_asks_as_its_end_comes_in_reach() {
    let mut ui = Ui::new(400., 400.);
    let heights = VariableHeights::new(&ui.runtime, 3, 20.);
    let reached = Rc::new(Cell::new(0));
    let calls = reached.clone();
    let rows = heights.clone();
    ui.mount(
        measured_virtual_list(
            ui.signal(0.),
            heights.clone(),
            1,
            |index| index,
            |_, _, _| div().h(20.),
        )
        .on_end(0., move || {
            calls.set(calls.get() + 1);
            if rows.len() < 30 {
                let _ = rows.resize(rows.len() + 10);
            }
        })
        .size(200., 300.),
    );
    for _ in 0..10 {
        ui.prepare_frame();
    }
    // 3, 13 rows were short of 300 points; 23 rows (460) fill it.
    assert_eq!(reached.get(), 2);
    assert_eq!(heights.len(), 23);
}
