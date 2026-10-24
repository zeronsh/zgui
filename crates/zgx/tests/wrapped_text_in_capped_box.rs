use zgx::{compose::prelude::*, reactive::Signal, widgets::Ui};

#[test]
fn capped_box_grows_to_the_lines_its_text_wraps_to() {
    // A chat bubble: right-aligned, at most 80% of the column, its text
    // wrapping inside. The box must be as tall as the wrapped text.
    let mut ui = Ui::new(800., 600.);
    let long = "i think zgxs text measurement is wrong, there are characters and lines bleeding out of user bubble, and more words to wrap";
    let mounted = ui.mount(
        row().w(600.).justify_end().child(
            column()
                .id("bubble")
                .min_w(0.)
                .max_w_percent(80.)
                .px(16.)
                .py(10.)
                .text_size(14.)
                .line_height(22.)
                .child(
                    column().w_full().child(
                        column()
                            .w_full()
                            .shrink_0()
                            .child(text(long).id("text").w_full().text_wrap(true)),
                    ),
                ),
        ),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let bubble = scene.bounds(mounted.find("bubble").unwrap());
    let text = scene.bounds(mounted.find("text").unwrap());
    assert!(bubble.width <= 480.5, "bubble {bubble:?}");
    assert!(text.height > 22.5, "text wraps: {text:?}");
    assert!(
        bubble.y + bubble.height >= text.y + text.height + 10. - 0.5,
        "bubble {bubble:?} holds text {text:?}"
    );
}

#[test]
fn capped_box_in_a_measured_row_grows_to_its_wrapped_text() {
    // As the chat transcript builds it: the bubble in a measured row, its
    // text beside an absolute probe, under hidden fold controls.
    let mut ui = Ui::new(1000., 700.);
    let long = "i think zgxs text measurement is wrong, there are characters and lines bleeding out of user bubble, and more words to wrap";
    let heights = VariableHeights::new(&ui.runtime, 0, 80.).anchor_end();
    let offset = ui.signal(0.);
    let bubble = move |_: Signal<usize>, n: usize, _: &mut Context| {
        let body = column()
            .w_full()
            .shrink_0()
            .child(
                text_signal(move || long.to_owned())
                    .id(format!("text-{n}"))
                    .w_full()
                    .text_wrap(true)
                    .text_size(14.)
                    .line_height(22.),
            )
            .child(
                canvas(|_| zgx::canvas::Canvas::new())
                    .absolute()
                    .left(0.)
                    .top(0.)
                    .right(0.)
                    .bottom(0.),
            );
        let clip = column().w_full().child(body).reactive_style(Styles::new);
        let hidden = || div().h(22.).reactive_style(|| Styles::new().hidden());
        column().w_full().items_center().child(
            column().w_full().max_w(832.).px(48.).child(
                row().w_full().justify_end().child(
                    column()
                        .id(format!("bubble-{n}"))
                        .min_w(0.)
                        .max_w_percent(80.)
                        .rounded(16.)
                        .px(16.)
                        .py(10.)
                        .text_size(14.)
                        .line_height(22.)
                        .child(clip)
                        .child(hidden())
                        .child(hidden()),
                ),
            ),
        )
    };
    let mounted = ui.mount(
        measured_rows(offset, heights, 2, || vec![0_usize, 1], |_, _| 80., bubble)
            .w(744.)
            .h(600.),
    );
    for _ in 0..3 {
        ui.prepare_frame();
    }
    let scene = ui.scene.borrow();
    for n in 0..2 {
        let bubble = scene.bounds(mounted.find(&format!("bubble-{n}")).unwrap());
        let text = scene.bounds(mounted.find(&format!("text-{n}")).unwrap());
        assert!(text.height > 22.5, "text wraps: {text:?}");
        assert!(
            bubble.y + bubble.height >= text.y + text.height + 10. - 0.5,
            "bubble {bubble:?} holds text {text:?}"
        );
    }
}
