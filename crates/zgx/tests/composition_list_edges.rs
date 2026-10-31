//! A virtual list's start coming in reach (`View::on_start`), whether it
//! rests at its end (`View::on_at_end`), and its rows in view staying where
//! they are as rows are added above them (a history's earlier page).
use std::{cell::Cell, cell::RefCell, rc::Rc};
use zgx::{
    compose::{ViewHandle, prelude::*},
    reactive::Signal,
    widgets::Ui,
};

/// A 200-point list of 20-point rows keyed `keys`, measured.
fn history(ui: &mut Ui, keys: Signal<Vec<u32>>) -> (Signal<f32>, VariableHeights, ViewHandle) {
    let offset = ui.signal(0_f32);
    let heights = VariableHeights::new(&ui.runtime, 0, 20.);
    let rows = keys.clone();
    let view = ui.mount(
        measured_rows(
            offset.clone(),
            heights.clone(),
            1,
            move || rows.get(),
            |_: &[u32], _| 20.,
            |_, key, _| div().id(format!("row-{key}")).h(20.),
        )
        // A list that loads earlier pages keeps its place as they come.
        .on_start(0., || {})
        .size(200., 200.),
    );
    (offset, heights, view)
}

#[test]
fn its_start_in_reach_is_told_once_until_its_content_grows() {
    let mut ui = Ui::new(400., 400.);
    let count = ui.signal(50_usize);
    let offset = ui.signal(400_f32);
    let reached = Rc::new(Cell::new(0));
    let (calls, read) = (reached.clone(), count.clone());
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            move || read.get(),
            |index| index,
            |_, _, _| div().h(20.),
        )
        .on_start(50., move || calls.set(calls.get() + 1))
        .size(200., 200.),
    );
    ui.prepare_frame();
    assert_eq!(
        reached.get(),
        0,
        "400 points down: the start is out of reach"
    );
    offset.set(40.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 1, "40 points down: in reach");
    offset.set(0.);
    ui.prepare_frame();
    offset.set(30.);
    ui.prepare_frame();
    assert_eq!(reached.get(), 1, "the same content: told once");
    count.set(60);
    ui.prepare_frame();
    assert_eq!(reached.get(), 2, "grown while in reach: told again");
}

#[test]
fn it_says_whether_it_rests_at_its_end() {
    let mut ui = Ui::new(400., 400.);
    let offset = ui.signal(0_f32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let told = seen.clone();
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            || 30,
            |index| index,
            |_, _, _| div().h(20.),
        )
        .on_at_end(move |at_end| told.borrow_mut().push(at_end))
        .size(200., 200.),
    );
    ui.prepare_frame();
    assert_eq!(*seen.borrow(), [false], "told once laid out");
    // 600 points of rows in 200: the end is at 400.
    offset.set(398.);
    ui.prepare_frame();
    assert_eq!(*seen.borrow(), [false, true], "within a few points of it");
    offset.set(399.);
    ui.prepare_frame();
    assert_eq!(*seen.borrow(), [false, true], "told only as it changes");
    offset.set(300.);
    ui.prepare_frame();
    assert_eq!(*seen.borrow(), [false, true, false]);
}

#[test]
fn rows_added_above_leave_the_rows_in_view_where_they_are() {
    let mut ui = Ui::new(400., 400.);
    let keys = ui.signal((100..140_u32).collect::<Vec<_>>());
    let (offset, heights, view) = history(&mut ui, keys.clone());
    ui.prepare_frame();
    ui.prepare_frame();
    // Row 110 at the top of the view, 5 points of it above.
    offset.set(205.);
    ui.prepare_frame();
    ui.prepare_frame();
    let shown = |ui: &Ui| {
        let scene = ui.scene.borrow();
        let row = view
            .find("row-110")
            .and_then(|id| scene.contains(id).then(|| scene.transformed_bounds(id)))
            .expect("row 110 shown");
        row.y
    };
    let before = shown(&ui);
    // An earlier page: ten rows above.
    keys.update(|keys| {
        keys.splice(0..0, 90..100);
    });
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(heights.len(), 50);
    assert_eq!(offset.get(), 405., "moved down by the ten rows added");
    assert_eq!(shown(&ui), before, "row 110 where it was");
    // Rows added below change nothing in view.
    keys.update(|keys| keys.extend(140..150));
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(offset.get(), 405.);
    assert_eq!(shown(&ui), before);
}
