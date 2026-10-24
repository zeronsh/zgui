//! `dir = "rtl"` as scripts write it: a row from the right, the start edges
//! at the right, `pl` still on the left.
mod common;
use common::Harness;

#[test]
fn dir_rtl_runs_a_row_from_the_right() {
    let mut h = Harness::inline(
        r##"return ui.column { w = 300, dir = "rtl", items = "start",
            ui.row { w = 300, gap = 10,
                ui.div { id = "a", w = 50, h = 10, shrink = 0 },
                ui.div { id = "b", w = 30, h = 10, shrink = 0 },
            },
            ui.row { w = 300, ps = 20, ui.div { id = "logical", w = 50, h = 10 } },
            ui.row { w = 300, pl = 20, ui.div { id = "physical", w = 50, h = 10 } },
            ui.text { "שלום", id = "text" },
        }"##,
    );
    assert_eq!(h.bounds("a").x, 250.);
    assert_eq!(h.bounds("b").x, 210.);
    assert_eq!(h.bounds("logical").x, 230.);
    assert_eq!(h.bounds("physical").x, 250.);
    // A column's children start at its right.
    let text = h.bounds("text");
    assert_eq!(text.x + text.width, 300.);
}
