//! Zeron's look, shared by the sidebar, transcript and composer: the dark
//! theme's resolved tokens (`crates/theme/src/builtins.rs` seeds through
//! `Theme::from_variant`), its fonts and icons, the motion curves and specs
//! of `crates/ui/src/motion.rs`, and the page clock every animation reads.

use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc, time::Instant};
use zgx::{
    compose::{Tasks, prelude::*},
    frame::FrameClock,
    input::InputEvent,
    reactive::Signal,
    scene::Color,
    svg::SvgData,
};
use zgx_desktop::FontData;

// Tokens ----------------------------------------------------------------------

pub const BG: Color = Color(6, 6, 6, 255);
pub const TEXT: Color = Color(232, 232, 234, 255);
pub const TEXT_MUTED: Color = Color(169, 169, 174, 255);
pub const TEXT_FAINT: Color = Color(133, 133, 138, 255);
pub const ACCENT: Color = Color(139, 124, 246, 255);
pub const DANGER: Color = Color(248, 113, 113, 255);
pub const DANGER_MUTED: Color = Color(244, 146, 147, 255);
pub const WARNING: Color = Color(250, 204, 21, 255);
pub const SUCCESS: Color = Color(52, 211, 153, 255);
/// `theme.border` / `theme.border_strong`: white at 10% / 18%.
pub const BORDER: Color = Color(255, 255, 255, 26);
pub const BORDER_STRONG: Color = Color(255, 255, 255, 46);
/// The window's tint over the native blur (`glass()`): shell at 80%.
pub const GLASS: Color = Color(13, 13, 13, 204);
/// `glass_hover()` / `element_hover`: white at 11%.
pub const GLASS_HOVER: Color = Color(255, 255, 255, 28);
/// Floating glass (`composer_sidebar_tint`, `popover::surface_bg`).
pub const SURFACE_TINT: Color = Color(8, 8, 8, 38);
/// `composer_surface_border()`: hsla(210°, .18, .78, .09).
pub const COMPOSER_BORDER: Color = Color(189, 199, 209, 23);
pub const CODE_WASH: Color = Color(139, 124, 246, 56);
/// The harness brand tint for Claude marks.
pub const CLAUDE: Color = Color(217, 119, 87, 255);
/// The accent's glyph palette (light, mid, deep): spinner rows.
pub const GLYPH: [Color; 3] = [
    Color(171, 161, 249, 255),
    Color(139, 124, 246, 255),
    Color(114, 102, 202, 255),
];
/// The working indicator's row tints.
pub const GRADIENT: [Color; 3] = [
    Color(182, 211, 239, 255),
    Color(237, 177, 133, 255),
    Color(248, 136, 160, 255),
];

/// `color` at `alpha` times its own alpha.
pub fn alpha(color: Color, alpha: f32) -> Color {
    Color(
        color.0,
        color.1,
        color.2,
        (f32::from(color.3) * alpha.clamp(0., 1.)).round() as u8,
    )
}

/// `ink(a)`: white at `a` in the dark theme.
pub fn ink(a: f32) -> Color {
    alpha(Color(255, 255, 255, 255), a)
}

/// `hairline(a)`: white at `a` in the dark theme.
pub fn hairline(a: f32) -> Color {
    ink(a)
}

/// `wash(a)`: hsl(0, 0%, 92%) at `a`.
pub fn wash(a: f32) -> Color {
    alpha(Color(235, 235, 235, 255), a)
}

/// Zeron's `motion::mix`: sRGB lerp with premultiplied alpha.
pub fn mix(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    let (fa, ta) = (f32::from(from.3) / 255., f32::from(to.3) / 255.);
    let a = fa + (ta - fa) * t;
    if a <= 0. {
        return Color(0, 0, 0, 0);
    }
    let channel = |f: u8, c: u8| {
        let premultiplied = f32::from(f) * fa + (f32::from(c) * ta - f32::from(f) * fa) * t;
        (premultiplied / a).round().clamp(0., 255.) as u8
    };
    Color(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
        (a * 255.).round() as u8,
    )
}

// Fonts -------------------------------------------------------------------------

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

/// Zeron's bundled faces, registered before anything shapes.
pub fn fonts() -> Vec<FontData> {
    [
        &include_bytes!("fonts/Geist.ttf")[..],
        include_bytes!("fonts/Geist-Italic.ttf"),
        include_bytes!("fonts/Geist-Medium.ttf"),
        include_bytes!("fonts/Geist-SemiBold.ttf"),
        include_bytes!("fonts/Geist-Bold.ttf"),
        include_bytes!("fonts/GeistMono.ttf"),
        include_bytes!("fonts/GeistMono-Medium.ttf"),
        include_bytes!("fonts/GeistMono-SemiBold.ttf"),
    ]
    .into_iter()
    .map(|bytes| FontData::new(Arc::<[u8]>::from(bytes)).expect("bundled font"))
    .collect()
}

// Motion ------------------------------------------------------------------------

/// A CSS cubic-bezier, solved as Zeron's `CubicBezier`: Newton steps, then
/// bisection, output clamped to [0, 1].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Curve(pub f32, pub f32, pub f32, pub f32);

impl Curve {
    pub fn eval(self, x: f32) -> f32 {
        let x = x.clamp(0., 1.);
        if x <= 0. || x >= 1. {
            return x;
        }
        let Curve(x1, y1, x2, y2) = self;
        let bez = |t: f32, a: f32, b: f32| {
            let u = 1. - t;
            3. * u * u * t * a + 3. * u * t * t * b + t * t * t
        };
        let slope = |t: f32, a: f32, b: f32| {
            let u = 1. - t;
            3. * u * u * a + 6. * u * t * (b - a) + 3. * t * t * (1. - b)
        };
        let mut t = x;
        for _ in 0..8 {
            let error = bez(t, x1, x2) - x;
            if error.abs() < 1e-6 {
                return bez(t, y1, y2).clamp(0., 1.);
            }
            let d = slope(t, x1, x2);
            if d.abs() < 1e-6 {
                break;
            }
            t -= error / d;
        }
        let (mut lo, mut hi) = (0_f32, 1_f32);
        t = x;
        for _ in 0..32 {
            let value = bez(t, x1, x2);
            if (value - x).abs() < 1e-6 {
                break;
            }
            if value < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.;
        }
        bez(t, y1, y2).clamp(0., 1.)
    }
}

pub const EASE_OUT_EXPO: Curve = Curve(0.16, 1., 0.3, 1.);
pub const EASE_OUT: Curve = Curve(0., 0., 0.58, 1.);
pub const EASE: Curve = Curve(0.25, 0.1, 0.25, 1.);
pub const EASE_OUT_QUINT: Curve = Curve(0.22, 1., 0.36, 1.);
pub const EASE_IN_OUT: Curve = Curve(0.42, 0., 0.58, 1.);
pub const EASE_TAILWIND: Curve = Curve(0.4, 0., 0.2, 1.);
pub const EASE_OUT_SOFT: Curve = Curve(0.2, 0., 0., 1.);

/// A duration (seconds) and its curve.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spec(pub f32, pub Curve);

impl Spec {
    /// Progress `elapsed` seconds in.
    pub fn at(self, elapsed: f32) -> f32 {
        self.1.eval(elapsed / self.0)
    }
}

pub const FADE_IN: Spec = Spec(0.5, EASE_OUT_EXPO);
pub const FADE_QUICK: Spec = Spec(0.15, EASE);
pub const MENU_IN: Spec = Spec(0.14, EASE);
pub const MENU_OUT: Spec = Spec(0.1, EASE);
pub const DIALOG_IN: Spec = Spec(0.18, EASE);
pub const RESIZE: Spec = Spec(0.2, EASE_OUT);
pub const COLLAPSE: Spec = Spec(0.18, EASE_OUT);
pub const HOVER_FADE: Spec = Spec(0.15, EASE_TAILWIND);
pub const GLYPH_STATE: Spec = Spec(0.28, EASE_OUT_QUINT);
pub const RESORT: Spec = Spec(0.26, EASE_OUT_QUINT);
pub const SCROLL_GLIDE: Spec = Spec(0.5, EASE_IN_OUT);

/// Zeron's activity-grid opacity (`gspin_opacity`): 1 down to `dim`
/// linearly over the first 45% of a cycle, held until 92%, back to 1.
pub fn gspin(phase: f32, dim: f32) -> f32 {
    let t = phase.rem_euclid(1.);
    if t < 0.45 {
        1. - (1. - dim) * (t / 0.45)
    } else if t < 0.92 {
        dim
    } else {
        dim + (1. - dim) * ((t - 0.92) / 0.08)
    }
}

/// A value easing from `from` to `target`, starting at `at`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tween {
    pub from: f32,
    pub target: f32,
    pub at: f32,
}

impl Tween {
    pub fn settled(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            at: f32::NEG_INFINITY,
        }
    }
    pub fn value(self, now: f32, spec: Spec) -> f32 {
        self.from + (self.target - self.from) * spec.at(now - self.at)
    }
    /// Retarget from wherever it is now, so reversals don't jump.
    pub fn toward(self, target: f32, now: f32, spec: Spec) -> Self {
        Self {
            from: self.value(now, spec),
            target,
            at: now,
        }
    }
    pub fn end(self, spec: Spec) -> f32 {
        self.at + spec.0
    }
}

// Clock ---------------------------------------------------------------------------

/// Looping ambient motion (shimmer, pulses) runs at Zeron's pulse rate.
const AMBIENT_HZ: f64 = 30.;

/// The page clock: full rate until `until`, `AMBIENT_HZ` while any ambient
/// loop is registered, stopped otherwise. Views read it with `at`, which
/// stops tracking once their animation has settled.
#[derive(Clone)]
pub struct Anim {
    /// Full-rate time for fades and tweens.
    pub clock: Signal<f32>,
    /// Time for looping decoration (spinners, shimmer), stepped at
    /// `AMBIENT_HZ` even while `clock` runs at the display rate.
    pub ambient_clock: Signal<f32>,
    origin: Instant,
    until: Rc<Cell<f32>>,
    ambient: Rc<Cell<u32>>,
    running: Rc<Cell<bool>>,
    tasks: Tasks,
    frames: FrameClock,
}

impl Anim {
    pub fn new(cx: &mut Context) -> Self {
        Self {
            clock: cx.state(0.),
            ambient_clock: cx.state(0.),
            origin: Instant::now(),
            until: Rc::new(Cell::new(0.)),
            ambient: Rc::new(Cell::new(0)),
            running: Rc::new(Cell::new(false)),
            tasks: cx.tasks(),
            frames: cx.frames(),
        }
    }
    pub fn now(&self) -> f32 {
        self.origin.elapsed().as_secs_f32()
    }
    /// Keep full-rate frames coming for `seconds` more.
    pub fn keep(&self, seconds: f32) {
        self.until.set(self.until.get().max(self.now() + seconds));
        self.run();
    }
    /// Register an ambient loop for as long as the guard lives.
    pub fn ambient(&self) -> AmbientGuard {
        self.ambient.set(self.ambient.get() + 1);
        self.run();
        AmbientGuard(self.ambient.clone())
    }
    fn run(&self) {
        if self.running.replace(true) {
            return;
        }
        let anim = self.clone();
        self.tasks.spawn(async move {
            loop {
                let full = anim.now() < anim.until.get();
                if !full && anim.ambient.get() == 0 {
                    break;
                }
                let frame = if full {
                    anim.frames.next().await
                } else {
                    anim.frames.next().max_rate(AMBIENT_HZ).await
                };
                // The frame's presentation time, not the wake-up time: steps
                // land a whole refresh apart however late the task runs.
                let at = frame
                    .time
                    .saturating_duration_since(anim.origin)
                    .as_secs_f32();
                let at = at.max(anim.clock.with_untracked(|c| *c));
                anim.clock.set(at);
                // A millisecond of slack keeps a 120 Hz display on every
                // fourth refresh rather than drifting to every fifth.
                let last = anim.ambient_clock.with_untracked(|c| *c);
                if at - last >= 1. / AMBIENT_HZ as f32 - 0.001 {
                    anim.ambient_clock.set(at);
                }
            }
            anim.running.set(false);
        });
    }
    /// The time, tracked until `end` and constant afterwards.
    pub fn at(&self, end: f32) -> f32 {
        let now = self.clock.with_untracked(|now| *now);
        if now >= end { now } else { self.clock.get() }
    }
    /// A tween's value, tracking the clock only while it moves.
    pub fn tween(&self, tween: &Signal<Tween>, spec: Spec) -> f32 {
        let t = tween.get();
        t.value(self.at(t.end(spec)), spec)
    }
    /// Retarget a tween and keep frames coming while it moves.
    pub fn retarget(&self, tween: &Signal<Tween>, target: f32, spec: Spec) {
        let now = self.now();
        if tween.with_untracked(|t| t.target == target) {
            return;
        }
        tween.update(|t| *t = t.toward(target, now, spec));
        self.keep(spec.0 + 0.05);
    }
}

pub struct AmbientGuard(Rc<Cell<u32>>);

impl Drop for AmbientGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// A pointer-hover blend (Zeron's `hover_blend`): 0 at rest, 1 hovered,
/// eased over `HOVER_FADE` and reversible mid-way.
#[derive(Clone)]
pub struct Hover {
    pub tween: Signal<Tween>,
    anim: Anim,
}

impl Hover {
    pub fn new(cx: &mut Context, anim: &Anim) -> Self {
        Self {
            tween: cx.state(Tween::settled(0.)),
            anim: anim.clone(),
        }
    }
    /// The eased blend, tracked while it moves.
    pub fn get(&self) -> f32 {
        self.anim.tween(&self.tween, HOVER_FADE)
    }
    /// Whether the pointer is over it (the target, not the blend).
    pub fn on(&self) -> bool {
        self.tween.with(|t| t.target > 0.5)
    }
    pub fn set(&self, on: bool) {
        self.anim
            .retarget(&self.tween, if on { 1. } else { 0. }, HOVER_FADE);
    }
    /// Track the pointer over `view`.
    pub fn track(&self, view: View) -> View {
        let hover = self.clone();
        view.on_event(move |event| match event.event {
            InputEvent::PointerEnter => hover.set(true),
            InputEvent::PointerLeave | InputEvent::PointerCancel => hover.set(false),
            _ => {}
        })
    }
}

// Icons ---------------------------------------------------------------------------

const ICONS: &[(&str, &[u8])] = &[
    ("chevron", include_bytes!("icons/alt-arrow-down.svg")),
    ("alt-arrow-down", include_bytes!("icons/alt-arrow-down.svg")),
    (
        "alt-arrow-right",
        include_bytes!("icons/alt-arrow-right.svg"),
    ),
    ("alt-arrow-up", include_bytes!("icons/alt-arrow-up.svg")),
    (
        "antigravity-mark",
        include_bytes!("icons/antigravity-mark.svg"),
    ),
    (
        "archive-minimalistic",
        include_bytes!("icons/archive-minimalistic.svg"),
    ),
    (
        "archive-up-minimalistic",
        include_bytes!("icons/archive-up-minimalistic.svg"),
    ),
    ("arrow-left", include_bytes!("icons/arrow-left.svg")),
    ("arrow-right", include_bytes!("icons/arrow-right.svg")),
    ("arrow-up", include_bytes!("icons/arrow-up.svg")),
    ("arrow-up-right", include_bytes!("icons/arrow-up-right.svg")),
    ("bot", include_bytes!("icons/bot.svg")),
    ("check", include_bytes!("icons/check.svg")),
    ("checklist", include_bytes!("icons/checklist.svg")),
    ("claude-mark", include_bytes!("icons/claude-mark.svg")),
    ("close", include_bytes!("icons/close.svg")),
    ("close-circle", include_bytes!("icons/close-circle.svg")),
    ("copy", include_bytes!("icons/copy.svg")),
    ("cursor-mark", include_bytes!("icons/cursor-mark.svg")),
    (
        "danger-triangle",
        include_bytes!("icons/danger-triangle.svg"),
    ),
    ("devin-mark", include_bytes!("icons/devin-mark.svg")),
    ("document", include_bytes!("icons/document.svg")),
    ("document-add", include_bytes!("icons/document-add.svg")),
    ("fast-tier", include_bytes!("icons/fast-tier.svg")),
    ("fast-tier-bold", include_bytes!("icons/fast-tier-bold.svg")),
    ("folder", include_bytes!("icons/folder.svg")),
    (
        "folder-with-files",
        include_bytes!("icons/folder-with-files.svg"),
    ),
    ("git-branch", include_bytes!("icons/git-branch.svg")),
    ("global", include_bytes!("icons/global.svg")),
    ("grok-mark", include_bytes!("icons/grok-mark.svg")),
    ("hermes-mark", include_bytes!("icons/hermes-mark.svg")),
    ("magnifer", include_bytes!("icons/magnifer.svg")),
    ("microphone", include_bytes!("icons/microphone.svg")),
    (
        "more-horizontal",
        include_bytes!("icons/more-horizontal.svg"),
    ),
    ("openai-mark", include_bytes!("icons/openai-mark.svg")),
    ("opencode-mark", include_bytes!("icons/opencode-mark.svg")),
    ("paperclip", include_bytes!("icons/paperclip.svg")),
    ("pen", include_bytes!("icons/pen.svg")),
    ("pi-mark", include_bytes!("icons/pi-mark.svg")),
    ("plus", include_bytes!("icons/plus.svg")),
    ("pull-request", include_bytes!("icons/pull-request.svg")),
    ("settings", include_bytes!("icons/settings.svg")),
    ("star-bold", include_bytes!("icons/star-bold.svg")),
    ("terminal", include_bytes!("icons/terminal.svg")),
    ("thought", include_bytes!("icons/chat-round-line.svg")),
    (
        "chat-round-line",
        include_bytes!("icons/chat-round-line.svg"),
    ),
    ("widget", include_bytes!("icons/widget.svg")),
    ("wifi-off", include_bytes!("icons/wifi-off.svg")),
    ("worktree", include_bytes!("icons/worktree.svg")),
    ("wrap-text", include_bytes!("icons/wrap-text.svg")),
    // Older names the transcript still uses.
    ("search", include_bytes!("icons/magnifer.svg")),
];

/// Polychrome file icons (Zeron's file-icon theme), by icon name.
const FILE_ICONS: &[(&str, &[u8])] = &[
    (
        "brackets-purple",
        include_bytes!("icons/files/brackets-purple.svg"),
    ),
    (
        "brackets-yellow",
        include_bytes!("icons/files/brackets-yellow.svg"),
    ),
    ("code-orange", include_bytes!("icons/files/code-orange.svg")),
    ("document", include_bytes!("icons/files/document.svg")),
    ("gear", include_bytes!("icons/files/gear.svg")),
    ("go", include_bytes!("icons/files/go.svg")),
    ("image", include_bytes!("icons/files/image.svg")),
    ("js", include_bytes!("icons/files/js.svg")),
    ("lock", include_bytes!("icons/files/lock.svg")),
    ("markdown", include_bytes!("icons/files/markdown.svg")),
    ("python", include_bytes!("icons/files/python.svg")),
    ("react-ts", include_bytes!("icons/files/react-ts.svg")),
    ("rust", include_bytes!("icons/files/rust.svg")),
    ("shell", include_bytes!("icons/files/shell.svg")),
    ("svg", include_bytes!("icons/files/svg.svg")),
    ("swift", include_bytes!("icons/files/swift.svg")),
    ("text", include_bytes!("icons/files/text.svg")),
    ("ts", include_bytes!("icons/files/ts.svg")),
    ("yaml", include_bytes!("icons/files/yaml.svg")),
];

thread_local! {
    static ICON_DATA: HashMap<&'static str, SvgData> = ICONS
        .iter()
        .map(|(name, bytes)| (*name, SvgData::new(*bytes).expect("bundled icon")))
        .collect();
    static FILE_ICON_DATA: HashMap<&'static str, Arc<SvgData>> = FILE_ICONS
        .iter()
        .map(|(name, bytes)| (*name, Arc::new(SvgData::new(*bytes).expect("bundled file icon"))))
        .collect();
}

pub fn icon_data(name: &str) -> SvgData {
    ICON_DATA.with(|icons| {
        icons
            .get(name)
            .or_else(|| icons.get("widget"))
            .cloned()
            .expect("icon")
    })
}

/// A monochrome icon tinted `color`, `size` square.
pub fn icon(name: &str, size: f32, color: Color) -> View {
    svg(name, Arc::new(icon_data(name).tinted(color))).size(size, size)
}

/// The polychrome file icon for a path, by its extension.
pub fn file_icon_data(path: &str) -> Arc<SvgData> {
    let extension = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let name = match extension.as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "ts",
        "tsx" | "jsx" => "react-ts",
        "js" | "mjs" | "cjs" => "js",
        "json" | "jsonc" => "brackets-yellow",
        "md" | "mdx" => "markdown",
        "toml" => "gear",
        "yaml" | "yml" => "yaml",
        "css" | "scss" => "brackets-purple",
        "html" => "code-orange",
        "py" => "python",
        "go" => "go",
        "swift" => "swift",
        "sh" | "bash" | "zsh" => "shell",
        "lock" => "lock",
        "svg" => "svg",
        "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
        "txt" => "text",
        _ => "document",
    };
    FILE_ICON_DATA.with(|icons| icons.get(name).cloned().expect("file icon"))
}

pub fn file_icon(path: &str, size: f32) -> View {
    svg("file", file_icon_data(path)).size(size, size)
}

/// A harness's brand mark and tint (Claude's is its brand orange; the rest
/// take the caller's colour).
pub fn harness_mark(harness: &str) -> (&'static str, Option<Color>) {
    match harness {
        h if h.starts_with("claude") => ("claude-mark", Some(CLAUDE)),
        h if h.starts_with("codex") || h.starts_with("openai") => ("openai-mark", None),
        h if h.starts_with("cursor") => ("cursor-mark", None),
        h if h.starts_with("devin") => ("devin-mark", None),
        h if h.starts_with("grok") => ("grok-mark", None),
        h if h.starts_with("hermes") => ("hermes-mark", None),
        h if h.starts_with("pi") => ("pi-mark", None),
        h if h.starts_with("opencode") => ("opencode-mark", None),
        h if h.starts_with("antigravity") || h.starts_with("gemini") => ("antigravity-mark", None),
        _ => ("bot", None),
    }
}

/// A harness mark at `size`: brand-tinted, else `fallback`.
pub fn harness_icon(harness: &str, size: f32, fallback: Color) -> View {
    let (name, tint) = harness_mark(harness);
    icon(name, size, tint.unwrap_or(fallback))
}
