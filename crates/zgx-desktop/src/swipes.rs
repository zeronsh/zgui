//! macOS's swipe between pages, as Safari goes back and forward on a
//! trackpad: zgx decides which scroll gestures are swipes (one starting
//! across where nothing under it scrolls that way, over a view that takes
//! navigation: `Ui::set_navigation_swipes`), the host has winit's view hand
//! the rest of such a gesture to AppKit's tracking
//! (`winit::platform::macos::begin_navigation_swipe`), and a swipe that
//! completes, or a three-finger one, steps back or forward from where it
//! began (`InputDispatcher::navigate`). The hooks are the vendored winit's
//! (`zgx_winit_patched`).
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Weak},
};
use winit::window::{Window, WindowId};
use zgx::input::Navigation;

struct Target {
    ui: zgx::widgets::WeakUi,
    window: Weak<Window>,
    /// Wake the loop: what a step back or forward changes draws.
    wake: Box<dyn Fn()>,
}

thread_local! {
    /// The windows' interfaces, by their winit window.
    static WINDOWS: RefCell<HashMap<WindowId, Target>> = RefCell::default();
}

/// `window` shows `ui`: swipes over it navigate. The first hands winit's
/// views the answer.
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
        winit::platform::macos::set_navigation_swipes(Some(std::rc::Rc::new(Windows)));
    }
}

/// `window` closed.
pub(crate) fn remove(window: WindowId) {
    WINDOWS.with(|windows| windows.borrow_mut().remove(&window));
}

/// A trackpad gesture begins over `ui`: it may swipe between pages if the
/// system has two-finger scrolls do so, a setting read afresh each gesture
/// (it changes as the app runs).
pub(crate) fn gesture_began(ui: &zgx::widgets::Ui) {
    ui.set_navigation_swipes(winit::platform::macos::swipes_from_scrolls());
}

/// After a scroll over `ui`: a gesture that became a swipe is AppKit's to
/// track from its next scroll.
pub(crate) fn after_scroll(ui: &zgx::widgets::Ui, window: Option<&Window>) {
    if ui.take_navigation_swipe()
        && let Some(window) = window
    {
        winit::platform::macos::begin_navigation_swipe(window);
    }
}

/// A swipe over `ui` at `(x, y)` (its points) went back or forward: the
/// view under where it began steps that way. Whether one did.
fn swiped(ui: &zgx::widgets::Ui, x: f32, y: f32, back: bool) -> bool {
    let navigation = if back {
        Navigation::Back
    } else {
        Navigation::Forward
    };
    ui.input.navigate(&ui.scene, x, y, navigation)
}

struct Windows;
impl winit::platform::macos::NavigationSwipes for Windows {
    fn swiped(&self, window: WindowId, x: f64, y: f64, back: bool) {
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
            return;
        };
        if swiped(&ui, (x / scale) as f32, (y / scale) as f32, back) {
            WINDOWS.with(|windows| {
                if let Some(target) = windows.borrow().get(&window) {
                    (target.wake)();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::platform::macos::NavigationSwipes;
    use zgx::compose::prelude::*;

    #[test]
    fn a_swipe_steps_back_or_forward_from_where_it_began() {
        let mut ui = zgx::widgets::Ui::new(400., 300.);
        let went = std::rc::Rc::new(RefCell::new(Vec::new()));
        let log = went.clone();
        ui.mount(
            column()
                .w(400.)
                .h(150.)
                .on_navigate(move |navigation| log.borrow_mut().push(navigation)),
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
        Windows.swiped(window, 100., 50., true);
        Windows.swiped(window, 100., 50., false);
        // Below the view that navigates: nothing does.
        Windows.swiped(window, 100., 250., true);
        assert_eq!(*went.borrow(), [Navigation::Back, Navigation::Forward]);
        assert_eq!(woken.get(), 2, "each step wakes the loop to draw");
        remove(window);
        Windows.swiped(window, 100., 50., true);
        assert_eq!(went.borrow().len(), 2, "closed");
    }
}
