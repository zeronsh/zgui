use zgx::{compose::prelude::*, widgets::Ui};

#[test]
fn switched_full_size_panes_follow_window_resizes() {
    // A sidebar beside a growing column holding a switched pane: a titlebar,
    // and a viewport with a composer anchored to its bottom (a chat app).
    let mut ui = Ui::new(1000., 700.);
    let mounted = ui.mount(
        row()
            .w_full()
            .h_full()
            .items_stretch()
            .child(div().w(256.).h_full())
            .child(
                column().grow().min_w(0.).child(
                    switch(
                        || (),
                        |_, _| {
                            column()
                                .id("pane")
                                .w_full()
                                .h_full()
                                .child(div().w_full().h(38.))
                                .child(
                                    overlay()
                                        .w_full()
                                        .grow()
                                        .min_h(0.)
                                        .child(div().id("list").w_full().h_full())
                                        .child(
                                            column()
                                                .absolute()
                                                .left(0.)
                                                .bottom(0.)
                                                .w_full()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .id("composer")
                                                        .w_full()
                                                        .max_w(768.)
                                                        .h(60.),
                                                ),
                                        ),
                                )
                        },
                    )
                    .w_full()
                    .h_full(),
                ),
            ),
    );
    ui.prepare_frame();
    for (width, height) in [(1000., 700.), (1200., 800.), (900., 600.), (1000., 700.)] {
        ui.scene.borrow_mut().resize(width, height);
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let pane = scene.bounds(mounted.find("pane").unwrap());
        let list = scene.bounds(mounted.find("list").unwrap());
        let composer = scene.bounds(mounted.find("composer").unwrap());
        assert_eq!(
            (pane.x, pane.width, pane.height),
            (256., width - 256., height),
            "pane at {width}x{height}"
        );
        assert_eq!(
            (list.width, list.height),
            (width - 256., height - 38.),
            "list at {width}x{height}"
        );
        assert_eq!(
            composer.y + composer.height,
            height,
            "composer bottom at {width}x{height}"
        );
    }
}

#[test]
fn nested_switched_panes_follow_window_resizes() {
    let mut ui = Ui::new(1000., 700.);
    let mounted = ui.mount(
        switch(
            || (),
            |_, _| {
                row()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .child(div().w(256.).h_full())
                    .child(
                        column().grow().min_w(0.).child(
                            switch(
                                || (),
                                |_, _| {
                                    column()
                                        .id("pane")
                                        .w_full()
                                        .h_full()
                                        .child(div().id("list").w_full().grow())
                                },
                            )
                            .w_full()
                            .h_full(),
                        ),
                    )
            },
        )
        .w_full()
        .h_full(),
    );
    ui.prepare_frame();
    for (width, height) in [(1200., 800.), (900., 600.)] {
        ui.scene.borrow_mut().resize(width, height);
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let pane = scene.bounds(mounted.find("pane").unwrap());
        assert_eq!(
            (pane.width, pane.height),
            (width - 256., height),
            "pane at {width}x{height}"
        );
    }
}

#[test]
fn measured_rows_beside_a_bottom_bar_follow_window_resizes() {
    let mut ui = Ui::new(1000., 700.);
    let heights = VariableHeights::new(&ui.runtime, 0, 80.).anchor_end();
    let offset = ui.signal(0.);
    let mounted = ui.mount(
        row()
            .w_full()
            .h_full()
            .items_stretch()
            .child(div().w(256.).h_full())
            .child(
                column().grow().flex_shrink(1.).min_w(0.).child(
                    column()
                        .id("pane")
                        .w_full()
                        .h_full()
                        .child(div().w_full().h(38.))
                        .child(
                            overlay()
                                .w_full()
                                .grow()
                                .flex_shrink(1.)
                                .min_h(0.)
                                .child(
                                    measured_rows(
                                        offset.clone(),
                                        heights.clone(),
                                        2,
                                        || (0..40).collect::<Vec<usize>>(),
                                        |_, _| 60.,
                                        |_, n, _| div().w_full().h(60.).id(format!("row-{n}")),
                                    )
                                    .id("list")
                                    .w_full()
                                    .h_full(),
                                )
                                .child(
                                    column()
                                        .absolute()
                                        .left(0.)
                                        .bottom(0.)
                                        .w_full()
                                        .items_center()
                                        .child(div().id("composer").w_full().max_w(768.).h(60.)),
                                ),
                        ),
                ),
            ),
    );
    ui.prepare_frame();
    for (width, height) in [(1200., 800.), (900., 600.)] {
        ui.scene.borrow_mut().resize(width, height);
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let list = scene.bounds(mounted.find("list").unwrap());
        let composer = scene.bounds(mounted.find("composer").unwrap());
        assert_eq!(
            (list.width, list.height),
            (width - 256., height - 38.),
            "list at {width}x{height}"
        );
        assert_eq!(
            composer.y + composer.height,
            height,
            "composer bottom at {width}x{height}"
        );
    }
}
