//! Text dragged in from another app (a browser's selection, an editor's)
//! drops into a field, as in AppKit's text views and a browser's fields:
//! the platform's drag reaches winit's windows
//! (`winit::platform::text_drop`), and the window's destinations hear it as
//! a drag from another of the app's windows carrying `DraggedText`
//! (`InputDispatcher::drag_over`). What it carries is known only as it
//! drops (X11 hands it over then), so over a window it names no text.
//! Wayland's drags reach the data worker instead (`wayland_data`), which
//! asks the window's host the same (`over`, `out`, `dropped`). winit's hook
//! is the vendored winit's (`zgx_winit_patched`): with crates.io's, only
//! Wayland's drags drop.
use std::cell::RefCell;
#[cfg(zgx_winit_patched)]
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
#[cfg(zgx_winit_patched)]
use winit::window::{Window, WindowId};
use zgx::{actions::Action, input::DraggedText};

#[cfg(zgx_winit_patched)]
struct Target {
    ui: zgx::widgets::WeakUi,
    window: Weak<Window>,
    /// Wake the loop: a destination's change draws.
    wake: Box<dyn Fn()>,
}

thread_local! {
    /// The windows' interfaces, by their winit window.
    #[cfg(zgx_winit_patched)]
    static WINDOWS: RefCell<HashMap<WindowId, Target>> = RefCell::default();
    /// What a drag over a window carries while it is: text, not yet known.
    static HOVER: RefCell<Option<Action>> = const { RefCell::new(None) };
}

/// `window` shows `ui`: text dragged in from other apps drops there. The
/// first hands winit's windows the destinations.
#[cfg(zgx_winit_patched)]
pub(crate) fn add(window: &Arc<Window>, ui: &zgx::widgets::Ui, wake: impl Fn() + 'static) {
    let first = WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.insert(
            window.id(),
            Target {
                ui: ui.downgrade(),
                window: Arc::downgrade(window),
                wake: Box::new(wake),
            },
        );
        windows.len() == 1
    });
    if first {
        winit::platform::text_drop::set_text_drops(Some(std::rc::Rc::new(Windows)));
    }
}

/// `window` closed.
#[cfg(zgx_winit_patched)]
pub(crate) fn remove(window: WindowId) {
    WINDOWS.with(|windows| windows.borrow_mut().remove(&window));
}

/// `window`'s interface, and the drag's point there (physical pixels) in
/// its coordinates.
#[cfg(zgx_winit_patched)]
fn at(
    window: WindowId,
    x: f64,
    y: f64,
    then: impl FnOnce(&zgx::widgets::Ui, f32, f32) -> bool,
) -> bool {
    let found = WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let target = windows.get(&window)?;
        let scale = target
            .window
            .upgrade()
            .map_or(1., |window| window.scale_factor());
        Some((target.ui.upgrade()?, scale))
    });
    let Some((ui, scale)) = found else {
        return false;
    };
    let taken = then(&ui, (x / scale) as f32, (y / scale) as f32);
    wake(window);
    taken
}

#[cfg(zgx_winit_patched)]
fn wake(window: WindowId) {
    WINDOWS.with(|windows| {
        if let Some(target) = windows.borrow().get(&window) {
            (target.wake)();
        }
    });
}

fn hover() -> Action {
    HOVER.with(|hover| {
        hover
            .borrow_mut()
            .get_or_insert_with(|| Action::new(DraggedText::new(String::new())))
            .clone()
    })
}

/// A drag carrying text from another app is over `ui` at `(x, y)` (its
/// points): whether it would drop there. A drag of the app's own the
/// platform has (Windows asks its windows' drop targets of it too) is the
/// host's to carry to its windows: none here.
pub(crate) fn over(ui: &zgx::widgets::Ui, x: f32, y: f32) -> bool {
    !crate::native_drag::handed_off() && ui.input.drag_over(&ui.scene, &hover(), x, y)
}

/// The drag left `ui`, or was let go of where nothing took it.
pub(crate) fn out(ui: &zgx::widgets::Ui) {
    if HOVER.with(|hover| hover.borrow_mut().take()).is_some() {
        ui.input.drag_out(&ui.scene);
    }
}

/// The drag was let go of over `ui` at `(x, y)`, carrying `text`: whether
/// it was taken.
pub(crate) fn dropped(ui: &zgx::widgets::Ui, x: f32, y: f32, text: &str) -> bool {
    if crate::native_drag::handed_off() {
        return false;
    }
    HOVER.with(|hover| hover.borrow_mut().take());
    let text = Action::new(DraggedText::new(text));
    ui.input.drag_drop(&ui.scene, &text, x, y)
}

#[cfg(zgx_winit_patched)]
struct Windows;
#[cfg(zgx_winit_patched)]
impl winit::platform::text_drop::TextDrops for Windows {
    fn over(&self, window: WindowId, x: f64, y: f64) -> bool {
        at(window, x, y, over)
    }
    fn out(&self, window: WindowId) {
        at(window, 0., 0., |ui, _, _| {
            out(ui);
            false
        });
    }
    fn dropped(&self, window: WindowId, x: f64, y: f64, text: &str) -> bool {
        at(window, x, y, |ui, x, y| dropped(ui, x, y, text))
    }
}

#[cfg(all(test, zgx_winit_patched))]
mod tests {
    use super::*;
    use winit::platform::text_drop::TextDrops;
    use zgx::compose::prelude::*;

    #[test]
    fn text_dragged_in_drops_into_the_field_under_it() {
        let mut ui = zgx::widgets::Ui::new(400., 300.);
        let value = ui.signal(String::from("ab"));
        let fixed = ui.signal(String::from("kept"));
        let view = ui.mount(
            column()
                .child(
                    text_input("Name", value.clone())
                        .id("field")
                        .size(200., 30.),
                )
                .child(
                    text_input("Fixed", fixed.clone())
                        .read_only(true)
                        .size(200., 30.),
                ),
        );
        ui.prepare_frame();
        // A placeholder id, never a window's; no winit window: scale 1.
        let window = WindowId::dummy();
        let woken = std::rc::Rc::new(std::cell::Cell::new(0));
        WINDOWS.with(|windows| {
            let woken = woken.clone();
            windows.borrow_mut().insert(
                window,
                Target {
                    ui: ui.downgrade(),
                    window: Weak::new(),
                    wake: Box::new(move || woken.set(woken.get() + 1)),
                },
            );
        });
        let drops = Windows;
        assert!(drops.over(window, 150., 15.), "a field takes text");
        assert!(!drops.over(window, 100., 45.), "a read-only one, none");
        assert!(drops.over(window, 150., 15.));
        drops.out(window);
        assert!(drops.over(window, 150., 15.));
        assert!(drops.dropped(window, 150., 15., " dropped"));
        ui.prepare_frame();
        assert_eq!(value.get(), "ab dropped");
        assert_eq!(fixed.get(), "kept");
        assert_eq!(ui.input.focused(), view.find("field"));
        assert_eq!(woken.get(), 6, "each wakes the loop to draw");
        remove(window);
        assert!(!drops.over(window, 150., 15.), "closed");
    }
}
