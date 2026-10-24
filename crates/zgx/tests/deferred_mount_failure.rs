use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use zgx::{
    compose::{ViewHandle, prelude::*},
    widgets::Ui,
};
struct Resource(Rc<Cell<usize>>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn failing_view(drops: Rc<Cell<usize>>) -> View {
    component(move |cx| {
        cx.retain(Resource(drops));
        column()
            .child(text("staged"))
            .child(text_signal(|| panic!("initial binding failed")))
    })
}
#[test]
fn initial_binding_failure_after_outer_batch_cleans_only_its_mount() {
    let mut ui = Ui::new(400., 300.);
    let old = ui.mount(text("existing"));
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    let drops = Rc::new(Cell::new(0));
    let runtime = ui.runtime.clone();
    let mut handle = None;
    let failed = catch_unwind(AssertUnwindSafe(|| {
        runtime.batch(|| {
            handle = Some(ui.mount(failing_view(drops.clone())));
        })
    }));
    assert!(failed.is_err());
    assert!(old.is_mounted());
    assert!(
        !handle.unwrap().is_mounted(),
        "deferred failed mount must not survive"
    );
    assert_eq!(drops.get(), 1);
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    runtime.flush();
    let recovered = ui.mount(text("recovered"));
    assert!(recovered.is_mounted());
}
#[test]
fn initial_binding_failure_after_active_effect_cleans_returned_mount_handle() {
    let ui = Rc::new(RefCell::new(Ui::new(400., 300.)));
    let old = ui.borrow_mut().mount(text("existing"));
    let trigger = ui.borrow().signal(false);
    let read = trigger.clone();
    let runtime = ui.borrow().runtime.clone();
    let target = ui.clone();
    let handle = Rc::new(RefCell::new(None::<ViewHandle>));
    let output = handle.clone();
    let drops = Rc::new(Cell::new(0));
    let resources = drops.clone();
    let _driver = runtime.effect(move || {
        if read.get() {
            *output.borrow_mut() = Some(target.borrow_mut().mount(failing_view(resources.clone())));
        }
    });
    let nodes = ui.borrow().scene.borrow().len();
    let effects = runtime.effect_count();
    let failed = catch_unwind(AssertUnwindSafe(|| trigger.set(true)));
    assert!(failed.is_err());
    assert!(old.is_mounted());
    assert!(!handle.borrow().as_ref().unwrap().is_mounted());
    assert_eq!(drops.get(), 1);
    assert_eq!(ui.borrow().scene.borrow().len(), nodes);
    assert_eq!(runtime.effect_count(), effects);
    runtime.flush();
}

#[test]
fn deferred_failure_preserves_unrelated_mount_created_later_in_same_batch() {
    let mut ui = Ui::new(400., 300.);
    let runtime = ui.runtime.clone();
    let drops = Rc::new(Cell::new(0));
    let mut failed_handle = None;
    let mut healthy_handle = None;
    let failed = catch_unwind(AssertUnwindSafe(|| {
        runtime.batch(|| {
            failed_handle = Some(ui.mount(failing_view(drops.clone())));
            healthy_handle = Some(ui.mount(text("healthy").id("healthy")));
        })
    }));
    assert!(failed.is_err());
    assert!(!failed_handle.unwrap().is_mounted());
    let healthy = healthy_handle.unwrap();
    assert!(healthy.is_mounted());
    runtime.flush();
    assert!(healthy.find("healthy").is_some());
    assert_eq!(drops.get(), 1);
    assert_eq!(ui.scene.borrow().children(ui.root()), &[healthy.node()]);
}

#[test]
fn binding_initial_callback_can_unmount_its_owner_without_resurrected_ownership() {
    let mut ui = Ui::new(400., 300.);
    let baseline = ui.runtime.effect_count();
    let view = ui.mount(text("owner"));
    let owner = view.node();
    let remove = view.clone();
    let drops = Rc::new(Cell::new(0));
    let resource = Resource(drops.clone());
    ui.bind(owner, move || {
        let _ = &resource;
        remove.unmount();
    });
    assert!(!view.is_mounted());
    assert_eq!(ui.runtime.effect_count(), baseline);
    assert_eq!(drops.get(), 1);
}

#[test]
fn binding_removed_owner_is_rejected_before_registering_an_effect() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(text("owner"));
    let owner = view.node();
    view.unmount();
    let effects = ui.runtime.effect_count();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let failed = catch_unwind(AssertUnwindSafe(|| {
        ui.bind(owner, move || observed.set(observed.get() + 1))
    }));
    assert!(failed.is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(ui.runtime.effect_count(), effects);
}
