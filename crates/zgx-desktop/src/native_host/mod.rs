//! A window's native views (web pages, platform views) composited with its
//! zgx content; see `zgx::native_view` for the model.
//!
//! Each frame, after layout: split the scene's paint into the base plane (the
//! window renderer, under native views) and the overlay plane (a second
//! renderer presenting into a layer above them, sized to the overlay paint);
//! then present both planes and move the native views and the platform
//! backdrops under overlay glass in one Core Animation transaction (macOS)
//! or DirectComposition commit (Windows, with the `webview2` feature), so
//! they never drift a frame apart. A window without native views does none
//! of it. On Linux (the `webkitgtk` feature) pages are images in the
//! window's own paint instead (`webkitgtk.rs`), and none of this applies.
#[cfg(all(windows, feature = "webview2"))]
mod dcomp;
#[cfg(all(target_os = "macos", feature = "ghostty"))]
mod ghostty;
#[cfg(all(target_os = "linux", feature = "webkitgtk"))]
mod gtk;
#[cfg(all(target_os = "linux", feature = "webkitgtk"))]
pub(crate) mod webkitgtk;
#[cfg(all(target_os = "macos", feature = "ghostty"))]
pub(crate) use ghostty::processes_changed;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod webkit;
#[cfg(all(windows, feature = "webview2"))]
mod webview2;
#[cfg(any(all(windows, feature = "webview2"), test))]
mod webview2_keys;

use std::rc::Rc;
use zgx::{
    native_view::{NativeViews, PlaneSplit, PointerStep},
    scene::{Rect, Scene},
    widgets::Ui,
};

#[cfg(target_os = "macos")]
pub use macos::{BackdropReport, NativeDiagnostics, NativeViewReport};
#[cfg(target_os = "macos")]
pub(crate) use macos::{disable_cursor_rects, set_cursor};
#[cfg(target_os = "macos")]
pub(crate) type NativeDiagnosticsHandle = NativeDiagnostics;
#[cfg(not(target_os = "macos"))]
pub(crate) type NativeDiagnosticsHandle = Rc<()>;

#[cfg(all(windows, feature = "webview2"))]
use dcomp as platform;
#[cfg(target_os = "macos")]
use macos as platform;

/// How long frames keep to the plain layer after native views move (see
/// `GpuRenderer::present_plain_for`): long enough to cover a scroll's
/// momentum or an animation's frames in between.
#[cfg(target_os = "macos")]
const SYNCED: std::time::Duration = std::time::Duration::from_millis(500);

pub(crate) struct NativeHost {
    pub natives: NativeViews,
    /// Shared with the window's context (`WindowContext::native_diagnostics`).
    pub diagnostics: NativeDiagnosticsHandle,
    split: Rc<PlaneSplit>,
    /// `split` in the zoomed window's points, where the platform places its
    /// views (`PlaneSplit::zoomed`); `split` itself at 100%.
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    placed: Rc<PlaneSplit>,
    /// The window's zoom (`Ui::zoom`) at the last plan.
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    zoom: f32,
    /// The window's DirectComposition tree, made with its renderer.
    #[cfg(all(windows, feature = "webview2"))]
    composition: Option<Rc<dcomp::Composition>>,
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    surfaces: Option<platform::Surfaces>,
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    overlay: Option<zgx_gpu::GpuRenderer>,
    /// The overlay renderer drew this frame (present it).
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    overlay_drawn: bool,
    /// The scale the overlay renderer and its layer were made for.
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    overlay_scale: f32,
    /// The overlay layer shows its plane (it had paint at the last present).
    #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
    overlay_live: bool,
    /// Native geometry changed and has not been applied yet.
    #[cfg_attr(
        not(any(target_os = "macos", all(windows, feature = "webview2"))),
        allow(dead_code)
    )]
    pending: bool,
    /// What clip views' hit tests consult (macOS's clip views).
    #[cfg_attr(
        not(any(target_os = "macos", all(windows, feature = "webview2"))),
        allow(dead_code)
    )]
    scene: std::rc::Weak<std::cell::RefCell<Scene>>,
    #[cfg_attr(
        not(any(target_os = "macos", all(windows, feature = "webview2"))),
        allow(dead_code)
    )]
    input: zgx::input::InputDispatcher,
    _waker: zgx::native_view::WakerGuard,
    /// Wakes the window when WebKitGTK's thread has something for it.
    #[cfg(all(target_os = "linux", feature = "webkitgtk"))]
    _pages_waker: webkitgtk::WakerGuard,
}

impl NativeHost {
    pub fn new(ui: &Ui, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let wake = std::sync::Arc::new(wake);
        let natives = NativeViews::new();
        // Pages go in DirectComposition visuals, which only Direct3D 12
        // presents into: a `WGPU_BACKEND` without it leaves them placeholders.
        #[cfg(all(windows, feature = "webview2"))]
        let supported = webview2::available()
            && std::env::var("WGPU_BACKEND").map_or(true, |backends| {
                let backends = backends.to_ascii_lowercase();
                backends.contains("dx12") || backends.contains("d3d12")
            });
        #[cfg(not(all(windows, feature = "webview2")))]
        let supported = cfg!(target_os = "macos");
        natives.set_supported(supported);
        Self {
            natives,
            diagnostics: Default::default(),
            split: Rc::new(PlaneSplit::default()),
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            placed: Rc::new(PlaneSplit::default()),
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            zoom: 1.,
            #[cfg(all(windows, feature = "webview2"))]
            composition: None,
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            surfaces: None,
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            overlay: None,
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            overlay_drawn: false,
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            overlay_scale: 0.,
            #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
            overlay_live: false,
            pending: false,
            scene: Rc::downgrade(&ui.scene),
            input: ui.input.clone(),
            #[cfg(all(target_os = "linux", feature = "webkitgtk"))]
            _pages_waker: webkitgtk::add_waker(wake.clone()),
            _waker: zgx::native_view::set_waker(move || wake()),
        }
    }
    /// The window's renderer, sharing `context` when given. On Windows with
    /// web views, it presents into the base visual of a DirectComposition
    /// tree of the window's own, so pages and the overlay plane can go above it.
    pub fn renderer_for(
        &mut self,
        window: std::sync::Arc<winit::window::Window>,
        context: Option<&zgx_gpu::GpuContext>,
        transparent: bool,
    ) -> Result<zgx_gpu::GpuRenderer, zgx_gpu::GpuError> {
        // A window takes one target: the last one goes first.
        #[cfg(all(windows, feature = "webview2"))]
        if self.natives.supported() {
            self.surfaces = None;
            self.composition = None;
            match dcomp::Composition::new(&window) {
                Ok(composition) => match zgx_gpu::GpuRenderer::for_window_visual(
                    window.clone(),
                    composition.base.clone(),
                    composition.device.clone(),
                    context,
                ) {
                    Ok(renderer) => {
                        self.composition = Some(Rc::new(composition));
                        return Ok(renderer);
                    }
                    // Another shared context may suit the window: the caller tries it.
                    Err(error) if error.is_surface_incompatible() => return Err(error),
                    // No Direct3D 12 (Vulkan presents to the window, not to
                    // visuals): the window as without native views.
                    Err(error) => {
                        eprintln!("zgx: native views unavailable: {error}");
                        drop(composition);
                        self.natives.set_supported(false);
                    }
                },
                Err(error) => {
                    eprintln!("zgx: native views unavailable: {error}");
                    self.natives.set_supported(false);
                }
            }
        }
        zgx_gpu::GpuRenderer::for_window_with_transparency(window, context, transparent)
    }
    /// Whether this window has native views to keep up (now or last frame).
    pub fn active(&self) -> bool {
        !self.natives.is_empty() || !self.split.is_empty()
    }
    /// Whether `node` is a native view's node (pointer over the page: the
    /// platform view owns the cursor).
    pub fn is_native(&self, node: zgx::scene::NodeId) -> bool {
        self.natives.contains(node)
    }
    /// The pointer moved to window point `(x, y)` (the host's own report):
    /// what zgx hears of it (see `NativePointer`). `held`: a press of
    /// zgx's follows the pointer.
    pub fn pointer_moved(&self, x: f32, y: f32, held: bool) -> PointerStep {
        #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
        if let Some(surfaces) = &self.surfaces {
            let (step, report) = surfaces.router().track(x, y, held);
            report.apply();
            return step;
        }
        let _ = (x, y, held);
        PointerStep::Move
    }
    /// The pointer left the window, or the window went to the back.
    pub fn pointer_left(&self) {
        #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
        if let Some(surfaces) = &self.surfaces {
            surfaces.router().pointer.left().apply();
        }
    }
    /// Whether the pointer is over a native view taking input (it shows
    /// its own cursor).
    pub fn pointer_over_native(&self) -> bool {
        #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
        if let Some(surfaces) = &self.surfaces {
            return surfaces.router().pointer.over().is_some();
        }
        false
    }
    /// Where the pointer last moved to that the host did not hear itself
    /// (a native view had keyboard focus, so AppKit sent the move there).
    pub fn take_unheard_pointer(&self) -> Option<(f32, f32)> {
        #[cfg(target_os = "macos")]
        if let Some(surfaces) = &self.surfaces {
            return surfaces.router().unheard.take();
        }
        None
    }
    /// The native views went away with the window (suspend/recreate).
    pub fn suspend(&mut self) {
        #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
        {
            self.overlay = None;
            self.surfaces = None;
            self.overlay_live = false;
            self.overlay_drawn = false;
        }
        #[cfg(all(windows, feature = "webview2"))]
        {
            self.composition = None;
        }
        self.split = Rc::new(PlaneSplit::default());
        #[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
        {
            self.placed = self.split.clone();
        }
        self.natives.set_split(self.split.clone());
    }
}

#[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
impl NativeHost {
    /// After the scene's flush: split this frame's paint around native views
    /// and give each plane's renderer its part, at content `scale` in a
    /// window at `zoom` (`Ui::zoom`; native views are placed in its points).
    /// Returns damage the split adds (paint that moved between planes
    /// repaints on both).
    pub fn plan(
        &mut self,
        scene: &Scene,
        window: &winit::window::Window,
        renderer: &mut zgx_gpu::GpuRenderer,
        scale: f32,
        zoom: f32,
    ) -> Result<Vec<Rect>, zgx_gpu::GpuError> {
        // Frames go with native geometry, in one transaction, while there is any.
        renderer.present_with_native_views(self.active());
        // Unsupported (no runtime, no surfaces): native nodes paint placeholders.
        if !self.active() || !self.natives.supported() {
            return Ok(Vec::new());
        }
        if self.surfaces.is_none() {
            let router = platform::Router {
                scene: self.scene.clone(),
                input: self.input.clone(),
                natives: self.natives.clone(),
                pointer: Default::default(),
                #[cfg(target_os = "macos")]
                unheard: Default::default(),
                zoom: std::cell::Cell::new(1.),
            };
            #[cfg(target_os = "macos")]
            let surfaces = macos::Surfaces::new(window, router);
            #[cfg(all(windows, feature = "webview2"))]
            let surfaces = match &self.composition {
                Some(composition) => dcomp::Surfaces::new(window, composition.clone(), router),
                None => Err("the window presents through no visual of zgx's".into()),
            };
            match surfaces {
                Ok(surfaces) => {
                    #[cfg(target_os = "macos")]
                    surfaces.report_to(&self.diagnostics);
                    self.surfaces = Some(surfaces);
                }
                Err(error) => {
                    eprintln!("zgx: native views unavailable: {error}");
                    self.natives.set_supported(false);
                    return Ok(Vec::new());
                }
            }
        }
        let (split, damage) = PlaneSplit::compute(scene, &self.natives.nodes(), &self.split);
        let split = Rc::new(split);
        let surfaces = self.surfaces.as_mut().expect("made above");
        // A new zoom moves every view without the split changing.
        let rezoomed = self.zoom != zoom;
        if rezoomed {
            self.zoom = zoom;
            surfaces.set_zoom(zoom);
        }
        let moved = surfaces.differs(&split, &self.split) || rezoomed;
        if moved {
            self.pending = true;
            #[cfg(target_os = "macos")]
            renderer.present_plain_for(SYNCED);
        }
        if split.is_empty() {
            renderer.clear_plane();
        } else {
            renderer.set_plane(split.clone(), zgx::native_view::Plane::Base);
            #[cfg(target_os = "macos")]
            if split.has_overlay() {
                // Base and overlay frames show together, in the transaction.
                renderer.present_plain_for(SYNCED);
            }
        }
        // The overlay plane: made for the first paint over a native view,
        // kept, and made again for a new display scale (its layer's too).
        if self.overlay.is_some() && self.overlay_scale != scale {
            self.overlay = None;
            self.overlay_live = false;
        }
        if split.has_overlay() && self.overlay.is_none() {
            self.overlay_scale = scale;
            #[cfg(target_os = "macos")]
            let overlay = zgx_gpu::GpuRenderer::for_overlay_layer(
                // Its layer's pixels to a point: the display's.
                surfaces.overlay_host(scale / zoom),
                &renderer.context(),
                scale,
            )?;
            #[cfg(all(windows, feature = "webview2"))]
            let overlay = surfaces.overlay_renderer(&renderer.context(), scale)?;
            self.overlay = Some(overlay);
        }
        if let Some(overlay) = &mut self.overlay {
            overlay.set_plane(split.clone(), zgx::native_view::Plane::Overlay);
            if let Some(needed) = split.overlay_bounds() {
                let viewport = scene.bounds(scene.root());
                let rect = plane_rect(overlay.plane_rect(), needed, viewport);
                overlay.set_plane_rect(rect);
                if !self.overlay_live {
                    overlay.repaint_all();
                }
            }
        }
        // Zoomed again only when something native moved: the platform
        // reads only native views and backdrops (`differs`), and copying the
        // whole split every frame of a zoomed window cost its sets' and maps'
        // allocations for nothing.
        if zoom == 1. {
            self.placed = split.clone();
        } else if moved {
            self.placed = Rc::new(split.zoomed(zoom));
        }
        self.split = split.clone();
        self.natives.set_split(split);
        Ok(damage)
    }
    /// Render the overlay plane's part of this frame's `damage`.
    pub fn render(&mut self, scene: &Scene, damage: &[Rect]) -> Result<(), zgx_gpu::GpuError> {
        self.overlay_drawn = false;
        if let Some(overlay) = &mut self.overlay
            && self.split.has_overlay()
        {
            overlay.render(scene, damage)?;
            self.overlay_drawn = true;
        }
        Ok(())
    }
    /// Open the transaction a frame's planes and native geometry commit in.
    pub fn begin(&self) -> Option<platform::Transaction> {
        #[cfg(target_os = "macos")]
        return self.active().then(macos::Transaction::begin);
        // Also for the frame that takes the last page out (DirectComposition
        // shows nothing until it commits, where Core Animation commits itself).
        #[cfg(all(windows, feature = "webview2"))]
        return self
            .composition
            .as_ref()
            .filter(|_| self.active() || self.pending)
            .map(|composition| dcomp::Transaction::begin(composition.clone()));
    }
    /// Within `begin`'s transaction: show the overlay plane's frame (before
    /// the base plane's, which waits for its refresh point).
    pub fn present_overlay(&mut self) -> Result<(), zgx_gpu::GpuError> {
        let Some(surfaces) = &mut self.surfaces else {
            return Ok(());
        };
        let live = self.split.has_overlay();
        if live != self.overlay_live {
            zgx_gpu::present_log::log(|| format!("overlay live={live}"));
        }
        if let Some(overlay) = &mut self.overlay
            && live
            && self.overlay_drawn
        {
            overlay.present()?;
        }
        if let Some(overlay) = &self.overlay {
            let rect = live.then(|| overlay.plane_rect()).flatten();
            zgx_gpu::present_log::log(|| {
                format!("overlay show rect={rect:?} drawn={}", self.overlay_drawn)
            });
            // In the window's points: the zoom's times the plane's.
            let zoom = self.zoom;
            surfaces.show_overlay(
                rect.map(|r| Rect::new(r.x * zoom, r.y * zoom, r.width * zoom, r.height * zoom)),
            );
        }
        if self.overlay_live
            && !live
            && let Some(overlay) = &mut self.overlay
        {
            // Hidden while nothing paints on it: one surface is enough.
            overlay.trim();
        }
        self.overlay_live = live;
        self.overlay_drawn = false;
        Ok(())
    }
    /// Within the frame's transaction: move native views and backdrops to
    /// this frame's placements.
    pub fn apply(&mut self) {
        if let Some(surfaces) = &mut self.surfaces {
            surfaces.apply(&self.natives, &self.placed);
        }
        self.pending = false;
    }
    /// Apply geometry that changed in a frame without zgx pixels to show
    /// (a native node moved, painting nothing), in a transaction of its own.
    pub fn flush(&mut self) {
        if self.pending {
            let transaction = self.begin();
            self.apply();
            drop(transaction);
        }
    }
    /// Keep zgx's focus and the page's keyboard focus agreeing (a click in
    /// a page focuses its node; a click on zgx content leaves the page).
    pub fn sync_focus(&mut self, ui: &Ui) {
        if let Some(surfaces) = &mut self.surfaces {
            surfaces.sync_focus(ui, &self.natives);
        }
    }
    /// Release what an idle window does not need.
    pub fn trim(&mut self) {
        if !self.split.has_overlay()
            && let Some(overlay) = &mut self.overlay
        {
            overlay.trim();
        }
    }
}

#[cfg(not(any(target_os = "macos", all(windows, feature = "webview2"))))]
impl NativeHost {
    pub fn plan(
        &mut self,
        _: &Scene,
        _: &winit::window::Window,
        _: &mut zgx_gpu::GpuRenderer,
        _: f32,
        _: f32,
    ) -> Result<Vec<Rect>, zgx_gpu::GpuError> {
        Ok(Vec::new())
    }
    pub fn render(&mut self, _: &Scene, _: &[Rect]) -> Result<(), zgx_gpu::GpuError> {
        Ok(())
    }
    pub fn begin(&self) -> Option<()> {
        None
    }
    pub fn present_overlay(&mut self) -> Result<(), zgx_gpu::GpuError> {
        Ok(())
    }
    pub fn apply(&mut self) {}
    pub fn flush(&mut self) {}
    pub fn sync_focus(&mut self, _: &Ui) {}
    pub fn trim(&mut self) {}
}

/// The overlay surface's box for overlay paint covering `needed`: the
/// current one while it holds `needed` without being much larger, else
/// `needed` with a margin to grow and move into (an animating popover
/// keeps its surfaces), within the window.
#[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
fn plane_rect(current: Option<Rect>, needed: Rect, viewport: Rect) -> Rect {
    let needed = needed
        .intersection(viewport)
        .unwrap_or(Rect::new(viewport.x, viewport.y, 1., 1.));
    if let Some(current) = current
        && contains(current, needed)
        && current.width * current.height <= 3. * (needed.width * needed.height).max(64. * 64.)
    {
        return current;
    }
    let margin = 24f32.max(needed.width.max(needed.height) * 0.1);
    needed
        .expand(margin)
        .intersection(viewport)
        .unwrap_or(needed)
}

#[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x + 0.01
        && outer.y <= inner.y + 0.01
        && outer.x + outer.width >= inner.x + inner.width - 0.01
        && outer.y + outer.height >= inner.y + inner.height - 0.01
}

/// One shape a native view's cut masks it to, in window coordinates.
#[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Rect(Rect),
    Round(zgx::scene::RoundRect),
}
/// The shapes cutting a box: `clip`, unless every rounded shape lies inside
/// it, then each rounded rectangle; `None` where a level cuts nothing.
#[cfg(any(target_os = "macos", all(windows, feature = "webview2")))]
fn shapes(clip: Rect, rounded: &[zgx::scene::RoundRect], levels: usize) -> Vec<Option<Shape>> {
    let rounded: Vec<zgx::scene::RoundRect> = rounded
        .iter()
        .copied()
        .filter(|round| round.radii.iter().any(|r| *r > 0.))
        .collect();
    let mut shapes = Vec::with_capacity(levels);
    if rounded.is_empty() || !rounded.iter().all(|round| contains(clip, round.rect)) {
        shapes.push(Some(Shape::Rect(clip)));
    }
    shapes.extend(rounded.iter().map(|round| Some(Shape::Round(*round))));
    // Deeper nesting than the levels: the outermost go (zgx's own paint
    // keeps two rounded clips too).
    shapes.truncate(levels);
    shapes.resize(levels, None);
    shapes
}
