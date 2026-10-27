//! Native platform views (a web view, an `NSView`) laid out as zgx nodes.
//!
//! A native view is a node like any other: layout gives it a box, ancestors
//! clip it (rectangles, rounded corners, clip insets), transforms, scrolling
//! and opacity move and fade it, and hiding or unmounting it hides the view.
//! The platform composites the view's own pixels, so zgx content that paints
//! after it (a popover, a menu, a floating composer) must be presented on a
//! plane above it. [`PlaneSplit`] decides, from the paint order, which nodes'
//! paint goes on that overlay plane: only paint that comes after a native view
//! and overlaps it, or overlaps paint already on the overlay, so content
//! elsewhere keeps rendering (and blurring) as it does without native views.
//! Overlay glass cannot sample the platform's pixels; each overlay node with a
//! backdrop blur is reported as a [`BackdropPlacement`] for the platform's own
//! backdrop. Pointer input over a native view goes to it unless zgx content is
//! drawn there or an interactive zgx node is in front ([`PlaneSplit::route`]).
//!
//! Everything here is platform independent and headless: the desktop host
//! (`zgx-desktop`) maps placements onto native views and layers.
use crate::{
    affine::Projective,
    input::InputDispatcher,
    scene::{NodeId, NodeKind, PaintItem, Rect, RoundRect, Scene},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::{Rc, Weak},
};

/// What a native node shows.
#[derive(Clone)]
pub enum NativeContent {
    /// A web page (see [`crate::web_view`]).
    Web(crate::web_view::WebView),
    /// A terminal session (see [`crate::terminal`]).
    Terminal(crate::terminal::Terminal),
    /// A platform view the host knows how to mount: on macOS a
    /// `Retained<NSView>` (see `zgx_desktop::native_view`).
    Platform(Rc<dyn Any>),
}
impl NativeContent {
    /// A short description for placeholders and diagnostics.
    pub fn label(&self) -> String {
        match self {
            Self::Web(web) => {
                let url = web.state().with_untracked(|state| state.url.clone());
                if url.is_empty() {
                    "Web view".into()
                } else {
                    format!("Web view · {url}")
                }
            }
            Self::Terminal(term) => {
                let title = term.state().with_untracked(|state| state.title.clone());
                if title.is_empty() {
                    "Terminal".into()
                } else {
                    format!("Terminal · {title}")
                }
            }
            Self::Platform(_) => "Native view".into(),
        }
    }
}

struct Entry {
    token: u64,
    node: NodeId,
    content: NativeContent,
}
#[derive(Default)]
struct Inner {
    entries: RefCell<Vec<Entry>>,
    next: Cell<u64>,
    revision: Cell<u64>,
    supported: Cell<bool>,
    split: RefCell<Rc<PlaneSplit>>,
}
/// A window's mounted native views: provided to components by the desktop
/// host (`WindowContext::render`), read by it every frame.
#[derive(Clone, Default)]
pub struct NativeViews(Rc<Inner>);
/// Keeps a native node registered; dropping it (the node unmounting) removes it.
pub struct NativeRegistration {
    views: Weak<Inner>,
    token: u64,
}
impl Drop for NativeRegistration {
    fn drop(&mut self) {
        if let Some(inner) = self.views.upgrade() {
            let removed = {
                let mut entries = inner.entries.borrow_mut();
                let before = entries.len();
                entries.retain(|entry| entry.token != self.token);
                before != entries.len()
            };
            if removed {
                inner.revision.set(inner.revision.get() + 1);
                wake();
            }
        }
    }
}
impl NativeViews {
    /// No native views, and not supported until `set_supported`.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether this window composites native views itself. Without it (the
    /// software renderer, headless tests, platforms without a backend yet),
    /// native nodes render a labelled placeholder box instead.
    pub fn set_supported(&self, supported: bool) {
        self.0.supported.set(supported);
    }
    /// Whether this window composites native views itself (see `set_supported`).
    pub fn supported(&self) -> bool {
        self.0.supported.get()
    }
    /// Register `node` as showing `content`; it stays registered while the
    /// registration lives.
    pub fn register(&self, node: NodeId, content: NativeContent) -> NativeRegistration {
        let token = self.0.next.get() + 1;
        self.0.next.set(token);
        self.0.entries.borrow_mut().push(Entry {
            token,
            node,
            content,
        });
        self.0.revision.set(self.0.revision.get() + 1);
        wake();
        NativeRegistration {
            views: Rc::downgrade(&self.0),
            token,
        }
    }
    /// Mounted native nodes and what they show, in registration order.
    pub fn views(&self) -> Vec<(NodeId, NativeContent)> {
        self.0
            .entries
            .borrow()
            .iter()
            .map(|entry| (entry.node, entry.content.clone()))
            .collect()
    }
    /// The mounted native nodes, in registration order.
    pub fn nodes(&self) -> Vec<NodeId> {
        self.0
            .entries
            .borrow()
            .iter()
            .map(|entry| entry.node)
            .collect()
    }
    /// What `node` shows, if it is a registered native node.
    pub fn content(&self, node: NodeId) -> Option<NativeContent> {
        self.0
            .entries
            .borrow()
            .iter()
            .find(|entry| entry.node == node)
            .map(|entry| entry.content.clone())
    }
    /// Whether `node` is a registered native node.
    pub fn contains(&self, node: NodeId) -> bool {
        self.0
            .entries
            .borrow()
            .iter()
            .any(|entry| entry.node == node)
    }
    /// Whether no native node is mounted.
    pub fn is_empty(&self) -> bool {
        self.0.entries.borrow().is_empty()
    }
    /// Changes whenever a native node mounts or unmounts.
    pub fn revision(&self) -> u64 {
        self.0.revision.get()
    }
    /// The split of the frame last presented (see [`PlaneSplit::route`]).
    pub fn split(&self) -> Rc<PlaneSplit> {
        self.0.split.borrow().clone()
    }
    /// Record the split of the frame just presented (the desktop host calls this).
    pub fn set_split(&self, split: Rc<PlaneSplit>) {
        *self.0.split.borrow_mut() = split;
    }
}

type Waker = (u64, Rc<dyn Fn()>);
thread_local! {
    static POSTED: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
    static WAKERS: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
    static NEXT_WAKER: Cell<u64> = const { Cell::new(0) };
}
/// Run `delivery` at the next safe point of the UI thread's event loop
/// ([`deliver_posted`]), waking it. Platform callbacks (a navigation
/// delegate, a script reply) can arrive while zgx is mid-dispatch or holds
/// the scene; they post their state changes rather than applying them.
pub fn post(delivery: impl FnOnce() + 'static) {
    POSTED.with(|posted| posted.borrow_mut().push_back(Box::new(delivery)));
    wake();
}
/// Run everything [`post`]ed so far (and anything those post). Returns
/// whether anything ran. Hosts call it where no zgx state is borrowed.
pub fn deliver_posted() -> bool {
    let mut ran = false;
    // Bounded: a delivery posting another delivers it on the next pass.
    for _ in 0..64 {
        let batch = POSTED.with(|posted| std::mem::take(&mut *posted.borrow_mut()));
        if batch.is_empty() {
            break;
        }
        ran = true;
        for delivery in batch {
            delivery();
        }
    }
    ran
}
/// Wake hosts' event loops (see [`set_waker`]).
pub fn wake() {
    let wakers: Vec<_> =
        WAKERS.with(|wakers| wakers.borrow().iter().map(|(_, w)| w.clone()).collect());
    for waker in wakers {
        waker();
    }
}
/// Install a function waking a host's event loop when something is posted.
pub fn set_waker(waker: impl Fn() + 'static) -> WakerGuard {
    let id = NEXT_WAKER.with(|next| {
        let id = next.get() + 1;
        next.set(id);
        id
    });
    WAKERS.with(|wakers| wakers.borrow_mut().push((id, Rc::new(waker))));
    WakerGuard(id)
}
/// Removes its waker when dropped.
pub struct WakerGuard(u64);
impl Drop for WakerGuard {
    fn drop(&mut self) {
        let id = self.0;
        let _ = WAKERS.try_with(|wakers| wakers.borrow_mut().retain(|(other, _)| *other != id));
    }
}

/// Which presentation plane a renderer draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    /// Under native views: everything not on the overlay.
    Base,
    /// Above native views.
    Overlay,
}

/// Where a native view goes this frame, in window (logical) coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct NativePlacement {
    /// The native node.
    pub node: NodeId,
    /// The view's box on screen: its laid-out box under its transforms (the
    /// covering box when rotated or skewed).
    pub frame: Rect,
    /// Its laid-out size. Differs from `frame`'s when an ancestor scales it:
    /// the view keeps this size and is drawn scaled.
    pub size: (f32, f32),
    /// The rectangular clip of its ancestors (clip insets included).
    pub clip: Option<Rect>,
    /// Rounded clips around it, innermost first: its own corners first when
    /// it has a radius (an embedded page is cut to its box's corners, as a
    /// CSS iframe is), then those of rounded clipping ancestors.
    pub rounded: Vec<RoundRect>,
    /// Its effective opacity (ancestors' included).
    pub opacity: f32,
    /// Paint order among native views, lowest first.
    pub order: usize,
}
impl NativePlacement {
    /// The part of `frame` its clips leave visible, if any.
    pub fn visible(&self) -> Option<Rect> {
        match self.clip {
            Some(clip) => self.frame.intersection(clip),
            None => (self.frame.width > 0. && self.frame.height > 0.).then_some(self.frame),
        }
    }
}
/// An overlay node with a backdrop blur: the platform blurs what lies under
/// it (native views included) inside its rounded box, and the overlay plane
/// draws the node's own tint, border and content on top.
#[derive(Clone, Debug, PartialEq)]
pub struct BackdropPlacement {
    /// The overlay node with the blur.
    pub node: NodeId,
    /// The blurred box in window coordinates.
    pub rect: Rect,
    /// Its corner radii (top-left, top-right, bottom-right, bottom-left).
    pub radii: [f32; 4],
    /// The rectangular clip of its ancestors.
    pub clip: Option<Rect>,
    /// Ancestors' rounded clips, innermost first.
    pub rounded: Vec<RoundRect>,
    /// Gaussian sigma in window points (scaled by transformed ancestors).
    pub sigma: f32,
    /// Its effective opacity (ancestors' included).
    pub opacity: f32,
}
impl BackdropPlacement {
    /// The part of `rect` its clip leaves visible, if any.
    pub fn visible(&self) -> Option<Rect> {
        match self.clip {
            Some(clip) => self.rect.intersection(clip),
            None => (self.rect.width > 0. && self.rect.height > 0.).then_some(self.rect),
        }
    }
}

/// Where pointer input at a point goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerRoute {
    /// The native view of this node.
    Native(NodeId),
    /// zgx (its own hit testing decides the target, if any).
    Ui,
}

/// What zgx hears of a pointer move, by where it went (see [`NativePointer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerStep {
    /// The move, as ever (over zgx, or held by a press of zgx's).
    Move,
    /// The pointer just came over the native view of this node, which takes
    /// input there: a `PointerLeave`, so zgx's hovered nodes have theirs.
    Leave(NodeId),
    /// Still over that native view: nothing.
    Over(NodeId),
}
/// The pages to tell where the pointer is after a move (see
/// [`NativePointer::moved`]): apply with [`PageReport::apply`].
#[derive(Default)]
pub struct PageReport {
    /// A page the pointer left: its pointer is `None` now.
    pub left: Option<crate::web_view::WebView>,
    /// The page under the pointer and the point, from its top left.
    pub at: Option<(crate::web_view::WebView, (f32, f32))>,
}
impl PageReport {
    /// Whether no page needs telling.
    pub fn is_empty(&self) -> bool {
        self.left.is_none() && self.at.is_none()
    }
    /// Set the pages' pointers (from a delivery or the host's own event
    /// handling: where no zgx state is borrowed).
    pub fn apply(self) {
        if let Some(web) = self.left {
            web.set_pointer(None);
        }
        if let Some((web, point)) = self.at {
            web.set_pointer(Some(point));
        }
    }
}
/// The pointer over a window's native views, as its host follows it.
///
/// Over a native view that takes input there (see [`PlaneSplit::route`]),
/// zgx hears one `PointerLeave` and then nothing until the pointer is back
/// over zgx, so whatever it hovered has its leave (the DOM's semantics: a
/// page is no part of the zgx nodes around it). Pages hear where the
/// pointer is over their visible box (zgx content drawn on them included),
/// as [`crate::web_view::WebView::pointer`]; terminals report their own.
/// Costs nothing in a window without native views.
#[derive(Default)]
pub struct NativePointer {
    over: Cell<Option<NodeId>>,
    page: RefCell<Option<crate::web_view::WeakWebView>>,
}
impl NativePointer {
    /// Not over any native view.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether the pointer is over a native view taking input (zgx had its
    /// leave): the native view shows its own cursor.
    pub fn over(&self) -> Option<NodeId> {
        self.over.get()
    }
    /// The pointer moved to window point `(x, y)`, where `route` sends
    /// input; `held`: a press of zgx's follows the pointer (it hears every
    /// move, wherever). What zgx hears, and the pages to tell.
    pub fn moved(
        &self,
        natives: &NativeViews,
        route: PointerRoute,
        held: bool,
        x: f32,
        y: f32,
    ) -> (PointerStep, PageReport) {
        let report = self.report(natives, x, y);
        let step = match route {
            PointerRoute::Native(node) if !held => {
                if self.over.replace(Some(node)).is_some() {
                    PointerStep::Over(node)
                } else {
                    PointerStep::Leave(node)
                }
            }
            _ => {
                self.over.set(None);
                PointerStep::Move
            }
        };
        (step, report)
    }
    /// The pointer left the window (or the window went to the back): no
    /// native view has it, no page does.
    pub fn left(&self) -> PageReport {
        self.over.set(None);
        PageReport {
            left: self.page.take().and_then(|page| page.upgrade()),
            at: None,
        }
    }
    fn report(&self, natives: &NativeViews, x: f32, y: f32) -> PageReport {
        let split = natives.split();
        // The front-most page whose visible box holds the point.
        let under = split.natives().iter().rev().find_map(|placement| {
            let visible = placement.visible()?;
            let inside = x >= visible.x
                && y >= visible.y
                && x < visible.x + visible.width
                && y < visible.y + visible.height;
            if !inside {
                return None;
            }
            match natives.content(placement.node)? {
                NativeContent::Web(web) => {
                    Some((web, (x - placement.frame.x, y - placement.frame.y)))
                }
                _ => None,
            }
        });
        let mut page = self.page.borrow_mut();
        let previous = page.as_ref().and_then(|page| page.upgrade());
        let left = previous.filter(|old| under.as_ref().is_none_or(|(web, _)| web != old));
        *page = under.as_ref().map(|(web, _)| web.downgrade());
        PageReport {
            left,
            // A page nothing follows the pointer over hears nothing: its
            // host need not wake for it.
            at: under.filter(|(web, _)| web.wants_pointer()),
        }
    }
}

/// One frame's division of paint between the base plane (under native
/// views) and the overlay plane (above them), with native views' and overlay
/// backdrops' placements. Empty (no native views) costs nothing.
#[derive(Clone, Debug, Default)]
pub struct PlaneSplit {
    natives: Vec<NativePlacement>,
    native_nodes: HashSet<NodeId>,
    /// Nodes whose own paint is on the overlay, with where it lands.
    overlay: HashMap<NodeId, Rect>,
    /// Isolated (layer) subtrees holding overlay paint, and base paint.
    overlay_layers: HashSet<NodeId>,
    base_layers: HashSet<NodeId>,
    /// Overlay backdrop blurs with overlay paint under them (the overlay's
    /// own blur pass is needed there; elsewhere the platform's suffices).
    blur_needed: HashSet<NodeId>,
    backdrops: Vec<BackdropPlacement>,
    overlay_bounds: Option<Rect>,
    /// Layers whose split changed since the previous frame: renderers drop
    /// their cached textures.
    changed_layers: Vec<NodeId>,
}

/// The overlay region is kept as at most this many rectangles; beyond, the
/// two nearest merge (conservative: more paint may join the overlay).
const REGION_LIMIT: usize = 48;

struct Walk<'a> {
    scene: &'a Scene,
    natives: &'a HashSet<NodeId>,
    previous: &'a PlaneSplit,
    /// Visible rectangles of native views painted so far.
    native_rects: Vec<Rect>,
    /// Where overlay paint lands so far.
    region: Vec<Rect>,
    split: PlaneSplit,
    damage: Vec<Rect>,
    changed: HashSet<NodeId>,
    /// Overlay paint last frame, visited on the base plane now.
    left: HashSet<NodeId>,
}
#[derive(Clone)]
struct Context {
    /// From the items' space to the window's (None: the identity).
    matrix: Option<Projective>,
    opacity: f32,
    clip: Option<Rect>,
    rounded: Vec<RoundRect>,
    /// Isolated ancestors, outermost first.
    layers: Vec<NodeId>,
}
fn map_rect(matrix: Option<Projective>, rect: Rect) -> Rect {
    match matrix {
        Some(matrix) => matrix.bounds(rect),
        None => rect,
    }
}
/// A scale-and-translate matrix's scales, if it is one (rounded clips and
/// radii map exactly only through those).
fn axis_scales(matrix: Option<Projective>) -> Option<(f32, f32)> {
    match matrix.map(Projective::affine) {
        None => Some((1., 1.)),
        Some(Some(m)) if m.b.abs() < 1e-5 && m.c.abs() < 1e-5 && m.a > 0. && m.d > 0. => {
            Some((m.a, m.d))
        }
        Some(_) => None,
    }
}
/// Its linear part's mean scale (at the origin, in perspective).
fn magnification(matrix: Option<Projective>) -> f32 {
    match matrix {
        None => 1.,
        Some(m) => ((m.a * m.d - m.b * m.c).abs()).sqrt() / m.pw.abs().max(1e-6),
    }
}
fn map_round(matrix: Option<Projective>, round: RoundRect) -> Option<RoundRect> {
    let (sx, sy) = axis_scales(matrix)?;
    let scale = sx.min(sy);
    Some(RoundRect {
        rect: map_rect(matrix, round.rect),
        radii: round.radii.map(|radius| radius * scale),
        feather: crate::scene::Feather::new(round.feather.points() * scale),
    })
}
fn intersect(clip: Option<Rect>, other: Option<Rect>) -> Option<Rect> {
    match (clip, other) {
        (Some(a), Some(b)) => Some(a.intersection(b).unwrap_or(Rect::new(a.x, a.y, 0., 0.))),
        (a, b) => a.or(b),
    }
}
/// A box's corner radii, clamped as the renderer clamps them.
pub fn corner_radii(kind: &NodeKind, bounds: Rect) -> [f32; 4] {
    let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = kind else {
        return [0.; 4];
    };
    let corners = quad
        .decoration
        .as_ref()
        .and_then(|detail| detail.corners)
        .unwrap_or(crate::decoration::Corners::all(quad.radius));
    let most = bounds.width.min(bounds.height).max(0.) / 2.;
    [
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ]
    .map(|radius| {
        if radius.is_finite() {
            radius.clamp(0., most)
        } else {
            0.
        }
    })
}
/// Whether an item draws anything of its own.
fn paints(item: &PaintItem<'_>) -> bool {
    if item.effects.blur_radius > 0. {
        return true;
    }
    match item.kind {
        NodeKind::Container(_) => false,
        NodeKind::Rect(color) => color.3 > 0,
        NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => {
            quad.fill.3 > 0
                || quad.has_border()
                || !quad.shadows().is_empty()
                || quad
                    .decoration
                    .as_ref()
                    .is_some_and(|detail| detail.background.is_some())
        }
        _ => true,
    }
}
/// Where an item's own paint can land, in its space.
fn paint_bounds(item: &PaintItem<'_>) -> Rect {
    match item.kind {
        NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad.paint_bounds(item.bounds),
        NodeKind::Svg(svg) => svg.paint_bounds(item.bounds),
        NodeKind::Image(image) => image.paint_bounds(item.bounds),
        // Glyphs can overhang their box a little.
        NodeKind::Text { .. } | NodeKind::RichText { .. } => item.bounds.expand(2.),
        _ => item.bounds,
    }
}
impl Walk<'_> {
    fn overlaps(&self, rect: Rect) -> bool {
        self.native_rects
            .iter()
            .any(|native| native.intersects(rect))
            || self.region.iter().any(|painted| painted.intersects(rect))
    }
    fn add_region(&mut self, rect: Rect) {
        let holds = |held: &Rect| {
            held.x <= rect.x
                && held.y <= rect.y
                && held.x + held.width >= rect.x + rect.width
                && held.y + held.height >= rect.y + rect.height
        };
        if self.region.iter().any(holds) {
            return;
        }
        self.region.push(rect);
        if self.region.len() > REGION_LIMIT {
            // Merge the pair whose union wastes the least area.
            let mut best = (f32::INFINITY, 0, 1);
            for i in 0..self.region.len() {
                for j in i + 1..self.region.len() {
                    let (a, b) = (self.region[i], self.region[j]);
                    let union = a.union(b);
                    let waste =
                        union.width * union.height - a.width * a.height - b.width * b.height;
                    if waste < best.0 {
                        best = (waste, i, j);
                    }
                }
            }
            let (_, i, j) = best;
            let merged = self.region[i].union(self.region[j]);
            self.region.swap_remove(j);
            self.region[i] = merged;
        }
    }
    /// Walk `items` (one layer's, in paint order); returns whether they hold
    /// base paint and overlay paint.
    fn walk(&mut self, items: &[PaintItem<'_>], context: &Context) -> (bool, bool) {
        let (mut base, mut overlay) = (false, false);
        for item in items {
            if item.border_pass {
                continue;
            }
            let clip = intersect(
                context.clip,
                item.clip.map(|clip| map_rect(context.matrix, clip)),
            );
            let opacity = context.opacity * item.effects.opacity;
            let mut rounded: Vec<RoundRect> = item
                .rounded()
                .iter()
                .flatten()
                .filter_map(|round| map_round(context.matrix, *round))
                .collect();
            rounded.extend(context.rounded.iter().copied());
            if item.isolated {
                let inner = self.scene.layer_items(Some(item.id));
                let matrix = match (item.projective(), context.matrix) {
                    (Some(own), Some(outer)) => Some(own.then(outer)),
                    (own, outer) => own.or(outer),
                };
                let mut layers = context.layers.clone();
                layers.push(item.id);
                let child = Context {
                    matrix,
                    opacity,
                    clip,
                    rounded,
                    layers,
                };
                let (inner_base, inner_overlay) = self.walk(&inner, &child);
                if inner_base {
                    self.split.base_layers.insert(item.id);
                }
                if inner_overlay {
                    self.split.overlay_layers.insert(item.id);
                }
                let was = (
                    self.previous.base_layers.contains(&item.id),
                    self.previous.overlay_layers.contains(&item.id),
                );
                if was != (inner_base, inner_overlay) {
                    self.changed.insert(item.id);
                }
                base |= inner_base;
                overlay |= inner_overlay;
                continue;
            }
            let frame = map_rect(context.matrix, item.bounds);
            if self.natives.contains(&item.id) {
                let mut own = Vec::with_capacity(rounded.len() + 1);
                let radii = corner_radii(item.kind, item.bounds);
                if radii.iter().any(|radius| *radius > 0.)
                    && let Some(round) = map_round(
                        context.matrix,
                        RoundRect {
                            rect: item.bounds,
                            radii,
                            feather: Default::default(),
                        },
                    )
                {
                    own.push(round);
                }
                own.extend(rounded.iter().copied());
                let placement = NativePlacement {
                    node: item.id,
                    frame,
                    size: (item.bounds.width, item.bounds.height),
                    clip,
                    rounded: own,
                    opacity,
                    order: self.split.natives.len(),
                };
                if let Some(visible) = placement.visible() {
                    self.native_rects.push(visible);
                }
                self.split.native_nodes.insert(item.id);
                self.split.natives.push(placement);
                // Its own background paints under it.
                if paints(item) {
                    base = true;
                }
                continue;
            }
            if !paints(item) {
                continue;
            }
            let painted = map_rect(context.matrix, paint_bounds(item));
            let Some(painted) = (match clip {
                Some(clip) => painted.intersection(clip),
                None => Some(painted),
            }) else {
                continue;
            };
            let on_overlay = !self.native_rects.is_empty() && self.overlaps(painted);
            let was = self.previous.overlay.get(&item.id).copied();
            if on_overlay {
                if item.effects.blur_radius > 0. {
                    let area = frame;
                    if self.region.iter().any(|held| held.intersects(area)) {
                        self.split.blur_needed.insert(item.id);
                    }
                    let radii = corner_radii(item.kind, item.bounds);
                    let scale = axis_scales(context.matrix).map_or(1., |(sx, sy)| sx.min(sy));
                    self.split.backdrops.push(BackdropPlacement {
                        node: item.id,
                        rect: frame,
                        radii: radii.map(|radius| radius * scale),
                        clip,
                        rounded: rounded.clone(),
                        sigma: item.effects.blur_radius * magnification(context.matrix),
                        opacity,
                    });
                }
                self.split.overlay.insert(item.id, painted);
                self.split.overlay_bounds = Some(match self.split.overlay_bounds {
                    Some(bounds) => bounds.union(painted),
                    None => painted,
                });
                self.add_region(painted);
                overlay = true;
                if was.is_none() {
                    self.damage.push(painted);
                    self.changed.extend(context.layers.iter().copied());
                }
            } else {
                base = true;
                if let Some(was) = was {
                    self.left.insert(item.id);
                    self.damage.push(was);
                    self.damage.push(painted);
                    self.changed.extend(context.layers.iter().copied());
                }
            }
        }
        (base, overlay)
    }
}

impl PlaneSplit {
    /// Split `scene`'s paint around the native views among `natives` (other
    /// nodes are ignored), given the previous frame's split. Returns the split
    /// and the damage its changes add: where paint moved between planes, both
    /// planes repaint. Call after the scene's layout is up to date.
    pub fn compute(scene: &Scene, natives: &[NodeId], previous: &PlaneSplit) -> (Self, Vec<Rect>) {
        let natives: HashSet<NodeId> = natives
            .iter()
            .copied()
            .filter(|node| scene.contains(*node))
            .collect();
        if natives.is_empty() {
            let damage = previous.overlay.values().copied().collect();
            let split = Self {
                changed_layers: previous
                    .overlay_layers
                    .iter()
                    .chain(&previous.base_layers)
                    .copied()
                    .collect(),
                ..Self::default()
            };
            return (split, damage);
        }
        // Only paint over native views or over overlay paint can join the
        // overlay, so the walk visits subtrees reaching those places: where
        // they were last frame, widened until this frame's are all inside
        // (a fixed point; the last attempt walks everything).
        let viewport = scene.bounds(scene.root());
        let mut reach: Vec<Rect> = previous
            .natives
            .iter()
            .filter_map(NativePlacement::visible)
            .chain(previous.overlay.values().copied())
            .collect();
        if reach.is_empty() {
            reach.push(viewport);
        }
        let mut attempt = 0;
        let mut walk = loop {
            let mut walk = Walk {
                scene,
                natives: &natives,
                previous,
                native_rects: Vec::new(),
                region: Vec::new(),
                split: Self::default(),
                damage: Vec::new(),
                changed: HashSet::new(),
                left: HashSet::new(),
            };
            let everything = attempt == 3;
            let items = match everything {
                true => scene.layer_items(None),
                false => scene.layer_items_within(None, Some(&reach)),
            };
            walk.walk(
                &items,
                &Context {
                    matrix: None,
                    opacity: 1.,
                    clip: None,
                    rounded: Vec::new(),
                    layers: Vec::new(),
                },
            );
            if everything {
                break walk;
            }
            let covered = |rect: &Rect| {
                reach.iter().any(|held| {
                    held.x <= rect.x
                        && held.y <= rect.y
                        && held.x + held.width >= rect.x + rect.width
                        && held.y + held.height >= rect.y + rect.height
                })
            };
            // Natives on screen the walk missed (moved out of last frame's
            // reach); one missed inside it has no ink (not marked
            // `Scene::set_platform_painted`): only a full walk finds it.
            let missed: Vec<Rect> = natives
                .iter()
                .filter(|node| !walk.split.native_nodes.contains(node))
                .filter(|node| scene.layout_visible(**node))
                .filter_map(|node| scene.visible_bounds(*node))
                .filter_map(|bounds| bounds.intersection(viewport))
                .collect();
            if missed.iter().any(covered) {
                attempt = 3;
                continue;
            }
            let wanted: Vec<Rect> = walk
                .native_rects
                .iter()
                .copied()
                .chain(walk.split.overlay.values().copied())
                .chain(missed)
                .filter(|rect| !covered(rect))
                .collect();
            if wanted.is_empty() {
                break walk;
            }
            reach.extend(wanted);
            if reach.len() > REGION_LIMIT {
                reach = vec![
                    reach
                        .iter()
                        .copied()
                        .reduce(Rect::union)
                        .unwrap_or(viewport),
                ];
            }
            attempt += 1;
        };
        let mut split = std::mem::take(&mut walk.split);
        let mut damage = std::mem::take(&mut walk.damage);
        // Paint that left the overlay unvisited (it moved out of reach, or
        // is gone): the overlay erases it, the base draws it where it is now.
        for (node, rect) in &previous.overlay {
            if split.overlay.contains_key(node) || walk.left.contains(node) {
                continue;
            }
            damage.push(*rect);
            if scene.contains(*node) {
                damage.extend(scene.visible_bounds(*node));
                walk.changed
                    .extend(scene.ancestors(*node).filter(|id| scene.is_layer(*id)));
            }
        }
        // Layers that no longer exist need nothing; those still split
        // differently were collected.
        split.changed_layers = std::mem::take(&mut walk.changed).into_iter().collect();
        (split, damage)
    }
    /// Whether there are native views at all this frame.
    pub fn is_empty(&self) -> bool {
        self.natives.is_empty()
    }
    /// Its placements in a zoomed window's points (`Ui::zoom`: `zoom`
    /// points to each logical unit), where the platform puts its views: a
    /// view takes the zoomed size and lays out in it, as a page does under
    /// a browser's zoom, rather than being drawn scaled. Renderers keep the
    /// split itself, in logical units.
    pub fn zoomed(&self, zoom: f32) -> Self {
        let rect = |r: Rect| Rect::new(r.x * zoom, r.y * zoom, r.width * zoom, r.height * zoom);
        let round = |r: &mut RoundRect| {
            r.rect = rect(r.rect);
            r.radii = r.radii.map(|radius| radius * zoom);
            r.feather = crate::scene::Feather::new(r.feather.points() * zoom);
        };
        let mut split = self.clone();
        for native in &mut split.natives {
            native.frame = rect(native.frame);
            native.size = (native.size.0 * zoom, native.size.1 * zoom);
            native.clip = native.clip.map(rect);
            native.rounded.iter_mut().for_each(round);
        }
        for backdrop in &mut split.backdrops {
            backdrop.rect = rect(backdrop.rect);
            backdrop.radii = backdrop.radii.map(|radius| radius * zoom);
            backdrop.clip = backdrop.clip.map(rect);
            backdrop.rounded.iter_mut().for_each(round);
            backdrop.sigma *= zoom;
        }
        split.overlay.values_mut().for_each(|r| *r = rect(*r));
        split.overlay_bounds = split.overlay_bounds.map(rect);
        split
    }
    /// Whether anything paints on the overlay plane.
    pub fn has_overlay(&self) -> bool {
        !self.overlay.is_empty()
    }
    /// This frame's native views' placements, in paint order.
    pub fn natives(&self) -> &[NativePlacement] {
        &self.natives
    }
    /// `node`'s placement this frame, if it is a native view.
    pub fn native(&self, node: NodeId) -> Option<&NativePlacement> {
        self.natives.iter().find(|placement| placement.node == node)
    }
    /// Whether `node` is a native view this frame.
    pub fn is_native(&self, node: NodeId) -> bool {
        self.native_nodes.contains(&node)
    }
    /// This frame's overlay backdrop blurs, for the platform to draw.
    pub fn backdrops(&self) -> &[BackdropPlacement] {
        &self.backdrops
    }
    /// Whether `node`'s own paint is on the overlay plane.
    pub fn in_overlay(&self, node: NodeId) -> bool {
        self.overlay.contains_key(&node)
    }
    /// The overlay paint's covering box in window coordinates.
    pub fn overlay_bounds(&self) -> Option<Rect> {
        self.overlay_bounds
    }
    /// Layers whose division changed since the previous split.
    pub fn changed_layers(&self) -> &[NodeId] {
        &self.changed_layers
    }
    /// Whether a renderer drawing `plane` draws `item` (from
    /// `Scene::layer_items`, at any depth): a layer when it holds paint of
    /// that plane, anything else when its own paint is on it.
    pub fn keeps(&self, item: &PaintItem<'_>, plane: Plane) -> bool {
        if self.natives.is_empty() {
            return plane == Plane::Base;
        }
        match (item.isolated, plane) {
            // A layer the split did not visit (out of reach of native views
            // and overlay paint) holds base paint only.
            (true, Plane::Base) => {
                self.base_layers.contains(&item.id) || !self.overlay_layers.contains(&item.id)
            }
            (true, Plane::Overlay) => self.overlay_layers.contains(&item.id),
            (false, Plane::Base) => !self.overlay.contains_key(&item.id),
            (false, Plane::Overlay) => self.overlay.contains_key(&item.id),
        }
    }
    /// Adjust an item a renderer of `plane` keeps: on the overlay, a backdrop
    /// blur over nothing of the overlay's own is the platform's alone.
    pub fn adjust(&self, item: &mut PaintItem<'_>, plane: Plane) {
        if plane == Plane::Overlay
            && !item.isolated
            && item.effects.blur_radius > 0.
            && !self.blur_needed.contains(&item.id)
        {
            item.effects.blur_radius = 0.;
        }
    }
    /// Keep (and adjust) the items a renderer of `plane` draws.
    pub fn filter(&self, items: &mut Vec<PaintItem<'_>>, plane: Plane) {
        if self.natives.is_empty() {
            if plane == Plane::Overlay {
                items.clear();
            }
            return;
        }
        items.retain(|item| self.keeps(item, plane));
        for item in items.iter_mut() {
            self.adjust(item, plane);
        }
    }
    /// Where pointer input at window point `(x, y)` goes: to a native view
    /// when zgx would target it there (see `InputDispatcher`'s hit testing:
    /// the front-most hit with a listener in its ancestry, the native node
    /// counting as one), unless overlay paint is drawn there first. Disabled
    /// or out-of-modal native views route to zgx (which ignores the input).
    /// Never panics while zgx state is borrowed; then answers `Ui`.
    pub fn route(&self, scene: &Scene, input: &InputDispatcher, x: f32, y: f32) -> PointerRoute {
        if self.natives.is_empty() {
            return PointerRoute::Ui;
        }
        let root = scene.root();
        'hits: for hit in scene.hit_test_all(x, y) {
            // Content the pointer passes through (`pointer_events = "none"`,
            // up to the first node with listeners) is not there for input,
            // even painted over the native view.
            let mut node = Some(hit);
            while let Some(id) = node {
                match input.try_options(id) {
                    None => return PointerRoute::Ui,
                    Some(Some(options)) if options.pass_through => continue 'hits,
                    _ => {}
                }
                if id == root || self.native_nodes.contains(&id) {
                    break;
                }
                match input.try_has_listeners(id) {
                    Some(false) => node = scene.parent(id),
                    _ => break,
                }
            }
            if self.overlay.contains_key(&hit) {
                return PointerRoute::Ui;
            }
            let mut node = Some(hit);
            while let Some(id) = node {
                if self.native_nodes.contains(&id) {
                    return match input.try_is_enabled(scene, id) {
                        Some(true) => PointerRoute::Native(id),
                        _ => PointerRoute::Ui,
                    };
                }
                match input.try_options(id) {
                    None => return PointerRoute::Ui,
                    Some(Some(options)) if options.disabled => return PointerRoute::Ui,
                    _ => {}
                }
                if id != root {
                    match input.try_has_listeners(id) {
                        Some(false) => {}
                        _ => return PointerRoute::Ui,
                    }
                } else {
                    break;
                }
                node = scene.parent(id);
            }
        }
        PointerRoute::Ui
    }
}
