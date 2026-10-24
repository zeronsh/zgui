//! Hover follows layout under a resting pointer: a button that slides away
//! (a tab opened before it) stops being hovered without the pointer moving.
use zgx::{compose::prelude::*, input::InputEvent, widgets::Ui};

#[test]
fn a_node_sliding_away_from_a_resting_pointer_is_no_longer_hovered() {
    let mut ui = Ui::new(300., 100.);
    let wide = ui.signal(false);
    let view = ui.mount(
        row()
            .child(div().reactive_style({
                let wide = wide.clone();
                move || Styles::new().w(if wide.get() { 120. } else { 40. }).h(30.)
            }))
            .child(
                div()
                    .id("button")
                    .size(30., 30.)
                    .hover(|s| s.bg(rgb(0x333333)))
                    .on_click(|| {}),
            )
            .child(div().id("after").size(30., 30.).on_click(|| {})),
    );
    ui.prepare_frame();
    let button = view.find("button").unwrap();
    ui.dispatch(InputEvent::PointerMove { x: 50., y: 10. });
    assert_eq!(ui.input.hovered(), Some(button));

    // The button moves 80 points right; the pointer is over the spacer now.
    wide.set(true);
    ui.prepare_frame();
    assert_ne!(ui.input.hovered(), Some(button));

    // And back under it: hovered again, still without a move.
    wide.set(false);
    ui.prepare_frame();
    assert_eq!(ui.input.hovered(), Some(button));

    // Outside the window: nothing to refresh.
    ui.dispatch(InputEvent::PointerLeave);
    wide.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.hovered(), None);
}
