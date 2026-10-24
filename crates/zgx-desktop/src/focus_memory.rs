//! Native activation is independent from the retained UI's logical focus.
use zgx::{input::InputEvent, scene::NodeId, widgets::Ui};

#[derive(Default)]
pub(crate) struct FocusMemory {
    remembered: Option<NodeId>,
    inactive: bool,
}
impl FocusMemory {
    pub(crate) fn deactivate(&mut self, ui: &Ui) {
        if self.inactive {
            return;
        }
        self.inactive = true;
        self.remembered = ui.input.focused();
        ui.scene.borrow_mut().prepare_layout();
        ui.input.dispatch(&ui.scene, InputEvent::PointerCancel);
        ui.input.dispatch(&ui.scene, InputEvent::Blur);
    }
    pub(crate) fn activate(&mut self, ui: &Ui) {
        if !self.inactive {
            return;
        }
        self.inactive = false;
        let remembered = self.remembered.take();
        let target = ui.input.focused().or(remembered).filter(|node| {
            let scene = ui.scene.borrow();
            scene.contains(*node) && ui.input.is_enabled(&scene, *node)
        });
        if let Some(node) = target {
            if ui.input.focused() == Some(node) {
                ui.input.dispatch_to(&ui.scene, node, InputEvent::Focus);
            } else {
                ui.input.focus(&ui.scene, Some(node));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    fn focused_button(ui: &mut Ui) -> NodeId {
        let node = ui.button(ui.root(), "Focus target", 120., || {});
        assert!(ui.input.focus(&ui.scene, Some(node)));
        node
    }
    #[test]
    fn repeated_deactivation_preserves_focus_and_sends_blur_once() {
        let mut ui = Ui::new(320., 200.);
        let node = focused_button(&mut ui);
        let blurs = Rc::new(Cell::new(0));
        let count = blurs.clone();
        ui.on_event(node, true, move |event| {
            if matches!(event.event, InputEvent::Blur) {
                count.set(count.get() + 1);
            }
        });
        let mut memory = FocusMemory::default();
        memory.deactivate(&ui);
        // Focus may be cleared independently by a native/platform transition.
        ui.input.focus(&ui.scene, None);
        let before = blurs.get();
        memory.deactivate(&ui);
        assert_eq!(blurs.get(), before);
        memory.activate(&ui);
        assert_eq!(ui.input.focused(), Some(node));
        assert_eq!(before, 2); // Native blur, then explicit logical focus clearing.
    }
    #[test]
    fn deactivation_cancels_pressed_controls_before_blurring() {
        use zgx::input::PointerButton;
        let mut ui = Ui::new(320., 200.);
        let clicks = Rc::new(Cell::new(0));
        let count = clicks.clone();
        ui.button(ui.root(), "Click", 120., move || count.set(count.get() + 1));
        ui.dispatch(InputEvent::PointerDown {
            x: 5.,
            y: 5.,
            button: PointerButton::Primary,
        });
        let mut memory = FocusMemory::default();
        memory.deactivate(&ui);
        memory.activate(&ui);
        ui.dispatch(InputEvent::PointerUp {
            x: 5.,
            y: 5.,
            button: PointerButton::Primary,
        });
        assert_eq!(clicks.get(), 0);
    }
    #[test]
    fn removed_or_disabled_targets_are_not_restored() {
        for remove in [false, true] {
            let mut ui = Ui::new(320., 200.);
            let node = focused_button(&mut ui);
            let mut memory = FocusMemory::default();
            memory.deactivate(&ui);
            if remove {
                ui.remove(node);
            } else {
                ui.set_disabled(node, true);
            }
            memory.activate(&ui);
            assert_ne!(ui.input.focused(), Some(node));
            assert!(memory.remembered.is_none());
        }
    }
    #[test]
    fn activation_consumes_memory_and_does_not_replay_focus() {
        let mut ui = Ui::new(320., 200.);
        let node = focused_button(&mut ui);
        let focuses = Rc::new(Cell::new(0));
        let count = focuses.clone();
        ui.on_event(node, true, move |event| {
            if matches!(event.event, InputEvent::Focus) {
                count.set(count.get() + 1);
            }
        });
        let mut memory = FocusMemory::default();
        memory.deactivate(&ui);
        memory.activate(&ui);
        assert_eq!(focuses.get(), 1);
        assert!(memory.remembered.is_none());
        ui.input.focus(&ui.scene, None);
        memory.activate(&ui);
        assert_eq!(ui.input.focused(), None);
        assert_eq!(focuses.get(), 1);
        memory.deactivate(&ui);
        memory.activate(&ui);
        assert_eq!(ui.input.focused(), None);
    }
}
