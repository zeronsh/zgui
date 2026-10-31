//! Mounted nodes scripts can reach by `id`, `class` or `ref`: what selectors
//! find and what animations write. Each element carries the values its
//! animations hold. Compositor values (translation, opacity, scale, rotation,
//! skew) are written straight to the scene, once per frame and element; the
//! rest go through the node's reactive style. Both are also resolved over
//! the node's own styles whenever those re-run (hover, state), so an
//! animated value never snaps back.
use crate::node::{Lit, Prop};
use crate::transform::{LocalTransform, local_transform, set_local_transform};
use mlua::{UserData, UserDataFields, UserDataMethods};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};
use zgx::{
    compose::NodeHandle,
    reactive::{Runtime, Signal},
    scene::{NodeId, Scene, Transform},
};

/// An input listener added to an element after it mounted.
pub(crate) type Listener = Rc<dyn Fn(&mut zgx::input::EventContext)>;

/// Animated values over the node's own styles. `None`: not animated.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Animated {
    pub translate: Option<(f32, f32)>,
    /// A layout animation's translation, added to the node's own.
    pub offset: Option<(f32, f32)>,
    pub opacity: Option<f32>,
    /// Gaussian blur radius of the node's content, in points.
    pub blur: Option<f32>,
    /// Color filters: `[brightness, contrast, saturate]`.
    pub color: Option<[f32; 3]>,
    /// Scale, rotation and skew; the origin stays the node's own.
    pub local: Option<LocalTransform>,
    /// Where the node's mask sits, from its origin.
    pub mask: Option<(f32, f32)>,
    /// The clip inset, `[top, right, bottom, left]` in points.
    pub clip: Option<[f32; 4]>,
    /// A shader fill's time and parameters.
    pub shader: Option<zgx::decoration::ShaderUniforms>,
    /// Which of them animations drive (bit 0 the time, 1 to 16 the
    /// parameters): only those are written, so parameters the node's style
    /// sets (theme colours) stay.
    pub shader_slots: u32,
    /// A layer shader's time and parameters, and which animations drive
    /// (boxed: few elements have a layer shader).
    pub layer: Option<Box<zgx::decoration::ShaderUniforms>>,
    pub layer_slots: u32,
    pub styles: Vec<(Prop, Lit)>,
}

/// A node's literal translation and local transform.
pub(crate) type BaseTransform = (Option<(f32, f32)>, Option<LocalTransform>);

/// `hit = "target"`'s goals for an element (`Element::sync_hit`).
#[derive(Clone, Default)]
pub(crate) struct HitGoals {
    /// Where its style is taking it.
    style: zgx::scene::HitTarget,
    /// Where `anime` animations are taking properties, each the last value
    /// of the track that holds it.
    anime: Vec<(crate::anime::Key, f32)>,
}
thread_local! {
    /// `hit = "target"`'s goals, by element, for the few that ask
    /// (`Element::mark_hit`): kept apart, so that no element grows or drops
    /// anything more for them.
    static HIT_GOALS: RefCell<Vec<(std::rc::Weak<Element>, HitGoals)>> =
        const { RefCell::new(Vec::new()) };
}
/// Whether any element ever asked: what an animation reads first, all an
/// app with none pays.
static ANY_HIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether any element ever asked to hit at its target: what animations
/// read before looking (`ANY_HIT`).
#[inline(always)]
pub(crate) fn any_hit() -> bool {
    ANY_HIT.load(std::sync::atomic::Ordering::Relaxed)
}
/// `f` on `element`'s goals, if it asked; those of elements gone go.
fn with_goals<R>(element: &Element, f: impl FnOnce(&mut HitGoals) -> R) -> Option<R> {
    HIT_GOALS.with(|goals| {
        let mut goals = goals.borrow_mut();
        goals.retain(|(held, _)| held.strong_count() > 0);
        goals
            .iter_mut()
            .find(|(held, _)| std::ptr::eq(held.as_ptr(), element))
            .map(|(_, goals)| f(goals))
    })
}
pub(crate) struct Element {
    handle: OnceCell<NodeHandle>,
    pub id: Option<Rc<str>>,
    pub classes: Rc<[Rc<str>]>,
    /// The element's kind (`"button"`, `"text"`…), for type selectors.
    pub tag: &'static str,
    pub animated: RefCell<Animated>,
    /// Bumped when animated style values change: the node's reactive style
    /// reads it. Compositor values do not need it.
    pub revision: Signal<u64>,
    styles_changed: Cell<bool>,
    compositor_changed: Cell<bool>,
    /// A baked loop to hand the scene at the next flush (`Some(None)` ends it).
    #[cfg(target_os = "macos")]
    bake: RefCell<Option<Option<Rc<zgx::scene::BakedLoop>>>>,
    /// Queued for this frame's flush.
    queued: Cell<bool>,
    /// Its node went: its selector entries are stale (`Registered`).
    unregistered: Cell<bool>,
    /// The animation that owns each animated property (`replace`
    /// composition): later animations take a property over.
    pub owners: RefCell<Vec<(crate::anime::Key, crate::anime::engine::Owner)>>,
    /// The scene, kept weakly so a frame's flush groups elements by window
    /// without upgrading the UI per element.
    scene: OnceCell<std::rc::Weak<RefCell<Scene>>>,
    /// Pointer and key listeners added after mount (`createDraggable`).
    pub listeners: RefCell<Vec<Listener>>,
    /// A scroll view's offset (points), for scroll observers.
    pub scroll: OnceCell<(Signal<f32>, bool)>,
    /// What the node's literals set: where an animation starts from before
    /// the scene has them.
    base: Cell<BaseTransform>,
    /// The literal opacity and content blur.
    base_effects: Cell<(f32, f32)>,
    /// The literal color filters.
    base_color: Cell<[f32; 3]>,
    /// The literal corner radii.
    base_corners: Cell<Option<zgx::decoration::Corners>>,
    /// The literal clip inset.
    base_clip: Cell<Option<[f32; 4]>>,
    /// The literal shader parameters.
    base_shader: Cell<Option<zgx::decoration::ShaderUniforms>>,
    /// The literal layer shader parameters (boxed, as `Animated::layer`).
    base_layer: Cell<Option<Box<zgx::decoration::ShaderUniforms>>>,
}
impl Element {
    pub fn new(
        runtime: &Runtime,
        id: Option<Rc<str>>,
        classes: Rc<[Rc<str>]>,
        tag: &'static str,
    ) -> Rc<Self> {
        Rc::new(Self {
            handle: OnceCell::new(),
            id,
            classes,
            tag,
            animated: RefCell::default(),
            revision: runtime.signal(0),
            styles_changed: Cell::new(false),
            compositor_changed: Cell::new(false),
            #[cfg(target_os = "macos")]
            bake: RefCell::new(None),
            queued: Cell::new(false),
            unregistered: Cell::new(false),
            owners: RefCell::default(),
            scene: OnceCell::new(),
            listeners: RefCell::default(),
            scroll: OnceCell::new(),
            base: Cell::new((None, None)),
            base_effects: Cell::new((1., 0.)),
            base_color: Cell::new([1.; 3]),
            base_corners: Cell::new(None),
            base_clip: Cell::new(None),
            base_shader: Cell::new(None),
            base_layer: Cell::new(None),
        })
    }
    pub fn node(&self) -> Option<NodeId> {
        self.handle.get().map(NodeHandle::node)
    }
    pub fn scene(&self) -> Option<Rc<RefCell<Scene>>> {
        let scene = self.scene.get()?.upgrade()?;
        let contains = scene.borrow().contains(self.node()?);
        contains.then_some(scene)
    }
    pub fn scene_weak(&self) -> Option<&std::rc::Weak<RefCell<Scene>>> {
        self.scene.get()
    }
    /// Its laid-out size, followed: read in an effect or a computed, it
    /// runs again when layout resizes the node, before the frame shows (a
    /// container query, through a ref's `width` and `height`). None while
    /// it is not in a scene.
    pub fn size(&self) -> Option<(f32, f32)> {
        self.scene()?;
        Some(self.handle.get()?.observe_size()?.get())
    }
    pub fn is_mounted(&self) -> bool {
        self.handle.get().is_some_and(NodeHandle::is_mounted)
    }
    /// Mounted once and since removed: it will not come back.
    pub fn is_gone(&self) -> bool {
        self.handle.get().is_some_and(|h| !h.is_mounted())
    }
    /// The current translation: animated, or the node's own.
    pub fn translate(&self) -> (f32, f32) {
        if let Some(t) = self.animated.borrow().translate {
            return t;
        }
        if let Some(t) = self.base.get().0 {
            return t;
        }
        // The scene's has a layout animation's offset in it.
        let (ox, oy) = self.offset();
        self.read(|scene, id| {
            let t = scene.transform(id);
            (t.x - ox, t.y - oy)
        })
        .unwrap_or_default()
    }
    pub fn opacity(&self) -> f32 {
        if let Some(o) = self.animated.borrow().opacity {
            return o;
        }
        self.read(|scene, id| scene.effects(id).opacity)
            .unwrap_or(1.)
    }
    pub fn blur(&self) -> f32 {
        if let Some(b) = self.animated.borrow().blur {
            return b;
        }
        self.read(|scene, id| scene.effects(id).filter_blur)
            .unwrap_or(0.)
    }
    pub fn set_blur(&self, b: f32) {
        let mut animated = self.animated.borrow_mut();
        if animated.blur != Some(b) {
            animated.blur = Some(b);
            self.compositor_changed.set(true);
        }
    }
    pub fn mask_offset(&self) -> (f32, f32) {
        if let Some(m) = self.animated.borrow().mask {
            return m;
        }
        self.read(|scene, id| scene.mask(id).map_or((0., 0.), |m| m.offset))
            .unwrap_or_default()
    }
    /// The clip inset now, in points (`[top, right, bottom, left]`).
    pub fn clip_inset(&self) -> [f32; 4] {
        if let Some(c) = self.animated.borrow().clip {
            return c;
        }
        if let Some(c) = self.base_clip.get() {
            return c;
        }
        self.read(|scene, id| {
            let b = scene.bounds(id);
            scene.clip_inset(id).map_or([0.; 4], |c| {
                let px = |l: zgx::layout::Length, basis: f32| match l {
                    zgx::layout::Length::Px(v) => v,
                    zgx::layout::Length::Percent(f) => f * basis,
                    _ => 0.,
                };
                [
                    px(c.top, b.height),
                    px(c.right, b.width),
                    px(c.bottom, b.height),
                    px(c.left, b.width),
                ]
            })
        })
        .unwrap_or_default()
    }
    pub fn set_clip_side(&self, side: usize, value: f32) {
        let current = self.animated.borrow().clip;
        // The other sides start from where the clip is now.
        let mut clip = current.unwrap_or_else(|| self.clip_inset());
        if current.is_some() && clip[side] == value {
            return;
        }
        clip[side] = value;
        self.animated.borrow_mut().clip = Some(clip);
        self.compositor_changed.set(true);
    }
    pub fn set_mask_offset(&self, offset: (f32, f32)) {
        let mut animated = self.animated.borrow_mut();
        if animated.mask != Some(offset) {
            animated.mask = Some(offset);
            self.compositor_changed.set(true);
        }
    }
    pub fn local(&self) -> LocalTransform {
        if let Some(l) = self.animated.borrow().local {
            return l;
        }
        if let Some(l) = self.base.get().1 {
            return l;
        }
        self.read(local_transform).unwrap_or_default()
    }
    pub fn base_corners(&self) -> Option<zgx::decoration::Corners> {
        self.base_corners.get()
    }
    pub fn set_base_color(&self, color: [f32; 3]) {
        self.base_color.set(color);
    }
    /// The color filters now, `[brightness, contrast, saturate]`.
    pub fn color(&self) -> [f32; 3] {
        self.animated
            .borrow()
            .color
            .unwrap_or_else(|| self.base_color.get())
    }
    pub fn set_color_part(&self, part: usize, value: f32) {
        let mut color = self.color();
        if self.animated.borrow().color.is_some() && color[part] == value {
            return;
        }
        color[part] = value;
        self.animated.borrow_mut().color = Some(color);
        self.compositor_changed.set(true);
    }
    pub fn set_base_shader(&self, uniforms: Option<zgx::decoration::ShaderUniforms>) {
        self.base_shader.set(uniforms);
    }
    /// The shader fill's uniforms now.
    pub fn shader_uniforms(&self) -> zgx::decoration::ShaderUniforms {
        if let Some(u) = self.animated.borrow().shader {
            return u;
        }
        if let Some(u) = self.base_shader.get() {
            return u;
        }
        self.read(|scene, id| scene.shader_uniforms(id))
            .unwrap_or_default()
    }
    pub fn set_base_layer(&self, uniforms: Option<zgx::decoration::ShaderUniforms>) {
        let mut base = self.base_layer.take();
        match (&mut base, uniforms) {
            (Some(held), Some(uniforms)) => **held = uniforms,
            (_, uniforms) => base = uniforms.map(Box::new),
        }
        self.base_layer.set(base);
    }
    /// The literal layer shader parameters, if any.
    fn base_layer(&self) -> Option<zgx::decoration::ShaderUniforms> {
        let base = self.base_layer.take();
        let uniforms = base.as_deref().copied();
        self.base_layer.set(base);
        uniforms
    }
    /// The layer shader's uniforms now.
    pub fn layer_uniforms(&self) -> zgx::decoration::ShaderUniforms {
        if let Some(u) = self.animated.borrow().layer.as_deref() {
            return *u;
        }
        if let Some(u) = self.base_layer() {
            return u;
        }
        self.read(|scene, id| scene.layer_shader(id).map(|layer| layer.uniforms))
            .flatten()
            .unwrap_or_default()
    }
    /// Sets the layer shader's time (`slot` 0) or one of its sixteen
    /// parameters (1 to 16): composites again, no repaint.
    pub fn set_layer_slot(&self, slot: usize, value: f32) {
        let current = self.animated.borrow().layer.as_deref().copied();
        let mut u = current.unwrap_or_else(|| self.layer_uniforms());
        let target = match slot {
            0 => &mut u.time,
            n => &mut u.params[(n - 1) / 4][(n - 1) % 4],
        };
        if current.is_some() && *target == value {
            return;
        }
        *target = value;
        let mut animated = self.animated.borrow_mut();
        // Written in place while it animates: no allocation a frame.
        match &mut animated.layer {
            Some(held) => **held = u,
            None => animated.layer = Some(Box::new(u)),
        }
        animated.layer_slots |= 1 << slot;
        drop(animated);
        self.compositor_changed.set(true);
    }
    /// Sets the time (`slot` 0) or one of the sixteen parameters (1 to 16).
    pub fn set_shader_slot(&self, slot: usize, value: f32) {
        let current = self.animated.borrow().shader;
        let mut u = current.unwrap_or_else(|| self.shader_uniforms());
        let target = match slot {
            0 => &mut u.time,
            n => &mut u.params[(n - 1) / 4][(n - 1) % 4],
        };
        if current.is_some() && *target == value {
            return;
        }
        *target = value;
        let mut animated = self.animated.borrow_mut();
        animated.shader = Some(u);
        animated.shader_slots |= 1 << slot;
        drop(animated);
        self.compositor_changed.set(true);
    }
    /// Hands the scene `bake` (see `Scene::set_baked_loop`) at the next flush.
    #[cfg(target_os = "macos")]
    pub(crate) fn set_bake(&self, bake: Option<Rc<zgx::scene::BakedLoop>>) {
        *self.bake.borrow_mut() = Some(bake);
        self.compositor_changed.set(true);
    }
    pub fn set_base_clip(&self, clip: Option<[f32; 4]>) {
        self.base_clip.set(clip);
    }
    pub fn set_base_corners(&self, corners: Option<zgx::decoration::Corners>) {
        self.base_corners.set(corners);
    }
    pub fn set_base(
        &self,
        translate: Option<(f32, f32)>,
        local: Option<LocalTransform>,
        effects: (f32, f32),
    ) {
        self.base.set((translate, local));
        self.base_effects.set(effects);
    }
    /// What the node's own literals translate and transform it by.
    pub fn base_transform(&self) -> BaseTransform {
        self.base.get()
    }
    /// Binds an element only Rust animates (a layout animation's) to its
    /// mounted node, without making it something selectors find.
    pub fn attach(&self, handle: NodeHandle) {
        if let Some(scene) = handle.scene() {
            let _ = self.scene.set(Rc::downgrade(&scene));
        }
        let _ = self.handle.set(handle);
    }
    /// It's leaving, gone from the data it showed: while its exit plays it
    /// takes no input (`NodeHandle::leave`), so a second click or a key acts
    /// on what is still there.
    pub(crate) fn leave(&self) {
        if let Some(handle) = self.handle.get() {
            handle.leave();
        }
    }
    /// Whether its node takes hits where it's going (`hit = "target"`).
    /// One load an app with none pays (`ANY_HIT`); the look-up apart.
    #[inline(always)]
    pub(crate) fn hits_at_target(&self) -> bool {
        ANY_HIT.load(std::sync::atomic::Ordering::Relaxed) && self.asked_hit()
    }
    #[cold]
    #[inline(never)]
    fn asked_hit(&self) -> bool {
        with_goals(self, |_| ()).is_some()
    }
    /// Its node takes hits where it's going (`hit = "target"`): from now,
    /// before it mounts and an animation in `on_mount` starts.
    pub(crate) fn mark_hit(self: &Rc<Self>) {
        ANY_HIT.store(true, std::sync::atomic::Ordering::Relaxed);
        if with_goals(self, |_| ()).is_none() {
            HIT_GOALS.with(|goals| {
                goals
                    .borrow_mut()
                    .push((Rc::downgrade(self), HitGoals::default()))
            });
        }
    }
    /// Where its style is taking it (`hit = "target"`): with `anime`'s goals
    /// over it, where it takes hits.
    pub(crate) fn set_style_hit(&self, style: zgx::scene::HitTarget) {
        if with_goals(self, |goals| goals.style = style).is_some() {
            self.sync_hit();
        }
    }
    /// Where an `anime` animation of `key` is taking it, or `None` once it
    /// lets the property go. Nothing unless its node hits at its target.
    pub(crate) fn anime_goal(&self, key: &crate::anime::Key, goal: Option<f32>) {
        let held = with_goals(self, |goals| {
            goals.anime.retain(|(k, _)| k != key);
            if let Some(goal) = goal {
                goals.anime.push((key.clone(), goal));
            }
        });
        if held.is_some() {
            self.sync_hit();
        }
    }
    /// Its hit target: its style's goals, then `anime`'s over them.
    fn sync_hit(&self) {
        use crate::anime::Key;
        let Some(target) = with_goals(self, |hit| hit.clone()).map(|hit| {
            let mut target = hit.style;
            // What `anime` wrote is shown over the style (`shown`): an
            // animation let go of mid-way left it there.
            let animated = self.animated.borrow();
            if let Some((x, y)) = animated.translate {
                target.translation = zgx::scene::Transform { x, y };
            }
            if animated.local.is_some() {
                target.local = animated.local;
            }
            if let Some(opacity) = animated.opacity {
                target.gone = opacity <= 0.;
            }
            drop(animated);
            let mut local = target.local.unwrap_or_else(|| self.local());
            let mut turned = false;
            for (key, v) in &hit.anime {
                turned |= matches!(
                    key,
                    Key::Scale
                        | Key::ScaleX
                        | Key::ScaleY
                        | Key::Rotate
                        | Key::RotateX
                        | Key::RotateY
                        | Key::Perspective
                        | Key::Skew
                        | Key::SkewX
                        | Key::SkewY
                );
                match key {
                    Key::X => target.translation.x = *v,
                    Key::Y => target.translation.y = *v,
                    Key::Opacity => target.gone = *v <= 0.,
                    Key::Scale => local.scale = (*v, *v),
                    Key::ScaleX => local.scale.0 = *v,
                    Key::ScaleY => local.scale.1 = *v,
                    Key::Rotate => local.rotate = *v,
                    Key::RotateX => local.rotate_x = *v,
                    Key::RotateY => local.rotate_y = *v,
                    Key::Perspective => local.perspective = v.max(0.),
                    Key::Skew => local.skew = (*v, *v),
                    Key::SkewX => local.skew.0 = *v,
                    Key::SkewY => local.skew.1 = *v,
                    // A layout animation's offset: its target is layout's place.
                    _ => {}
                }
            }
            if turned {
                target.local = Some(local);
            }
            target
        }) else {
            return;
        };
        if let (Some(scene), Some(node)) = (self.scene(), self.node()) {
            scene.borrow_mut().set_hit_target(node, Some(target));
        }
    }
    pub fn read<R>(&self, f: impl FnOnce(&Scene, NodeId) -> R) -> Option<R> {
        let scene = self.scene()?;
        let scene = scene.borrow();
        Some(f(&scene, self.node()?))
    }
    /// A layout animation's translation now.
    pub fn offset(&self) -> (f32, f32) {
        self.animated.borrow().offset.unwrap_or_default()
    }
    pub fn set_offset(&self, o: (f32, f32)) {
        let mut animated = self.animated.borrow_mut();
        if animated.offset != Some(o) {
            animated.offset = Some(o);
            self.compositor_changed.set(true);
        }
    }
    /// The translation the node shows at: its own and a layout animation's.
    pub fn shown_translate(&self, own: Option<(f32, f32)>) -> Option<(f32, f32)> {
        self.shown(&self.animated.borrow(), own)
    }
    /// `shown_translate` from values already borrowed.
    fn shown(&self, animated: &Animated, own: Option<(f32, f32)>) -> Option<(f32, f32)> {
        match (animated.translate, animated.offset) {
            (t, None) => t,
            (t, Some((ox, oy))) => {
                let (x, y) = t.or(own).or(self.base.get().0).unwrap_or_default();
                Some((x + ox, y + oy))
            }
        }
    }
    pub fn set_translate(&self, t: (f32, f32)) {
        let mut animated = self.animated.borrow_mut();
        if animated.translate != Some(t) {
            animated.translate = Some(t);
            self.compositor_changed.set(true);
        }
    }
    pub fn set_opacity(&self, o: f32) {
        let mut animated = self.animated.borrow_mut();
        if animated.opacity != Some(o) {
            animated.opacity = Some(o);
            self.compositor_changed.set(true);
        }
    }
    pub fn set_local(&self, l: LocalTransform) {
        let mut animated = self.animated.borrow_mut();
        if animated.local != Some(l) {
            animated.local = Some(l);
            self.compositor_changed.set(true);
        }
    }
    pub fn set_style(&self, prop: Prop, lit: Lit) {
        let mut animated = self.animated.borrow_mut();
        match animated.styles.iter_mut().find(|(p, _)| *p == prop) {
            Some((_, old)) if *old == lit => return,
            Some((_, old)) => *old = lit,
            None => animated.styles.push((prop, lit)),
        }
        self.styles_changed.set(true);
    }
    /// Forget the animated values of `keys` (all of them for `None`): each
    /// returns to what the node's own literals set, as removing an inline
    /// style lets the stylesheet's value show again.
    pub fn clear_only(&self, keys: Option<&[crate::anime::Key]>) {
        use crate::anime::Key;
        let wants = |k: &Key| keys.is_none_or(|keys| keys.contains(k));
        let any = |ks: &[Key]| ks.iter().any(&wants);
        let cleared = {
            let mut animated = self.animated.borrow_mut();
            let mut cleared = Animated::default();
            if any(&[Key::X, Key::Y]) {
                cleared.translate = animated.translate.take();
            }
            if any(&[Key::OffsetX, Key::OffsetY]) {
                cleared.offset = animated.offset.take();
            }
            if wants(&Key::Opacity) {
                cleared.opacity = animated.opacity.take();
            }
            if wants(&Key::Blur) {
                cleared.blur = animated.blur.take();
            }
            if any(&[Key::Brightness, Key::Contrast, Key::Saturate]) {
                cleared.color = animated.color.take();
            }
            if any(&[
                Key::Scale,
                Key::ScaleX,
                Key::ScaleY,
                Key::Rotate,
                Key::Skew,
                Key::SkewX,
                Key::SkewY,
                Key::RotateX,
                Key::RotateY,
                Key::Perspective,
            ]) {
                cleared.local = animated.local.take();
            }
            if any(&[Key::MaskX, Key::MaskY]) {
                cleared.mask = animated.mask.take();
            }
            if any(&[Key::ClipTop, Key::ClipRight, Key::ClipBottom, Key::ClipLeft]) {
                cleared.clip = animated.clip.take();
            }
            if keys.is_none_or(|keys| keys.iter().any(|k| matches!(k, Key::Shader(_)))) {
                cleared.shader = animated.shader.take();
                animated.shader_slots = 0;
            }
            if keys.is_none_or(|keys| keys.iter().any(|k| matches!(k, Key::Layer(_)))) {
                cleared.layer = animated.layer.take();
                animated.layer_slots = 0;
            }
            let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut animated.styles)
                .into_iter()
                .partition(|(p, _)| wants(&Key::Style(*p)));
            animated.styles = kept;
            cleared.styles = gone;
            cleared
        };
        // The animations that owned these properties stop writing them.
        self.owners.borrow_mut().retain(|(key, owner)| {
            let clears = match key {
                Key::Style(p) => cleared.styles.iter().any(|(q, _)| q == p),
                Key::X | Key::Y => cleared.translate.is_some(),
                Key::OffsetX | Key::OffsetY => cleared.offset.is_some(),
                Key::Opacity => cleared.opacity.is_some(),
                Key::Blur => cleared.blur.is_some(),
                Key::Brightness | Key::Contrast | Key::Saturate => cleared.color.is_some(),
                Key::MaskX | Key::MaskY => cleared.mask.is_some(),
                Key::ClipTop | Key::ClipRight | Key::ClipBottom | Key::ClipLeft => {
                    cleared.clip.is_some()
                }
                Key::Layer(_) => cleared.layer.is_some(),
                Key::Field(_) => false,
                _ => cleared.local.is_some(),
            };
            if clears {
                owner.track.set(true);
            }
            !clears
        });
        let (base_translate, base_local) = self.base.get();
        if let (Some(scene), Some(id)) = (self.scene(), self.node()) {
            let mut scene = scene.borrow_mut();
            if cleared.translate.is_some() || cleared.offset.is_some() {
                let (x, y) = self
                    .shown_translate(None)
                    .or(base_translate)
                    .unwrap_or_default();
                scene.set_transform(id, Transform { x, y });
            }
            if cleared.opacity.is_some() || cleared.blur.is_some() || cleared.color.is_some() {
                let (opacity, blur) = self.base_effects.get();
                let mut effects = scene.effects(id);
                if cleared.color.is_some() {
                    let [brightness, contrast, saturate] = self.base_color.get();
                    effects.brightness = brightness;
                    effects.contrast = contrast;
                    effects.saturate = saturate;
                }
                if cleared.opacity.is_some() {
                    effects.opacity = opacity;
                }
                if cleared.blur.is_some() {
                    effects.filter_blur = blur;
                }
                scene.set_effects(id, effects);
            }
            if cleared.local.is_some() {
                let origin = local_transform(&scene, id).origin;
                let local = base_local.unwrap_or(LocalTransform {
                    origin,
                    ..LocalTransform::IDENTITY
                });
                set_local_transform(&mut scene, id, local);
            }
            if cleared.layer.is_some() {
                scene.set_layer_uniforms(id, self.base_layer().unwrap_or_default());
            }
        }
        // The node's style re-runs and restores what it sets itself.
        self.revision.update(|r| *r += 1);
    }
    /// Queue for `flush`; returns whether it was not already queued.
    pub fn queue(&self) -> bool {
        !self.queued.replace(true)
    }
    /// Drops queued writes for an element no longer in a scene.
    pub fn flush_detached(&self) {
        self.queued.set(false);
        self.compositor_changed.set(false);
        self.styles_changed.set(false);
    }
    /// Writes this frame's compositor values to the scene. Style changes are
    /// returned to bump once the scene is released.
    pub fn flush(&self, scene: &mut Scene) -> bool {
        self.queued.set(false);
        if self.compositor_changed.replace(false)
            && let Some(id) = self.node()
            && scene.contains(id)
        {
            #[cfg(target_os = "macos")]
            if let Some(bake) = self.bake.borrow_mut().take() {
                scene.set_baked_loop(id, bake);
            }
            let animated = self.animated.borrow();
            let translate = self.shown(&animated, None).map(|(x, y)| Transform { x, y });
            let effects =
                (animated.opacity.is_some() || animated.blur.is_some() || animated.color.is_some())
                    .then(|| {
                        let mut effects = scene.effects(id);
                        if let Some([brightness, contrast, saturate]) = animated.color {
                            effects.brightness = brightness;
                            effects.contrast = contrast;
                            effects.saturate = saturate;
                        }
                        if let Some(opacity) = animated.opacity {
                            effects.opacity = opacity;
                        }
                        // `blur` is the content's blur; `blur_radius` is the backdrop's.
                        if let Some(blur) = animated.blur {
                            effects.filter_blur = blur;
                        }
                        effects
                    });
            match (translate, effects) {
                // Moving and fading: one damage of each look, not two.
                (Some(translate), Some(effects)) => {
                    scene.set_transform_and_effects(id, translate, effects)
                }
                (Some(translate), None) => scene.set_transform(id, translate),
                (None, Some(effects)) => scene.set_effects(id, effects),
                (None, None) => {}
            }
            if let Some(local) = animated.local {
                set_local_transform(scene, id, local);
            }
            if let Some(offset) = animated.mask {
                scene.set_mask_offset(id, offset);
            }
            if let Some([t, r, b, l]) = animated.clip {
                scene.set_clip_inset(id, Some(zgx::scene::ClipInset::px(t, r, b, l)));
            }
            if let Some(uniforms) = animated.shader {
                let mut shown = scene.shader_uniforms(id);
                if animated.shader_slots & 1 != 0 {
                    shown.time = uniforms.time;
                }
                for slot in 1..=16 {
                    if animated.shader_slots & (1 << slot) != 0 {
                        let (v, i) = ((slot - 1) / 4, (slot - 1) % 4);
                        shown.params[v][i] = uniforms.params[v][i];
                    }
                }
                scene.set_shader_uniforms(id, shown);
            }
            if let Some(uniforms) = animated.layer.as_deref() {
                let mut shown = scene
                    .layer_shader(id)
                    .map(|layer| layer.uniforms)
                    .unwrap_or_default();
                if animated.layer_slots & 1 != 0 {
                    shown.time = uniforms.time;
                }
                for slot in 1..=16 {
                    if animated.layer_slots & (1 << slot) != 0 {
                        let (v, i) = ((slot - 1) / 4, (slot - 1) % 4);
                        shown.params[v][i] = uniforms.params[v][i];
                    }
                }
                scene.set_layer_uniforms(id, shown);
            }
        }
        self.styles_changed.replace(false)
    }
}

/// Every targetable element, by id and class.
#[derive(Default)]
pub(crate) struct Elements {
    /// Set while work waits for elements to mount; a registration then
    /// wakes the engine once to retry it.
    watching: Cell<bool>,
    mounted: Cell<bool>,
    wake: RefCell<Option<Rc<dyn Fn()>>>,
    by_node: RefCell<HashMap<NodeId, Weak<Element>>>,
    ids: RefCell<HashMap<Rc<str>, Registered>>,
    classes: RefCell<HashMap<Rc<str>, Registered>>,
    tags: RefCell<HashMap<&'static str, Registered>>,
}
/// The elements under one id, class or kind. An element that unmounts is
/// counted stale, not searched for: rows sharing a class unmounting together
/// would each search the list, N² for N rows. The list sheds its stale
/// entries once they are half of it.
#[derive(Default)]
struct Registered {
    entries: Vec<Weak<Element>>,
    stale: usize,
}
impl Registered {
    fn live(&self) -> impl Iterator<Item = Rc<Element>> + '_ {
        self.entries
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|e| !e.unregistered.get())
    }
    /// Counts one entry stale; whether the list is now empty.
    fn unregistered(&mut self) -> bool {
        self.stale += 1;
        if self.stale * 2 > self.entries.len() {
            self.entries
                .retain(|e| e.upgrade().is_some_and(|e| !e.unregistered.get()));
            self.stale = 0;
        }
        self.entries.is_empty()
    }
}
impl Elements {
    pub fn set_wake(&self, wake: Rc<dyn Fn()>) {
        *self.wake.borrow_mut() = Some(wake);
    }
    pub fn watch(&self, watching: bool) {
        self.watching.set(watching);
        self.mounted.set(false);
    }
    /// Whether an element mounted since work started waiting; clears it.
    pub fn mounted_since_watch(&self) -> bool {
        self.watching.get() && self.mounted.replace(false)
    }
    /// The element mounted at `node`, if it is targetable.
    pub fn at(&self, node: NodeId) -> Option<Rc<Element>> {
        self.by_node.borrow().get(&node).and_then(Weak::upgrade)
    }
    /// Registers a mounted element until `handle`'s node is removed.
    pub fn register(self: &Rc<Self>, element: &Rc<Element>, handle: NodeHandle) {
        self.by_node
            .borrow_mut()
            .insert(handle.node(), Rc::downgrade(element));
        if self.watching.get() && !self.mounted.replace(true) {
            let wake = self.wake.borrow().clone();
            if let Some(wake) = wake {
                wake();
            }
        }
        let _ = element.handle.set(handle.clone());
        if let Some(scene) = handle.scene() {
            let _ = element.scene.set(Rc::downgrade(&scene));
        }
        let weak = Rc::downgrade(element);
        if let Some(id) = &element.id {
            self.ids
                .borrow_mut()
                .entry(id.clone())
                .or_default()
                .entries
                .push(weak.clone());
        }
        for class in element.classes.iter() {
            self.classes
                .borrow_mut()
                .entry(class.clone())
                .or_default()
                .entries
                .push(weak.clone());
        }
        self.tags
            .borrow_mut()
            .entry(element.tag)
            .or_default()
            .entries
            .push(weak.clone());
        handle.retain(Registration {
            elements: Rc::downgrade(self),
            element: element.clone(),
        });
    }
    fn unregister(&self, element: &Rc<Element>) {
        element.unregistered.set(true);
        fn stale<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Registered>, key: &K) {
            if map.get_mut(key).is_some_and(Registered::unregistered) {
                map.remove(key);
            }
        }
        if let Some(id) = &element.id {
            stale(&mut self.ids.borrow_mut(), id);
        }
        for class in element.classes.iter() {
            stale(&mut self.classes.borrow_mut(), class);
        }
        stale(&mut self.tags.borrow_mut(), &element.tag);
        if let Some(node) = element.node() {
            self.by_node.borrow_mut().remove(&node);
        }
    }
    /// Elements matching a selector list: `#id`, `.class`, a kind such as
    /// `button`, `*`, comma separated, in document order without repeats.
    pub fn select(&self, selector: &str) -> Vec<Rc<Element>> {
        let mut found: Vec<Rc<Element>> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for part in selector.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            // Compound selectors (`button.primary`) match all their parts.
            let (tag, rest) = match part.find(['#', '.']) {
                Some(0) => (None, part),
                Some(i) => (Some(&part[..i]), &part[i..]),
                None => (Some(part), ""),
            };
            let mut ids = Vec::new();
            let mut classes = Vec::new();
            let mut cursor = rest;
            while let Some(c) = cursor.chars().next() {
                let end = cursor[1..].find(['#', '.']).map_or(cursor.len(), |i| i + 1);
                let name = &cursor[1..end];
                if c == '#' {
                    ids.push(name);
                } else {
                    classes.push(name);
                }
                cursor = &cursor[end..];
            }
            let candidates: Vec<Rc<Element>> = if let Some(id) = ids.first() {
                let ids = self.ids.borrow();
                ids.get(*id).map_or_else(Vec::new, |r| r.live().collect())
            } else if let Some(class) = classes.first() {
                let classes = self.classes.borrow();
                classes
                    .get(*class)
                    .map_or_else(Vec::new, |r| r.live().collect())
            } else if let Some(tag) = tag.filter(|t| *t != "*") {
                let tags = self.tags.borrow();
                tags.get(tag).map_or_else(Vec::new, |r| r.live().collect())
            } else {
                self.tags
                    .borrow()
                    .values()
                    .flat_map(Registered::live)
                    .collect()
            };
            for element in candidates {
                let matches = element.is_mounted()
                    && tag.is_none_or(|t| t == "*" || t == element.tag)
                    && ids.iter().all(|id| element.id.as_deref() == Some(*id))
                    && classes
                        .iter()
                        .all(|c| element.classes.iter().any(|own| &**own == *c));
                if matches && seen.insert(Rc::as_ptr(&element)) {
                    found.push(element);
                }
            }
        }
        sort_by_document_order(&mut found);
        found
    }
}

/// Orders elements as they appear in the tree (depth first), the order
/// anime.js staggers selector matches in.
pub(crate) fn sort_by_document_order(elements: &mut [Rc<Element>]) {
    if elements.len() < 2 {
        return;
    }
    let Some(scene) = elements.iter().find_map(|e| e.scene()) else {
        return;
    };
    let scene = scene.borrow();
    // Each node's place among its siblings, a parent's children indexed
    // once: a thousand rows of one list are not each searched for. Node ids
    // hash by a multiply (Fx), not SipHash: these lookups are all it does.
    let mut places: rustc_hash::FxHashMap<NodeId, usize> = Default::default();
    let mut indexed: rustc_hash::FxHashSet<NodeId> = Default::default();
    let mut path = |element: &Rc<Element>| {
        let mut path = Vec::new();
        let mut node = element.node();
        while let Some(id) = node {
            let parent = scene.parent(id);
            if let Some(parent) = parent {
                if indexed.insert(parent) {
                    for (i, child) in scene.children(parent).iter().enumerate() {
                        places.insert(*child, i);
                    }
                }
                path.push(places.get(&id).copied().unwrap_or(0));
            }
            node = parent;
        }
        path.reverse();
        path
    };
    let mut keyed: Vec<(Vec<usize>, Rc<Element>)> =
        elements.iter().map(|e| (path(e), e.clone())).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (slot, (_, element)) in elements.iter_mut().zip(keyed) {
        *slot = element;
    }
}

struct Registration {
    elements: Weak<Elements>,
    element: Rc<Element>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(elements) = self.elements.upgrade() {
            elements.unregister(&self.element);
        }
    }
}

/// The UTF-8 byte offset of the `chars`th character of `text` (its end past it).
fn byte_offset(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

/// `ui.ref()`: the elements a node or nodes with `ref = r` mounted. Scripts
/// reading it (`ref:get()`, `ref.element`, `ref.mounted`) in an effect or
/// computed follow it: it changes as its elements mount and unmount.
pub(crate) struct Ref {
    elements: RefCell<Vec<Weak<Element>>>,
    /// Bumped on each attach and detach.
    changed: Signal<u64>,
}
impl Ref {
    pub fn new(runtime: &Runtime) -> Rc<Self> {
        Rc::new(Self {
            elements: RefCell::default(),
            changed: runtime.signal(0),
        })
    }
    /// `element` mounted with this ref, at `handle`: until its node goes.
    pub fn attach(self: &Rc<Self>, element: &Rc<Element>, handle: &NodeHandle) {
        {
            let mut elements = self.elements.borrow_mut();
            elements.retain(|e| e.upgrade().is_some_and(|e| e.is_mounted()));
            elements.push(Rc::downgrade(element));
        }
        handle.retain(Detach {
            r: Rc::downgrade(self),
            element: Rc::downgrade(element),
        });
        self.changed.update(|n| *n += 1);
    }
    /// The mounted elements, in document order. Untracked: what the engine
    /// resolves as targets must not subscribe the effect that animates them.
    pub fn elements(&self) -> Vec<Rc<Element>> {
        let mut live: Vec<_> = self
            .elements
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|e| e.is_mounted())
            .collect();
        sort_by_document_order(&mut live);
        live
    }
    /// The first mounted element.
    pub fn first(&self) -> Option<Rc<Element>> {
        let (first, more) = {
            let elements = self.elements.borrow();
            let mut live = elements
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|e| e.is_mounted());
            (live.next()?, live.next().is_some())
        };
        // Only several need the scene to order them.
        if more {
            self.elements().into_iter().next()
        } else {
            Some(first)
        }
    }
    /// Subscribes the running effect or computed.
    pub fn track(&self) {
        self.changed.with(|_| ());
    }
}
/// Takes an element off its ref as its node is removed, and tells whoever
/// reads the ref.
struct Detach {
    r: Weak<Ref>,
    element: Weak<Element>,
}
impl Drop for Detach {
    fn drop(&mut self) {
        let Some(r) = self.r.upgrade() else { return };
        let ptr = self.element.as_ptr();
        r.elements
            .borrow_mut()
            .retain(|e| e.as_ptr() != ptr && e.strong_count() > 0);
        r.changed.update(|n| *n += 1);
    }
}
#[derive(Clone)]
pub(crate) struct RefUd(pub Rc<Ref>);
impl UserData for RefUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("count", |_, r| {
            r.0.track();
            Ok(r.0.elements().len())
        });
        fields.add_field_method_get("mounted", |_, r| {
            r.0.track();
            Ok(r.0.first().is_some())
        });
        fields.add_field_method_get("element", |_, r| {
            r.0.track();
            Ok(r.0.first().map(ElementUd))
        });
        // Its element's laid-out size, followed (a container query).
        fields.add_field_method_get("width", |_, r| {
            r.0.track();
            Ok(r.0.first().and_then(|e| e.size()).map(|size| size.0))
        });
        fields.add_field_method_get("height", |_, r| {
            r.0.track();
            Ok(r.0.first().and_then(|e| e.size()).map(|size| size.1))
        });
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // The element while one is mounted, else nil; tracked. From a
        // handler, what it set mounts first (a state switching what shows):
        // the element is the one there now.
        methods.add_method("get", |lua, r, ()| {
            settle(lua);
            r.0.track();
            Ok(r.0.first().map(ElementUd))
        });
        methods.add_method("peek", |lua, r, ()| {
            settle(lua);
            Ok(r.0.first().map(ElementUd))
        });
        methods.add_method("elements", |lua, r, ()| {
            r.0.track();
            lua.create_sequence_from(r.0.elements().into_iter().map(ElementUd))
        });
    }
}

/// An element handed to scripts: function-based animation values receive
/// their target as one.
#[derive(Clone)]
pub(crate) struct ElementUd(pub Rc<Element>);
/// What a state just set is synced into (an input's text), and what an
/// input just edited synced out to, before an input's selection is read or
/// set: a handler setting the text, then selecting in it, selects in the
/// new text (`Runtime::settle`).
fn settle(lua: &mlua::Lua) {
    off_clock(lua, |runtime| runtime.settle());
}
/// `work` with the runtime, off the calling script's clock: what an input's
/// text changing updates (a long document laid out again) is not the
/// script's to pay for, as what a state's write updates is not.
fn off_clock<R>(lua: &mlua::Lua, work: impl FnOnce(&zgx::reactive::Runtime) -> R) -> Option<R> {
    // Out of the app data first: what it runs may call scripts.
    let (runtime, vm) = crate::node::env(lua)
        .map(|env| (env.runtime.clone(), env.vm.upgrade()))
        .ok()?;
    Some(match vm {
        Some(vm) => vm.off_clock(|| work(&runtime)),
        None => work(&runtime),
    })
}
/// `ui.editor`: a field's undo and the byte under a point, for an app's own
/// undo stack across panes and drops where the pointer is. On `ui`, made
/// when first read, so an element's metatable stays as small as it was.
pub(crate) fn install_editor(lua: &mlua::Lua, ui: &mlua::Table) -> mlua::Result<()> {
    type El<'a> = mlua::UserDataRef<ElementUd>;
    let editor_of = |e: &El| e.0.handle.get().and_then(NodeHandle::editor);
    let table = lua.create_table()?;
    table.set(
        "undo",
        lua.create_function(move |lua, e: El| {
            settle(lua);
            let Some(editor) = editor_of(&e) else {
                return Ok(false);
            };
            let mut done = false;
            off_clock(lua, |_| done = editor.undo());
            settle(lua);
            Ok(done)
        })?,
    )?;
    table.set(
        "redo",
        lua.create_function(move |lua, e: El| {
            settle(lua);
            let Some(editor) = editor_of(&e) else {
                return Ok(false);
            };
            let mut done = false;
            off_clock(lua, |_| done = editor.redo());
            settle(lua);
            Ok(done)
        })?,
    )?;
    table.set(
        "can_undo",
        lua.create_function(move |lua, e: El| {
            settle(lua);
            Ok(editor_of(&e).is_some_and(|editor| editor.editor.borrow().can_undo()))
        })?,
    )?;
    table.set(
        "can_redo",
        lua.create_function(move |lua, e: El| {
            settle(lua);
            Ok(editor_of(&e).is_some_and(|editor| editor.editor.borrow().can_redo()))
        })?,
    )?;
    table.set(
        "offset_at",
        lua.create_function(move |lua, (e, x, y): (El, f32, f32)| {
            settle(lua);
            Ok(editor_of(&e).and_then(|editor| editor.offset_at(x, y)))
        })?,
    )?;
    // `ui.editor.scroll_to_line(el, n, { align })`: line `n` from 1 at that
    // place in the view, its caret left where it is.
    table.set(
        "scroll_to_line",
        lua.create_function(
            move |lua, (e, line, options): (El, f64, Option<mlua::Table>)| {
                use zgx::compose::RowAlign;
                let align = match options {
                    Some(options) => options.get::<Option<String>>("align")?,
                    None => None,
                };
                let align = match align.as_deref() {
                    None | Some("nearest") => RowAlign::Nearest,
                    Some("start") => RowAlign::Start,
                    Some("center") => RowAlign::Center,
                    Some("end") => RowAlign::End,
                    Some(other) => {
                        return Err(mlua::Error::runtime(format!(
                            "scroll_to_line's align is \"start\", \"center\", \"end\" or \"nearest\", not `{other}`"
                        )));
                    }
                };
                settle(lua);
                let Some(editor) = editor_of(&e) else {
                    return Ok(false);
                };
                if !(line.is_finite() && line >= 1.) {
                    return Ok(false);
                }
                off_clock(lua, |_| editor.scroll_to_line(line as usize - 1, align));
                Ok(true)
            },
        )?,
    )?;
    table.set_readonly(true);
    ui.set("editor", table)
}

impl UserData for ElementUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, e| Ok(e.0.id.as_deref().map(str::to_owned)));
        fields.add_field_method_get("tag", |_, e| Ok(e.0.tag));
        fields.add_field_method_get("mounted", |_, e| Ok(e.0.is_mounted()));
        fields.add_field_method_get("width", |_, e| Ok(e.0.read(|s, id| s.bounds(id).width)));
        fields.add_field_method_get("height", |_, e| Ok(e.0.read(|s, id| s.bounds(id).height)));
        fields.add_field_method_get("x", |_, e| Ok(e.0.read(|s, id| s.bounds(id).x)));
        fields.add_field_method_get("y", |_, e| Ok(e.0.read(|s, id| s.bounds(id).y)));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Keyboard focus to the node or its first focusable descendant.
        methods.add_method("focus", |_, e, ()| {
            Ok(e.0.handle.get().is_some_and(NodeHandle::focus))
        });
        // A list's row in view: `el:scroll_to_row(i, { align = "center" })`,
        // `i` from 1, `align` start, center, end or nearest (the default).
        methods.add_method(
            "scroll_to_row",
            |_, e, (index, options): (f64, Option<mlua::Table>)| {
                use zgx::compose::RowAlign;
                let align = match options {
                    Some(options) => options.get::<Option<String>>("align")?,
                    None => None,
                };
                let align = match align.as_deref() {
                    None | Some("nearest") => RowAlign::Nearest,
                    Some("start") => RowAlign::Start,
                    Some("center") => RowAlign::Center,
                    Some("end") => RowAlign::End,
                    Some(other) => {
                        return Err(mlua::Error::runtime(format!(
                            "scroll_to_row's align is \"start\", \"center\", \"end\" or \"nearest\", not {other:?}"
                        )));
                    }
                };
                if !(index.is_finite() && index >= 1.) {
                    return Ok(false);
                }
                let row = index as usize - 1;
                Ok(e.0
                    .handle
                    .get()
                    .is_some_and(|list| list.scroll_to_row(row, align)))
            },
        );
        // An input's selection, in characters (Unicode code points, from
        // 0, as `utf8.len` counts them): `el:select_all()`,
        // `el:set_selection(a, b)` (`b` defaults to `a`, a caret) and
        // `local a, b = el:selection()` (nil for other elements). What a new
        // selection lays out and colours again (a long document shown at its
        // end, every line above highlighted first) is off the clock, as an
        // edit's is.
        methods.add_method("select_all", |lua, e, ()| {
            settle(lua);
            let handle = e.0.handle.get();
            Ok(off_clock(lua, |_| handle.is_some_and(NodeHandle::select_all)).unwrap_or(false))
        });
        methods.add_method(
            "set_selection",
            |lua, e, (anchor, focus): (i64, Option<i64>)| {
                settle(lua);
                let Some(editor) = e.0.handle.get().and_then(NodeHandle::editor) else {
                    return Ok(false);
                };
                let (anchor, focus) = {
                    let text = editor.editor.borrow();
                    let whole = text.text();
                    let byte = |chars: i64| byte_offset(&whole, chars.max(0) as usize);
                    (byte(anchor), byte(focus.unwrap_or(anchor)))
                };
                off_clock(lua, |_| editor.set_selection(anchor, focus));
                Ok(true)
            },
        );
        methods.add_method("selection", |lua, e, ()| {
            settle(lua);
            let Some(editor) = e.0.handle.get().and_then(NodeHandle::editor) else {
                return Ok((None, None));
            };
            let (anchor, focus) = editor.selection();
            let text = editor.editor.borrow();
            let chars = |byte: usize| text.slice(0..byte).chars().count();
            Ok((Some(chars(anchor)), Some(chars(focus))))
        });
        // In bytes, as `string.find` counts them (a code editor's search):
        // `el:set_selection_bytes(a, b)` (from 0: `string.find`'s match
        // `s, e` is `s - 1, e`), `local a, b = el:selection_bytes()`. A
        // selection set this way scrolls into view, focused or not.
        methods.add_method(
            "set_selection_bytes",
            |lua, e, (anchor, focus): (i64, Option<i64>)| {
                settle(lua);
                let Some(editor) = e.0.handle.get().and_then(NodeHandle::editor) else {
                    return Ok(false);
                };
                let byte = |b: i64| b.max(0) as usize;
                let (anchor, focus) = (byte(anchor), byte(focus.unwrap_or(anchor)));
                off_clock(lua, |_| editor.set_selection(anchor, focus));
                Ok(true)
            },
        );
        methods.add_method("selection_bytes", |lua, e, ()| {
            settle(lua);
            let Some(editor) = e.0.handle.get().and_then(NodeHandle::editor) else {
                return Ok((None, None));
            };
            let (anchor, focus) = editor.selection();
            Ok((Some(anchor), Some(focus)))
        });
        // The selection replaced by `text`, as typing it would (one step of
        // undo): a search's Replace, a snippet.
        methods.add_method("replace_selection", |lua, e, text: String| {
            settle(lua);
            let Some(editor) = e.0.handle.get().and_then(NodeHandle::editor) else {
                return Ok(false);
            };
            off_clock(lua, |_| editor.paste(&text));
            // The state it is bound to holds the new text when this returns.
            settle(lua);
            Ok(true)
        });
        // A picture of the element as the window last showed it, at most
        // `max_width` pixels wide (480 if not given): the `src` of an image
        // showing it, replaced on each call; nil if it is not laid out, or
        // the host takes none. Native views in it show as what is under them.
        methods.add_method("snapshot", |lua, e, max_width: Option<u32>| {
            let Some(handle) = e.0.handle.get() else {
                return Ok(None);
            };
            let Some(image) = handle.snapshot(max_width.unwrap_or(480)) else {
                return Ok(None);
            };
            let env = crate::node::env(lua)?;
            let name = format!("zgx.element:{:x}", Rc::as_ptr(&e.0) as usize);
            // Kept while its node is: a picture of what is gone goes too.
            if !env.assets.is_hosted(&name) {
                handle.retain(crate::lower::HostedPicture(
                    Rc::downgrade(&env.assets),
                    name.clone(),
                ));
            }
            Ok(Some(env.host_image(&name, image)))
        });
        methods.add_method("has_class", |_, e, class: String| {
            Ok(e.0.classes.iter().any(|c| **c == *class))
        });
        methods.add_method("blur", |_, e, ()| {
            if let Some(handle) = e.0.handle.get() {
                handle.blur();
            }
            Ok(())
        });
    }
}
