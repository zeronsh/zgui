//! The variable-height list scene's row (zgx_workload::vlist), shared by
//! the windowed `vlist_workload` and the headless `micro_bench`.
use zgx::compose::prelude::*;
use zgx_workload::vlist;

/// Rows not yet measured: a three-line row with no image.
pub const ESTIMATE: f32 =
    2. * vlist::PAD.1 + vlist::LINE_HEIGHT + vlist::GAP + 3. * vlist::LINE_HEIGHT;

pub fn chat_row(index: usize) -> View {
    let mut body = column()
        .w_full()
        .px(vlist::PAD.0)
        .py(vlist::PAD.1)
        .gap(vlist::GAP)
        .bg(rgb(vlist::ROW_COLORS[index % 2]))
        .child(
            row()
                .gap(8.)
                .h(vlist::LINE_HEIGHT)
                .child(text(vlist::author(index)).text_color(rgb(vlist::AUTHOR)))
                .child(text(vlist::time(index)).text_color(rgb(vlist::MUTED))),
        )
        .child(text(vlist::text(index)).w(vlist::TEXT_WIDTH));
    if vlist::has_image(index) {
        body = body.child(
            div()
                .size(vlist::IMAGE.0, vlist::IMAGE.1)
                .bg(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

/// What a list of them sits in: the scene's font and colours, its text
/// wrapping (zgx's text wraps only when asked; the rows inherit it).
pub fn frame(list: View) -> View {
    column()
        .w_full()
        .h_full()
        .bg(rgb(vlist::BACKGROUND))
        .font_family(vlist::FONT)
        .text_size(vlist::FONT_SIZE)
        .line_height(vlist::LINE_HEIGHT)
        .text_wrap(true)
        .text_color(rgb(vlist::TEXT))
        .child(list)
}
