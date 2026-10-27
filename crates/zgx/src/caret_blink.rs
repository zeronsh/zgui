//! The focused editor's caret blinking as the system's do (`Ui::set_caret_blink`).
//!
//! Each blink recolours the caret's rectangle, so it damages that rectangle
//! alone and changes no geometry; the host wakes for it as for any other
//! interaction deadline (`Ui::next_interaction_deadline`), so a blinking
//! caret costs one small frame a phase and no frame loop. It shows solid
//! while the editor redraws it (typing, selecting, moving the caret), and
//! stops, solid, after the system's timeout, while the window is not
//! presented, and as the editor loses the focus (its window deactivating).
//!
//! A host can draw the caret itself (`Ui::set_native_caret`; macOS, as a
//! Core Animation layer over the window's content, blinking by an animation
//! of its own): while the caret shows plainly (`Scene::plain_box`), nothing
//! paints over it and it marks no composition or selection, the editor's
//! caret paints nothing and sets no deadline, so a focused field left alone
//! wakes nothing and the GPU does no work. Every Metal submission makes the
//! driver hold about 176 MB for its command queue for some 8 s after it
//! (M5, 2026-10-27), so a caret blinking through frames kept a window's
//! footprint that high for as long as a field had the focus.
use crate::scene::{Color, NodeId, NodeKind, Rect};
use crate::widgets::Ui;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// How a caret blinks: shown for `on`, hidden for `off`, and after `timeout`
/// without the user typing or moving it, solid. Read from the system by the
/// desktop host (Windows' `GetCaretBlinkTime` and caret timeout, macOS's
/// insertion point blink periods, GTK's `cursor-blink-time` and
/// `cursor-blink-timeout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretBlink {
    /// How long each blink shows the caret.
    pub on: Duration,
    /// How long each blink hides it.
    pub off: Duration,
    /// `None`: as long as the editor has the focus.
    pub timeout: Option<Duration>,
}

impl CaretBlink {
    /// The shortest phase: shorter would be a flicker, not a blink.
    const SHORTEST: Duration = Duration::from_millis(50);

    fn held(self) -> Self {
        Self {
            on: self.on.max(Self::SHORTEST),
            off: self.off.max(Self::SHORTEST),
            timeout: self.timeout,
        }
    }
}

/// The caret a host draws itself (`Ui::native_caret`): where, how and since
/// when it blinks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeCaret {
    /// The caret's box, in window points (before the window's zoom).
    pub rect: Rect,
    /// Its colour while shown.
    pub color: Color,
    /// How long each blink shows it.
    pub on: Duration,
    /// How long each blink hides it.
    pub off: Duration,
    /// When it last showed solid (the editor drew it: typed, moved,
    /// focused), the blink's start: shown for `on` from then, hidden for
    /// `off`, and again.
    pub since: Instant,
}

/// The document's one blinking caret: the focused editor's.
#[derive(Default)]
pub(crate) struct Blink {
    settings: Cell<Option<CaretBlink>>,
    caret: Cell<Option<NodeId>>,
    /// The caret's colour while it is shown: hidden, it is this, transparent.
    color: Cell<Option<Color>>,
    hidden: Cell<bool>,
    next: Cell<Option<Instant>>,
    /// When it stops, solid (`CaretBlink::timeout`).
    stop: Cell<Option<Instant>>,
    /// When the editor last drew it shown: a native caret's blink starts
    /// there.
    since: Cell<Option<Instant>>,
    /// The editor's caret marks a bare insertion point: no composition, no
    /// selection (a native caret draws only that).
    bare: Cell<bool>,
    /// The host can draw the caret itself (`Ui::set_native_caret`).
    native: Cell<bool>,
    /// The caret the host draws, while it does (`settle`).
    drawn: Cell<Option<NativeCaret>>,
    /// Counts changes of `drawn`, for the host to apply them once.
    serial: Cell<u64>,
}

impl Blink {
    /// The editor drew its caret shown (as the user typed, selected or moved
    /// it, or it took the focus): solid now, blinking from a phase on.
    /// `bare`: it marks an insertion point alone (no composition, no
    /// selection), which the host may draw itself.
    pub(crate) fn restart(&self, caret: NodeId, color: Color, now: Instant, bare: bool) {
        self.caret.set(Some(caret));
        self.color.set(Some(color));
        self.since.set(Some(now));
        self.bare.set(bare);
        self.hidden.set(false);
        let settings = self.settings.get();
        self.next.set(settings.map(|s| now + s.on));
        self.stop.set(
            settings
                .and_then(|s| s.timeout)
                .map(|timeout| now + timeout),
        );
    }

    /// The editor drew its caret hidden (it lost the focus, or composes).
    pub(crate) fn release(&self, caret: NodeId) {
        if self.caret.get() == Some(caret) {
            self.caret.set(None);
            self.hidden.set(false);
            self.next.set(None);
            self.stop.set(None);
            self.since.set(None);
            self.bare.set(false);
        }
    }

    fn set(&self, ui: &Ui, settings: Option<CaretBlink>) {
        let settings = settings.map(CaretBlink::held);
        if self.settings.replace(settings) == settings {
            return;
        }
        if let (Some(caret), Some(color)) = (self.caret.get(), self.color.get()) {
            self.show(ui, caret, color);
            self.restart(caret, color, Instant::now(), self.bare.get());
        }
    }
    /// Whether the host draws the caret now: the editor paints its own
    /// transparent (`settle` gives it back otherwise, whichever editor's).
    pub(crate) fn drawn_natively(&self) -> bool {
        self.drawn.get().is_some()
    }

    fn show(&self, ui: &Ui, caret: NodeId, color: Color) {
        if self.hidden.replace(false) {
            let mut scene = ui.scene.borrow_mut();
            if scene.contains(caret) {
                scene.set_kind(caret, NodeKind::Rect(color));
            }
        }
    }
}

impl Ui {
    /// Blink the focused editor's caret (`CaretBlink`), as the system's
    /// carets blink; `None` (the default, as before a host sets it) keeps
    /// it solid. The desktop host follows the system's setting.
    pub fn set_caret_blink(&self, blink: Option<CaretBlink>) {
        self.caret_blink().set(self, blink);
    }

    /// How the caret blinks (`Ui::set_caret_blink`).
    pub fn caret_blink_settings(&self) -> Option<CaretBlink> {
        self.caret_blink().settings.get()
    }

    /// Whether the focused editor's caret is in a blink's hidden phase
    /// (worked out from the blink's start while the host draws it).
    pub fn caret_blinked_off(&self) -> bool {
        let blink = self.caret_blink();
        match blink.drawn.get() {
            Some(caret) => {
                let cycle = (caret.on + caret.off).as_secs_f64();
                let into = caret.since.elapsed().as_secs_f64() % cycle;
                into >= caret.on.as_secs_f64()
            }
            None => blink.hidden.get(),
        }
    }
    /// Let the host draw the caret itself when it can (`native_caret`); off
    /// (the default) the editor paints it, blinking through frames.
    #[doc(hidden)]
    pub fn set_native_caret(&self, native: bool) {
        self.caret_blink().native.set(native);
    }
    /// The caret the host draws itself now, if it does, with a count that
    /// changes whenever it does (moved, restarted, recoloured, shown or
    /// given back to the editor): the host applies a change once, in the
    /// frame that shows it, or alone when there is none.
    #[doc(hidden)]
    pub fn native_caret(&self) -> (u64, Option<NativeCaret>) {
        let blink = self.caret_blink();
        (blink.serial.get(), blink.drawn.get())
    }
}

/// After layout, before the frame's damage: whether the host draws the
/// caret (`Ui::native_caret`), changing the editor's caret between painted
/// and transparent where that changes. Nothing while no host can.
pub(crate) fn settle(ui: &Ui) {
    let blink = ui.caret_blink();
    if !blink.native.get() && blink.drawn.get().is_none() {
        return;
    }
    let want = (|| {
        if !blink.native.get() || !blink.bare.get() || !ui.is_presented() {
            return None;
        }
        let (caret, color, settings, since) = (
            blink.caret.get()?,
            blink.color.get()?,
            blink.settings.get()?,
            blink.since.get()?,
        );
        // One that stops after a timeout stays the editor's: it shows solid then.
        if settings.timeout.is_some() {
            return None;
        }
        let scene = ui.scene.borrow();
        if !scene.contains(caret) {
            return None;
        }
        // Its colour as last painted: a style's colour alone recolours the
        // node without drawing the editor again.
        let color = match scene.kind(caret) {
            NodeKind::Rect(painted) if painted.3 > 0 => *painted,
            _ => color,
        };
        // A popup's caret draws in the popup's window.
        let popups = scene.popups();
        if !popups.is_empty()
            && std::iter::once(caret)
                .chain(scene.ancestors(caret))
                .any(|id| popups.contains(&id))
        {
            return None;
        }
        let rect = scene.plain_box(caret)?;
        if rect.width <= 0. || rect.height <= 0. || scene.painted_over(caret, rect) {
            return None;
        }
        Some(NativeCaret {
            rect,
            color,
            on: settings.on,
            off: settings.off,
            since,
        })
    })();
    let was = blink.drawn.get();
    if want != was {
        blink.serial.set(blink.serial.get().wrapping_add(1));
        blink.drawn.set(want);
    }
    let Some(caret) = blink.caret.get() else {
        return;
    };
    let mut scene = ui.scene.borrow_mut();
    if !scene.contains(caret) {
        return;
    }
    // The editor's caret paints exactly when the host's doesn't: whatever
    // drew the editor in between (blurred and focused again before this),
    // set here, where nothing changes it.
    let mut effects = scene.effects(caret);
    let opacity = if want.is_some() { 0. } else { 1. };
    if effects.opacity == opacity {
        return;
    }
    if want.is_some() {
        // The host's now: the editor's paints nothing, shown in full colour
        // for when it paints again.
        if let Some(color) = blink.color.get() {
            scene.set_kind(caret, NodeKind::Rect(color));
        }
        blink.hidden.set(false);
        blink.next.set(None);
    } else if was.is_some() {
        // Given back, still shown (it moved under something, a selection
        // began): solid, blinking from a phase on, as when it is drawn.
        blink.hidden.set(false);
        let now = Instant::now();
        blink.next.set(blink.settings.get().map(|s| now + s.on));
    } else {
        // The editor's own, painted as the editor set it.
        return;
    }
    effects.opacity = opacity;
    scene.set_effects(caret, effects);
}

/// When the caret next changes, while one blinks in a presented window.
pub(crate) fn next(ui: &Ui) -> Option<Instant> {
    let blink = ui.caret_blink();
    // The host's blinks by itself.
    if blink.drawn.get().is_some() {
        return None;
    }
    let caret = blink.caret.get()?;
    if !ui.is_presented() || !ui.scene.borrow().contains(caret) {
        return None;
    }
    blink.next.get()
}

/// Show or hide the caret if its phase is over, or stop it, solid, at its
/// timeout. Whether it changed.
pub(crate) fn advance(ui: &Ui, now: Instant) -> bool {
    let blink = ui.caret_blink();
    if blink.drawn.get().is_some() {
        return false;
    }
    let (Some(caret), Some(color), Some(settings)) =
        (blink.caret.get(), blink.color.get(), blink.settings.get())
    else {
        return false;
    };
    let Some(due) = blink.next.get().filter(|due| *due <= now) else {
        return false;
    };
    if !ui.is_presented() || !ui.scene.borrow().contains(caret) {
        return false;
    }
    if blink.stop.get().is_some_and(|stop| stop <= now) {
        let changed = blink.hidden.get();
        blink.show(ui, caret, color);
        blink.next.set(None);
        return changed;
    }
    let hidden = !blink.hidden.get();
    blink.hidden.set(hidden);
    let shown = if hidden {
        Color(color.0, color.1, color.2, 0)
    } else {
        color
    };
    ui.scene.borrow_mut().set_kind(caret, NodeKind::Rect(shown));
    // From the phase's own start, unless the wake came a phase late.
    let phase = if hidden { settings.off } else { settings.on };
    let start = if now.saturating_duration_since(due) < phase {
        due
    } else {
        now
    };
    blink.next.set(Some(start + phase));
    true
}
