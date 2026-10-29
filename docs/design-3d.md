# 3D in zgx: design note (3D lane, Oct 27 2026)

Read on origin/main 8fede829 (`lane/3d` worktree). Nothing built: quiet window 34.

## 1. What others do

| | How 3D content gets on screen | What it teaches |
| --- | --- | --- |
| Web `<canvas>` (WebGL/WebGPU) | The canvas is a compositor texture. Script draws in rAF; the compositor applies CSS transforms, border-radius, opacity and clip to that texture. | Render to a texture, composite as a box. |
| three.js | Continuous `setAnimationLoop` by default. | Not on demand, and that costs power. |
| model-viewer | Renders on demand: only while the camera moves, an animation plays, or a load lands. One three.js renderer and GL context is shared by every `<model-viewer>`. | On demand, one shared device. |
| Flutter | `Texture(textureId)` composites a platform texture; `markTextureFrameAvailable` asks for a frame. Flutter GPU + flutter_scene (Impeller) add a retained glTF scene on top. | Same two layers as A and B. |
| Qt Quick | `QQuickRhiItem` renders into a texture, retained, with `update()` asking for a frame. Qt Quick 3D (`View3D`, `Model`, `PerspectiveCamera`, `DirectionalLight`, `PrincipledMaterial`, glTF) is declarative 3D and renders offscreen by default. | Closest precedent for A plus B. |
| egui | `egui_wgpu::CallbackTrait` (`prepare` / `paint`) draws inline in egui's render pass. | The app must match egui's pass (format, MSAA, no depth); rounding and opacity don't apply. |
| iced | `shader` widget: `Primitive::prepare` / `render` into the frame target, clipped. | Same as egui. |
| Slint | Imports a `wgpu::Texture` as an `Image`. | Exactly A's model. |
| GPUI | `surface()` shows CVPixelBuffers (macOS); no general 3D. | zgx's NativeSurface already matches it. |
| Bevy / Godot | A render-to-texture camera shown as an image (ImageNode, SubViewport). | Same model as A. |

**Choice: render to a texture.** Inline drawing (egui, iced) can't work here: zgx's common macOS frame is raw Metal (`metal_frame.rs`), not a wgpu pass. It also loses rounding, opacity, layers and kept-frame reuse, and any UI change would re-run the 3D. With a texture, a UI-only change never re-renders 3D, and a 3D frame damages only its own rect.

## 2. A: the GPU view (Rust)

### Core (`zgx`, no wgpu dependency)

```rust
// zgx::gpu_view (new module)
#[derive(Clone)]
pub struct GpuView { /* Rc: id, painter (Rc<dyn Any>), frame serial, size, clock task */ }
impl GpuView {
    pub fn new(painter: impl std::any::Any) -> Self;
    pub fn id(&self) -> u64;
    /// Ask for a new frame. Coalesced: several calls before a render give one frame.
    pub fn invalidate(&self);
    /// A new frame every display frame while on (FrameClock). Off: idle.
    pub fn animate(&self, on: bool);
}
// zgx::compose (new function)
/// A box whose pixels `view` renders on the GPU, at the box's device size.
pub fn gpu_view(label: impl Into<String>, view: &GpuView) -> View;
// zgx::image::ImageData (new methods)
impl ImageData {
    /// A frame of a GPU view: no CPU pixels (`pixels()` is empty). Renderers draw it from the view's texture.
    pub fn gpu_frame(&self) -> Option<GpuFrame>;   // GpuFrame { view: u64, width, height }
}
// zgx::scene::Scene (new method)
/// Show a new frame of the same GPU view: damages the node's box only.
/// No content_revision bump, no layout, kept frame stays.
pub fn set_image_frame(&mut self, id: NodeId, image: Arc<ImageData>);
```

### Renderer (`zgx-gpu`)

```rust
pub use wgpu;                       // wgpu becomes a public dependency (as in egui-wgpu and iced)
pub mod viewport {
    pub trait Viewport: 'static {
        /// Draw a frame into `frame.target`: premultiplied alpha, `frame.size` device pixels.
        fn render(&mut self, frame: &mut ViewportFrame<'_>);
        /// The device changed (lost and recreated, or another window's): drop GPU resources.
        fn device_changed(&mut self) {}
    }
    pub struct ViewportFrame<'a> {
        pub device: &'a wgpu::Device,
        pub queue: &'a wgpu::Queue,
        pub encoder: &'a mut wgpu::CommandEncoder,
        pub target: &'a wgpu::TextureView,
        /// Also viewable as its sRGB twin, for linear shading.
        pub format: wgpu::TextureFormat,
        pub size: [u32; 2],
        pub scale: f32,
        /// FrameClock time of the frame being drawn.
        pub time: std::time::Duration,
    }
    /// A GpuView that `painter` draws.
    pub fn view(painter: impl Viewport) -> zgx::gpu_view::GpuView;
}
```

`zgx-desktop` re-exports both.

```rust
let cube = zgx_desktop::viewport::view(Cube::default());
cx.render(column().child(gpu_view("Cube", &cube).w_full().h(400.).rounded(12.)));
// drag handler: cube.invalidate();
```

Apps keep their own depth and MSAA textures, sized from `frame.size`. Input works as on any box: pointer, wheel and focus handlers on the element.

## 3. How it composites, and its damage

- **The node** is a plain `NodeKind::Image`, built the way compose builds a canvas: a replaced root plus a sized child. Each frame is a new `ImageData` (fresh id) marked `gpu_frame`.
  - Every existing image capability then applies unchanged: clip, ancestor rounded clips (Region), opacity, edge fades, colour filters, masks, layers under transforms (rotate_x/y with perspective), and hit testing.
  - The compose element wraps it in the same clip container fitted images use. A pixel test confirms its own rounding.
- **Size**: the element observes its content size, as canvas does (`observe_content_size_deferred`). Box size × `pixel_scale` is the frame size; a resize gives a new frame at the new size.
- **A new frame** (`invalidate`, animate tick, resize): `Scene::set_image_frame` swaps the image and damages its box.
  - It avoids `set_kind`, whose `content_revision` bump costs a full `prune` walk every frame.
  - Images aren't `changes_unmarked`, so the kept frame patches the one leaf and the window keeps its kept frame.
- **Renderer**: the image draw path already does `if !images.contains_key(id) { upload_image(image) }` (lib.rs:6303).
  - Only `upload_image` (the miss path) learns `gpu_frame()`. It takes the painter from the core's registry, gets a pooled texture (one per view, reallocated on size change), and runs `Viewport::render` into an encoder submitted before zgx's frame on the same queue.
  - It inserts `(bind, bytes, Some(texture))` under the new id, so the direct Metal path stays eligible.
  - It evicts the view's previous id itself (the registry remembers it), so prune isn't needed.
- **No churn per frame** (coordinator's watch item): each view keeps a two-slot ring of `Arc<ImageData>`. The slot the scene no longer holds is re-stamped in place with a fresh id (`Arc::get_mut`), and the renderer swaps that view's one `images` entry and pooled texture in place. viewport_bench at 120 Hz reports allocations and bytes per frame, cache inserts and evictions, `images.len()`, whether the frame was kept, and that `prune` didn't run.
- **wgpu**: `zgx_gpu::wgpu` is crates.io wgpu 29.0.4, the crate the renderer links (only wgpu-hal is patched, and only inside this repo). A wgpu major bump is a zgx-gpu breaking bump, and the docs say so.
- **Ordering**: one wgpu device and one queue, zgx's. App work is submitted before zgx's Metal command buffer on that queue, so hazards are tracked (wgpu textures). A second queue would also cost the ~176 MB of Metal driver memory each queue holds.
- **Damage**: only the view's box (ancestor-mapped through layers by `damage_node`), up to 16 scissor regions as ever. UI changes over a still view reuse its texture with no painter call. Partial redraws overlapping the view sample the same texture.
- **Device loss**: the renderer calls `device_changed` and invalidates its views. Recovery already rebuilds the renderer.
- **Software raster** (`raster.rs`): draws a GPU frame as its box's fill (nothing to read back).

## 4. Where zero cost comes from

1. **No struct grows.** Node, Rare, Opened, PaintItem, Quad, Draw and KeptItem stay the same: a view is an Image node.
   - New code: the private compose `Kind::GpuView` variant (payload ≤ NativeSurface's; `size_of::<View>()` asserted unchanged); `ImageData` gains one private field (heap, behind Arc); the core registry is a `thread_local!` touched only by views.
   - Node byte offsets get an `offset_of!` check anyway (#709 lesson).
2. **No new branch on any path that runs without a view.**
   - The image cache hit path is untouched. The only check is inside `upload_image`, which runs once per new image: a constant, not per tick.
   - `set_kind`, `flush`, the paint walk and `prune` are untouched; `set_image_frame` is a new function.
   - symsizes diff against main, to catch outlining (#347/#764 lessons).
3. **No device work until a view renders.** The painter makes its own pipelines on its first `render`. zgx-3d builds its pipelines at its first frame.
4. **Idle stays idle.** A view renders only on `invalidate`, resize or `animate(true)` (FrameClock: the display link runs only while asked). zgx-3d renders only when a bound value, the orbit (until settled) or a playing clip changes.
5. **Binary size, measured.**
   - A: a few KB in zgx and zgx-gpu (wgpu is already linked).
   - B: a new crate, `zgx-3d`, linked only if used; zgx-script gets it under a `scene3d` feature and glTF under `gltf`. Both default on only if the measured size cost (zgx bin, stripped, release) is small. My threshold proposal: ≤150 KB for `scene3d`, else opt-in. (Landed: `scene3d` is off by default in zgx-script and on in zgx-run. A headless host would otherwise link wgpu for it, 1.7 MB in the gate's `list_state_bench`. In a host that already links the renderer it costs zgx-run 123 KiB built for shipping (`dist`, on against off at one commit; 158 KiB in `release`).) The glTF reader is my own over the serde_json already in zgx-script, not the `gltf` crate, unless it's measured smaller.
6. **Gates**: exact container gate.py ≤0 on all 31 cells (ticks 60/300/900 to tell constants from per-tick), heap peak ≤ main, style/anime/script benches, offset_of, symsizes.

## 5. Cost with a view on screen, measured against a minimal wgpu app

**Expected extra cost**: one composite of the view rect (a textured quad sampled once per pixel), plus zgx's per-frame floor for one patched leaf.

**How to measure**: `crates/zgx-gpu/examples/viewport_bench.rs` runs the same painter (zgx-3d: a glTF helmet plus 1,000 instanced cubes, orbiting at 120 Hz) once inside zgx and once in `wgpu_minimal_bench.rs` (winit + a swapchain, the same painter straight to the surface). Per frame it records:
- retired instructions;
- GPU Mcycles (not GPU time);
- footprint;
- pacing p99.

Runs happen on home-laptop / M4 in a quiet window, plus the 4090 for GPU.

**If the composite shows up**: an opaque, untransformed, unobscured view could present as its own plane (CALayer / DComp visual, `planes.rs`), skipping the copy. That waits on numbers.

## 6. D3D11

**Agreed with the WING-PC lane (43ff3156), Oct 27.**

Their code is local on WING-PC: branch `windows/d3d11-renderer`, unpushed. It opens wgpu's noop backend as a shell plus a D3D11 device, records frames in `d3d11_frame.rs`, and doesn't draw images or layers yet.

**The rule: never hand an app zgx's internal device.** `ViewportFrame.device` belongs to the view's GPU context, which can differ by path:
- **wgpu D3D12 (today's default), Mac, Linux:** it is zgx's own device. The texture is sampled directly: no sharing, no overhead.
- **D3D11 path:** a D3D12 wgpu device for views.
  - **Lifetime:** opened with the first view, on a background thread (it takes about 180 ms), and dropped with the last.
  - **Adapter:** the same as D3D11's, matched by LUID (`expose_preferred_adapter`).
  - **The texture:** made in D3D12 with `D3D12_HEAP_FLAG_SHARED` and wrapped for wgpu by `texture_from_raw`. D3D11 opens it with `ID3D11Device1::OpenSharedResource1` and samples it as a GPU image.
  - **Sync:** a shared `ID3D12Fence`, opened on the D3D11 side with `ID3D11Device5::OpenSharedFence`. `ID3D11DeviceContext4::Wait` runs before the frame samples it and `Signal` after; the wait is on the GPU, so the CPU never stalls.
- **Fallback:** only if interop fails (no shared fences, no matching adapter), use the wgpu D3D12 renderer. A whole-window switch is not the default: GpuContext is shared across windows, so switching would rebuild the renderer mid-session.

**Costs while a view lives on the D3D11 path:**
- the D3D12 floor comes back: about +40 MiB private and about 1.2 M cycles/s from the driver at rest;
- per frame: wgpu's submit (about 0.2 M cycles) plus one fence Signal and Wait.

Apps without views pay nothing.

**Who builds what.**
- The 3D lane: zgx-gpu takes a view's device, queue and target through one internal seam (`ViewHost`: device, queue, target texture, and an "after render" hook). The wgpu path is the trivial implementation.
- The D3D11 lane: the D3D11 `ViewHost` (shared texture plus fence) and the "GPU image" texture source in `d3d11_frame`.
- `device_changed` fires when a view's device changes (device loss, the D3D11 device opening or closing).

What follows is the proposal as first written, before the lane answered.

**My proposal**: the app keeps a real device.
- On the D3D11 path, the first view creates a DX12 wgpu device (lazily: zero cost without views).
- It renders into a shared NT-handle texture (`D3D12_HEAP_FLAG_SHARED`), which the D3D11 renderer opens with `OpenSharedResource1`.
- The two sync through a shared fence (`ID3D12Fence` ↔ `ID3D11Fence`), as Chromium does with Dawn and ANGLE.
- `Viewport` doesn't change; `ViewportFrame.device` is that DX12 device.

A noop device would draw nothing silently, so it's ruled out. **Fallback**: a window showing a view uses the wgpu DX12 renderer. Final answer once the lane replies.

## 7. B: `ui.scene3d` (Luau), built on A

### The `zgx-3d` crate (Rust, also usable from Rust apps)

- A retained scene graph implementing `Viewport`: one forward pass, Depth32, 4× MSAA resolving into the frame, Khronos-neutral tone mapping.
- PBR-lite metallic-roughness (GGX): one directional light, up to 4 point lights, hemisphere ambient; base-colour, metal-rough, normal and emissive maps.
- Meshes shared and instanced. Primitives: box, sphere, plane, cylinder, cone, torus.
- Renders on change only.

### Bindings

- Every 3D prop is a number, a state, a function or a `ui.tween` / `ui.spring` Binding.
- The binding's signal writes the zgx-3d node in Rust and invalidates the view, coalesced per frame: no Luau per frame.
- anime drives 3D through states it already targets (`anime.animate(angle, …)`), so anime's `Key` and `Target` stay untouched for now. A direct Object3D target comes later only if it's needed and measured ≤0 on anime_bench.
- **Orbit** is in Rust: drag rotates, wheel or pinch zooms, shift-drag pans. Damping runs on FrameClock until it settles; `auto_rotate` keeps animating. `orbit = state` reads and writes it.

### Types (`zgx.d.luau.in`)

```luau
declare extern type Object3D with end
type Num = number | Readable<number> | Binding | (() -> number)
export type Vec3 = { number }
type Vec3Prop = { Num } | Readable<Vec3> | (() -> Vec3)
type ColorProp = Color | Readable<Color> | (() -> Color)
export type Hit3D = { point: Vec3, normal: Vec3, distance: number, id: string? }
export type Material = {
	color: ColorProp?, metallic: Num?, roughness: Num?, emissive: ColorProp?,
	opacity: Num?, map: string?, unlit: boolean?,
}
export type Object3DProps = {
	id: string?,
	position: Vec3Prop?,
	-- degrees, XYZ order
	rotation: Vec3Prop?,
	scale: (Num | Vec3Prop)?,
	visible: (boolean | Readable<boolean> | (() -> boolean))?,
	on_click: ((hit: Hit3D) -> ())?,
	on_hover: ((over: boolean) -> ())?,
}
export type MeshProps = Merge<Object3DProps, {
	geometry: "box" | "sphere" | "plane" | "cylinder" | "cone" | "torus",
	size: Vec3Prop?, radius: Num?, segments: number?, material: Material?,
}>
export type ModelProps = Merge<Object3DProps, {
	-- .glb / .gltf, relative to the script
	src: string,
	animation: (string | Readable<string>)?,
	playing: (boolean | Readable<boolean>)?,
	-- scaled to fit a cube this size, if given
	fit: number?,
	material: Material?,
}>
export type LightProps = Merge<Object3DProps, {
	kind: "directional" | "point" | "hemisphere",
	color: ColorProp?, ground: ColorProp?, intensity: Num?, direction: Vec3Prop?, range: Num?,
}>
export type GroupProps = Merge<Object3DProps, { [number]: Object3D }>
export type Camera = {
	fov: Num?, position: Vec3Prop?, target: Vec3Prop?,
	near: number?, far: number?, orthographic: boolean?,
}
export type Orbit = {
	azimuth: State<number>?, elevation: State<number>?, distance: State<number>?,
	-- degrees a second
	auto_rotate: Num?,
	damping: number?, zoom: boolean?, pan: boolean?,
	min_distance: number?, max_distance: number?,
	min_elevation: number?, max_elevation: number?,
}
export type Scene3DProps = Merge<Props, {
	camera: Camera?,
	orbit: (boolean | Orbit)?,
	-- nil: transparent over the box's bg
	background: ColorProp?,
	environment: { sky: ColorProp?, ground: ColorProp?, intensity: Num? }?,
	label: string?,
	[number]: Object3D,
}>
-- in `declare ui`:
scene3d: (props: Scene3DProps) -> Node,
mesh: (props: MeshProps) -> Object3D,
model: (props: ModelProps) -> Object3D,
light: (props: LightProps) -> Object3D,
group: (props: GroupProps) -> Object3D,
```

### Example

```luau
local spin = ui.use_state(0)
ui.scene3d {
	w = "100%", h = 360, rounded = 12,
	camera = { fov = 40, position = { 0, 1.2, 4 } },
	orbit = { auto_rotate = 15 },
	ui.light { kind = "directional", direction = { -1, -2, -1 }, intensity = 3 },
	ui.model { src = "assets/helmet.glb", fit = 2, rotation = { 0, ui.spring(spin), 0 } },
	ui.mesh { geometry = "plane", size = { 6, 1, 6 }, position = { 0, -1, 0 }, material = { color = "$surface" } },
}
```

### Lint, assets, docs

- **Lint**: `mesh`, `model`, `light` and `group` join `NOT_ELEMENTS`, with BUILDS where needed. The read-once rule extends to `:get()` in 3D props, and per-frame-state flags orbit or rotation driven by `ui.every`.
- **Assets**: glTF/GLB load off the main thread (the remote-image pattern: a status signal plus a rebuild), cached by src across reloads with mtime watching. `bundle.rs` keeps `glb` and `gltf` files.
- **Docs**: `docs/reference/3d.md`, plus a hot-reload demo in `examples/luau/scene3d`.

## 8. Milestones

**A1: GPU view.**
- Code: `gpu_view`, `GpuView`, `set_image_frame`, the `Viewport` trait and registry, the miss-path render, Metal direct-path eligibility, device-loss hook, software-raster fill.
- Tests (gpu, headless `GpuRenderer`):
  - a solid frame reads back;
  - clipped by a rounded parent;
  - opacity;
  - under rotate_y with perspective;
  - `invalidate` damages exactly its box and calls the painter once;
  - a UI change elsewhere calls the painter 0 times;
  - resize renders at the new device size;
  - no `content_revision` bump;
  - the kept frame survives;
  - a view without animate asks no frames;
  - device loss calls `device_changed`.
- Example: `zgx-desktop/examples/viewport.rs`, a lit spinning cube in raw wgpu with drag-to-orbit, plus a Linux probe `viewport_smoke.py` registered in `linux_smokes.py`, plus `docs/viewport.md`.
- Gates: exact gate, size, idle, semver-checks.

**A2: Windows.** The D3D11 interplay (per the lane), then `viewport_bench` against `wgpu_minimal_bench` on Mac, Windows and Linux.

**B1: zgx-3d.** Primitives, camera, lights, PBR-lite, depth and MSAA, a Rust API, render on change. Pixel tests: depth order, lighting monotonic in angle, transparency.

**B2: Luau.** `ui.scene3d`, `mesh`, `light`, `group`, camera and orbit, bindings and springs, types, lint, docs, demo. Script tests: no Luau per frame while a spring runs; idle after orbit settles.

**B3: glTF and more.**
- glTF/GLB (feature `gltf`): textures, the node tree, animation clips.
- Picking (`on_click`, `on_hover`).
- One directional shadow map.
- Each piece gets size and perf numbers.

## 9. A3: a view presented as its own plane (macOS first)

**Why.** Composited, a full-window view costs +25% app GPU (sampling its texture into the window each frame), and the WindowServer composites zgx's tiles: 9.0% GPU against 1.9% for a bare CAMetalLayer (window 35).

**What.** A view whose box shows it whole and opaque gets a CAMetalLayer of its own. The painter draws into that layer's drawable, and its image draws nothing in the window.

**When it may.** The app promises every pixel opaque (new `GpuView::set_opaque(true)`; zgx-3d sets it when a world has a background). In the frame's placement:
- the box is untransformed: `size == frame` size;
- no rectangular clip cuts it;
- no rounded clip;
- opacity is 1.

Anything else draws composited, as today.

**Placing it.** It reuses the native-view machinery rather than adding a second:
- **Registration.** The view registers with `NativeViews` as `NativeContent::Platform`: a layer-hosting `NSView` (zgx-desktop) around the CAMetalLayer, whose `hitTest:` returns nil so input stays zgx's. A desktop-provided service makes it; `mount_gpu_view` asks for it only when the view is opaque and the service exists. Without one, nothing changes.
- **Geometry** comes from `PlaneSplit` placements and commits in the window's CA transaction.
- **Overlays.** zgx paint over the view goes on the existing overlay plane above it, which draws only that paint's box. That's cheaper than compositing the whole view under a small HUD, and it's exact. The window cell measures it against the composite.
- **Input.** `PlaneSplit::route` already sends input over a native node to zgx when the node has listeners. The view's own handlers (orbit) keep working because the platform view never takes hits.

**Drawing it.** On perf's drawable frame source, there's no second clock:
- the window's `draw_frame` acquires the plane's drawable on the same tick when the view has a new frame;
- the painter draws into it through the `view_device`/`view_texture` seam, with the drawable as the target;
- it presents asynchronously while the box is still, and with `presentsWithTransaction` (commit, waitUntilScheduled, present) on frames where the box moves, with the window's own drawable, as perf's PR does for the window.

**The window under it.**
- **Planed:** the renderer skips the view's image item, and the planed rect is taken out of the frame's damage, because an opaque layer covers it. So a view frame costs the window nothing but the painter.
- **Switching to the plane:** the frame that first presents the plane also stops drawing the image, in the same transaction.
- **Switching back** (a transform, a clip, the app turning opacity off): that frame damages the view's box, draws the composited image, and hides the plane in the same transaction. The view's texture is made again at once if it was dropped. No frame shows neither.
- **Tests:** a headless sequence of plane/composite switches checking the image item is skipped exactly when planed, with damage accounted. A macOS probe captures each switch frame (framepace capture plus pixel reads) looking for a frame with neither.

**Cost when unused.** None. With no GPU view, no plane service is asked for, `PlaneSplit` stays inactive (`active()` is false without natives), and the renderer's skip set is empty. The exact gate checks it. A composited view keeps A1's path unchanged.

**Measure.** A window cell after perf's PR lands: the viewport scene with zgx planed, wgpu, and zgx composited, compared on GPU Mcycles, WindowServer GPU, power, CPU, footprint and pacing.

**Measured (window 36, M5 Max at 120 Hz, 37f8c17f).**
- Planed against composited: WindowServer GPU 1.9% against 9.1%, and GPU power 92 against 208 mW. App CPU +1.0 point, WindowServer CPU +2.4, footprint +23 MB. Against wgpu alone it's level on GPU, at +0.25 app CPU.
- The view's `CADisplayLink` with a skipped tick (the default then) showed doubled frames, p99 17.6 ms, every round. Skipping on the display's link paced well but put WindowServer's GPU back at 8%. The window's own clock was best.
- Every mode blocked the main thread in `nextDrawable` (3–8 ms a frame).
- So A3 landed on the window's clock and never blocks: a plane frame waits a tick while two of its pool of three are on their way, and each shows a refresh at least.
- A per-launch "phase lottery" that held 30% of launches at 60 shown/s was a lost count, not a phase: drawables counted after their present, the window server sometimes showing one first. Counted before, 0 of 32 launches were.
- Planes stay opt-in (`set_opaque`).

**Direct flip or composited (window 37 and after, M5 Max).** Non-blocking, the window server's GPU read 7.8% busy again, against 1.9% at 37f8c17f. Presents traced against their shown times show the plane has two self-sustaining states:
- **A, flipped:** a frame handed over about 1 ms after a refresh waits queued and shows at the next. The window server flips it straight to the screen (GPU 2%), 19 ms from its frame's start. While a frame waits queued, no drawable is free at the next tick, so the next frame waits for the refresh too. The blocking path of window 36 sat here, by chance.
- **B, composited:** a frame handed over about 2.2 ms before a refresh shows on it. The window server composites it (GPU 7.6%), 10.8 ms from its frame's start. The last frame has always shown by the next tick, so a drawable is always free.
- Ruled out first, each measured: the pool's size, minimum-duration presents, presented handlers, `presentDrawable:atTime:`, where in the refresh the present lands, how regular presents are, and main's changes since. A drawable thread per plane (waiting in `nextDrawable` off the main thread) still sat in B.
- **Chosen: B**, by power, not busy time: the system's GPU 95 mW planed in B against wgpu's 89 and composited's 204 (IOReport, window 37). A would cost a refresh of latency and WindowServer CPU to save those 6 mW.

**Paced by the drawable, not its showing (M1 Max, after window 37).** On the M1 the plane drew 80 frames a second at 120 Hz while wgpu and composited drew 120:
- Little's law: two frames on their way, each 25 ms from its start to its showing, allow 80 a second. The cap held back nearly every frame (the wake that draws an owed frame fired 75 times a second).
- The cap counted frames by their presented handlers, which Core Animation calls later than the drawable is free: forced to draw just after each refresh (A), the M5 showed 60 to 114 frames a second, frames 15.7 ms on their way. So it held back frames the pool could already take. A minimum duration of half a refresh changed nothing.
- wgpu waits in `nextDrawable`, which returns when a drawable is free: 120 on the M1. So the plane waits there too, on a thread of its own (`plane_drawables`), and nothing counts handlers. Pool 3 (as wgpu's): 120 on the M1 (start-to-shown 27.4 ms) and the M5 (19.1 ms).
- Pool 2 came a refresh sooner (M1 19.1 ms, M5 10.8) but missed more: M5 0.9% of frames late in the worst stretches, bursts of 36 in two seconds, against pool 3's 0.6% (the handler-paced path's own 0.3–0.55%); M1 0.5% against 0.2%. Pool 3 is kept.
- Its cost on the M5: WindowServer CPU about 23% against 19% handler-paced (wgpu's own, 22–23%), so waiting in `nextDrawable` costs the window server, whoever waits. App CPU +0.1 to +1.2 points (the thread included). The system's GPU and WindowServer GPU are unchanged.
- **Memory** (render lane, footprint regions): +26 MB planed against composited was the three view-size drawables (+22 MB of IOSurfaces) and the composited texture kept alive under the plane (~13 MB), now dropped once the plane shows: about +20 MB left (M5, 364–370 against 347 MB).
- **Decided (after window 38): zgx-3d stays composited, planes opt-in.** The app CPU is the platform's, so no plane can tie composited on it:
  - **Per thread** (M5, retired instructions, `PROC_PIDTHREADCOUNTS`), in M a second:

    | | system threads | main | drawables thread |
    | --- | --- | --- | --- |
    | planed | 100 | 51–59 | 7 |
    | composited | 54 | 65 | — |

  - **Where the system threads go** (stack samples): queues only a `CAMetalLayer`'s presents run. These are FramePacing's per-layer state sync (macOS 26), Core Animation's metal-layer event listener, its mach port replies, `CA::Context` and three times the completion traffic. wgpu's surfaces pay the same.
  - **Private switches:** `lowLatency` and `serverSyncEnabled` changed nothing (tried locally; not shippable).
  - **What's left of ours** (the thread, and the wake that hands a drawable over: up to ~20 M a second) is a possible cut, though cutting it changes no decision.
  - **M1 Max:** 159 against 111 M instructions a second, 0.34 against 0.66 W, 169 against 153 MiB.
- **A ring of IOSurface sublayers instead (lane/3d-plane-cpu, shelved):**
  - It's the window tiles' technique: contents set once, a frame flipping `hidden` from its painter's completion handler.
  - It used the least app CPU of all: M5 88–107 M instructions a second, M1 96 M.
  - But the window server composites a plain layer every frame, however it flips (`hidden`, opacity, contents), with untagged surfaces, or flipped in the window's paced transaction. WindowServer GPU was 10.2% against composited's 8.6% (M1 15.3 against 11.4), and system GPU about composited's. Only a `CAMetalLayer` gets the cheaper path.
  - Its resize was left broken.

**Windows next.** The same, with a DComp visual for the plane's swap chain (native_host/dcomp.rs), with the D3D11 lane.
