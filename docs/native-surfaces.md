# Native CoreVideo surfaces (macOS)

`native_surface(label, Rc<NativeSurface>)` and `native_surface_signal(label, || frame)` provide a retained component with padding, dimensions, decoration, clipping, opacity and `ObjectFit`. The default is centered Contain. The view owns its pixels; use ordinary sibling/parent components for controls.

`unsafe NativeSurface::from_pixel_buffer(CFRetained<CVPixelBuffer>)` adopts an initialized, Metal-compatible immutable frame. The producer must finish writing before adoption and must not mutate its pixels or attachments while the scene or renderer retains it. Pixel-buffer pools cannot reclaim the retained source. Supported formats are premultiplied BGRA and full-range bi-planar NV12 (`420f`). NV12 uses full-range BT.601 conversion, matching GPUI 0.2.2; video-range, other color matrices and HDR are not claimed.

CoreVideo planes become textures on the renderer's own Metal device via wgpu's HAL import. BGRA is sampled directly; NV12 is converted to an RGBA texture by a compute shader. There is no CPU pixel readback or upload in this rendering path. Repainting the same frame reuses the import/conversion; a new immutable frame identity invalidates paint. Source dimensions, plane geometry, device texture limits and a combined 64 MiB active/in-flight texel-payload budget and 1,024-frame metadata limit are checked. The payload estimate excludes external IOSurface row padding and driver/CoreVideo bookkeeping; it is not a process-RSS bound. Backpressure waits for retired submitted reads only if a replacement would otherwise exceed that budget.

CVMetalTexture wrappers and their source frames remain alive until queue-completion callbacks permit retirement. Renderer teardown explicitly waits for outstanding device work before releasing them, including when no later frame/event arrives. Normal frames do not wait for GPU completion. Native surfaces are UI-thread resources; they deliberately do not assert arbitrary CoreVideo attachments are Send/Sync.

The reference CPU rasterizer omits native surfaces; it does not silently read them back. This API is exposed only on macOS and requires a Metal renderer.

The [macOS continuation packet](platform-validation/macos-continuation/README.md) records execution on macOS 26.4 / Apple M5 Max: the complete Metal suite passed, including `metal_imports_native_bgra_and_nv12_reuses_frames_and_releases_on_teardown` and the retained component fitting/binding test. The native `native_surfaces --smoke-test` window also alternated four BGRA/NV12 frames successfully. These establish real imported pixels, frame reuse and ownership/teardown behavior; they are not inferred from the earlier ARM cross-check or Linux WGSL validation. They do not establish video playback throughput, presentation cadence or hardware performance rankings.

The Mac validation bundle runs the complete GPU rendering suite, including native surfaces, before native window probes. Its interactive checklist also covers layout/media painting, clipboard exchange, editing and three separate native IME scenarios. The IME fixture has a 20-second lifetime per launch. Each operator case keeps a separate log; FAIL exits unsuccessfully, while skipped or unrecognized answers remain pending. CI runs the automated subset and preserves the pending manual checklist.

# Web views by platform

A web view (`zgx::web_view`, `ui.webview`) is the platform's own page, laid out as a node. How each platform shows it, and what that leaves out:

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Engine | WKWebView | WebView2 (`webview2` feature) | WebKitGTK 4.1 (`webkitgtk` feature; zgx-run has it) |
| Shown as | a native view between zgx's base and overlay planes, one Core Animation transaction | a DirectComposition visual between the planes, one commit | an image: drawn offscreen (`GtkOffscreenWindow`), uploaded into a GPU view's texture, composited with zgx's paint |
| Clip, rounded corners, opacity, transforms | yes | yes | yes |
| zgx content over the page | overlay plane | overlay plane | ordinary paint |
| Glass over the page | blurs the page | tint only | blurs the page |
| Pixels per frame | none copied | none copied | two CPU copies of the damaged part |
| Keys, reserved shortcuts, fall-through | yes | yes | yes |
| Input method | AppKit's | the Text Services Framework | the window's (XIM, text-input-v3), through a WebKit input method context of zgx's |
| `<select>` popups | WebKit's | WebView2's | a zgx menu |
| Clipboard | the system's | the system's | X11 the system's; Wayland through zgx's, as text |
| Cursor | the page's | the page's | named CSS cursors; image cursors show the arrow |
| Page accessibility | the platform's | the platform's | in the window's tree: the page's own (AT-SPI), under its box |
| Drags out of the page, files dropped on it | the platform's | the platform's | no |
| Missing engine | — | placeholder | placeholder (the library is opened as the first page shows) |

Linux's choice: Wayland cannot embed another client's surface, an X11 child window can't be faded or rounded smoothly or covered by translucent zgx content, and WPE WebKit isn't packaged on Ubuntu 24.04; drawing offscreen behaves the same on X11 and Wayland. See [the Linux notes](linux.md#web-views).
