//! Grid areas, named lines and dense flow: items placed by name, as CSS
//! Grid places them. The numbers are worked out from its rules.
use zgx::{
    compose::prelude::*,
    grid_tracks::{GridFlow, GridLines, GridTracks},
    scene::Rect,
    widgets::Ui,
};

fn bounds(ui: &Ui, view: &zgx::compose::ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(view.find(id).unwrap())
}
fn tracks(text: &str) -> GridTracks {
    text.parse().unwrap()
}
fn lines(text: &str) -> GridLines {
    text.parse().unwrap()
}

#[test]
fn items_fill_the_areas_they_name() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div()
            .grid()
            .w(300.)
            .h(200.)
            .grid_template_columns(tracks("100 1fr"))
            .grid_template_rows(tracks("20 1fr"))
            .grid_template_areas("head head\nside main".parse().unwrap())
            // In the document out of order: the areas place them.
            .child(div().id("main").grid_area("main"))
            .child(div().id("side").grid_area("side"))
            .child(div().id("head").grid_area("head")),
    );
    ui.prepare_frame();
    assert_eq!(bounds(&ui, &view, "head"), Rect::new(0., 0., 300., 20.));
    assert_eq!(bounds(&ui, &view, "side"), Rect::new(0., 20., 100., 180.));
    assert_eq!(bounds(&ui, &view, "main"), Rect::new(100., 20., 200., 180.));
}

#[test]
fn named_lines_and_numbers_place_an_item() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div()
            .grid()
            .w(300.)
            .grid_template_columns(tracks("[left] 100 [mid] 1fr [right]"))
            .child(div().id("b").h(10.).grid_column(lines("mid / right")))
            .child(div().id("a").h(10.).grid_column(lines("left / span 1")))
            .child(div().id("all").h(10.).grid_column(lines("1 / -1"))),
    );
    ui.prepare_frame();
    let b = bounds(&ui, &view, "b");
    assert_eq!((b.x, b.width), (100., 200.));
    let a = bounds(&ui, &view, "a");
    assert_eq!((a.x, a.width), (0., 100.));
    let all = bounds(&ui, &view, "all");
    assert_eq!((all.x, all.width), (0., 300.));
}

/// Three columns, items spanning 2, 2 and 1. Along rows, the third goes
/// after the second (row 2, column 3); dense, into the hole the second
/// left in row 1.
#[test]
fn dense_flow_fills_holes() {
    let place = |flow: Option<GridFlow>| {
        let mut ui = Ui::new(400., 300.);
        let mut grid = div().grid().w(300.).grid_cols(3);
        if let Some(flow) = flow {
            grid = grid.grid_flow(flow);
        }
        let view = ui.mount(
            grid.child(div().h(10.).col_span(2))
                .child(div().h(10.).col_span(2))
                .child(div().id("c").h(10.)),
        );
        ui.prepare_frame();
        let c = bounds(&ui, &view, "c");
        (c.x, c.y)
    };
    assert_eq!(place(None), (200., 10.));
    assert_eq!(place(Some(GridFlow::RowDense)), (200., 0.));
}

#[test]
fn a_name_and_a_number_place_on_both_axes() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div()
            .grid()
            .w(300.)
            .h(100.)
            .grid_cols(3)
            .grid_template_rows(tracks("[top] 50 [bottom] 50"))
            .child(
                div()
                    .id("x")
                    .grid_column(lines("2 / span 2"))
                    .grid_row(lines("bottom")),
            ),
    );
    ui.prepare_frame();
    assert_eq!(bounds(&ui, &view, "x"), Rect::new(100., 50., 200., 50.));
}
