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
network requests. Image decoding is available through `zgx_gpu::assets`.

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
[animation example](../crates/zgx-desktop/examples/animated_image.rs) accepts a
GIF path and includes a pause/resume control. A `TaskRunner` provider is required
for asynchronous loading/playback; the desktop host supplies one.

For code-driven animation (spinners, transitions, springs), await the window's display-paced frame clock instead of a timer; see [animation frames](application.md#animation-frames).

## Decoded pixels kept while an image shows (a post-0.1.6 candidate)

A decoded image stays in memory while a node shows it, and, once none
does, within the cache's budget (`ScriptHost::image_cache`, 48 MiB by
default). The GPU's copy lives in the renderer's atlas, which may evict it
while the node still shows. The CPU copy is then what uploads it again: a
scroll back to it costs an upload, not a decode.

A mounted image could keep its encoded file instead, as 3D textures do
(`zgx_3d::Texture`), and decode it again when the atlas has evicted it and
it shows again, or after a lost device. That trades heap for decodes:

- **Heap:** a page of 40 images of 640 by 480 (30 PNG, 10 JPEG) holds 49.2
  MB of decoded pixels while it shows, against 18.9 MB of files. Real
  photographs compress far better than that page's patterns: their JPEGs
  are 5 to 20 times smaller than their pixels.
- **Decodes:** 1.7 ms a PNG, 2.4 ms a JPEG of that size (M5 Max, one core).
  A scroll back over ten evicted images would decode for about 20 ms, more
  than two frames at 120 Hz, so it would have to decode off the main
  thread, the image showing a frame or two late.

Not in 0.1.6: today's trade (heap held while an image shows) keeps scroll
backs free of decodes. Each decode already holds about the image once
(`assets::decode_image`: PNG 1.09, JPEG 1.36 images at its peak).

## Image formats are features

SVG, PNG, JPEG and GIF decoding are cargo features of `zgx-gpu`, `zgx-desktop`
and `zgx-script` (`svg`, `png`, `jpeg`, `gif`), all on by default. An app that
never shows a format can leave its decoder out of the binary:

```toml
zgx-desktop = { version = "…", default-features = false, features = ["png"] }
```

A source in a format left out fails to decode, with an error naming the feature,
as any undecodable image does. Canvases, gradients, shadows and decorations need
none of them. Leaving all four out takes about 1 MB off `component_workload`
(12.8 MB to 11.8 MB on macOS arm64).
