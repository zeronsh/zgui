# Image loading and animation

`ImageCache::load(key, fetch)` shares one pending application-supplied future among
requests for that key. Successful decoded images enter a bounded LRU cache;
failures can be retried. Dropping the last pending request drops its transport
future. The default pixel budget is 16 MiB, configurable up to 64 MiB; at most
256 keys are retained and keys are limited to 1024 UTF-8 bytes. Clearing a cache
cannot allow an older in-flight generation to replace a newer request.

`async_image(label, cache, key, fetch, loading_view, error_view)` wraps this in an
owned component with loading, success and error children. Use a shared cache
through a provider when multiple components load the same resources. Transport
and credentials belong to the application; the framework does not start hidden
network requests. Image decoding is available through `zgui_gpu::assets`.

`decode_gif` composites GIF offsets and Keep/Background/Previous disposal into
bounded immutable full frames. Background disposal clears to transparent for
GUI compositing. An absent repeat extension plays once; finite GIF repetitions
add to the first play. Zero-delay frames use 100 ms. Decoded pixels are limited
to 64 MiB and 4096 frames. Application-provided `Animation` data uses frame
intervals from 10 ms to one day and equal-sized frames.

`animated_image` and `animated_image_controlled` are ordinary styled image views.
They use a monotonic timeline, skip missed frames after a delayed wake, and retain
the final frame when finite playback ends. Each active player has one cancelable
sleep. Fully clipped/offscreen/zero-opacity allocations pause; host visibility,
minimization, occlusion and suspension also pause through `Ui::set_presented`.
Uncovered visibility resumes from the paused position. Overlap by another
ordinary sibling is not an occlusion test. Removal cancels the owned task.

Frames remain decoded, but GPU textures follow the renderer's live-image policy:
it need not retain every animation frame on the GPU. The
[animation example](../crates/zgui-desktop/examples/animated_image.rs) accepts a
GIF path and includes a pause/resume control. A `TaskRunner` provider is required
for asynchronous loading/playback; the desktop host supplies one.

For code-driven animation (spinners, transitions, springs), await the window's display-paced frame clock instead of a timer; see [animation frames](application.md#animation-frames).

## Image formats are features

SVG, PNG, JPEG and GIF decoding are cargo features of `zgui-gpu`, `zgui-desktop`
and `zgui-script` (`svg`, `png`, `jpeg`, `gif`), all on by default. An app that
never shows a format can leave its decoder out of the binary:

```toml
zgui-desktop = { version = "…", default-features = false, features = ["png"] }
```

A source in a format left out fails to decode, with an error naming the feature,
as any undecodable image does. Canvases, gradients, shadows and decorations need
none of them. Leaving all four out takes about 1 MB off `component_workload`
(12.8 MB to 11.8 MB on macOS arm64).
