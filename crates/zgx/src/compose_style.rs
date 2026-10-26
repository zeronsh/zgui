//! Styling lowered onto retained nodes; state changes preserve mounted children.
use crate::style::Opt32;
use crate::{
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Color, NodeId, NodeKind, QuadStyle},
    style::Styles,
    theme::{ThemeScope, Themed, tok},
    widgets::Ui,
};

/// What text inherits. A colour, size or family that comes from a theme
/// token stays the token: each text node resolves it from its own theme
/// scope, so a theme change restyles the text that shows it without
/// changing (and so re-running) what it is inherited through, and text in a
/// scoped subtree (a light card) takes the scope's value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Typography {
    pub display: crate::text_layout::TextOptions,
    pub color: Themed<Color>,
    pub size: Themed<f32>,
    pub wrap: bool,
    pub font: crate::text_layout::FontStyle,
    /// The family's token, when it comes from one (`font.family` is then
    /// the default and unused).
    pub family: Option<crate::theme::TokenId>,
}
impl Typography {
    /// The colour to show, read from `scope` if it is a token (subscribing).
    pub(crate) fn color_in(&self, scope: Option<&ThemeScope>) -> Color {
        resolve_color(self.color, scope)
    }
    pub(crate) fn size_in(&self, scope: Option<&ThemeScope>) -> f32 {
        resolve_size(self.size, scope)
    }
    pub(crate) fn font_in(&self, scope: Option<&ThemeScope>) -> crate::text_layout::FontStyle {
        let mut font = self.font.clone();
        if let Some(token) = self.family
            && scope.is_some()
        {
            font.family = resolve_family(token, scope);
        }
        font
    }
    /// With colour, size and family resolved: what a text node shows. The
    /// body tokens (`text.primary`, `font.size.md`, `font.sans`, what text
    /// inherits unless a view says otherwise) are read together, in place.
    fn shown(mut self, scope: Option<&ThemeScope>, subscribed: &mut bool) -> Self {
        let is = |value: Themed<f32>, token: crate::theme::TokenId| matches!(value, Themed::Token(t) if t == token);
        let body_color = matches!(self.color, Themed::Token(t) if t == tok::TEXT_PRIMARY.0);
        let body_size = is(self.size, tok::FONT_SIZE_MD.0);
        let body_family = self.family == Some(tok::FONT_SANS.0);
        if let Some(scope) = scope
            && (body_color || body_size || body_family)
        {
            read_body(scope, subscribed, |body| {
                if body_color {
                    self.color = Themed::Value(body.color);
                }
                if body_size {
                    self.size = Themed::Value(body.size);
                }
                if body_family && self.font.family != body.family {
                    self.font.family = body.family.clone();
                }
            });
            if body_family {
                self.family = None;
            }
        }
        self.color = Themed::Value(resolve_color(self.color, scope));
        self.size = Themed::Value(resolve_size(self.size, scope));
        if let Some(token) = self.family.take()
            && scope.is_some()
        {
            self.font.family = resolve_family(token, scope);
        }
        self
    }
    /// A resolved size (see `shown`).
    fn size_value(size: Themed<f32>) -> f32 {
        match size {
            Themed::Value(size) => size,
            Themed::Token(_) => 16.,
        }
    }
}
/// A colour, read from `scope` if it is a token (subscribing).
#[inline(always)]
fn resolve_color(color: Themed<Color>, scope: Option<&ThemeScope>) -> Color {
    match (color, scope) {
        (Themed::Value(color), _) => color,
        // The body tokens text inherits read as one dependency.
        (Themed::Token(token), Some(scope)) if token == tok::TEXT_PRIMARY.0 => {
            scope.with_body(|body| body.color)
        }
        (Themed::Token(token), Some(scope)) => {
            scope.get(token).color().unwrap_or(Color(0, 0, 0, 0))
        }
        (Themed::Token(_), None) => Color(0, 0, 0, 0),
    }
}
fn resolve_family(
    token: crate::theme::TokenId,
    scope: Option<&ThemeScope>,
) -> crate::text_layout::FontFamily {
    match scope {
        Some(scope) if token == tok::FONT_SANS.0 => scope.with_body(|body| body.family.clone()),
        Some(scope) => scope.get(token).font().unwrap_or_default(),
        None => Default::default(),
    }
}
/// The body text style in `scope`, read by a text node's style effect: it
/// subscribes the effect the first time, for good, and reads untracked.
fn read_body<R>(
    scope: &ThemeScope,
    subscribed: &mut bool,
    f: impl FnOnce(&crate::theme::Body) -> R,
) -> R {
    if !std::mem::replace(subscribed, true) {
        scope.subscribe_body();
    }
    scope.with_body_untracked(f)
}
/// Whether `token` resolves to `family` in `scope` (subscribing), without
/// cloning what it resolves to.
fn family_is(
    token: crate::theme::TokenId,
    scope: Option<&ThemeScope>,
    family: &crate::text_layout::FontFamily,
    subscribed: &mut bool,
) -> bool {
    match scope {
        Some(scope) if token == tok::FONT_SANS.0 => {
            read_body(scope, subscribed, |body| body.family == *family)
        }
        _ => resolve_family(token, scope) == *family,
    }
}
#[inline(always)]
fn resolve_size(size: Themed<f32>, scope: Option<&ThemeScope>) -> f32 {
    match (size, scope) {
        (Themed::Value(size), _) => size,
        (Themed::Token(token), Some(scope)) if token == tok::FONT_SIZE_MD.0 => {
            scope.with_body(|body| body.size)
        }
        (Themed::Token(token), Some(scope)) => scope.get(token).number().unwrap_or(16.),
        (Themed::Token(_), None) => 16.,
    }
}
/// Which token, if any, sets `field` in `styles` (a blend of two is passed
/// on as the colour it resolved to).
fn token_of(styles: &Styles, field: crate::theme::Field) -> Option<crate::theme::TokenId> {
    styles
        .tokens
        .as_ref()?
        .0
        .iter()
        .rev()
        .find(|(bound, _, _)| *bound == field)
        .and_then(|(_, token, mix)| mix.is_none().then_some(*token))
}
/// A resolved text colour as typography carries it: the token if one set it.
fn themed_color(styles: &Styles) -> Option<Themed<Color>> {
    let color = styles.text_color?;
    Some(match token_of(styles, crate::theme::Field::TextColor) {
        Some(token) => Themed::Token(token),
        None => Themed::Value(color),
    })
}
/// Styles laid over a node's own while it is hovered, active, focused or
/// disabled. Boxed: most views set none, and every view carries the set.
#[derive(Default)]
pub(crate) struct Variants {
    pub hover: Option<Box<Styles>>,
    pub active: Option<Box<Styles>>,
    pub focus: Option<Box<Styles>>,
    pub disabled: Option<Box<Styles>>,
    /// A built-in widget's own hover, active and focus styles, where the
    /// view sets none: made the first time each comes into effect, so a
    /// widget never hovered, pressed or focused keeps none of them. They
    /// set no typography.
    pub defaults: [Option<fn() -> Styles>; 3],
    /// Hover, active, focus and disabled styles that change (`View::
    /// reactive_interaction`): they replace the four above when it returns
    /// them.
    pub reactive: Option<InteractionStyles>,
}
/// Run in a node's style effect, so it follows what it reads: the hover,
/// active, focus and disabled styles when they changed since its last call,
/// or `None` to keep them.
pub(crate) type InteractionStyles = Box<dyn FnMut() -> Option<[Option<Styles>; 4]>>;
#[derive(Clone, Copy, PartialEq)]
enum ActivationKey {
    Space,
    Enter,
}
#[derive(Clone, Copy, Default, PartialEq)]
struct Interaction {
    hover: bool,
    pointer_active: bool,
    /// Pressed, but the pointer left it with the button held: it looks
    /// unpressed until the pointer comes back (as a native button does).
    pressed_outside: bool,
    key_active: Option<ActivationKey>,
    focus: bool,
    /// The focus shows (CSS's `:focus-visible`): it came from the keyboard,
    /// the node edits text, or a key went to it since.
    focus_visible: bool,
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn mount_style(
    ui: &mut Ui,
    node: NodeId,
    base: Styles,
    inherited: Signal<Typography>,
    dynamic: Option<Box<dyn FnMut() -> Styles>>,
    variants: Variants,
    disabled: Option<Box<dyn FnMut() -> bool>>,
    interactive: bool,
) -> Signal<Typography> {
    mount_style_with_intrinsic(
        ui,
        node,
        base,
        inherited,
        dynamic.map(|mut style| {
            Box::new(move |out: &mut Option<Styles>| *out = Some(style())) as DynamicStyle
        }),
        variants,
        disabled,
        interactive,
        None,
        None,
        None,
        None,
    )
}

/// Resolved styles, typography, intrinsic size and content layout target.
type AppliedInputs = (Styles, Typography, Option<(f32, f32)>, Option<NodeId>);
/// Paint properties an animation changes every frame: `Styles`' opacity,
/// blurs, edge fade, mask offset, translation, local transform and text
/// colour.
#[derive(Clone, Copy, PartialEq, Default)]
struct Paint {
    opacity: Opt32,
    blur: Opt32,
    edge_fade: Opt32,
    filter_blur: Opt32,
    brightness: Opt32,
    contrast: Opt32,
    saturate: Opt32,
    mask_offset: Option<(f32, f32)>,
    clip_inset: Option<crate::scene::ClipInset>,
    transform: Option<crate::scene::Transform>,
    local: crate::style::TransformPatch,
    text_color: Option<crate::scene::Color>,
}
impl Paint {
    /// The bits (`crate::style::bit`) of the properties paint holds.
    const BITS: u64 = {
        use crate::style::bit;
        bit::OPACITY
            | bit::BLUR
            | bit::EDGE_FADE
            | bit::FILTER_BLUR
            | bit::BRIGHTNESS
            | bit::CONTRAST
            | bit::SATURATE
            | bit::MASK_OFFSET
            | bit::CLIP_INSET
            | bit::TRANSFORM
            | bit::LOCAL_TRANSFORM
            | bit::TEXT_COLOR
    };
    /// Take `styles`' paint properties out, leaving them unset. None set
    /// (`Styles::set_bits`): none to take.
    fn take(styles: &mut Styles) -> Self {
        let set = styles.set_bits();
        if set & Self::BITS == 0 {
            return Self::default();
        }
        use crate::style::bit;
        // Each of `BITS` taken below, left unset. One whose bit is unset
        // is unset already (`Styles::set`): only those set are taken (an
        // animation sets two or three of the twelve).
        let styles = styles.fields_taken(Self::BITS);
        macro_rules! taken {
            ($field:ident, $bit:ident) => {
                match set & bit::$bit {
                    0 => Default::default(),
                    _ => std::mem::take(&mut styles.$field),
                }
            };
        }
        Self {
            opacity: taken!(opacity, OPACITY),
            blur: taken!(blur, BLUR),
            edge_fade: taken!(edge_fade, EDGE_FADE),
            filter_blur: taken!(filter_blur, FILTER_BLUR),
            brightness: taken!(brightness, BRIGHTNESS),
            contrast: taken!(contrast, CONTRAST),
            saturate: taken!(saturate, SATURATE),
            mask_offset: taken!(mask_offset, MASK_OFFSET),
            clip_inset: taken!(clip_inset, CLIP_INSET),
            transform: taken!(transform, TRANSFORM),
            local: taken!(local_transform, LOCAL_TRANSFORM),
            text_color: taken!(text_color, TEXT_COLOR),
        }
    }
    fn put(self, styles: &mut Styles) {
        // Nothing to write over nothing.
        if styles.set_bits() & Self::BITS == 0 && self == Self::default() {
            return;
        }
        let styles = styles.fields_mut(Self::BITS);
        styles.opacity = self.opacity;
        styles.blur = self.blur;
        styles.edge_fade = self.edge_fade;
        styles.filter_blur = self.filter_blur;
        styles.brightness = self.brightness;
        styles.contrast = self.contrast;
        styles.saturate = self.saturate;
        styles.mask_offset = self.mask_offset;
        styles.clip_inset = self.clip_inset;
        styles.transform = self.transform;
        styles.local_transform = self.local;
        styles.text_color = self.text_color;
    }
    /// Which properties are set, whatever their values.
    fn set(&self) -> ([bool; 11], u8) {
        let set = [
            self.brightness.is_some(),
            self.contrast.is_some(),
            self.saturate.is_some(),
            self.clip_inset.is_some(),
            self.opacity.is_some(),
            self.blur.is_some(),
            self.edge_fade.is_some(),
            self.filter_blur.is_some(),
            self.mask_offset.is_some(),
            self.transform.is_some(),
            self.text_color.is_some(),
        ];
        (set, self.local.set)
    }
    /// `self` with what `over` sets laid over it, as `Styles::merge` does.
    /// What `over` sets laid over `self`, as `Styles::merge` does. In
    /// place: by value, each of the 150-byte paints was copied in and the
    /// result out again, every run of an animated style.
    fn lay(&mut self, over: &Self) {
        macro_rules! lay {
            ($($field:ident),*) => {$(
                if over.$field.is_some() {
                    self.$field = over.$field;
                }
            )*};
        }
        lay!(
            opacity,
            blur,
            edge_fade,
            filter_blur,
            brightness,
            contrast,
            saturate,
            mask_offset,
            clip_inset,
            transform,
            text_color
        );
        self.local = self.local.over(over.local);
    }
    fn of(styles: &Styles) -> Self {
        if styles.set_bits() & Self::BITS == 0 {
            return Self::default();
        }
        Self {
            opacity: styles.opacity,
            blur: styles.blur,
            edge_fade: styles.edge_fade,
            filter_blur: styles.filter_blur,
            brightness: styles.brightness,
            contrast: styles.contrast,
            saturate: styles.saturate,
            mask_offset: styles.mask_offset,
            clip_inset: styles.clip_inset,
            transform: styles.transform,
            local: styles.local_transform,
            text_color: styles.text_color,
        }
    }
}
/// Give `output` the text colour `color`, notifying what inherits it if
/// that changes it: in place, where `Signal::update` would clone and compare
/// the whole typography (an animated text colour does this every frame).
fn set_color(output: &Signal<Typography>, color: Themed<crate::scene::Color>) {
    output.mutate(|typography| std::mem::replace(&mut typography.color, color) != color);
}
/// Set `node`'s mask from `resolved`: its image and size, or else the
/// node's own mask, at its offset if set. Nodes without one cost nothing.
fn apply_mask(
    scene: &mut crate::scene::Scene,
    node: NodeId,
    resolved: &Styles,
    original: &Option<crate::scene::ImageMask>,
) {
    let mask = match &resolved.rare().mask {
        Some(mask) => mask.as_ref().map(|(image, size)| crate::scene::ImageMask {
            image: image.clone(),
            offset: (0., 0.),
            size: *size,
        }),
        None if original.is_none() => return,
        None => original.clone(),
    };
    let mask = mask.map(|mask| crate::scene::ImageMask {
        offset: resolved.mask_offset.unwrap_or(mask.offset),
        ..mask
    });
    scene.set_mask(node, mask);
}
/// A paint-only run's effects, mask, clip inset (unless `inset_kept`) and
/// translation, for a node its dynamic style moves and fades (an
/// animation's every frame): its old and new look damaged once each, not
/// once per write. The inset, written after the move, damages at the new
/// place rather than the old: each covers what it paints at both, so the
/// order changes no pixel.
#[inline(never)]
fn show_moved_and_faded(
    scene: &mut crate::scene::Scene,
    node: NodeId,
    transform: crate::scene::Transform,
    effects: crate::scene::Effects,
    applied: &Styles,
    originals: &Originals,
    inset_kept: bool,
) {
    scene.set_transform_and_effects(node, transform, effects);
    apply_mask(scene, node, applied, &originals.mask);
    if !inset_kept {
        scene.set_clip_inset(node, applied.clip_inset.or(originals.inset));
    }
}
/// The inputs of the last full application besides paint: the dynamic
/// styles without their paint properties, the interaction and disabled
/// state, the inherited typography's version if it was read, the intrinsic
/// size and layout target. A run that matches them changes paint alone.
struct Settled {
    dynamic: Option<Styles>,
    paint: Option<([bool; 11], u8)>,
    /// Whether that paint sets a translation and an opacity: a paint-only
    /// run writes them together (`show_moved_and_faded`).
    moves_and_fades: bool,
    state: Interaction,
    disabled: bool,
    /// Whether the inherited typography was read, at `parent_version`. Its
    /// version changes only with its value, so the version alone tells: a
    /// copy of it made every memo a size class bigger (1,400 bytes).
    parent: bool,
    parent_version: u64,
    intrinsic: Option<(f32, f32)>,
    content: Option<NodeId>,
    /// A text node's size and family as inherited (tokens, maybe): a
    /// paint-only run resolves them again, to follow their tokens.
    text: Option<(Themed<f32>, Option<crate::theme::TokenId>)>,
}
/// What a re-run compares against: the inputs last applied to the scene
/// (unchanged results skip rebuilding and rewriting the node) and, for
/// paint-only changes, the last full application's other inputs.
struct Memo {
    applied: Option<AppliedInputs>,
    settled: Option<Settled>,
    /// The layout style the last full application gave the node: a run that
    /// changes only box sizes and margins patches it (see `boxes`).
    shown: Option<crate::scene::Style>,
    /// Whether the last run wrote a size change with no dynamic paint
    /// (false after any other application). Another such run writes no
    /// paint: a bar growing every frame re-wrote its unchanged effects,
    /// transform and clip, which also undid what else had set them since.
    /// (Only for size changes: checking every run cost animated paint more
    /// than it saved.)
    painted: bool,
    /// What `Scene::set_box` left from patching `shown`'s box: `None` once
    /// `shown` is made or written another way.
    boxed: Option<u32>,
}
/// Which box fields `styles` sets (width, height, margins), whatever their
/// values: a run that sets the same ones, with other values, changes only
/// those fields of the node's layout style.
#[inline(always)]
fn boxes(styles: &Styles) -> (bool, bool, bool) {
    (
        styles.width.is_some() || styles.width_percent.is_some(),
        styles.height.is_some() || styles.height_percent.is_some(),
        styles.margin.is_some(),
    )
}
/// The box fields' values (see `boxes`).
type Sizes = (Opt32, Opt32, Opt32, Opt32, Option<crate::scene::Insets>);
fn sizes(styles: &Styles) -> Sizes {
    (
        styles.width,
        styles.width_percent,
        styles.height,
        styles.height_percent,
        styles.margin,
    )
}
fn set_sizes(styles: &mut Styles, sizes: Sizes) {
    use crate::style::bit;
    let fields = styles.fields_mut(
        bit::WIDTH | bit::WIDTH_PERCENT | bit::HEIGHT | bit::HEIGHT_PERCENT | bit::MARGIN,
    );
    (
        fields.width,
        fields.width_percent,
        fields.height,
        fields.height_percent,
        fields.margin,
    ) = sizes;
}
/// `style` with `next`'s box fields, as `apply_resolved` sets them.
fn patch_sizes(style: &mut crate::scene::Style, next: &Styles) {
    if next.width.is_some() || next.width_percent.is_some() {
        style.width = next.width.get();
        style.width_percent = next.width_percent.get();
    }
    if next.height.is_some() || next.height_percent.is_some() {
        style.height = next.height.get();
        style.height_percent = next.height_percent.get();
    }
    if let Some(margin) = next.margin {
        style.margin = margin;
    }
}
/// A computed style (`View::reactive_style`), written into its caller's
/// slot: returned, the 440-byte style was copied once made, a tenth of
/// what a style effect re-running spent.
pub(crate) type DynamicStyle = Box<dyn FnMut(&mut Option<Styles>)>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn mount_style_with_intrinsic(
    ui: &mut Ui,
    node: NodeId,
    base: Styles,
    inherited: Signal<Typography>,
    mut dynamic: Option<DynamicStyle>,
    variants: Variants,
    mut disabled: Option<Box<dyn FnMut() -> bool>>,
    interactive: bool,
    intrinsic: Option<Signal<(f32, f32)>>,
    layout_target: Option<Signal<Option<NodeId>>>,
    object_fit: Option<Signal<crate::style::ObjectFit>>,
    theme: Option<std::rc::Rc<crate::theme::ThemeScope>>,
) -> Signal<Typography> {
    let mut base = base;
    let mut reactive = variants.reactive;
    // A node setting no typography (nor through a variant or a dynamic
    // style) shows its parent's: it passes that signal on, not a copy.
    // Interaction styles that change might set it.
    let own = dynamic.is_some()
        || reactive.is_some()
        || base.sets_typography()
        || [
            &variants.hover,
            &variants.active,
            &variants.focus,
            &variants.disabled,
        ]
        .into_iter()
        .any(|variant| variant.as_deref().is_some_and(Styles::sets_typography));
    let defaults = variants.defaults;
    debug_assert!(
        defaults
            .iter()
            .flatten()
            .all(|default| !default().sets_typography()),
        "default variants set no typography"
    );
    let typography = if own {
        ui.signal(inherited.with_untracked(Clone::clone))
    } else {
        inherited.clone()
    };
    let output = own.then(|| typography.clone());
    // Only a node whose style can follow interaction keeps its state: most
    // styled nodes have no hover, press or focus style and are not
    // interactive, and a signal and its subscription each cost them.
    let interaction = (interactive
        || reactive.is_some()
        || variants.hover.is_some()
        || variants.active.is_some()
        || variants.focus.is_some()
        || defaults.iter().any(Option::is_some))
    .then(|| ui.signal(Interaction::default()));
    if let Some(interaction) = &interaction {
        let interaction = interaction.clone();
        // Only a press that shows (or activates) follows the pointer: a
        // view styled on hover alone leaves it to what is under it (a window
        // drag region around it).
        let presses =
            interactive || reactive.is_some() || variants.active.is_some() || defaults[1].is_some();
        let (input, owner) = (ui.input.clone(), ui.downgrade());
        ui.on_event(node, interactive, move |event| {
            if event.phase == EventPhase::Capture {
                return;
            }
            if event.default_prevented()
                && matches!(
                    event.event,
                    InputEvent::PointerDown { .. } | InputEvent::KeyDown { .. }
                )
            {
                return;
            }
            interaction.update(|state| match event.event {
                InputEvent::PointerEnter => {
                    state.hover = true;
                    // Back with the press still held (its capture in it):
                    // pressed again. A press let go elsewhere is over.
                    let held = state.pointer_active
                        && input.captured().is_some_and(|captured| {
                            owner
                                .upgrade()
                                .is_some_and(|ui| ui.scene.borrow().is_within(captured, node))
                        });
                    state.pointer_active = held;
                    state.pressed_outside = false;
                }
                InputEvent::PointerLeave => {
                    state.hover = false;
                    state.pressed_outside = state.pointer_active;
                }
                InputEvent::PointerDown {
                    button: PointerButton::Primary,
                    ..
                } => {
                    state.pointer_active = true;
                    state.pressed_outside = false;
                    if event.target == node && presses {
                        event.capture_pointer();
                    }
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    state.pointer_active = false;
                    state.pressed_outside = false;
                }
                InputEvent::Focus if event.target == node => {
                    state.focus = true;
                    state.focus_visible = input.focus_visible_now()
                        || owner.upgrade().is_some_and(|ui| ui.is_editor(node));
                }
                InputEvent::Blur if event.target == node => {
                    state.focus = false;
                    state.focus_visible = false;
                    state.pointer_active = false;
                    state.key_active = None;
                }
                InputEvent::KeyDown {
                    key: Key::Space,
                    repeat: false,
                    ..
                } if event.target == node => {
                    state.key_active = Some(ActivationKey::Space);
                    state.focus_visible = true;
                }
                InputEvent::KeyDown {
                    key: Key::Enter,
                    repeat: false,
                    ..
                } if event.target == node => {
                    state.key_active = Some(ActivationKey::Enter);
                    state.focus_visible = true;
                }
                // A key to it (not a shortcut) shows its focus.
                InputEvent::KeyDown { modifiers, .. }
                    if event.target == node
                        && !modifiers.meta
                        && !modifiers.control
                        && !modifiers.alt =>
                {
                    state.focus_visible = true;
                }
                InputEvent::KeyUp {
                    key: Key::Space, ..
                } if event.target == node && state.key_active == Some(ActivationKey::Space) => {
                    state.key_active = None
                }
                InputEvent::KeyUp {
                    key: Key::Enter, ..
                } if event.target == node && state.key_active == Some(ActivationKey::Enter) => {
                    state.key_active = None
                }
                _ => {}
            });
        });
    }
    let mut variants: [Option<Box<Styles>>; 4] = [
        variants.hover,
        variants.active,
        variants.focus,
        variants.disabled,
    ];
    let weak = ui.downgrade();
    let originals = Originals {
        style: Some(ui.scene.borrow().style(node))
            .filter(|style| !style.is_default())
            .map(Box::new),
        effects: ui.scene.borrow().effects(node),
        transform: ui.scene.borrow().transform(node),
        local: ui.scene.borrow().local_transform(node),
        isolated: ui.scene.borrow().is_isolated(node),
        mask: ui.scene.borrow().mask(node).cloned(),
        inset: ui.scene.borrow().clip_inset(node),
        kind: ui.scene.borrow().kind(node).clone(),
    };
    let mut previous_disabled = None;
    // What re-runs compare against, kept from a node's second run: most
    // nodes are styled once, and each memo costs about 1.2 KB.
    let mut memo: Option<Box<Memo>> = None;
    // A frosted glass's: whether it last painted opaque (`opaque_glass`).
    // `None` for a node without a backdrop blur, which never reads it.
    let mut glass: Option<bool> = None;
    let mut ran = false;
    // Theme tokens in the base and variants resolve in place, each run
    // reading (and so subscribing to) exactly the tokens they use.
    // Text resolves inherited tokens from the scope; styles with tokens of
    // their own, or computed ones (which may return tokens), resolve theirs.
    let is_text = matches!(originals.kind, NodeKind::Text { .. });
    let scope = if is_text { theme.clone() } else { None };
    let needs_theme = dynamic.is_some()
        || reactive.is_some()
        || base.has_tokens()
        || variants
            .iter()
            .any(|v| v.as_deref().is_some_and(Styles::has_tokens))
        || defaults.iter().any(Option::is_some);
    // Kept boxed, and only by a node with one: most have none, and inline
    // the three made every style effect a size class bigger.
    let defaults = defaults
        .iter()
        .any(Option::is_some)
        .then(|| Box::new(defaults));
    let mut theme = theme
        .filter(|_| needs_theme)
        .map(|scope| crate::theme::StyleTheme::new(scope, &base, &variants));
    // Whether this text node's effect subscribed to its scope's body text
    // style (`read_body`).
    let mut body_subscribed = false;
    // Whether the effect subscribed to the inherited typography for good: a
    // node restyled every frame then compares its version without tracking.
    let mut parent_subscribed = false;
    ui.bind(node, move || {
        let _span = crate::profile::nested(crate::profile::Phase::Style);
        // A `Ui` only for a full application: a run that changes paint or a
        // box alone needs just the scene and themes.
        let Some(storage) = weak.upgrade_storage() else {
            return;
        };
        if !storage.scene.borrow().contains(node) {
            return;
        }
        if std::mem::replace(&mut ran, true) && memo.is_none() {
            memo = Some(Box::new(Memo {
                applied: None,
                settled: None,
                shown: None,
                painted: false,
                boxed: None,
            }));
        }
        // Interaction styles that changed take their slots, their tokens
        // resolved below as those of a slot never resolved.
        let mut swapped = false;
        if let Some(next) = reactive.as_mut().and_then(|styles| styles()) {
            for (slot, (variant, next)) in variants.iter_mut().zip(next).enumerate() {
                if let Some(theme) = &mut theme {
                    if let Some(next) = &next {
                        theme.include(next);
                    }
                    theme.replaced(slot + 1);
                }
                *variant = next.map(Box::new);
            }
            swapped = true;
        }
        let mut produced = None;
        if let Some(dynamic) = dynamic.as_mut() {
            dynamic(&mut produced);
        }
        let state = interaction.as_ref().map(Signal::get).unwrap_or_default();
        let is_disabled = disabled.as_mut().is_some_and(|read| read());
        let in_effect = [
            !is_disabled && state.hover,
            !is_disabled
                && ((state.pointer_active && !state.pressed_outside) || state.key_active.is_some()),
            !is_disabled && state.focus && state.focus_visible,
            is_disabled,
        ];
        if let Some(defaults) = &defaults {
            for (i, default) in defaults.iter().enumerate() {
                if in_effect[i]
                    && variants[i].is_none()
                    && let Some(make) = default
                {
                    let variant = make();
                    if let Some(theme) = &mut theme {
                        theme.include(&variant);
                    }
                    variants[i] = Some(Box::new(variant));
                }
            }
        }
        let themed = theme
            .as_mut()
            .is_some_and(|theme| theme.refresh(&mut base, &mut variants, in_effect));
        if let (Some(theme), Some(produced)) = (&theme, &mut produced) {
            theme.resolve(produced);
        }
        let [hover, active, focus, disabled_variant] = &variants;
        let active_variants = [
            (in_effect[0], hover),
            (in_effect[1], active),
            (in_effect[2], focus),
            (in_effect[3], disabled_variant),
        ];
        // Animations re-run this every frame, mostly to change paint alone:
        // when nothing else differs from the last full application, update
        // just that, without resolving styles and typography again.
        if let Some(Memo {
            applied: Some(applied),
            settled: Some(last),
            shown,
            painted,
            boxed,
        }) = memo.as_deref_mut()
            && last.state == state
            && last.disabled == is_disabled
            && !themed
            && !swapped
        {
            let paint = produced.as_mut().map(Paint::take);
            // Unchanged, or a box that changes only its size or margins (a
            // bar growing): the same fields set, other values, no interaction
            // style over them, and the rest compared once.
            let (same, resized) = match (&produced, &last.dynamic) {
                // Paint alone (taken out above): nothing else to compare.
                (Some(now), Some(then)) if now.set_bits() | then.set_bits() == 0 => (true, false),
                (Some(now), Some(then)) => match now.eq_then_sizes(then) {
                    None => (false, false),
                    Some(sizes) => (
                        sizes,
                        !sizes
                            && shown.is_some()
                            && boxes(now) == boxes(then)
                            && active_variants.iter().all(|(enabled, variant)| {
                                !enabled
                                    || variant
                                        .as_deref()
                                        .is_none_or(|v| boxes(v) == (false, false, false))
                            }),
                    ),
                },
                (None, None) => (true, false),
                _ => (false, false),
            };
            let unchanged = glass.is_none_or(|opaque| {
                opaque == storage.themes.system_preferences().reduced_transparency
            }) && paint.map(|p| p.set()) == last.paint
                && (same || resized)
                && (!last.parent || inherited.version() == last.parent_version)
                && intrinsic.as_ref().map(|size| size.get()) == last.intrinsic
                && layout_target.as_ref().and_then(|target| target.get()) == last.content
                && last.text.as_ref().is_none_or(|(size, family)| {
                    let scope = scope.as_deref();
                    resolve_size(*size, scope) == Typography::size_value(applied.1.size)
                        && family.is_none_or(|token| {
                            family_is(token, scope, &applied.1.font.family, &mut body_subscribed)
                        })
                });
            if unchanged {
                if resized
                    && let (Some(now), Some(then), Some(style)) =
                        (&produced, &mut last.dynamic, shown.as_mut())
                {
                    // As `Styles::merge` lays them over the base.
                    use crate::style::bit;
                    let next = &mut applied.0;
                    if now.width.is_some() || now.width_percent.is_some() {
                        let fields = next.fields_mut(bit::WIDTH | bit::WIDTH_PERCENT);
                        fields.width = now.width;
                        fields.width_percent = now.width_percent;
                    }
                    if now.height.is_some() || now.height_percent.is_some() {
                        let fields = next.fields_mut(bit::HEIGHT | bit::HEIGHT_PERCENT);
                        fields.height = now.height;
                        fields.height_percent = now.height_percent;
                    }
                    if let Some(margin) = now.margin {
                        let mask = if now.margin_sides == 0 {
                            15
                        } else {
                            now.margin_sides
                        };
                        crate::style::merge_insets(
                            next.fields_mut(bit::MARGIN)
                                .margin
                                .get_or_insert_with(Default::default),
                            margin,
                            mask,
                        );
                    }
                    set_sizes(then, sizes(now));
                    patch_sizes(style, next);
                    storage.scene.borrow_mut().set_box(node, style, boxed);
                }
                // The paint written last time, from the same inputs.
                // Resized with no dynamic paint, as last time: nothing to write.
                let none = resized && paint.is_none_or(|paint| paint.set() == ([false; 11], 0));
                if std::mem::replace(painted, none) && none {
                    return;
                }
                let mut resolved = Paint::of(&base);
                if let Some(paint) = &paint {
                    resolved.lay(paint);
                }
                for (enabled, variant) in active_variants {
                    if enabled && let Some(variant) = variant {
                        resolved.lay(&Paint::of(variant));
                    }
                }
                let paint_color = resolved.text_color;
                let bound = theme.as_ref().is_some_and(|theme| theme.themed())
                    || produced.as_ref().is_some_and(Styles::has_tokens);
                resolved.put(&mut applied.0);
                let mut effects = originals.effects;
                if let Some(value) = resolved.opacity.get() {
                    effects.opacity = value;
                }
                if let Some(value) = resolved.blur.get() {
                    effects.blur_radius = value;
                }
                if let Some(value) = resolved.edge_fade.get() {
                    effects.edge_fade = value;
                }
                if let Some(value) = resolved.filter_blur.get() {
                    effects.filter_blur = value;
                }
                if let Some(value) = resolved.brightness.get() {
                    effects.brightness = value;
                }
                if let Some(value) = resolved.contrast.get() {
                    effects.contrast = value;
                }
                if let Some(value) = resolved.saturate.get() {
                    effects.saturate = value;
                }
                // The text colour matters to text, and to what inherits it:
                // the last patch that sets it decides it, a token if it
                // bound one (only patches with tokens need looking at).
                let mut text_color = None;
                if is_text || output.is_some() {
                    let parent_color = inherited.with_untracked(|parent| parent.color);
                    let color = match paint_color {
                        None => parent_color,
                        Some(value) if !bound => Themed::Value(value),
                        Some(value) => {
                            let mut color = themed_color(&base);
                            if let (Some(value), Some(produced)) =
                                (paint.and_then(|p| p.text_color), &produced)
                            {
                                color = Some(
                                    match token_of(produced, crate::theme::Field::TextColor) {
                                        Some(token) => Themed::Token(token),
                                        None => Themed::Value(value),
                                    },
                                );
                            }
                            for (enabled, variant) in active_variants {
                                if enabled
                                    && let Some(variant) = variant
                                    && let Some(own) = themed_color(variant)
                                {
                                    color = Some(own);
                                }
                            }
                            color.unwrap_or(Themed::Value(value))
                        }
                    };
                    if is_text {
                        let shown = resolve_color(color, scope.as_deref());
                        applied.1.color = Themed::Value(shown);
                        text_color = Some(shown);
                        if let Some(output) = &output {
                            set_color(output, color);
                        }
                    } else if applied.1.color != color {
                        applied.1.color = color;
                        if let Some(output) = &output {
                            set_color(output, color);
                        }
                    }
                }
                let mut scene = storage.scene.borrow_mut();
                let transform = resolved.transform.unwrap_or(originals.transform);
                let inset = applied.0.clip_inset.or(originals.inset);
                let local = resolved.local.resolve(originals.local);
                let (inset_kept, transform_kept, local_kept) =
                    scene.paint_kept(node, inset, transform, local);
                if last.moves_and_fades {
                    show_moved_and_faded(
                        &mut scene, node, transform, effects, &applied.0, &originals, inset_kept,
                    );
                } else {
                    scene.set_effects(node, effects);
                    apply_mask(&mut scene, node, &applied.0, &originals.mask);
                    if !inset_kept {
                        scene.set_clip_inset(node, inset);
                    }
                    if !transform_kept {
                        scene.set_transform(node, transform);
                    }
                }
                if !local_kept {
                    scene.set_local_transform(node, local);
                }
                if let Some(color) = text_color
                    && let NodeKind::Text {
                        text,
                        font_size,
                        color: old,
                    } = scene.kind(node)
                    && *old != color
                {
                    let kind = NodeKind::Text {
                        text: text.clone(),
                        color,
                        font_size: *font_size,
                    };
                    scene.set_kind(node, kind);
                }
                return;
            }
            if let (Some(produced), Some(paint)) = (&mut produced, paint) {
                paint.put(produced);
            }
        }
        let mut ui = Ui::from_storage(storage);
        let mut resolved = base.clone();
        if let Some(produced) = &produced {
            resolved.merge(produced);
        }
        for (enabled, variant) in active_variants {
            if enabled && let Some(variant) = variant {
                resolved.merge(variant);
            }
        }
        if disabled.is_some() && previous_disabled != Some(is_disabled) {
            previous_disabled = Some(is_disabled);
            ui.runtime
                .clone()
                .untracked(|| ui.set_disabled(node, is_disabled));
        }
        if let Some(output) = &object_fit {
            output.set(resolved.object_fit.unwrap_or_default());
        }
        // Only observe inherited typography when a field is actually inherited
        // (and from then on: a later run setting every field is rare).
        let tracks_parent = resolved.rare().text_overflow.is_none()
            || resolved.rare().line_clamp.is_none()
            || resolved.text_color.is_none()
            || resolved.text_size.is_none()
            || resolved.text_wrap.is_none()
            || resolved.font_family.is_none()
            || resolved.rare().font_features.is_none()
            || resolved.text_align.is_none()
            || resolved.rare().font_fallbacks.is_none()
            || resolved.font_weight.is_none()
            || resolved.italic.is_none()
            || resolved.line_height.is_none()
            || resolved.rare().letter_spacing.is_none()
            || resolved.rare().underline.is_none()
            || resolved.rare().underline_offset.is_none();
        let parent_version = inherited.version();
        if tracks_parent && !std::mem::replace(&mut parent_subscribed, true) {
            inherited.subscribe();
        }
        let parent = inherited.with_untracked(Clone::clone);
        // Nothing of its own (the usual dynamic style: sizes, colours): the
        // parent's typography, as it is.
        let current = if !resolved.sets_typography() {
            parent
        } else {
            let size = match (
                resolved.text_size.get(),
                token_of(&resolved, crate::theme::Field::TextSize),
            ) {
                (Some(_), Some(token)) => Themed::Token(token),
                (Some(size), None) => Themed::Value(size),
                (None, _) => parent.size,
            };
            let family = match (
                &resolved.font_family,
                token_of(&resolved, crate::theme::Field::FontFamily),
            ) {
                (Some(_), Some(token)) => (Default::default(), Some(token)),
                (Some(family), None) => (family.clone(), None),
                (None, _) => (parent.font.family.clone(), parent.family),
            };
            Typography {
                family: family.1,
                display: crate::text_layout::TextOptions {
                    overflow: resolved
                        .rare()
                        .text_overflow
                        .unwrap_or(parent.display.overflow),
                    line_clamp: resolved
                        .rare()
                        .line_clamp
                        .unwrap_or(parent.display.line_clamp),
                },
                color: themed_color(&resolved).unwrap_or(parent.color),
                size,
                wrap: resolved.text_wrap.unwrap_or(parent.wrap),
                font: crate::text_layout::FontStyle {
                    family: family.0,
                    features: resolved
                        .rare()
                        .font_features
                        .clone()
                        .unwrap_or_else(|| parent.font.features.clone()),
                    align: resolved.text_align.unwrap_or(parent.font.align),
                    fallbacks: resolved
                        .rare()
                        .font_fallbacks
                        .clone()
                        .unwrap_or_else(|| parent.font.fallbacks.clone()),
                    weight: resolved.font_weight.unwrap_or(parent.font.weight),
                    italic: resolved.italic.unwrap_or(parent.font.italic),
                    line_height: resolved.line_height.unwrap_or(parent.font.line_height),
                    letter_spacing: resolved
                        .rare()
                        .letter_spacing
                        .unwrap_or(parent.font.letter_spacing),
                    masked: false,
                    underline: {
                        let rare = resolved.rare();
                        let offset = rare
                            .underline_offset
                            .or_else(|| parent.font.underline.and_then(|u| u.offset()));
                        rare.underline
                            .unwrap_or(parent.font.underline.is_some())
                            .then(|| crate::text_layout::TextUnderline::new(offset))
                    },
                },
            }
        };
        // Compare before cloning: unchanged typography notifies nothing.
        if let Some(output) = &output
            && output.with_untracked(|old| *old != current)
        {
            output.set(current.clone());
        }
        // Read every remaining dependency before any early return so the
        // effect keeps its subscriptions.
        let intrinsic_size = intrinsic.as_ref().map(|size| size.get());
        let content_layout = layout_target.as_ref().and_then(|target| target.get());
        if let Some(memo) = &mut memo {
            let mut dynamic_rest = produced;
            let paint = dynamic_rest.as_mut().map(Paint::take);
            let moves_and_fades =
                paint.is_some_and(|p| p.transform.is_some() && p.opacity.is_some());
            let paint = paint.map(|p| p.set());
            memo.settled = Some(Settled {
                dynamic: dynamic_rest,
                paint,
                moves_and_fades,
                state,
                disabled: is_disabled,
                parent: tracks_parent,
                parent_version,
                intrinsic: intrinsic_size,
                content: content_layout,
                text: is_text.then_some((current.size, current.family)),
            });
        }
        // Text shows its colour, size and family resolved in its scope (a
        // token's change re-runs it); other nodes do not show typography.
        let current = if is_text {
            current.shown(scope.as_deref(), &mut body_subscribed)
        } else {
            current
        };
        let opaque = opaque_glass(&ui, &mut resolved, &originals.kind);
        glass = opaque;
        let inputs = (resolved, current, intrinsic_size, content_layout);
        if memo
            .as_ref()
            .is_some_and(|memo| memo.applied.as_ref() == Some(&inputs))
        {
            return;
        }
        // Animations mostly change paint effects or the transform, which need
        // neither layout nor a new paint kind, or a box's size and margins,
        // which need only those in its layout style: update just those.
        if let Some(Memo {
            applied: Some(previous),
            shown,
            painted,
            boxed,
            ..
        }) = memo.as_deref_mut()
            && previous.1 == inputs.1
            && previous.2 == inputs.2
            && previous.3 == inputs.3
            && (boxes(&previous.0) == boxes(&inputs.0) && shown.is_some()
                || (
                    previous.0.width,
                    previous.0.width_percent,
                    previous.0.height,
                ) == (inputs.0.width, inputs.0.width_percent, inputs.0.height)
                    && (previous.0.height_percent, previous.0.margin)
                        == (inputs.0.height_percent, inputs.0.margin))
        {
            let sizes = (
                previous.0.width,
                previous.0.width_percent,
                previous.0.height,
                previous.0.height_percent,
                previous.0.margin,
            );
            let next = &inputs.0;
            let fields = previous.0.fields_mut(
                crate::style::bit::WIDTH
                    | crate::style::bit::WIDTH_PERCENT
                    | crate::style::bit::HEIGHT
                    | crate::style::bit::HEIGHT_PERCENT
                    | crate::style::bit::MARGIN,
            );
            (
                fields.width,
                fields.width_percent,
                fields.height,
                fields.height_percent,
                fields.margin,
            ) = (
                next.width,
                next.width_percent,
                next.height,
                next.height_percent,
                next.margin,
            );
            let before = (
                previous.0.opacity,
                previous.0.blur,
                previous.0.edge_fade,
                previous.0.filter_blur,
                previous.0.brightness,
                previous.0.contrast,
                previous.0.saturate,
                previous.0.mask_offset,
                previous.0.clip_inset,
                previous.0.transform,
                previous.0.local_transform,
            );
            let next = &inputs.0;
            let fields = previous.0.fields_mut(Paint::BITS);
            (
                fields.opacity,
                fields.blur,
                fields.edge_fade,
                fields.filter_blur,
                fields.brightness,
                fields.contrast,
                fields.saturate,
                fields.mask_offset,
                fields.clip_inset,
                fields.transform,
                fields.local_transform,
            ) = (
                next.opacity,
                next.blur,
                next.edge_fade,
                next.filter_blur,
                next.brightness,
                next.contrast,
                next.saturate,
                next.mask_offset,
                next.clip_inset,
                next.transform,
                next.local_transform,
            );
            if previous.0 == *next {
                // Written here from the resolved styles, not a dynamic paint.
                *painted = false;
                let resized = sizes
                    != (
                        next.width,
                        next.width_percent,
                        next.height,
                        next.height_percent,
                        next.margin,
                    );
                let mut scene = ui.scene.borrow_mut();
                if resized && let Some(style) = shown {
                    patch_sizes(style, next);
                    *boxed = None;
                    scene.set_style(node, style.clone());
                }
                let mut effects = originals.effects;
                if let Some(value) = next.opacity.get() {
                    effects.opacity = value;
                }
                if let Some(value) = next.blur.get() {
                    effects.blur_radius = value;
                }
                if let Some(value) = next.edge_fade.get() {
                    effects.edge_fade = value;
                }
                if let Some(value) = next.filter_blur.get() {
                    effects.filter_blur = value;
                }
                if let Some(value) = next.brightness.get() {
                    effects.brightness = value;
                }
                if let Some(value) = next.contrast.get() {
                    effects.contrast = value;
                }
                if let Some(value) = next.saturate.get() {
                    effects.saturate = value;
                }
                scene.set_effects(node, effects);
                apply_mask(&mut scene, node, next, &originals.mask);
                scene.set_clip_inset(node, next.clip_inset.or(originals.inset));
                scene.set_transform(node, next.transform.unwrap_or(originals.transform));
                scene.set_local_transform(node, next.local_transform.resolve(originals.local));
                return;
            }
            let fields = previous.0.fields_mut(
                crate::style::bit::WIDTH
                    | crate::style::bit::WIDTH_PERCENT
                    | crate::style::bit::HEIGHT
                    | crate::style::bit::HEIGHT_PERCENT
                    | crate::style::bit::MARGIN,
            );
            (
                fields.width,
                fields.width_percent,
                fields.height,
                fields.height_percent,
                fields.margin,
            ) = sizes;
            let fields = previous.0.fields_mut(Paint::BITS);
            (
                fields.opacity,
                fields.blur,
                fields.edge_fade,
                fields.filter_blur,
                fields.brightness,
                fields.contrast,
                fields.saturate,
                fields.mask_offset,
                fields.clip_inset,
                fields.transform,
                fields.local_transform,
            ) = before;
        }
        let mut first: Option<AppliedInputs> = None;
        let (resolved, current, _, _) = match &mut memo {
            Some(memo) => &*memo.applied.insert(inputs),
            None => &*first.insert(inputs),
        };
        let style = apply_resolved(
            &ui,
            node,
            &originals,
            resolved,
            current,
            intrinsic_size,
            content_layout,
            layout_target.is_some(),
        );
        if let Some(memo) = &mut memo {
            memo.shown = Some(style);
            memo.painted = false;
            memo.boxed = None;
        }
    });
    typography
}

/// What a node showed before its style first applied: what properties no
/// style sets fall back to.
struct Originals {
    /// `None` for the default style, which nearly every node has before it
    /// is styled: inline, it made each node's style effect 168 bytes bigger.
    style: Option<Box<crate::scene::Style>>,
    effects: crate::scene::Effects,
    transform: crate::scene::Transform,
    local: crate::scene::LocalTransform,
    isolated: bool,
    mask: Option<crate::scene::ImageMask>,
    inset: Option<crate::scene::ClipInset>,
    kind: NodeKind,
}
/// Write a fully resolved style to the scene: the style effect's slow path
/// (layout, kind and paint), out of line so that the paint-only path every
/// animation frame takes stays small.
#[inline(never)]
/// With the system's transparency reduced, frosted glass paints opaque: no
/// backdrop blur, and its background solid (a clear one the theme's raised
/// surface). Only a node with a backdrop blur reads (and follows) the
/// setting; as a change of its styles, it restyles as any other.
/// Makes `resolved` the styles to apply; for frosted glass, returns whether
/// it is opaque. (In place: a 440-byte style moved in and out again was
/// copied twice by every full application.)
fn opaque_glass(ui: &Ui, resolved: &mut Styles, kind: &NodeKind) -> Option<bool> {
    if !resolved.blur.get().is_some_and(|sigma| sigma > 0.) {
        return None;
    }
    if !ui.theme().system_preferences().reduced_transparency {
        return Some(false);
    }
    let fill = resolved.background.unwrap_or(match kind {
        NodeKind::Panel { quad, .. } => quad.fill,
        _ => Color(0, 0, 0, 0),
    });
    let background = Some(if fill.3 == 0 {
        ui.theme().root().color(tok::SURFACE_RAISED)
    } else {
        Color(fill.0, fill.1, fill.2, 255)
    });
    let fields = resolved.fields_mut(crate::style::bit::BACKGROUND | crate::style::bit::BLUR);
    fields.background = background;
    fields.blur = Some(0.).into();
    Some(true)
}
#[allow(clippy::too_many_arguments)]
fn apply_resolved(
    ui: &Ui,
    node: NodeId,
    originals: &Originals,
    resolved: &Styles,
    current: &Typography,
    intrinsic_size: Option<(f32, f32)>,
    content_layout: Option<NodeId>,
    layout_target: bool,
) -> crate::scene::Style {
    let mut style = originals.style.as_deref().cloned().unwrap_or_default();
    if let Some(resolved_options) = &resolved.layout_options {
        if let Some(original) = &style.layout_options {
            let mut options = **original;
            options.merge(**resolved_options);
            if options != **original {
                style.layout_options = Some(std::sync::Arc::new(options));
            }
        } else {
            style.layout_options = Some(resolved_options.clone());
        }
    }
    macro_rules! field { ($($field:ident),* $(,)?) => { $(if let Some(value) = Option::from(resolved.$field) { style.$field = value; })* }; }
    let rare = resolved.rare();
    macro_rules! dimension { ($($field:ident),* $(,)?) => { $(if let Some(value) = rare.$field { style.$field = Some(value); })* }; }
    dimension!(min_width, max_width, min_height, max_height);
    if resolved.width.is_some() || resolved.width_percent.is_some() {
        style.width = resolved.width.get();
        style.width_percent = resolved.width_percent.get();
    }
    if resolved.height.is_some() || resolved.height_percent.is_some() {
        style.height = resolved.height.get();
        style.height_percent = resolved.height_percent.get();
    }
    field!(
        absolute,
        gap,
        clip,
        flex_grow,
        flex_shrink,
        align,
        justify,
        text_wrap,
        margin
    );
    // CSS `position`: relative and absolute exclude each other.
    if let Some(value) = rare.relative {
        style.relative = value;
    }
    if style.absolute {
        style.relative = false;
    }
    // A size set here is a constraint, not the default it replaces.
    if resolved.width.is_some() || resolved.width_percent.is_some() {
        style.intrinsic_width = false;
    }
    if resolved.height.is_some() || resolved.height_percent.is_some() {
        style.intrinsic_height = false;
    }
    if let Some(value) = rare.fade_edges {
        style.fade_edges = value;
    }
    if let Some(value) = rare.fade_insets {
        style.fade_insets = value;
    }
    if let Some(value) = rare.fade_edges_x {
        style.fade_edges_x = value;
    }
    if let Some(value) = rare.fade_insets_x {
        style.fade_insets_x = value;
    }
    if let Some(value) = rare.fade_overflow {
        style.fade_overflow = value;
    }
    if let Some(value) = rare.snap {
        style.snap = value;
    }
    if matches!(
        originals.kind,
        NodeKind::Text { .. } | NodeKind::RichText { .. }
    ) {
        style.text_wrap = current.wrap;
        style.text_options = current.display;
    }
    if let Some(padding) = resolved.padding {
        style.padding_edges = Some(padding);
    }
    if let Some((width, height)) = intrinsic_size {
        let padding = style
            .padding_edges
            .unwrap_or(crate::scene::Insets::all(style.padding));
        if resolved.width.is_none() && resolved.width_percent.is_none() {
            style.width = Some(width + padding.left.max(0.) + padding.right.max(0.));
        }
        if resolved.height.is_none() && resolved.height_percent.is_none() {
            style.height = Some(height + padding.top.max(0.) + padding.bottom.max(0.));
        }
    }

    let mut content_options = crate::layout::LayoutOptions::default();
    if layout_target {
        style.gap = 0.;
        style.align = crate::scene::Align::Stretch;
        style.justify = crate::scene::Justify::Start;
        if let Some(options) = &mut style.layout_options {
            let options = std::sync::Arc::make_mut(options);
            if options.display != Some(crate::layout::Display::None) {
                content_options.display = options.display.take();
            }
            content_options.wrap = options.wrap.take();
            content_options.reverse = options.reverse.take();
            content_options.align_content = options.align_content.take();
            content_options.gap_x = options.gap_x.take();
            content_options.gap_y = options.gap_y.take();
            content_options.columns = options.columns.take();
            content_options.rows = options.rows.take();
        }
    }
    let mut effects = originals.effects;
    if let Some(value) = resolved.opacity.get() {
        effects.opacity = value;
    }
    if let Some(value) = resolved.blur.get() {
        effects.blur_radius = value;
    }
    if let Some(value) = resolved.edge_fade.get() {
        effects.edge_fade = value;
    }
    if let Some(value) = resolved.filter_blur.get() {
        effects.filter_blur = value;
    }
    if let Some(value) = resolved.brightness.get() {
        effects.brightness = value;
    }
    if let Some(value) = resolved.contrast.get() {
        effects.contrast = value;
    }
    if let Some(value) = resolved.saturate.get() {
        effects.saturate = value;
    }
    let mut scene = ui.scene.borrow_mut();
    scene.set_style(node, style.clone());
    if let Some(direction) = resolved.rare().direction {
        scene.set_direction(node, Some(direction));
    }
    if let Some(content) = content_layout {
        let mut content_style = scene.style(content);
        content_style.gap = resolved.gap.get().unwrap_or(0.);
        content_style.align = resolved.align.unwrap_or(crate::scene::Align::Stretch);
        content_style.justify = resolved.justify.unwrap_or(crate::scene::Justify::Start);
        content_style.layout_options =
            (!content_options.is_empty()).then(|| std::sync::Arc::new(content_options));
        scene.set_style(content, content_style);
        if let Some(layout) = resolved.layout {
            scene.set_kind(content, NodeKind::Container(layout));
        }
    }

    scene.set_cursor(node, resolved.cursor);
    scene.set_effects(node, effects);
    apply_mask(&mut scene, node, resolved, &originals.mask);
    scene.set_clip_inset(node, resolved.clip_inset.or(originals.inset));
    scene.set_transform(node, resolved.transform.unwrap_or(originals.transform));
    scene.set_local_transform(node, resolved.local_transform.resolve(originals.local));
    scene.set_isolated(node, resolved.isolated.unwrap_or(originals.isolated));
    let kind = match &originals.kind {
        NodeKind::Container(layout) | NodeKind::Panel { layout, .. } => {
            let mut quad = match &originals.kind {
                NodeKind::Panel { quad, .. } => quad.clone(),
                _ => QuadStyle::default(),
            };
            if let Some(value) = resolved.background {
                quad.fill = value;
            }
            if let Some(value) = resolved.radius.get() {
                quad.radius = value;
            }
            if let Some(value) = resolved.border_color {
                quad.border_color = value;
            }
            if let Some(value) = resolved.border_width.get() {
                quad.border_width = value;
            }
            if let Some(value) = resolved.rare().shadow {
                quad.shadow = value;
            }
            if let Some(value) = resolved.rare().border_over {
                quad.border_over = value;
            }
            if let Some(value) = resolved.rare().outline {
                quad.outline = value.map(|(width, color)| crate::scene::Outline {
                    width,
                    color,
                    offset: 0.,
                });
            }
            if let (Some(offset), Some(outline)) =
                (resolved.rare().outline_offset, &mut quad.outline)
            {
                outline.offset = offset;
            }
            if quad.decoration.is_some()
                || resolved.paint_background.is_some()
                || resolved.corners.is_some()
                || resolved.border_edges.is_some()
                || resolved.rare().border_style.is_some()
                || resolved.rare().shadows.is_some()
                || resolved.rare().inset_shadows.is_some()
                || resolved.rare().clip_feather.is_some()
            {
                let mut detail = quad.decoration.as_deref().cloned().unwrap_or_default();
                if let Some(value) = &resolved.paint_background {
                    detail.background = value.clone();
                }
                if let Some(value) = resolved.corners {
                    detail.corners = value;
                }
                if let Some(value) = resolved.border_edges {
                    detail.border_widths = value;
                }
                if let Some(value) = resolved.rare().border_style {
                    detail.border_style = value;
                }
                if let Some(value) = &resolved.rare().shadows {
                    detail.shadows = value.clone();
                }
                if let Some(value) = &resolved.rare().inset_shadows {
                    detail.inset_shadows = value.clone();
                }
                if let Some(value) = resolved.rare().clip_feather {
                    detail.clip_feather = value;
                }
                quad.decoration = (detail != crate::decoration::Decoration::default())
                    .then(|| std::sync::Arc::new(detail));
            }
            if quad == QuadStyle::default() {
                NodeKind::Container(if layout_target {
                    *layout
                } else {
                    resolved.layout.unwrap_or(*layout)
                })
            } else {
                NodeKind::Panel {
                    layout: if layout_target {
                        *layout
                    } else {
                        resolved.layout.unwrap_or(*layout)
                    },
                    quad,
                }
            }
        }
        NodeKind::Text { .. } => match scene.kind(node) {
            NodeKind::Text { text, .. } => NodeKind::Text {
                text: text.clone(),
                color: resolve_color(current.color, None),
                font_size: Typography::size_value(current.size),
            },
            kind => kind.clone(),
        },
        _ => scene.kind(node).clone(),
    };
    if matches!(kind, NodeKind::Text { .. }) {
        scene.set_font(node, current.font.clone());
    }
    scene.set_kind(node, kind);
    if let Some(uniforms) = resolved.rare().shader_uniforms {
        scene.set_shader_uniforms(node, uniforms);
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Layout, Style};
    use crate::style::Styled;
    fn color(value: u8) -> Styles {
        Styles::from(crate::style::StylesFields {
            background: Some(Color(value, 0, 0, 255)),
            ..Default::default()
        })
    }
    fn fill(ui: &Ui, node: NodeId) -> Color {
        match ui.scene.borrow().kind(node) {
            NodeKind::Panel { quad, .. } => quad.fill,
            _ => panic!("expected panel"),
        }
    }
    fn inherited(ui: &Ui) -> Signal<Typography> {
        ui.signal(Typography {
            display: Default::default(),
            color: Themed::Value(Color(255, 255, 255, 255)),
            size: Themed::Value(16.),
            wrap: false,
            font: Default::default(),
            family: None,
        })
    }
    #[test]
    fn interaction_variants_restore_and_disabled_wins_without_layout() {
        let mut ui = Ui::new(100., 100.);
        let node = ui.container(
            ui.root(),
            Layout::Column,
            Style {
                width: Some(50.),
                height: Some(50.),
                ..Default::default()
            },
        );
        let disabled = ui.signal(false);
        let read_disabled = disabled.clone();
        let inherited = inherited(&ui);
        mount_style(
            &mut ui,
            node,
            color(1),
            inherited,
            None,
            Variants {
                hover: Some(Box::new(color(2))),
                active: Some(Box::new(color(3))),
                // An inset stroke: a focus ring that takes no layout room.
                focus: Some(Box::new(Styles::new().border(2.).border_over(true))),
                disabled: Some(Box::new(color(4))),
                defaults: Default::default(),
                reactive: None,
            },
            Some(Box::new(move || read_disabled.get())),
            true,
        );
        ui.scene.borrow_mut().flush();
        ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
        assert_eq!(fill(&ui, node), Color(2, 0, 0, 255));
        ui.dispatch(InputEvent::PointerDown {
            x: 10.,
            y: 10.,
            button: PointerButton::Primary,
        });
        assert_eq!(fill(&ui, node), Color(3, 0, 0, 255));
        assert_eq!(ui.input.focused(), Some(node));
        // A press's focus shows no ring (`:focus-visible`); a key to it does.
        assert!(
            matches!(ui.scene.borrow().kind(node), NodeKind::Panel { quad, .. } if quad.border_width == 0.)
        );
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Character("x".into()),
            modifiers: Default::default(),
            repeat: false,
        });
        assert!(
            matches!(ui.scene.borrow().kind(node), NodeKind::Panel { quad, .. } if quad.border_width == 2.)
        );
        assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
        disabled.set(true);
        assert_eq!(fill(&ui, node), Color(4, 0, 0, 255));
        assert_eq!(ui.input.focused(), None);
        assert!(
            matches!(ui.scene.borrow().kind(node), NodeKind::Panel { quad, .. } if quad.border_width == 0.)
        );
        assert!(ui.semantics.borrow().get(node).unwrap().disabled);
        disabled.set(false);
        assert_eq!(fill(&ui, node), Color(2, 0, 0, 255));
        ui.dispatch(InputEvent::PointerLeave);
        assert_eq!(fill(&ui, node), Color(1, 0, 0, 255));
    }
    #[test]
    fn typography_inherits_and_explicit_overrides_are_stable() {
        let mut ui = Ui::new(100., 100.);
        let parent = inherited(&ui);
        let text = ui.label(ui.root(), "retained", Style::default());
        let output = mount_style(
            &mut ui,
            text,
            Styles::default(),
            parent.clone(),
            None,
            Variants::default(),
            None,
            false,
        );
        let explicit = ui.label(ui.root(), "fixed", Style::default());
        let fixed = Typography {
            display: Default::default(),
            color: Themed::Value(Color(12, 34, 56, 255)),
            size: Themed::Value(20.),
            wrap: false,
            font: Default::default(),
            family: None,
        };
        let stable = mount_style(
            &mut ui,
            explicit,
            Styles::from(crate::style::StylesFields {
                text_color: Some(Color(12, 34, 56, 255)),
                text_size: Some(20.).into(),
                ..Default::default()
            }),
            parent.clone(),
            None,
            Variants::default(),
            None,
            false,
        );
        ui.scene.borrow_mut().flush();
        parent.set(Typography {
            display: Default::default(),
            color: Themed::Value(Color(1, 2, 3, 255)),
            size: Themed::Value(16.),
            wrap: false,
            font: Default::default(),
            family: None,
        });
        assert_eq!(output.get().color, Themed::Value(Color(1, 2, 3, 255)));
        assert_eq!(stable.get(), fixed);
        assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
        parent.set(Typography {
            display: Default::default(),
            color: Themed::Value(Color(1, 2, 3, 255)),
            size: Themed::Value(24.),
            wrap: false,
            font: Default::default(),
            family: None,
        });
        assert_eq!(output.get().size, Themed::Value(24.));
        assert_eq!(stable.get(), fixed);
        assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
    }
    /// A text restyled every frame takes the paint-only path, which reads
    /// no inherited typography: its parent's still reaches it.
    #[test]
    fn text_restyled_every_frame_follows_its_parents_typography() {
        let mut ui = Ui::new(100., 100.);
        let parent = inherited(&ui);
        let text = ui.label(ui.root(), "faded", Style::default());
        let fade = ui.signal(1_f32);
        let read = fade.clone();
        mount_style(
            &mut ui,
            text,
            Styles::default(),
            parent.clone(),
            Some(Box::new(move || Styles::new().opacity(read.get()))),
            Variants::default(),
            None,
            false,
        );
        for value in [0.9, 0.8, 0.7] {
            fade.set(value);
            ui.scene.borrow_mut().flush();
        }
        parent.update(|typography| typography.color = Themed::Value(Color(1, 2, 3, 255)));
        let color = |ui: &Ui| match ui.scene.borrow().kind(text) {
            NodeKind::Text { color, .. } => *color,
            _ => panic!("expected text"),
        };
        assert_eq!(color(&ui), Color(1, 2, 3, 255));
        fade.set(0.6);
        parent.update(|typography| typography.color = Themed::Value(Color(4, 5, 6, 255)));
        assert_eq!(color(&ui), Color(4, 5, 6, 255));
        assert_eq!(ui.scene.borrow().effects(text).opacity, 0.6);
    }
    /// Taking paint out of a style takes what it sets and leaves it
    /// unset, the rest of it as it was; put back, it is the same style.
    #[test]
    fn paint_taken_from_a_style_and_put_back_is_the_same_style() {
        let made = || {
            Styles::new()
                .w(10.)
                .opacity(0.4)
                .translate(3., 4.)
                .text_color(Color(1, 2, 3, 255))
        };
        let mut styles = made();
        let paint = Paint::take(&mut styles);
        assert_eq!(
            (paint.opacity.get(), paint.text_color, paint.blur.get()),
            (Some(0.4), Some(Color(1, 2, 3, 255)), None)
        );
        assert!(styles == Styles::new().w(10.));
        paint.put(&mut styles);
        assert!(styles == made());
        assert!(Paint::take(&mut Styles::new().w(1.)) == Paint::default());
    }
    /// A paint-only run writes the node's whole paint again: what else set
    /// its translation, clip or local transform since is undone, whether
    /// the style fades it alone or moves and fades it (written together).
    #[test]
    fn a_paint_only_run_writes_back_what_else_changed() {
        let mut ui = Ui::new(200., 200.);
        let value = ui.signal(0_f32);
        let styled = |ui: &mut Ui, moves: bool| {
            let node = ui.container(ui.root(), Layout::Column, Style::default());
            let inherited = inherited(ui);
            let value = value.clone();
            mount_style(
                ui,
                node,
                color(1).translate(5., 6.),
                inherited,
                Some(Box::new(move || {
                    let v = value.get();
                    let styles = Styles::new().opacity(1. - v / 100.);
                    if moves {
                        styles.translate(v, 2. * v)
                    } else {
                        styles
                    }
                })),
                Variants::default(),
                None,
                false,
            );
            node
        };
        let (faded, moved) = (styled(&mut ui, false), styled(&mut ui, true));
        for v in [10., 20.] {
            value.set(v);
            ui.scene.borrow_mut().flush();
        }
        for node in [faded, moved] {
            let mut scene = ui.scene.borrow_mut();
            scene.set_transform(node, crate::scene::Transform { x: 50., y: 60. });
            scene.set_clip_inset(node, Some(crate::scene::ClipInset::px(1., 2., 3., 4.)));
        }
        value.set(30.);
        let scene = ui.scene.borrow();
        let transform = |node| {
            let t = scene.transform(node);
            (t.x, t.y)
        };
        assert_eq!(transform(faded), (5., 6.));
        assert_eq!(transform(moved), (30., 60.));
        for node in [faded, moved] {
            assert_eq!(scene.clip_inset(node), None);
            assert_eq!(scene.effects(node).opacity, 0.7);
        }
    }
    /// Runs that only resize (sizes and margins, fading too) patch the last
    /// layout style instead of applying everything again: the node must end
    /// as a node styled once with the last values, the base's other margin
    /// sides kept.
    #[test]
    fn a_box_that_only_resizes_ends_as_one_styled_once() {
        let mut ui = Ui::new(200., 200.);
        let styled = |ui: &mut Ui, value: Signal<f32>| {
            let node = ui.container(ui.root(), Layout::Column, Style::default());
            let inherited = inherited(ui);
            mount_style(
                ui,
                node,
                color(1).ml(5.).mb(3.),
                inherited,
                Some(Box::new(move || {
                    let v = value.get();
                    Styles::new().w(v + 1.).h(v * 2.).mt(v).opacity(v / 100.)
                })),
                Variants::default(),
                None,
                false,
            );
            node
        };
        let value = ui.signal(10_f32);
        let resized = styled(&mut ui, value.clone());
        for v in [12., 30., 7., 41.] {
            value.set(v);
            ui.scene.borrow_mut().flush();
        }
        let last = ui.signal(41_f32);
        let once = styled(&mut ui, last);
        let scene = ui.scene.borrow();
        assert_eq!(scene.style(resized), scene.style(once));
        assert_eq!(scene.kind(resized), scene.kind(once));
        assert_eq!(scene.effects(resized), scene.effects(once));
        let style = scene.style(resized);
        assert_eq!((style.width, style.height), (Some(42.), Some(82.)));
        assert_eq!(
            (style.margin.top, style.margin.left, style.margin.bottom),
            (41., 5., 3.)
        );
    }
    #[test]
    fn removing_dynamic_properties_restores_base_and_inheritance() {
        let mut ui = Ui::new(100., 100.);
        let node = ui.container(ui.root(), Layout::Row, Style::default());
        let toggled = ui.signal(true);
        let read = toggled.clone();
        let parent = inherited(&ui);
        let output = mount_style(
            &mut ui,
            node,
            color(1),
            parent.clone(),
            Some(Box::new(move || {
                if read.get() {
                    Styles::from(crate::style::StylesFields {
                        background: Some(Color(9, 0, 0, 255)),
                        text_size: Some(40.).into(),
                        opacity: Some(0.5).into(),
                        ..Default::default()
                    })
                } else {
                    Styles::default()
                }
            })),
            Variants::default(),
            None,
            false,
        );
        assert_eq!(output.get().size, Themed::Value(40.));
        assert_eq!(fill(&ui, node), Color(9, 0, 0, 255));
        toggled.set(false);
        assert_eq!(output.get().size, Themed::Value(16.));
        assert_eq!(fill(&ui, node), Color(1, 0, 0, 255));
        assert_eq!(ui.scene.borrow().effects(node).opacity, 1.);
        parent.set(Typography {
            display: Default::default(),
            size: Themed::Value(20.),
            ..parent.get()
        });
        assert_eq!(output.get().size, Themed::Value(20.));
    }
    #[test]
    fn styled_noninteractive_child_preserves_button_activation() {
        use crate::{
            compose::{button, text},
            style::Styled,
        };
        let mut ui = Ui::new(120., 80.);
        let clicks = ui.signal(0);
        let write = clicks.clone();
        ui.mount(
            button()
                .size(100., 60.)
                .child(text("Child").hover(|s| s.text_color(Color(255, 0, 0, 255))))
                .on_click(move || {
                    write.update(|count| *count += 1);
                }),
        );
        ui.dispatch(InputEvent::PointerMove { x: 15., y: 15. });
        ui.dispatch(InputEvent::PointerDown {
            x: 15.,
            y: 15.,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x: 15.,
            y: 15.,
            button: PointerButton::Primary,
        });
        assert_eq!(clicks.get(), 1);
    }
    #[test]
    fn nested_button_activation_stops_at_nearest_clickable_owner() {
        use crate::{
            compose::{button, text},
            style::Styled,
        };
        let mut ui = Ui::new(140., 100.);
        let outer = ui.signal(0);
        let inner = ui.signal(0);
        let outer_write = outer.clone();
        let inner_write = inner.clone();
        let mounted = ui.mount(
            button()
                .size(120., 80.)
                .p(0.)
                .on_click(move || {
                    outer_write.update(|n| *n += 1);
                })
                .child(
                    button()
                        .id("inner")
                        .size(80., 40.)
                        .on_click(move || {
                            inner_write.update(|n| *n += 1);
                        })
                        .child(text("Inner").hover(|s| s.opacity(0.8))),
                ),
        );
        ui.scene.borrow_mut().prepare_layout();
        let bounds = ui.scene.borrow().bounds(mounted.find("inner").unwrap());
        let x = bounds.x + 15.;
        let y = bounds.y + 15.;
        ui.dispatch(InputEvent::PointerMove { x, y });
        ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
        assert_eq!(inner.get(), 1);
        assert_eq!(outer.get(), 0);
    }
    #[test]
    fn unchanged_label_is_not_reshaped_beside_an_animating_sibling() {
        use crate::{
            compose::{column, div, row, text},
            style::Styled,
        };
        use std::{cell::Cell, rc::Rc};
        let mut ui = Ui::new(600., 400.);
        let calls = Rc::new(Cell::new(0_u32));
        let counter = calls.clone();
        ui.scene
            .borrow_mut()
            .set_text_measurer(move |text: &str, _: f32, _: Option<f32>| {
                if text == "MATRIX" {
                    counter.set(counter.get() + 1);
                }
                (text.len() as f32 * 10., 20.)
            });
        let side = ui.signal(10_f32);
        let read = side.clone();
        // A caption and a pulsing cell in a growing card, as in a loader tile.
        let stage = |label: &str, cell: crate::compose::View| {
            column()
                .grow()
                .min_w(0.)
                .h(150.)
                .p(16.)
                .gap(10.)
                .child(text(label))
                .child(
                    column()
                        .w_full()
                        .grow()
                        .items_center()
                        .justify_center()
                        .child(cell),
                )
        };
        ui.mount(
            column().w_full().gap(20.).child(text("Motion")).child(
                row()
                    .w_full()
                    .gap(12.)
                    .child(stage("MATRIX", div().size(30., 30.)))
                    .child(stage(
                        "WAVE",
                        div().reactive_style(move || Styles::new().size(read.get(), read.get())),
                    )),
            ),
        );
        ui.scene.borrow_mut().flush();
        let settled = calls.get();
        for step in 0..8 {
            side.set(10. + step as f32);
            ui.scene.borrow_mut().flush();
        }
        assert_eq!(
            calls.get(),
            settled,
            "MATRIX was reshaped {} times",
            calls.get() - settled
        );
    }
    #[test]
    fn animated_text_colour_repaints_without_layout() {
        use crate::{compose::text, style::Styled};
        let mut ui = Ui::new(300., 200.);
        ui.scene
            .borrow_mut()
            .set_text_measurer(|text: &str, size: f32, _: Option<f32>| {
                (text.len() as f32 * size, size)
            });
        let (alpha, size) = (ui.signal(90_u8), ui.signal(10_f32));
        let (read_alpha, read_size) = (alpha.clone(), size.clone());
        let view = ui.mount(text("shimmer").reactive_style(move || {
            Styles::new()
                .text_color(Color(255, 255, 255, read_alpha.get()))
                .text_size(read_size.get())
        }));
        ui.scene.borrow_mut().flush();
        // The text node, under whatever the view mounted around it.
        let mut node = view.node();
        while !matches!(ui.scene.borrow().kind(node), NodeKind::Text { .. }) {
            node = ui.scene.borrow().children(node)[0];
        }
        let shown = |ui: &Ui| match ui.scene.borrow().kind(node) {
            NodeKind::Text {
                text,
                color,
                font_size,
            } => (text.to_string(), *color, *font_size),
            kind => panic!("expected text, found {kind:?}"),
        };
        let style = ui.scene.borrow().style(node);
        alpha.set(200);
        let report = ui.scene.borrow_mut().flush();
        assert_eq!(
            shown(&ui),
            ("shimmer".into(), Color(255, 255, 255, 200), 10.)
        );
        assert_eq!(report.layout_nodes, 0);
        assert!(!report.damage.is_empty());
        assert_eq!(ui.scene.borrow().style(node), style);
        // A size change still lays out, and colour changes keep applying after it.
        size.set(20.);
        let report = ui.scene.borrow_mut().flush();
        assert!(report.layout_nodes > 0);
        assert_eq!(ui.scene.borrow().bounds(node).width, 140.);
        alpha.set(90);
        ui.scene.borrow_mut().flush();
        assert_eq!(
            shown(&ui),
            ("shimmer".into(), Color(255, 255, 255, 90), 20.)
        );
    }
    #[test]
    fn boxed_text_reactive_wrapping_reflows_without_remounting() {
        use crate::{compose::text, style::Styled};
        let mut ui = Ui::new(300., 200.);
        ui.scene
            .borrow_mut()
            .set_text_measurer(|text: &str, _: f32, limit: Option<f32>| {
                let natural = text.len() as f32 * 10.;
                let width = limit.unwrap_or(natural).min(natural).max(1.);
                (width, (natural / width).ceil() * 20.)
            });
        let wrap = ui.signal(false);
        let read = wrap.clone();
        let view = ui.mount(
            text("abcdefghijabcdefghij")
                .id("box")
                .w(70.)
                .p(10.)
                .bg(Color(1, 2, 3, 255))
                .text_wrap(false)
                .reactive_style(move || Styles::new().text_wrap(read.get())),
        );
        ui.scene.borrow_mut().flush();
        let outer = view.node();
        let inner = ui.scene.borrow().children(outer)[0];
        assert_eq!(ui.scene.borrow().bounds(outer).height, 40.);
        wrap.set(true);
        let report = ui.scene.borrow_mut().flush();
        assert!(report.layout_nodes > 0);
        assert_eq!(ui.scene.borrow().bounds(outer).height, 100.);
        assert_eq!(ui.scene.borrow().bounds(inner).width, 50.);
        assert_eq!(ui.scene.borrow().children(outer), &[inner]);
        assert_eq!(view.find("box"), Some(outer));
        wrap.set(false);
        ui.scene.borrow_mut().flush();
        assert_eq!(ui.scene.borrow().bounds(outer).height, 40.);
        assert_eq!(ui.scene.borrow().children(outer), &[inner]);
    }
    #[test]
    fn font_properties_inherit_override_and_update_without_remount() {
        use crate::{
            compose::{column, text},
            style::Styled,
            text_layout::FontFamily,
        };
        let mut ui = Ui::new(300., 200.);
        let bold = ui.signal(false);
        let read = bold.clone();
        let view = ui.mount(
            column()
                .font_family(FontFamily::Serif)
                .italic(true)
                .reactive_style(move || {
                    Styles::new().font_weight(if read.get() { 700 } else { 400 })
                })
                .child(text("inherited").id("a"))
                .child(
                    text("override")
                        .id("b")
                        .font_family(FontFamily::Monospace)
                        .font_weight(500)
                        .italic(false),
                ),
        );
        let a = view.find("a").unwrap();
        let b = view.find("b").unwrap();
        ui.scene.borrow_mut().flush();
        assert_eq!(ui.scene.borrow().font(a).family, FontFamily::Serif);
        assert!(ui.scene.borrow().font(a).italic);
        assert_eq!(ui.scene.borrow().font(b).weight, 500);
        assert!(!ui.scene.borrow().font(b).italic);
        bold.set(true);
        assert_eq!(view.find("a"), Some(a));
        assert_eq!(ui.scene.borrow().font(a).weight, 700);
        assert_eq!(ui.scene.borrow().font(b).weight, 500);
        assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
        bold.set(true);
        assert!(ui.scene.borrow_mut().flush().is_idle());
    }
}
