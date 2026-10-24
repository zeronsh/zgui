use zgx::{compose::prelude::*, widgets::Ui};
#[test]
fn cursor_cascade_changes_without_layout_or_paint_and_disabled_regions_keep_explicit_style() {
    let mut ui = Ui::new(150., 100.);
    let busy = ui.signal(false);
    let view = ui.mount(
        row()
            .cursor(Cursor::Move)
            .child(
                div()
                    .id("child")
                    .size(50., 50.)
                    .disabled(true)
                    .reactive_style({
                        let busy = busy.clone();
                        move || {
                            Styles::new().cursor(if busy.get() {
                                Cursor::NotAllowed
                            } else {
                                Cursor::Grab
                            })
                        }
                    }),
            )
            .child(div().size(50., 50.)),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let node = view.find("child").unwrap();
    assert_eq!(ui.scene.borrow().cursor_for(node), Some(Cursor::Grab));
    assert_eq!(ui.scene.borrow().cursor_at(20., 20.), Some(Cursor::Grab));
    assert_eq!(ui.scene.borrow().cursor_at(70., 20.), Some(Cursor::Move));
    busy.set(true);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(report.damage.is_empty());
    assert_eq!(
        ui.scene.borrow().cursor_at(20., 20.),
        Some(Cursor::NotAllowed)
    );
    view.unmount();
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().cursor_at(20., 20.), None);
}
