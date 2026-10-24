use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use zgx::{
    compose::prelude::*,
    scene::{NodeId, NodeKind},
    widgets::Ui,
};

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn content(ui: &Ui, node: NodeId) -> String {
    match ui.scene.borrow().kind(node) {
        NodeKind::Text { text, .. } => text.to_string(),
        _ => panic!("expected text"),
    }
}
fn failing_binding(drops: Rc<Cell<usize>>) -> View {
    component(move |cx| {
        cx.retain(DropCount(drops));
        div().reactive_style(|| panic!("initial binding failed"))
    })
}

#[test]
fn render_initial_binding_failure_preserves_previous_root_and_allows_retry() {
    let mut ui = Ui::new(200., 100.);
    let old = ui.mount(text("existing"));
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    let drops = Rc::new(Cell::new(0));
    let result = catch_unwind(AssertUnwindSafe(|| {
        ui.render(
            column()
                .child(text("staged"))
                .child(failing_binding(drops.clone())),
        );
    }));
    assert!(result.is_err());
    assert!(old.is_mounted());
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    assert_eq!(drops.get(), 1);
    let replacement = ui.render(text("recovered"));
    assert!(!old.is_mounted());
    assert_eq!(content(&ui, replacement.node()), "recovered");
    replacement.unmount();
    assert_eq!(ui.runtime.effect_count(), 0);
}

#[test]
fn deferred_initial_failure_removes_only_its_public_mount_not_later_sibling() {
    let mut ui = Ui::new(200., 100.);
    let old = ui.mount(text("existing"));
    let runtime = ui.runtime.clone();
    let drops = Rc::new(Cell::new(0));
    let value = ui.signal(String::from("sibling"));
    let read = value.clone();
    let mut failed = None;
    let mut sibling = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        runtime.batch(|| {
            failed = Some(ui.mount(failing_binding(drops.clone())));
            sibling = Some(ui.mount(text_signal(move || read.get())));
        });
    }));
    assert!(result.is_err());
    let failed = failed.expect("mount returned before the outer batch flushed");
    let sibling = sibling.unwrap();
    assert!(!failed.is_mounted());
    assert!(old.is_mounted() && sibling.is_mounted());
    assert_eq!(drops.get(), 1);
    assert_eq!(
        ui.scene.borrow().children(ui.root()),
        &[old.node(), sibling.node()]
    );
    runtime.flush();
    value.set("still reactive".into());
    assert_eq!(content(&ui, sibling.node()), "still reactive");
    old.unmount();
    sibling.unmount();
    assert_eq!(runtime.effect_count(), 0);
}

#[test]
fn initial_conditional_and_keyed_descendant_binding_failures_cleanup_public_owner() {
    for keyed_case in [false, true] {
        let mut ui = Ui::new(200., 100.);
        let old = ui.mount(text("existing"));
        let nodes = ui.scene.borrow().len();
        let effects = ui.runtime.effect_count();
        let drops = Rc::new(Cell::new(0));
        let child_drops = drops.clone();
        let region = if keyed_case {
            keyed(|| vec![1], move |_, _| failing_binding(child_drops.clone()))
        } else {
            switch(|| true, move |_, _| failing_binding(child_drops.clone()))
        };
        let runtime = ui.runtime.clone();
        let mut failed = None;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.batch(|| {
                failed = Some(ui.mount(column().child(text("prefix")).child(region)));
            });
        }));
        assert!(result.is_err());
        assert!(!failed.unwrap().is_mounted());
        assert!(old.is_mounted());
        assert_eq!(drops.get(), 1);
        assert_eq!(ui.scene.borrow().len(), nodes);
        assert_eq!(runtime.effect_count(), effects);
        runtime.flush();
        let recovered = ui.mount(text("recovered"));
        recovered.unmount();
        old.unmount();
        assert_eq!(runtime.effect_count(), 0);
    }
}

#[test]
fn later_binding_panic_preserves_existing_view_and_subscriptions_for_retry() {
    let mut ui = Ui::new(200., 100.);
    let value = ui.signal(0);
    let read = value.clone();
    let mounted = ui.mount(text_signal(move || {
        let value = read.get();
        assert_ne!(value, 1, "later update failed");
        value.to_string()
    }));
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    assert!(catch_unwind(AssertUnwindSafe(|| value.set(1))).is_err());
    assert!(mounted.is_mounted());
    assert_eq!(content(&ui, mounted.node()), "0");
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    value.set(2);
    assert_eq!(content(&ui, mounted.node()), "2");
    mounted.unmount();
    assert_eq!(ui.runtime.effect_count(), 0);
}

#[test]
fn deferred_render_failure_cleans_new_root_without_restoring_removed_document() {
    let mut ui = Ui::new(200., 100.);
    let old = ui.mount(text("existing"));
    let runtime = ui.runtime.clone();
    let drops = Rc::new(Cell::new(0));
    let mut failed = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        runtime.batch(|| {
            failed = Some(ui.render(failing_binding(drops.clone())));
            assert!(!old.is_mounted(), "render already returned successfully");
            assert!(failed.as_ref().unwrap().is_mounted());
        });
    }));
    assert!(result.is_err());
    assert!(!failed.unwrap().is_mounted());
    assert!(
        !old.is_mounted(),
        "ownership cleanup is not document rollback"
    );
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
    assert_eq!(drops.get(), 1);
    assert_eq!(runtime.effect_count(), 0);
    runtime.flush();
    let recovered = ui.render(text("recovered"));
    assert_eq!(content(&ui, recovered.node()), "recovered");
}
