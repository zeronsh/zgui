# GPU views: an app's own wgpu drawing in a box

`gpu_view(label, &view)` shows a `zgx::gpu_view::GpuView` whose painter draws with wgpu: a 3D scene, a game, a decoder's output. The box is an ordinary one: size it, round it, clip it, fade it, turn it in perspective, put UI over it. It has no size of its own.

```rust
use zgx::compose::prelude::*;
use zgx_desktop::{viewport::{Viewport, ViewportFrame}, wgpu};

struct Scene { /* pipelines, buffers: made in the first `render` */ }
impl Viewport for Scene {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        // Record into frame.encoder, drawing all of frame.target
        // (frame.size device pixels, premultiplied alpha).
    }
    fn device_changed(&mut self) { /* drop what the old device made */ }
}

let view = zgx_desktop::viewport::view(Scene { /* … */ });
cx.render(gpu_view("Scene", &view).w_full().h(400.).rounded(12.).overflow_hidden());
view.invalidate();      // a new frame, before the box is next drawn
view.animate(true);     // a frame every refresh while it shows
```

`crates/zgx-desktop/examples/viewport.rs` draws a lit cube that turns while animating or as it is dragged.

## When a frame is drawn

A frame is drawn when the box first shows, when it resizes or the window's scale changes, when the view is invalidated, and, while it animates (`GpuView::animate(true)`), once per display refresh it shows. Animation stops while the box is offscreen, clipped away, transparent or its window hidden. Nothing else draws one: UI changes over or around it show its last frame again, and an idle window with a still view does no work. Calls to `invalidate` between two draws make one frame. Don't invalidate from `render`: animate instead.

Each frame is drawn at the box's size in device pixels (`frame.size`, `frame.scale` device pixels a point), into `frame.target`, a `Bgra8Unorm` texture of zgx's (`frame.format`). `frame.target_srgb` views it as `Bgra8UnormSrgb`, for shading in linear light, except on GL, which can't. Pixels are premultiplied; what isn't drawn over shows the box's background where the target was cleared to transparent. Keep depth, multisampled colour and the like of your own, sized from `frame.size`.

## How it composites

A frame is an image (`NodeKind::Image`) of its own identity and no pixels (`ImageData::gpu_frame`); its box shows it as any image. The renderer draws a frame the first time it draws that image, which is when its image cache misses: it runs the painter into an encoder of its own, submits it ahead of the frame sampling it on the same queue, and caches the view's texture under the frame's identity in place of the last frame's. A new frame repaints the box alone (`Scene::set_image_frame`) and is patched into a kept window frame as any image's change is. One texture is kept per box showing a view, drawn over frame after frame and made anew only at a new size.

## A layer of its own (macOS)

A view whose painter covers every pixel opaquely says so with `GpuView::set_opaque(true)` (a 3D scene with a background, a video). Where its box then shows it whole and alone, macOS gives the box a `CAMetalLayer` of its own:
- whole and alone means untransformed, unclipped, unrounded, unfaded, nothing painted over it, and not inside a cached layer;
- the painter draws straight into the layer's drawables (`frame.format` then `Bgra8UnormSrgb`, with no `target_srgb`), and neither zgx nor the window server composites it into the window's frame;
- a frame of the view alone presents by itself and leaves the window's frame as it was;
- a frame where the box moves, shows on its layer, or goes back into the window's frame (a clip, a fade, something drawn over it) commits with the window's frame, so no frame shows the view nowhere.

A plane keeps to the window's own clock, and never makes the main thread wait for a drawable. A thread of its own waits for its layer's next drawable (`nextDrawable`, as wgpu's surfaces wait) and hands it over with a wake; the plane shows its last frame meanwhile. The thread is made the first time a frame waits, sleeps until the next one does, and stops with the plane. The layer holds three drawables, as wgpu's do.

At 120 Hz the plane shows every refresh on an M5 Max and an M1 Max, frames shown 19 ms (M5) and 27 ms (M1) after they begin. Paced instead by when drawables were shown (Core Animation says so later than the drawable is free), an M1 Max's plane fell to 80 frames a second. Late frames, in the worst stretches: 0.6% on the M5 (paced by when they showed, 0.3–0.55%) and 0.2% on the M1.

`ZGX_PRESENT_LOG=1` logs each plane's frames every 240: those shown a second, drawables handed over a second, gaps between them, and start-to-shown.

It's opt-in: a view not marked opaque composites into the window's frame as before, and zgx-3d's views composite. The trade for one that is, in the viewport scene animating at 120 Hz, plane against composited (wgpu drawing the same scene alone in brackets):

| | M5 Max | M1 Max |
| --- | --- | --- |
| system GPU power | 104 against 205 mW (90) | 343 against 662 mW (316) |
| app instructions a second | 161–172 M against 122 M | 159 M against 111 M (115) |
| window server CPU | 21.4% against 18.6% (21.4) | 49.3% against 51.3% (47.7) |
| memory | 365 against 348 MB (335) | 169 against 153 MiB (149) |

The extra memory is the plane's three drawables (its composited texture goes once it shows).

Most of the extra CPU is Core Animation's, not zgx's. Counted by thread on the M5, the system's threads run about 100 M instructions a second for a plane against 54 M composited (about 380 K a frame): macOS's pacing of each `CAMetalLayer`'s frames (FramePacing) and Core Animation's presenting of its drawables, which a composited view never asks for. wgpu's own surfaces pay the same. zgx's own part:
- the drawables' thread, about 7 M a second;
- the wake that hands each drawable to the main thread, about 11–19 M a second more there than a view needing no wake.

Even both gone, a plane would use more CPU than composited; they remain a possible cut.

A ring of IOSurface sublayers, flipped as the window's tiles are, used the least app CPU of all (88–107 M a second on the M5, 96 M on the M1). It lost the GPU win, though: the window server composites a plain layer every frame however it flips (`hidden`, opacity or contents), so its GPU power was about composited's.

The GPU win comes with `CAMetalLayer`, and so does its CPU: a plane is for an app that weighs GPU power (a full-window 3D view on battery) above a few points of CPU.

A frame that switches a view between its layer and the window's frame costs about two frames' worth of time. That's measured at four switches a second; a view doing so rarely doesn't notice.

`scripts/viewport_probe.swift` captures `ZGX_MODE=switch viewport_workload` flipping between the two every quarter second, over magenta, and counts captures where magenta shows.

## Devices

`frame.device` and `frame.queue` are the ones the view is drawn with. They are the renderer's own where it draws with wgpu (Metal, Direct3D 12, Vulkan, GL). They may be another where it draws otherwise, so a painter makes its resources from the frame it is given and drops them in `device_changed` (a lost device made anew, the view shown in a window on another adapter).

Use the wgpu the renderer draws with, re-exported as `zgx_gpu::wgpu` (and `zgx_desktop::wgpu`): another major version is another crate whose types are not these. A new major version of wgpu is a new major version of zgx-gpu.

## Costs

Where no view shows, none: the image cache's hits never ask whether an image is a view's, no node or per-frame structure grows, and no device work is done. A view that is not invalidated costs what an image does. A frame costs its painter's work, one submission, and drawing its box's pixels from its texture once, which a window drawing straight into its surface doesn't pay.

The reference CPU rasterizer draws no GPU view's frame.
