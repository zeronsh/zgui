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
| Pixels per frame | none copied | none copied | three CPU copies of the damaged part ([below](#what-a-linux-page-frame-costs)) |
| Keys, reserved shortcuts, fall-through | yes | yes | yes |
| Input method | AppKit's | the Text Services Framework | the window's (XIM, text-input-v3), through a WebKit input method context of zgx's |
| `<select>` popups | WebKit's | WebView2's | a zgx menu |
| Clipboard | the system's | the system's | X11 the system's; Wayland through zgx's, as text |
| Cursor | the page's | the page's | named CSS cursors; image cursors show the arrow |
| Page accessibility | the platform's | the platform's | in the window's tree: the page's own (AT-SPI), under its box |
| Drags out of the page, files dropped on it | the platform's | the platform's | no |
| Missing engine | — | placeholder | placeholder (the library is opened as the first page shows) |

Linux's choice: Wayland cannot embed another client's surface, an X11 child window can't be faded or rounded smoothly or covered by translucent zgx content, and WPE WebKit isn't packaged on Ubuntu 24.04; drawing offscreen behaves the same on X11 and Wayland. See [the Linux notes](linux.md#web-views).

## What a Linux page frame costs

Each page frame takes three copies of its damaged part on the CPU:

1. WebKit paints its buffer into zgx's cairo image surface (`gtk_widget_draw`, on GTK's thread).
2. zgx copies the surface's rows into the frame it shares with the UI thread.
3. The UI thread hands them to wgpu (`write_texture`), which copies them into its staging memory before the GPU's upload.

Measured October 2026 on a WebGL page with full damage, at 1076 × 700 pixels (3.0 MB). The release `web_view` example ran in the zgx-linux container (arm64, 6 cores, no GPU), with WebKit drawing about 37 frames a second:

| Copy | Thread | Per page frame |
| --- | --- | --- |
| WebKit's paint into the surface | GTK | 457–832 µs |
| Into the shared frame | GTK | about 75 µs |
| `write_texture` | UI | 108–129 µs |

zgx's own copies (2 and 3) come to about 0.2 ms a frame at that size, roughly 0.6% of a core at 30 frames a second. That's a fifth or less of the path's cost. The rest is WebKit's paint, which GTK 3's public API cannot skip. So cutting them was left out:

- Painting into the shared buffer directly needs two buffers. On a partial update, WebKit then repaints the previous frame's damage too, at about six times a copy's cost per pixel.
- Painting into a mapped wgpu staging buffer would save copy 3 alone, for a ring of buffers passed between the threads.

On a machine with a GPU, WebKit 2.52 renders into dma-bufs, and GTK 3's offscreen window reads them back (on GTK's thread) before copy 2. That readback is what zero-copy would remove, and only a Linux machine with a GPU can measure it; the container and CI's runners have none. The routes, as packaged on Ubuntu 24.04:

- **WebKitGTK 4.1 on GTK 3 (today's):** the offscreen window gives zgx a cairo drawing and nothing else. WebKit's dma-bufs stay inside its private backing store.
- **WebKitGTK 6.0 on GTK 4.14 (packaged):** WebKit hands GTK its frames as dma-buf textures. Whether GTK 4.14 lets an embedder read a texture's file descriptors still needs checking. GTK 4 has no offscreen window, so pages would need another host.
- **WPE WebKit:** its `wpe_view_backend_exportable_fdo` API hands an embedder each frame as a dma-buf or EGLImage, the direct route into a wgpu texture. It isn't packaged on Ubuntu 24.04.

Idle stays free: a static page costs no CPU once drawn (0.0% over 20 s). Memory (PSS) on that page: the app 177 MB, WebKit's web process 202 MB, its network process 23 MB.

# Terminals

`zgx::terminal::terminal(&term)` (Luau `ui.terminal`) shows a `Terminal` controller's session in a node laid out like any other. Two backends draw it; the desktop host picks by its features:

| Feature | Platforms | Draws it | Needs to build |
| --- | --- | --- | --- |
| `ghostty` | macOS | a Ghostty surface, a native view over the window (as a web page is) | Zig 0.16 (Ghostty is built from a pinned source) |
| `terminal` | macOS, Linux, Windows | zgx itself, in the window's own text (`zgx-terminal`) | Rust alone |

With both on macOS, Ghostty draws. zgx-run turns `terminal` on, so Luau apps have terminals on every platform; a Rust app opts in with `zgx-desktop/terminal`. Without either a node shows a labelled placeholder and its controller still answers.

**The engine-drawn terminal** (`zgx-terminal`, built on `alacritty_terminal` 0.26):

- *The session*: a pseudoterminal (openpty, and `setsid`, on Linux and macOS; ConPTY on Windows) running the login shell (`$SHELL -l`; PowerShell on Windows) or `command` (`/bin/sh -c` on Unix, as written on Windows), with `TERM=xterm-256color`, `COLORTERM=truecolor` and `TERM_PROGRAM=zgx`. Its I/O runs on a thread of its own that sleeps in `poll` until the program writes or the app does; it parses output into the screen and wakes the UI once until the UI has read it, so a flood of output costs one update a frame and an idle shell nothing (no timers, no frames). 10,000 lines of scrollback.
- *Drawing*: each row of the grid is a canvas: backgrounds, the selection, the cursor and strikes are filled rectangles the GPU draws without rasterizing; text is runs of cells in one style, shaped in the platform's monospaced font with ligatures off; a wide character (CJK, emoji) or one whose glyph may come from another font (symbols, private-use icons) is placed alone, centred on its cells, so the grid never drifts. Only rows the screen model marks damaged are worked out again, and only those that differ draw again. When output scrolls the screen, the rows still on it move up with it (their container is moved, without layout) and only the new lines draw: a build log's line a frame repaints one row, not the screen. Output is read at most once a refresh: a program printing faster (ConPTY hands it over in small chunks, a thousand times a second) has all of it read and drawn at the next refresh, and output after a pause (a key's echo) is read as it arrives.
- *Fidelity*: 16 colours, 256 and true colour, bold, italic, dim, inverse, hidden, underline (every style drawn as a line), strikethrough, wide and emoji cells, cursor shapes (block, bar, underline; a hollow block unfocused), the alternate screen, bracketed paste, focus reporting, mouse reporting (X10, normal, button and any-motion tracking, UTF-8 and SGR encodings; Shift selects over it), the wheel scrolling scrollback or, on the alternate screen, sending arrows, DEC 2026 synchronized output, OSC 8 hyperlinks (⌘-click, Ctrl-click elsewhere, opens one), OSC 52 copy and, as the app answers (`Terminal::on_clipboard_read`, Luau `on_clipboard_read`), reads; colour queries (OSC 4, 10, 11) and device attributes.
- *Keys*: xterm's encodings (cursor and keypad application modes, modifiers as `CSI 1;m`), Ctrl chords as control characters, Alt as an Escape prefix (Option types its own characters on macOS). A program asking for the kitty keyboard protocol (`CSI > flags u`, as Neovim and coding agents do) gets it: disambiguated escapes (`CSI 99;5u` for Ctrl+C), event types (repeats and releases), alternate keys (the shifted key, and the US layout's base key), every key as an escape and its associated text; its flags answer `CSI ? u`. Modifier keys alone aren't reported. ⌘C and ⌘V on macOS, Ctrl+Shift+C and Ctrl+Shift+V elsewhere, copy the selection and paste; on Linux a selection made is the primary selection and a middle click pastes it (`Platform::primary_text`); other ⌘ chords are the app's. Input methods compose in a hidden editor kept at the cursor, which assistive technology doesn't see: TSF, XIM, Wayland's text input and AppKit place their candidates there, the composition shows at the cursor, and what they commit goes to the program.
- *State*: title (OSC 0/2), working directory (OSC 7), bell, progress (OSC 9;4), finished commands with their duration (OSC 133 C/D), notifications (OSC 9, OSC 777), the exit code, the grid's size, and the foreground process: the pty's foreground process group on Unix, the shell's newest descendant on Windows, or what `zgx_desktop::resolve_terminal_processes` says is behind it (a session server's client), with its Program Status (OSC 7501).
- *Accessibility*: the terminal is a document named Terminal; each row is a text label with what it shows, so a screen reader can read the screen line by line; the latest output is a polite live region of its own, saying the lines output brought or what a line gained, not the whole screen, and not the shell's echo of what was typed. Checked with Orca over AT-SPI on X11 (`terminal_smoke.py --orca`) and with macOS's AX API (rows are `AXStaticText`, the output an `AXApplicationStatus` group). The hidden editor keys go through isn't in the tree.
- *Bindings* (`Terminal::binding`) take Ghostty's names for what they share: `copy_to_clipboard`, `paste_from_clipboard`, `select_all`, `clear_screen`, `scroll_to_top`, `scroll_to_bottom`, `scroll_page_up`, `scroll_page_down`, `scroll_page_lines:N`, `scroll_page_fractional:F`, `increase_font_size:N`, `decrease_font_size:N`, `reset_font_size`, `text:…`, `csi:…`, `esc:…`.

What it can't do yet, and how it differs from Ghostty: no images (kitty graphics, sixel); an emoji with VS16 takes one cell, not two; underline styles (double, curly, dotted, dashed) all draw as one line, in the text's colour; the cursor doesn't blink; no Ghostty configuration, themes or shaders (its colours are a fixed dark palette, the default background the node's own); a glyph from a fallback font inside a run of narrow text can sit off the grid; `snapshot` is the node as the window last drew it.

Measured on an M5 Max (macOS 26.4), each in a 900×600 window, three runs each: catting 290 MB of coloured text took the engine-drawn terminal 4.1–5.8 s of the app's CPU against Ghostty's 6.8–8.3 s; idle after, it spent nothing in 5 s, Ghostty 0.02–0.03 s; its process kept 147–154 MB resident against Ghostty's 171–175 MB. Streaming output, its frames took about 1 ms of CPU. A terminal whose 10,000 lines of scrollback are full adds 13–17 MB.
