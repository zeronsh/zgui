//! Scrolling from the keyboard, as a page does: with nothing focused,
//! Space and Shift+Space, Page Up/Down, Home/End and the arrows scroll the
//! scroll view the user is working in (pressed in last, else under the
//! pointer). A focused control keeps its keys (an editor's, a slider's, a
//! list's navigation, a button's Space).
use crate::{
    input::{DispatchResult, InputEvent, Key},
    widgets::Ui,
};

/// A line's scroll, as Chrome's.
const LINE: f32 = 40.;
/// What of a viewport a page scrolls: Chrome's 87.5%.
const PAGE: f32 = 0.875;

/// What a key scrolls by: a page or line along one axis, or to an end.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ScrollKey {
    None,
    /// Space: a page, unless a focused control takes it.
    Space(f32),
    Page(f32),
    Line(f32, f32),
    End(f32),
}
impl ScrollKey {
    pub(crate) fn of(event: &InputEvent) -> Self {
        let InputEvent::KeyDown { key, modifiers, .. } = event else {
            return Self::None;
        };
        if modifiers.meta || modifiers.control || modifiers.alt {
            return Self::None;
        }
        match key {
            Key::Space => Self::Space(if modifiers.shift { -1. } else { 1. }),
            Key::PageDown => Self::Page(1.),
            Key::PageUp => Self::Page(-1.),
            Key::Home => Self::End(-1.),
            Key::End => Self::End(1.),
            _ if modifiers.shift => Self::None,
            Key::ArrowDown => Self::Line(0., LINE),
            Key::ArrowUp => Self::Line(0., -LINE),
            Key::ArrowRight => Self::Line(LINE, 0.),
            Key::ArrowLeft => Self::Line(-LINE, 0.),
            _ => Self::None,
        }
    }
}

impl Ui {
    /// Scroll for a key nothing took (`ScrollKey`), with nothing focused:
    /// the scroll view around what was last pressed, else under the pointer.
    pub(crate) fn scroll_after_dispatch(&self, key: ScrollKey, result: &mut DispatchResult) {
        if key == ScrollKey::None || result.default_prevented || self.input.focused().is_some() {
            return;
        }
        let from = self.last_pressed().or_else(|| self.input.hovered());
        let Some(from) = from else {
            return;
        };
        let viewport = {
            let scene = self.scene.borrow();
            if !scene.contains(from) {
                return;
            }
            // Inside one, or a scroll view itself (its root holds the
            // viewport, which holds the content).
            let content = std::iter::once(from)
                .chain(scene.ancestors(from))
                .find(|id| scene.is_scroll_content(*id))
                .or_else(|| {
                    scene
                        .children(from)
                        .iter()
                        .flat_map(|viewport| scene.children(*viewport))
                        .copied()
                        .find(|id| scene.is_scroll_content(*id))
                });
            content
                .and_then(|content| scene.parent(content))
                .map(|viewport| scene.bounds(viewport))
        };
        let Some(viewport) = viewport else {
            return;
        };
        let (delta_x, delta_y) = match key {
            ScrollKey::Page(way) | ScrollKey::Space(way) => {
                (0., way * (viewport.height * PAGE).max(LINE))
            }
            ScrollKey::Line(x, y) => (x, y),
            ScrollKey::End(way) => (0., way * f32::MAX / 4.),
            ScrollKey::None => return,
        };
        let scrolled = self
            .input
            .dispatch_to(
                &self.scene,
                from,
                InputEvent::Scroll {
                    x: viewport.x,
                    y: viewport.y,
                    delta_x,
                    delta_y,
                },
            )
            .default_prevented;
        if scrolled {
            result.default_prevented = true;
        }
    }
}
