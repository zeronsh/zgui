use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, input::InputEvent, reactive::Signal, widgets::Ui};

struct RestoreHeight {
    height: Signal<f32>,
    enabled: Rc<Cell<bool>>,
}
impl Drop for RestoreHeight {
    fn drop(&mut self) {
        if self.enabled.get() {
            self.height.set(40.);
        }
    }
}

#[test]
fn virtual_row_mount_and_disposal_feedback_is_reported_and_can_be_repaired() {
    let mut ui = Ui::new(400., 200.);
    let delivered = Rc::new(Cell::new(0));
    let read = delivered.clone();
    ui.on_event(ui.root(), false, move |_| read.set(read.get() + 1));
    let height = ui.signal(20.);
    let enabled = Rc::new(Cell::new(true));
    let mounted = Rc::new(Cell::new(0));
    let row_height = height.clone();
    let row_enabled = enabled.clone();
    let row_mounted = mounted.clone();
    let style_height = height.clone();
    let view = virtual_list(
        ui.signal(0.),
        20.,
        0,
        || 2,
        |index| index,
        move |_, key, cx| {
            if key == 1 {
                row_mounted.set(row_mounted.get() + 1);
                // A temporary safety guard makes a broken convergence check fail
                // this test instead of hanging the entire test process.
                assert!(
                    row_mounted.get() < 200,
                    "frame preparation failed to bound feedback"
                );
                if row_enabled.get() {
                    row_height.set(20.);
                }
                cx.retain(RestoreHeight {
                    height: row_height.clone(),
                    enabled: row_enabled.clone(),
                });
            }
            text(format!("Row {key}"))
        },
    )
    .w(200.)
    .reactive_style(move || Styles::new().h(style_height.get()));
    let handle = ui.mount(view);
    ui.try_prepare_frame().unwrap();
    height.set(40.);
    assert!(ui.try_prepare_frame().is_err());
    assert!(
        mounted.get() > 1,
        "row lifetime should drive repeated resize feedback"
    );
    assert!(
        ui.try_dispatch(InputEvent::PointerMove { x: 1., y: 1. })
            .is_err()
    );
    assert_eq!(delivered.get(), 0, "input must not use unstable geometry");
    enabled.set(false);
    height.set(40.);
    ui.try_prepare_frame().unwrap();
    ui.try_dispatch(InputEvent::PointerMove { x: 1., y: 1. })
        .unwrap();
    assert!(delivered.get() > 0);
    let settled_mounts = mounted.get();
    ui.try_prepare_frame().unwrap();
    assert_eq!(mounted.get(), settled_mounts);
    assert_eq!(ui.scene.borrow().bounds(handle.node()).height, 40.);
}
