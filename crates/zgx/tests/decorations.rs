use zgx::{
    compose::prelude::*,
    scene::{Insets, NodeKind},
    widgets::Ui,
};
#[test]
fn fluent_paint_cascade_retains_layout_and_solid_background_clears_gradient() {
    let mut ui = Ui::new(180., 140.);
    let gradient = ui.signal(true);
    let view = ui.mount(
        div()
            .id("card")
            .size(100., 80.)
            .rounded_corners(Corners {
                top_left: 20.,
                ..Default::default()
            })
            .border_edges(Insets {
                left: 8.,
                right: 2.,
                top: 4.,
                bottom: 6.,
            })
            .border_color(rgb(0xffffff))
            .reactive_style({
                let gradient = gradient.clone();
                move || {
                    if gradient.get() {
                        Styles::new().bg_gradient(
                            0.,
                            vec![
                                GradientStop {
                                    offset: 0.,
                                    color: rgb(0xff0000),
                                },
                                GradientStop {
                                    offset: 1.,
                                    color: rgb(0x0000ff),
                                },
                            ],
                        )
                    } else {
                        Styles::new().bg(rgb(0x00ff00))
                    }
                }
            }),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let node = view.find("card").unwrap();
    let NodeKind::Panel { quad, .. } = ui.scene.borrow().kind(node).clone() else {
        panic!()
    };
    assert!(quad.decoration.unwrap().background.is_some());
    gradient.set(false);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    let NodeKind::Panel { quad, .. } = ui.scene.borrow().kind(node).clone() else {
        panic!()
    };
    assert_eq!(quad.fill, rgb(0x00ff00));
    let detail = quad.decoration.unwrap();
    assert!(detail.background.is_none());
    assert_eq!(detail.border_widths.unwrap().left, 8.);
}
