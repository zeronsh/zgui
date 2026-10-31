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

## Textures: the size an image is drawn

An image's texture holds it at the size it is drawn, in device pixels, not
at its file's size: a 4000 by 3000 photo shown as a 300 by 420 card at 2x is
a 1120 by 840 texture (3.8 MB, not 48). It is made from the decoded pixels
halved as mip levels are, then brought to that size by area, so it is as
sharp as it shows and doesn't shimmer. Drawn larger (zoomed, a window moved
to a denser display, a `cover` crop enlarged), it is uploaded again at the
new size; drawn smaller, it keeps the one it has.

Image textures take at most four frames of the window's pixels, and at least
64 MiB. Past that, images not drawn this frame give theirs back, least
recently drawn first, and then what is drawn is held smaller (softer, said
once in the window's issues) rather than left out.

## Decoded pixels: kept files

A picture a script names by `src` keeps its file, not its pixels
(`ImageData::kept`, made by `zgx_gpu::assets::decode_image_kept`): loading it
reads its header alone. When it is first drawn, its pixels are decoded and
shrunk at once to the size it is drawn. A picture up to 256 by 256 pixels
(`zgx::image::SYNC_PIXELS`: an icon, an avatar) decodes then and there, in
the frame that first shows it, as before; a larger one decodes off the main
thread, one file at a time, so the frame before goes without it and the
window draws again as they arrive. `decode = "sync"` on `ui.image` (HTML's
`decoding`, `ImageData::set_decoding`) waits for any size, and `"async"`
sends even an icon off the thread. A file whose header reads but whose pixels
don't is tried once. Once the renderer has them as a texture they go, 1.5 s after the
last upload (`zgx::image::release_decoded`), whether or not the window keeps
animating. Drawn larger later, it decodes again at the new size.

A JPEG is never decoded larger than it is drawn: it decodes at 1/2, 1/4 or 1/8
of its size, the smallest that still covers it (`ImageData::kept_scaled`; the
DCT scaled, in zgx's fork of zune-jpeg, `vendor/zune-jpeg/ZGX_PATCH.md`), each
pixel the mean of those it stands for. Shrunk from there to the size drawn, it
is the picture decoding it whole and shrinking would give, near enough: on the
photo page below, a mean difference under 0.25 of a level a channel, 47.8 dB
at worst. A PNG decodes whole, as it has no smaller form.

The image cache's budget (`ScriptHost::image_cache`) counts these files'
bytes, and `zgx inspect memory` shows what is decoded at the moment as
`images decoded pixels held`. Code that borrows `ImageData::pixels` gets all
of them, decoded once and kept for the image's life; renderers ask for
`ImageData::pixels_at` instead.

On a page of eight 4000 by 3000 photos as 300 by 420 cards (macOS, 2x), the
process footprint went from 662 MB to 294 MB. Decoding them at half their size
(each drawn at 1120 by 840) took each decode's peak from 91.6 MiB to 18.6 MiB
and its time from 47 ms to 22 ms, and the process's peak from about 490 MB to
330 MB.

The threshold is from a page of 48 PNG icons drawn at 32 points (release,
this Mac, the median of two runs; "complete" is when the last one is drawn):

| Icons | Decoded at load (before) | Off the thread: first frame, complete | As drawn (`"auto"`) |
| --- | --- | --- | --- |
| 32 by 32 | 1.5 to 3.0 ms | 0.6 ms, 1.3 ms | 1.6 ms |
| 64 by 64 | 2.7 to 3.1 ms | 0.5 ms, 2.2 ms | 2.7 ms |
| 128 by 128 | 8.2 to 8.9 ms | 0.6 ms, 7.6 ms | 8.2 ms |
| 256 by 256 | 28.5 to 29.6 ms | 0.7 ms, 27.9 ms | 29.3 ms |

Decoding as drawn costs what decoding at load did, so a page of icons opens
in one frame, as it did before; one 256 by 256 PNG decodes in about half a
millisecond, one 1024 by 1024 in 8.

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
