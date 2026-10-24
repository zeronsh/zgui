//! A box that sizes to its content inside a narrower parent fits it (CSS
//! fit-content): no wider than the room unless its content cannot shrink,
//! so a wrapping row wraps instead of widening every stretched ancestor.
use zgx::{compose::prelude::*, scene::Align, scene::NodeId, widgets::Ui};

/// Three 120 px tiles in a row that may shrink and wrap, after a 10 px gutter.
fn tiles() -> View {
    let tile = || div().w(120.).h(10.);
    row().child(div().w(10.)).child(
        row()
            .grow()
            .flex_shrink(1.)
            .min_w(0.)
            .flex_wrap()
            .gap(4.)
            .child(tile())
            .child(tile())
            .child(tile()),
    )
}
/// Mount `view` in a 300 px column; the bounds along its first-child chain.
fn chain(view: View) -> Vec<zgx::scene::Rect> {
    let mut ui = Ui::new(400., 300.);
    // The view as wide as it makes itself (or asks to be), not stretched.
    let _view = ui.mount(column().w(300.).items_start().child(view));
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let mut id: NodeId = scene.children(scene.root())[0];
    let mut chain = Vec::new();
    loop {
        chain.push(scene.bounds(id));
        match scene.children(id).first() {
            Some(child) => id = *child,
            None => return chain,
        }
    }
}

#[test]
fn stretched_regions_in_a_presence_wrapper_keep_their_parents_width() {
    // As zgx-script's presence lowers: a full-width overlay, a full-width
    // div per entry, the content column stretching a region of rows.
    let region = column()
        .items_stretch()
        .align_self(Align::Stretch)
        .child(tiles());
    let chain = chain(
        overlay()
            .w_full()
            .child(div().w_full().child(column().items_stretch().child(region))),
    );
    // Column, overlay, div, content, region, row: all 300 wide.
    for (depth, rect) in chain[..6].iter().enumerate() {
        assert_eq!(rect.width, 300., "level {depth} of {chain:?}");
    }
    // The tiles shrink to the 290 left beside the gutter and wrap: two
    // rows of 10 and the 4 px gap.
    assert_eq!(chain[5].height, 24., "{chain:?}");
}

#[test]
fn shrink_to_fit_boxes_fit_the_room_but_not_below_their_content() {
    // A wrapping row in a column that does not stretch it still fits the
    // room: it wraps.
    let chain1 = chain(column().child(tiles()));
    assert_eq!(chain1[1].width, 300., "{chain1:?}");
    assert_eq!(chain1[1].height, 24.);
    // Tiles that cannot shrink keep their width (their min-content), as in
    // CSS, overflowing the room.
    let fixed = || div().w(120.).h(10.).shrink_0();
    let rigid = row().min_w(0.).child(fixed()).child(fixed()).child(fixed());
    let chain2 = chain(column().child(rigid));
    assert_eq!(chain2[1].width, 360., "{chain2:?}");
    // Under a full-width stretching parent, a region is its width even so.
    let rigid = row().min_w(0.).child(fixed()).child(fixed()).child(fixed());
    let chain3 = chain(
        column()
            .w_full()
            .items_stretch()
            .child(column().align_self(Align::Stretch).child(rigid)),
    );
    assert_eq!(
        (chain3[1].width, chain3[2].width),
        (300., 300.),
        "{chain3:?}"
    );
}
