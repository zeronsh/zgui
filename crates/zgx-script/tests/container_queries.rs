//! A ref's `width` and `height` are followed: a computed reading them runs
//! again when layout resizes its element, within the same frame (as a
//! browser's ResizeObserver and container queries settle before paint), so
//! a layout can answer the room it has. An element's own fields are plain
//! reads.
mod common;
use common::Harness;

#[test]
fn a_layout_answers_its_width_in_the_frame_it_changes() {
    let mut h = Harness::inline(
        r##"local main = ui.ref()
        local wide = ui.computed(function()
            return (main.width or 400) >= 300
        end)
        return ui.row { id = "app", ref = main, w_full = true, h_full = true,
            ui.show(function()
                return wide:get()
            end, ui.column { id = "sidebar", w = 100, shrink = 0 }),
            ui.column { id = "content", grow = 1 },
        }"##,
    );
    assert!(h.find("sidebar").is_some());
    assert_eq!(h.bounds("content").width, 300.);
    // Narrower than the breakpoint: one frame's layout hides the sidebar
    // and gives its room to the content.
    h.ui.scene.borrow_mut().resize(250., 300.);
    h.ui.prepare_frame();
    assert!(h.find("sidebar").is_none());
    assert_eq!(h.bounds("content").width, 250.);
    h.ui.scene.borrow_mut().resize(400., 300.);
    h.ui.prepare_frame();
    assert!(h.find("sidebar").is_some());
    assert_eq!(h.bounds("content").width, 300.);
}
