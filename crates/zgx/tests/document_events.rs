use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

#[test]
fn a_document_listener_hears_keys_nothing_is_focused_for_until_it_unmounts() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    let mut ui = Ui::new(200., 100.);
    let view = ui.mount(div().size(200., 100.).on_document_event(move |event| {
        if let InputEvent::KeyDown { key, .. } = event {
            log.borrow_mut().push(key.clone());
        }
    }));
    ui.prepare_frame();
    assert!(ui.input.focused().is_none());
    let key = |ui: &mut Ui, key: Key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        })
    };
    let result = key(&mut ui, Key::Escape);
    assert_eq!(result.target, None, "the key goes nowhere");
    assert_eq!(*heard.borrow(), [Key::Escape], "but the document hears it");
    view.unmount();
    ui.prepare_frame();
    key(&mut ui, Key::Escape);
    assert_eq!(heard.borrow().len(), 1, "not once it is gone");
}

#[test]
fn a_view_taking_unfocused_keys_hears_what_nothing_focused_takes() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    let mut ui = Ui::new(200., 100.);
    let view = ui.mount(
        div()
            .size(200., 100.)
            .takes_unfocused_keys()
            .on_event(move |cx| {
                if let InputEvent::KeyDown { key, .. } = &cx.event
                    && cx.phase != zgx::input::EventPhase::Capture
                {
                    log.borrow_mut().push(key.clone());
                }
            })
            .child(text_input("Field", ui.signal(String::new())).id("field")),
    );
    ui.prepare_frame();
    let key = |ui: &mut Ui, key: Key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        })
    };
    let result = key(&mut ui, Key::Escape);
    assert_eq!(
        result.target,
        Some(view.node()),
        "nothing focused: the view takes it"
    );
    // Focused, the field takes keys first and they bubble up to the view.
    let field = view.find("field").unwrap();
    ui.input.focus(&ui.scene, Some(field));
    let result = key(&mut ui, Key::ArrowLeft);
    assert_eq!(result.target, Some(field));
    assert_eq!(*heard.borrow(), [Key::Escape, Key::ArrowLeft]);
    view.unmount();
    ui.prepare_frame();
    assert_eq!(
        key(&mut ui, Key::Escape).target,
        None,
        "not once it is gone"
    );
}
