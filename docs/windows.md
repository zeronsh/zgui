# Windows desktop support

Use stable Rust with the `x86_64-pc-windows-msvc` target and Visual Studio Build
Tools (Desktop development with C++, including the Windows SDK). From PowerShell:

```powershell
cargo run -p zgx-desktop --example components --locked
cargo run -p zgx-desktop --example platform_services --locked
```

Windows uses the existing winit event loop and wgpu renderer (DX12/Vulkan),
arboard clipboard, AccessKit Windows accessibility backend, and rfd native file
pickers and message boxes. Both file pickers and prompts receive their owning
native window. Application menus use `cx.app_menu_bar()` and Control shortcuts.
No additional Windows-only dependency is needed.

### ARM64

`aarch64-pc-windows-msvc` builds too (`cargo check --workspace --all-targets
--locked` passes; no CI job runs it). Install the target
(`rustup target add aarch64-pc-windows-msvc`) and Build Tools' ARM64 compiler
("MSVC … ARM64 build tools"). A Developer prompt for ARM64 sets everything;
otherwise point the C code built for the target at the ARM64 compiler and
keep the host's environment for build scripts:

```powershell
$env:CC_aarch64_pc_windows_msvc = "...\VC\Tools\MSVC\<version>\bin\Hostx64\arm64\cl.exe"
$env:AR_aarch64_pc_windows_msvc = "...\VC\Tools\MSVC\<version>\bin\Hostx64\arm64\lib.exe"
cargo build --release --target aarch64-pc-windows-msvc
```

### Allocator

zgx leaves the global allocator to the application. Windows' default heap
is slower than mimalloc under zgx's allocation patterns. On an
i7-12700KF, release builds, with mimalloc as `#[global_allocator]`:
- `heavy_headless` takes 13% less time a tick: 1.77–1.97 ms down to
  1.51–1.54 ms;
- `anime_bench`'s build, mount and layout take 15–45% less time;
- its steady animation frames, which barely allocate, are unchanged;
- each process commits about 40 MiB more private memory, with about the same
  working set.

An application that mounts large trees often can opt in:

```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

### Memory

Most of a zgx process's private memory on Windows is the graphics driver's,
not zgx's. Measured on an RTX 4090 (NVIDIA 32.0.16.1742) by walking the
process's address space stage by stage:
- a Direct3D 12 device costs 39.5 MiB and its one queue another 20.5 MiB, a
  floor no app on Direct3D 12 goes under (GPUI's Direct3D 11 costs 43 MiB
  less, device to visible window: [below](#what-a-frame-costs-below-zgx));
- zgx's own share is small: a 2 MiB view heap (images hold at most 16,384
  views a window), wgpu's 4 MiB upload block, its caches.

zgx opens a device on the one adapter it draws with (DXGI's first for low
power, `vendor/wgpu-hal/ZGX_PATCH.md`): wgpu would otherwise open one on
every adapter to choose, and each driver keeps memory after it is released.

### Startup

A window's first frame used to compile its pipelines' shaders with FXC every
launch, about 45 ms. They are kept in `%LOCALAPPDATA%\zgx\shaders` (about
52 KB) from the first launch on, and read back after
(`vendor/wgpu-hal/ZGX_PATCH.md`). Measured with
`scripts/windows_startup_probe.py`, the lab scene's first text, medians of 12
alternating launches on an RTX 4090:

| | First text | CPU by then |
| --- | --- | --- |
| zgx, shaders compiled | 330 ms | 273 ms |
| zgx, shaders kept | 278 ms | 219 ms |
| GPUI Fast | 307 ms | 273 ms |

Most of the rest is the GPU driver opening its device (about 160 ms for a
Direct3D 11 device and 180 ms for Direct3D 12, alone in a process), a floor
any app on this GPU pays.

A zgx window appears with its content, never empty: it draws through a
DirectComposition visual with no redirection bitmap, so the compositor has
nothing of it to show until its first frame is presented, and showing the
window before drawing costs nothing. `scripts/windows_first_frame_probe.py`
guards it: it reads every composition of the window's header on screen as it
opens (Desktop Duplication) and fails on one that shows the window's
background alone. Windows fades and scales a new window in; those
compositions show the text, however faint, and pass. Two of the comparison
frameworks' windows, MyGo's and Rune's, each show one empty composition
every launch.

### What a frame costs below zgx

Much of an animating zgx frame's CPU is spent below zgx. On an RTX 4090
(NVIDIA 32.0.16.1742, i7-12700KF, 60 Hz), with each window in front and
uncovered, Windows' High performance power plan and medians of alternating
runs (`scripts/windows_compare.py`), `animation_workload` takes 1.69 M cycles
a frame and GPUI Fast's `animate` 1.62 M. GPUI Fast draws through Direct3D 11
into a DirectComposition flip swap chain of three buffers at sync interval 0,
as zgx presents through wgpu.

What the APIs cost alone, with nothing drawn, is measured by
`crates/zgx-gpu/examples/d3d_frame_floor.rs`. It uses one 1280x800 window and
the same kind of swap chain, and clears and presents each compositor frame.
Medians of 6 runs in rotating order, each window in front and uncovered,
under the High performance plan:

| Path | M cycles a frame, every thread | On the drawing thread (median K cycles) | Private memory added |
| --- | --- | --- | --- |
| No device (the message pump and `DwmFlush`) | 0.03 | | |
| Direct3D 11, the driver's internal threading off | 0.45 | clear 77, present 244 | 52 MiB |
| Direct3D 11, the driver's default | 0.83 | clear 5, present 97 (the rest on the driver's thread) | 52 MiB |
| Direct3D 12 | 0.74 | resets 46, recording 47, `ExecuteCommandLists` 145, fence signal 13, present 321 | 96 MiB |
| Direct3D 12 through wgpu, as zgx sets it up | 0.93 | acquire 31, encode 129, submit 224, present 325 | (not comparable: a plain `request_adapter` opens a device on every adapter) |

```powershell
cargo build --release -p zgx-gpu --example d3d_frame_floor
target\release\examples\d3d_frame_floor.exe d3d12   # or none, d3d11, d3d11-threaded, wgpu
```

On this machine:
- **Direct3D 12's fixed cost is the floor.** A frame costs 0.29 M cycles more
  than Direct3D 11 with the driver's threading off, which is how GPUI's renderer runs:
  its profile shows no busy driver thread. Swap chain flags and the feature
  level change nothing, and a Direct3D 12 renderer needs its direct queue.
- **wgpu adds about 0.19 M cycles a frame,** spread over some 25 functions
  rather than one to patch.
- **Below the floors, zgx's frame is the cheaper.** Take each path's floor
  from its frame: zgx's `animate` spends about 0.76 M cycles above wgpu's,
  GPUI Fast's about 1.17 M above Direct3D 11's. GPUI Fast's lead in that
  scene is the API's.
- **The memory gap is the API's.** Direct3D 12's device, queue and swap chain
  take 44 MiB more than Direct3D 11's. That is most of the difference between
  zgx's and GPUI Fast's private memory (about 122 and 87 MiB in `animate`).
- **At rest, Direct3D 12 keeps two of NVIDIA's driver threads busy** (about
  1.2 M cycles a second, `windows_idle_probe.py`); Direct3D 11 does not.

Release builds give the driver no object names (wgpu's
`DISCARD_HAL_LABELS`): wgpu would otherwise name every command list it
opens, five a frame, at about 3 K cycles each. A capture in PIX or RenderDoc
reads better with them: set `WGPU_DISCARD_HAL_LABELS=0` to keep them in a
release build. Debug builds keep them.

A direct Direct3D 11 renderer, with the driver's threading off, would save
about 0.48 M cycles a frame, 44 MiB and the driver's threads at rest on this
machine; a direct Direct3D 12 one, about 0.19 M cycles and no memory. (Measured
with the windows behind another and Windows' Balanced plan, these read 0.75 M
and 0.3 M: a covered window presents for less, and a clocked-down processor
counts more cycles for the same work.) These numbers are NVIDIA's alone:
AMD's and Intel's drivers price submissions and presents differently, and
need measuring before they guide a choice. GPU time was not measured; this
GPU is nearly idle at these loads. Under the Balanced plan, every mode's
cycles, the empty loop's too, once doubled together and then came back: the
processor clocked down. Hold High performance, and compare only modes run in
alternation.

### Direct3D 11 (experimental)

zgx-desktop's (and zgx-gpu's) `d3d11` feature, off by default, builds in a
renderer path drawing with Direct3D 11; `ZGX_D3D11=1` at run time takes it,
and without it nothing changes. No Direct3D 12 device is opened then: wgpu
runs on its no-op backend, keeping the renderer's handles without a driver,
and frames, the glyph atlas and the window's swap chain (the same
flip-sequential chain, dirty rectangles and pacing) are Direct3D 11's
(`crates/zgx-gpu/src/d3d11_frame.rs`). `ZGX_D3D11_THREADED=1` leaves the
driver's own threading on, which on NVIDIA costs the CPU win below.

```powershell
cargo build --release -p zgx-desktop --features d3d11 --example component_workload
$env:ZGX_D3D11 = "1"; target\release\examples\component_workload.exe
```

It draws what the Windows suite's scenes do: quads (borders, rounded
corners, shadows' shading), text, clip regions with their rounded corners
and fades, kept frames, and scroll copies. Images, isolated and transformed
layers, backdrop blur, polylines and shader fills are not drawn yet: a frame
needing one fails with an error naming it (and panics in a debug build),
since there is no Direct3D 12 to fall back to. What it draws is Direct3D 12's
frame byte for byte (`tests/d3d11_parity.rs`, at 1×, 1.5× and 2×). Both run
the same HLSL compiled by FXC: Direct3D 11 ahead of time (`dxbc/`), and zgx
pins wgpu's Direct3D 12 to FXC too. Left to choose, wgpu takes DXC wherever
dxcompiler.dll is on PATH (a developer shell with the Windows SDK), and DXC's
code rounds antialiased edges a level apart from FXC's.
`WGPU_DX12_COMPILER=dxc` still picks it.

What it trades, measured with the Windows suite on an RTX 4090
([results](results/d3d11-proof-windows-2026-10-27/)):
- **Memory:** about half. The lab scene at rest takes 61 MiB of private
  memory against 124, 6 MiB over a bare Direct3D 11 window; GPU shared memory
  goes from 31 MiB to 2.
- **CPU:** 15 to 36% fewer cycles in every animating scene, 47% fewer at
  rest. Startup to the first text: 227 ms against 280.
- **GPU:** about 10 µs more a frame (0.4 to 0.8 ms a second at 60 Hz, at the
  GPU's lowest clock). That is Direct3D 11's own: `d3d_frame_floor`, which
  only clears and presents, pays the same against Direct3D 12.
- Pacing, the compositor's work and board power are level.

These are NVIDIA's numbers; AMD's and Intel's drivers have not been measured.

## Presentation

wgpu runs on Direct3D 12 alone (`WGPU_BACKEND` still overrides, among the
backends built in). Every Windows 10 and 11 machine has a Direct3D 12
adapter: where the GPU's driver has no Direct3D 12, Windows' WARP renders in
software. GPUs that old (Intel before Skylake, AMD before GCN, NVIDIA before
Kepler) have no Windows Vulkan driver either, so Vulkan is a feature
(`vulkan`, on zgx-desktop, zgx-script or zgx-gpu), off by default, for a
driver whose Direct3D 12 fails: with it, Vulkan stands in where Direct3D 12
has no adapter. Swap chains use DXGI's flip model
at sync interval 0: a present replaces one the compositor has not taken yet,
without tearing, so input draws at once and shows on the next refresh even
while an animation runs. Animation frames start half a refresh after the
compositor's clock ticks (`DCompositionWaitForCompositorClock` on Windows 11,
`DwmFlush` on Windows 10), far from the point where it takes them, on one
thread that parks while nothing animates; a window at rest wakes for nothing.
Measured on an RTX 4090 at 60 Hz:

| | before | now |
| --- | --- | --- |
| Gallery at rest | 10.6 M cycles/s, Vulkan and D3D12 drivers loaded | 2.7 M cycles/s (the driver's own threads) |
| Keystroke to composed screen, at rest | 12–14 ms median | 12–14 ms median |
| Keystroke to composed screen, animating | 47 ms median | 12 ms median |
| Animation at the display's rate | 57.7–60 fps, timer drifting against refreshes | every refresh, none repeated or dropped |

A window presents through a swap chain of the renderer's own
(`crates/zgx-gpu/src/dxgi_present.rs`), made on wgpu's Direct3D 12 queue and
shown through the window's DirectComposition visual. It is flip-sequential,
so each buffer keeps what it held: the buffer is the retained frame. A frame
repaints what changed since that buffer was last shown and presents with the
regions it changed as `Present1`'s dirty rectangles, so the compositor
composes only those. wgpu's swap chain discards a buffer once shown, so
presenting through it means copying the whole retained frame into a buffer
every frame, with its barriers and a command list of its own. Measured
against it on an RTX 4090 at 60 Hz (alternating runs of one binary, in front,
High performance):

| Scene | Cycles a frame | App GPU | Compositor GPU | Compositor CPU |
| --- | --- | --- | --- | --- |
| animate | −1.8% | −33% | −16% | same |
| vlist fling | −1.9% | −45% | same | −7% |
| heavy | −4.1% | −12% | −5% | −25% |
| lab | −5.4% | −41% | −4% | −11% |
| at rest | same | none either way | same | same |

Startup to the first text: 346 → 280 ms (component_workload, medians of 12
launches). `ZGX_DXGI_PRESENT=0` presents through wgpu's swap chain, for a
release, to compare with; renderers presenting into a caller's visual
(`composition`) always do. `scripts/windows_present_probe.py` reads off the
screen that every buffer is brought up to date; with
`ZGX_DXGI_PRESENT_WHOLE=1` each present covers its whole buffer, which the
compositor otherwise takes only within the dirty rectangles.

Frames of an app's own timing (a 60 Hz stream, a timer) are drawn as their
timer fires, at whatever phase it has against the display's refresh. DWM
latches a frame for a refresh shortly before its vblank. Measured as each
present's time after the compositor's vblank against whether the screen then
missed a refresh (lab stream, 60 Hz, Desktop Duplication captures):

| Present after the vblank | Followed by a missed refresh |
| --- | --- |
| 0 to 9.5 ms | none (of about 5,000) |
| 13.5 to 15 ms | about 2% |
| the last 1.5 ms | 13% |

A timer drifts slowly against the display, so a launch whose timer sits near
the latch keeps there for seconds: in captures, about one launch in eight lost
about 60 refreshes in 8 s. A frame of the app's own timing ready within 3.5 ms
before the vblank (or 1 ms after it) now waits, on a high-resolution timer,
until 1 ms past it (`frame_source::latch_wait`; the window widens by 1 ms
while frames wait, so a timer at its edge does not alternate). Input presents
at once. With the lab stream's ticks pinned to a phase (alternating runs of
one binary, 6 captures each):

| Ticks at | Missed refreshes per capture, waiting | Not waiting |
| --- | --- | --- |
| 15.5 ms (near the latch) | 0, 1, 0, 0, 0, 0 | 76, 72, 54, 70, 53, 45 |
| 13 ms (the window's edge) | 0 each | 1, 0, 4, 0, 5, 0 |
| 6 ms | 0 each | 0, 0, 0, 0, 2, 0 |

The app's cycles, its GPU time and the compositor's are the same within the
runs' spread (lab stream +1.9%, heavy +0.5%, animate −1.8%), and a window at
rest does no work. Keeping the compositor's clock running for such frames
instead fixed the misses too but cost 6 to 8% more cycles on the lab stream (a
second wake a frame). `zgx inspect switch refresh_grid off` presents at once,
as before.

`ZGX_PRESENT_LOG=1` logs each shown present as DXGI reports it. The probes
measure the same things on any machine:

```powershell
cargo build --release -p zgx-desktop --example gallery --example input_latency --example animation_workload
python scripts/windows_idle_probe.py target/release/examples/gallery.exe
python scripts/windows_latency_probe.py target/release/examples/input_latency.exe --click 100,44 --region 24,24,520,40 --env ZGX_ANIMATE=1
python scripts/windows_pacing_probe.py target/release/examples/animation_workload.exe
```

The idle probe brings the window to the front, as a user has it, and fails
if it presents while at rest (`--max-presents`, none by default); it sums
zgx's own threads apart from the graphics driver's (`--max-own-cycles-per-second`).
The lab scene at rest, in front: no presents, and no cycles on zgx's own
threads; the 1.2 M cycles a second left are two of NVIDIA's Direct3D 12
driver threads, which Direct3D 11 (GPUI Fast's, MyGo's) does not keep busy.
The compositor redraws nothing of the window then (Desktop Duplication's
dirty rectangles over six seconds), though its GPU time shows 0.1 ms a
second while a zgx window is open, against none for theirs.

Resizing shows a frame drawn for each new size as soon as a window drawn
with GDI does, with no black or stretched frames on the way.
`scripts/windows_resize_probe.py` resizes `resize_probe` (edged in red, so a
frame for the new size shows red at its new right and bottom edges) and
captures it until that frame shows. `--reference` measures a plain GDI window
painting the same thing in `WM_PAINT`. Measured on an RTX 4090 at 60 Hz, at
100%:

| Change | GDI window | zgx | zgx, 3000 labels |
| --- | --- | --- | --- |
| `SetWindowPos` from another process | 33 ms; 10 black and 9 stale captures first | 16 ms; 1 stale | 22 ms; 1 black, 3 stale |
| Live resize, the right edge dragged (`--drag`) | 34 ms a step, none | 34 ms, none | 34 ms, none |
| Maximize and restore | 319 ms (Windows' animation) | 310 ms | 312 ms |
| Snap (Win+Left) | 297 ms (Windows' animation) | 298 ms | 308 ms |

```powershell
cargo build --release -p zgx-desktop --example resize_probe
python scripts/windows_resize_probe.py target/release/examples/resize_probe.exe --drag
python scripts/windows_resize_probe.py --reference --drag
```

## Title bars

`Titlebar::Inset` and `Titlebar::Hidden` are frameless windows that keep the
system shadow and Windows 11's rounded corners, resizing from bands 5 points in
from their edges. `Inset` adds Windows 11's caption buttons over the content's
top right, drawn by zgx and reported to Windows as its own (`WM_NCHITTEST`),
so they get the system's tooltips and the maximize button's snap layouts; the
buttons minimize, maximize, restore and close through the system's commands.
The title bar and the window menu (Alt+Space) follow the system's light or
dark mode. Where windows run right to left (the user's language, or
`ZGX_DIRECTION=rtl`), `Inset`'s buttons sit at the top left, Close outermost,
as Windows lays out a mirrored window's, and `WindowInfo::titlebar_area` and
`controls` mirror with them. A `Standard` title bar keeps the system's own
layout, its buttons at the right: winit makes its windows unmirrored, and
Windows mirrors a caption only for a window made so (`WS_EX_LAYOUTRTL`).

Windows present through a DirectComposition visual, so `WindowOptions::transparent`
windows are transparent where zgx paints nothing, and `WindowOptions::glass`
puts the Windows 11 system backdrop behind them: Acrylic for the heads-up and
transient materials, Mica for window and page backgrounds, Mica Alt for title
bars (see [window controls](window-controls.md)). Presentation through the visual
measured the same as a window's own swap chain: every frame shown, the same
keystroke latency and idle cost. Only transparent and glass windows present
with alpha: an opaque window's swap chain is opaque from the start
(`GpuRenderer::for_window_with_transparency`), so the compositor copies its
frames instead of blending them over what lies behind. One made opaque whose
first frame's background lets what is behind show is made again with alpha
(DXGI keeps a swap chain's alpha for its life). With alpha everywhere, the Desktop Window Manager spent 7 to 14% more
GPU time a frame on a zgx window than on an opaque one, more than on GPUI's
(`windows_compare.py` reads the compositor's share).
`scripts/windows_chrome_smoke.py` checks the buttons and the resize bands.

Windows take the executable's own icon (its first icon group, the one
Explorer shows) for the title bar, the taskbar and Alt+Tab, sized for the
window's DPI. Embed one as a resource from the app's build script, for
example with the `winresource` crate (`WindowsResource::new().set_icon("app.ico")`);
without one, Windows' default shows.

A new window that would not fit its monitor's work area (the screen less the
taskbar) at that monitor's scale shrinks to fit, never below its minimum
size, and moves inside it. A window given `bounds` keeps them.

## Popups

Menus, submenus, context menus, selects, popovers and tooltips reach past
their window, as Windows' own menus do: they flip and clamp to the work area
of the monitor their trigger is on, not to the window. Each shows in a window
of its own (`popup_window/win32.rs`): owned by its window (above it, minimized
with it, never on the taskbar) and never activated, so the window keeps the
keyboard and typing reaches a popover's field. A popup draws at its window's
scale, on whichever monitor it lands: zgx keeps it on its trigger's screen,
so on a monitor of another scale it sits beside a trigger the window draws
there at its own, the same size. It moves with its window, and closes as the app deactivates, when the
title bar is pressed, on Escape and on a press outside it.

A menu or popover that is one rounded panel gets DWM's rounded corners, rim
and shadow, as Windows 11 frames its menus.

Known gap: menus and popovers keep the theme's flat background, where Windows
11's own menus are acrylic. DWM draws a system backdrop
(`DWMSBT_TRANSIENTWINDOW`) only behind an activated window, and a flat colour
behind one never activated; a popup must not take activation from its window,
which keeps the keyboard. The undocumented `SetWindowCompositionAttribute`
acrylic is not used.

No popup window exists before the pointer first reaches something that opens
one, and a closed one is kept hidden for the next: an app that never opens a
popup has none. `ZGX_POPUP_WINDOWS=0` keeps popups inside their window, as
they are where Direct3D 12 is not available.

## Drags

A drag's preview shows in a window of its own, over every window, and moves
with the pointer without drawing again. It goes from one of the app's
windows to another (their destinations hear it, a tab torn off past them
hears `on_drag_outside`), exactly between windows on monitors of different
scales: their places are compared in the screen's pixels, each window's
points its scale apart (`popup_window::global_scale`). A drag that moves its
window (`View::drag_moves_window`, a window's only tab) moves it with the
pointer at once (`SetWindowPos`), the grab point staying under it; what is
beneath is found down the z-order past the moving window and drag previews,
skipping cloaked and click-through windows, as `WindowFromPoint` does.

A drag carrying what other apps take (`View::drag_export`) becomes Windows'
as it leaves the app's windows (`native_drag/win32.rs`): `DoDragDrop` with
the items as Windows apps read them (text as `CF_UNICODETEXT`, files as
`CF_HDROP`, a URL as `UniformResourceLocatorW` and as text, an image as
`PNG`), and the preview's last frame as the drag image, which Windows draws
and moves (`IDragSourceHelper`) over targets that show drag images, as
Explorer and browsers do. A target that describes the drop ("Copy to
Documents", Explorer's) has its description shown under the image, in
Windows' label: the source allows it (`DSH_ALLOWDROPDESCRIPTIONTEXT`). The
source hears `on_drag_end` accepted when an app
takes it. Back over the app's windows the drag is zgx's again: let go there,
zgx's destinations take it and Windows drops nothing. Escape, or letting go
where nothing takes it, slides the preview back to its source.

`DoDragDrop` runs a modal loop of its own, in place of winit's: windows go on
animating and painting through it (a frame every refresh), as through a live
resize.

## System settings

The built-in themes take these from Windows, read at startup and again when
the user changes them (`WM_SETTINGCHANGE`, `WM_SYSCOLORCHANGE`), in every
window whatever its title bar:

- The accent colour (Settings, Personalization, Colors): text selections are
  drawn in it with white text, as WinUI's text fields and Notepad draw them.
- High contrast (contrast themes): backgrounds, surfaces, text, borders, the
  accent, selections and hover take the contrast theme's system colours.
- Animation effects off (Accessibility, Visual effects): the motion duration
  tokens are zero, so what animates with them changes at once.
- Text size (Accessibility): the font size and line height tokens scale with
  it, 100% to 225%, and spacing and icons keep their size. A subtree whose
  layout depends on fixed text, like a terminal grid, opts out with
  `ThemeOverride::fixed_text_size`.
- Always show scrollbars off (Accessibility, Visual effects; the default):
  scrollbars hide at rest and show while the user scrolls
  (`Ui::set_overlay_scrollbars`), as Edge's and WinUI's do.
- The text cursor's thickness (Accessibility, Text cursor): editors draw
  their caret that many points wide (`SPI_GETCARETWIDTH`, `Ui::set_caret_width`),
  changed live. macOS and GTK have no such setting to follow. Windows' text
  cursor indicator does not find zgx's caret yet: it reads a system caret or
  UI Automation's `GetCaretRange`, and zgx gives neither.
- The text cursor's blink rate (Accessibility, Text cursor): the focused
  editor's caret blinks at it (`GetCaretBlinkTime` each way), solid while
  the user types or moves it, and stops after the caret timeout
  (`SPI_GETCARETTIMEOUT`, 5 s), as Windows' own carets do; "none" keeps it
  solid. Each blink is one small frame: about 3 M cycles on an RTX 4090, and
  a focused field at rest after the timeout costs what an unfocused window
  does (`windows_caret_blink_probe.py`).
- The wheel's lines and characters to scroll at a time (Bluetooth & devices,
  Mouse): a notch scrolls each line or character at 100/3 pixels, 100 for
  the default 3, as Chromium does; "one screen at a time" scrolls by the
  window's height less an eighth.

An app theme or `theme_scope` that sets a colour or duration token itself
keeps its own value; the text size scales the font tokens of every theme.

## Web views

`zgx::web_view` pages are WebView2, with the `webview2` feature of
`zgx-desktop` (off by default; without it nothing below is compiled and web
views render placeholders):

```powershell
cargo run -p zgx-desktop --features webview2 --example web_view
```

With the feature, each window owns its DirectComposition tree: the window
renderer presents into its bottom visual, each page's visual sits above it in
paint order inside visuals clipping it to its node (rounded corners included,
antialiased), and zgx paint over a page (a popover, a composer) goes on an
overlay plane presenting into the top visual. A frame's presents and the pages'
geometry commit together, so pages move with zgx content. Presentation is
otherwise unchanged; the probes above measured the same with and without the
feature (every frame shown, 12.7 against 12.8 ms keystroke latency, the same
idle cost).

Pages are visual-hosted (`ICoreWebView2CompositionController`): the window's
procedure routes the pointer as `PlaneSplit::route` decides, so zgx content
drawn over a page takes its clicks, and a click on zgx content takes the
keyboard back from a page. Navigation, its state and events, script evaluation,
messages (`window.zgx.postMessage`), user scripts, app schemes, the navigation
policy, new-window requests, profiles, zoom and DevTools work as on macOS. The
loader is linked in; the runtime is the system's (Windows 11 ships it, Windows
10 gets it with Edge). Without one, web views render placeholders and say so
once on stderr. Profiles live in `%LOCALAPPDATA%\<executable name>\WebView2`.

Unlike macOS:

- Glass over a page shows its tint, not a blur of the page (DirectComposition
  cannot blur another process's content).
- `WebViewState::back_list` and `forward_list` stay empty (WebView2 lists no
  history); `go_to_history(n)` goes through the page's `history.go(n)`.
- Form submissions are reported as `NavigationKind::Link` (user-initiated) or
  `Other`; progress is 0.1 at start, 0.5 when content arrives, 1 when loaded.
- App schemes are registered with the WebView2 environment: windows whose web
  views serve different sets of schemes keep their profiles in separate data
  folders.
- `load_html` with a `file:` base serves the page from
  `https://zgx-file.invalid/` along with the files of its folder (WebView2
  asks the app for no `file:` request), so its origin is that host.

As with WebKit without delegates for them, pages save no downloads and get no
permission (camera, microphone, location, notifications, clipboard reads).

`scripts/windows_webview_smoke.py` checks the page against zgx content
around and over it.

## Touch and pen

Windows gives a window touch and pen as pointer messages, not a mouse's, and
zgx takes them as its pointer. A pen is a mouse. A finger presses where it
lands; lifted there it clicks, and held there first (0.6 s) it right-clicks,
as Windows' press-and-hold does; held on selectable text it selects the
word, moving on extends the selection by words, and lifting opens its menu
(Copy), as Edge does. Moved, it pans the scroll views under it
(coasting on after a flick) and presses nothing, or, where nothing scrolls
that way, drags what it pressed: a slider, a selection. Two fingers pan
together; pinching does nothing yet.

## Activation

`Application::application_id` makes the app one instance an ID and session:
a later launch (a file opened from Explorer, a URL scheme) hands its files
and URLs to the running one as `ApplicationEvent::OpenUrls`, or with none an
`ApplicationEvent::Reopen`, and exits. The ID is also the AppUserModelID the
taskbar groups windows under. See [platform services](platform-services.md).

## Shipping a zgx app on Windows

zgx adds no packaging tooling; these are the pieces a Windows build needs,
and how they fit what zgx does at run time.

**The executable.**
- Build with `cargo build --release`.
- Mark the binary as a GUI app so no console window opens with it:
  `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` at the
  top of `main.rs`. Its standard output then goes nowhere, so log to a file
  if you need one.
- Rust's MSVC binaries link the C runtime dynamically (`vcruntime140.dll`),
  which not every machine has. Either link it statically
  (`-C target-feature=+crt-static`, e.g. in `.cargo/config.toml` under
  `[target.x86_64-pc-windows-msvc] rustflags`) or have the installer add the
  Visual C++ Redistributable.
- ARM64 machines run x64 builds emulated. Build `aarch64-pc-windows-msvc`
  as well for native speed (see ARM64 above).

**Size.** zgx's comparison app (`component_workload`, every image format)
is 12.1 MiB x64 built as it ships: `lto = "fat"` and `codegen-units = 1`,
the workspace's `dist` profile, which an app sets in its own `[profile.release]`
or a profile like it (thin LTO is 0.7 MiB larger). What else changes it,
measured on that app:
- Image formats (zgx-desktop's features, see
  [image loading](images-and-animation.md)): SVG is 0.75 MiB,
  JPEG 0.22, PNG 0.13. GIF's decoder is linked only into an app that shows a
  GIF.
- The `vulkan` feature adds 1.1 MiB (see Presentation).
- `panic = "abort"` saves 3.2 MiB, a quarter. It is the app's choice, not
  zgx's default: zgx contains a panic in an event handler, an effect or a
  task by unwinding it ([building an application](application.md)), and with abort
  such a panic ends the app instead.
- `opt-level = "s"` saves 3.0 MiB; measure the app's own frames before
  shipping it, since zgx's hot paths are measured at `opt-level = 3`.
- `strip` saves nothing on Windows: MSVC writes symbols to the `.pdb` beside
  the executable, which you keep for crash dumps rather than ship.

**Resources.** Embed them from a build script, for example with
`winresource`:
- An icon: zgx's windows take the executable's first icon group for the
  title bar, taskbar and Alt+Tab. Without one, Windows' default shows.
- Version information (product name, company, version): Explorer's
  Properties and Task Manager show it.
- An application manifest that declares the Common Controls 6 dependency.
  Without it, the system message boxes zgx's prompts use draw in the old
  style. zgx sets per-monitor DPI awareness itself at startup, so the
  manifest needn't, though declaring `PerMonitorV2` there does no harm.

**Identity and single instance.** Give the app an
`Application::application_id` (reverse DNS, `com.example.Mail`):
- It becomes the process's AppUserModelID, which the taskbar groups and pins
  windows by. The Start menu shortcut your installer makes must carry the
  same ID (the shortcut's `System.AppUserModel.ID` property; WiX's
  `ShortcutProperty`, Inno Setup's `AppUserModelID`), or pinning the running
  app and launching it from Start count as two apps.
- It also makes the app one instance a session: a second launch hands its
  files and URLs to the first (`ApplicationEvent::OpenUrls`) and exits.
  Register file associations and URL schemes to launch the executable with
  the file or URL as an argument (`"C:\…\app.exe" "%1"`). zgx reads existing
  paths and URLs from the arguments and leaves the rest to the app.
- Persisted script state (`ui.persist`) lives in `%APPDATA%\<app_id>`.

**Code signing.**
- Sign the executable, and the installer or package, with an Authenticode
  certificate (`signtool sign /fd sha256 /tr <timestamp URL> /td sha256`).
  Timestamping keeps the signature valid after the certificate expires.
- Unsigned downloads meet SmartScreen's "Windows protected your PC" warning
  until they build reputation. A signed one builds reputation by its
  publisher. A cloud signing service (Microsoft's Trusted Signing, or a
  certificate authority's) avoids keeping a hardware key yourself.

**Web views.** Apps built with the `webview2` feature need the WebView2
runtime:
- Windows 11 includes it, and most Windows 10 machines have it with Edge.
- An installer can make sure with Microsoft's Evergreen Bootstrapper or
  Standalone Installer.
- Or ship a Fixed Version runtime beside the app and point the
  `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` environment variable at it.
- The loader is linked into the executable, so there is no
  `WebView2Loader.dll` to ship.
- Without a runtime, web views show their placeholder and the rest of the
  app works.

**MSIX or a plain installer.**
- A plain executable, as a zip or with an MSI (WiX), Inno Setup or NSIS
  installer, is the simplest: the installer writes the Start menu shortcut
  (with the app ID above), the file associations and URL schemes (under
  `HKEY_CURRENT_USER\Software\Classes` for a per-user install), and the
  uninstall entry.
- MSIX gives a clean install and uninstall, updates through App Installer or
  the Store, and declares associations and schemes in its manifest
  (`uap:FileTypeAssociation`, `uap:Protocol`). The package gives the app an
  identity of its own. zgx still sets the application ID as the process's
  explicit AppUserModelID, so check that pinning and the taskbar behave as
  you expect in your package; zgx's CI does not build MSIX packages.

## Limitations

- Cancelling a file-picker or prompt future invalidates result delivery but
  does not dismiss its native dialog. A pending native dialog can retain its
  parent until dismissed. Backend failures can appear as cancellation.
- Rendered application menus are supported; native Win32 menu bars are not.
- Ghostty terminals and `NativeContent::Platform` views are macOS only; web
  views need the `webview2` feature (see Web views).
- The Linux benchmark sampler and the X11/Wayland smoke scripts do not run on
  Windows; Windows has its own (below).

## Smoke checks

`scripts/windows_*_smoke.py` drive an example through real Win32 input
(`SendInput`, so winit sees the messages a keyboard and mouse send) and check
what it reports. They need only Python's standard library and an unlocked,
interactive desktop, and take the pointer and keyboard while they run:

```powershell
cargo build -p zgx-desktop --example gallery --example dpi_transition --example native_ime --example selectable_text --example platform_services --example drag_drop --example scrollbars --example workbench --example form --example selectable_text --example popup_probe --example drag_probe --example drop_target_probe --example window_probe --example text_selection --example clipboard_image
python scripts/windows_desktop_smoke.py target/debug/examples/gallery.exe
python scripts/windows_dpi_smoke.py target/debug/examples/dpi_transition.exe
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean --tsf
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean --tsf --replace-while-composing
python scripts/windows_tsf_smoke.py target/debug/examples/native_ime.exe
python scripts/windows_tsf_japanese_smoke.py target/debug/examples/native_ime.exe
python scripts/windows_uia_find_smoke.py target/debug/examples/selectable_text.exe
python scripts/windows_platform_smoke.py target/debug/examples
python scripts/windows_settings_smoke.py target/debug/examples/scrollbars.exe --both
python scripts/windows_touch_smoke.py target/debug/examples
python scripts/windows_popup_smoke.py target/debug/examples
python scripts/windows_drag_smoke.py target/debug/examples
python scripts/windows_window_smoke.py target/debug/examples
python scripts/windows_text_smoke.py target/debug/examples/text_selection.exe
python scripts/windows_clipboard_image_smoke.py target/debug/examples/clipboard_image.exe
python scripts/windows_contrast_smoke.py target/debug/examples/text_selection.exe
python scripts/windows_caret_blink_probe.py target/release/examples/text_selection.exe
cargo build --release -p zgx-gpu --example present_check
python scripts/windows_present_probe.py target/release/examples/present_check.exe --lose
cargo build --release -p zgx-gpu --example framepace
cargo build --release -p zgx-desktop --example component_workload
python scripts/windows_first_frame_probe.py target/release/examples/component_workload.exe target/release/examples/framepace.exe
```

- `windows_first_frame_probe.py` (`component_workload`, idle): five launches
  at the same place; from before the window opens, no composition of its
  header strip is the background alone (luminance spread under 2 of 255)
  where the settled header shows text. Each launch prints the strip's spread
  per composition.

- `windows_present_probe.py` (`present_check`): a mark moves along a row
  over 30 frames, each drawing its damage alone; on screen every place it
  passed shows the background again and the last the mark, through the
  renderer's own swap chain (with dirty rectangles and whole), and wgpu's;
  `--lose` loses the device after, and a new renderer for the same window
  draws the frame again.

- `windows_desktop_smoke.py`: pointer activation, typing, selection, clipboard,
  Tab focus, resize and close in the gallery.
- `windows_dpi_smoke.py`: the display's scale goes 100% to 150% and back while
  `dpi_transition` runs (as Settings changes it, for the signed-in user, and
  restored after). Each scale must reach the window as per-monitor DPI, scale
  its physical size, keep its layout in logical pixels and take pointer and
  typing at the scaled positions.
- `windows_ime_smoke.py`: the Microsoft Korean IME composes hangul from real
  keystrokes (preedit, commit), and a click into another editor mid-composition
  commits it where it began. Korean composes without a dictionary, so it works
  on any install; `--install-korean` adds it to the user's input methods for the
  run and restores the list after. The Japanese and Chinese IMEs need their
  language's typing features (an administrator install) before they compose.
  `native_ime --open-ime` opens the IME itself: input methods ignore a
  synthesized on/off key. `--tsf` runs it through the Text Services Framework
  (`ZGX_TSF=1`, below), checking the editors' text. `--replace-while-composing`
  has the app set the field's text mid-composition (`native_ime
  --replace-while-composing`): the composition ends there and none of it is
  committed, through either path.
- `windows_tsf_smoke.py` (`ZGX_TSF=1`): the emoji panel (Win+., Enter) inserts
  through zgx's text store, and typing after it reaches the editor. It needs
  no input method beyond English.
- `windows_uia_find_smoke.py`: UI Automation finds text in a zgx document (`FindText`, AccessKit's zgx patch): a word forward, one without case, one backward, nothing for what it doesn't have, each match with its box.
- `windows_tsf_japanese_smoke.py` (`ZGX_TSF=1`): the Microsoft Japanese IME
  converts and commits 日本語 through the text store. It prints PENDING
  without the Japanese typing features installed.

### The Text Services Framework (`ZGX_TSF=1`)

Editors take input methods through winit's IMM32 path by default. With
`ZGX_TSF=1`, a window gives them Windows' Text Services Framework instead:
- **The text store.** An `ITextStoreACP2` of zgx's own (`tsf.rs`, over
  `text_store.rs`, which is tested on every platform) holds the focused
  editor's text in UTF-16 offsets, with the composition in place of the
  selection while one is open.
- **What it gives input methods.** They read the text around the caret and
  ask where any range of it shows (`GetTextExt`, through
  `EditorHandle::text_area`: the box around it across its lines, in the
  window's points through any zoom and the editor's scroll), to place their
  candidates and prediction lists at the clause they convert. Text services
  (the emoji panel, dictation, the touch keyboard) insert at the selection.
- **The composition** is the editor's preedit, committed when the input
  method ends it, as on other platforms. Its clauses are the input method's
  display attributes (`GUID_PROP_ATTRIBUTE`), read after each of its edits
  (`ITfTextEditSink::OnEndEdit`): each clause underlined apart from the next,
  thick under the one being converted.

Reconversion and input scopes are the next stages; the framework becomes the
default after them.
- `windows_platform_smoke.py`: selected display text copied as text and HTML
  and read by another process, text from another process pasted into an
  editor, the file picker, the save dialog (a name typed into it reaches the
  app as the path) and the message box (each owned by its window, the
  window disabled while it shows), and a file dragged in from another process
  through OLE drag and drop, as from Explorer.
- `windows_text_smoke.py` (`text_selection`): a double click within the
  system's double-click time selects a word, and presses further apart do
  not; a second press outside the system's double-click box (`SM_CXDOUBLECLK`
  by `SM_CYDOUBLECLK`) is a new first press, inside it a double click; a
  triple click selects the line; copied lines reach the clipboard ending in
  CR LF, as Windows apps take them, and CR LF pasted in leaves no CR in the
  editor; Windows' emoji panel (Win+.) opens under the caret, for carets at
  two places. Windows places the panel from the caret area zgx gives input
  methods (`ImmSetCandidateWindow`), where their candidate windows show:
  without it, the panel opens away from the window. The emoji Enter chooses
  in the panel is typed at the caret, through the input method's path. Selections are read
  through UI Automation. It saves the user's
  clipboard and restores it after, keeps what it puts there out of clipboard
  history, and skips the copy and paste checks while cloud clipboard sync is
  on.
- `windows_glass_smoke.py` (`glass_counter`): each glass material asks the
  compositor for its system backdrop, read back from outside the app
  (`DWMWA_SYSTEMBACKDROP_TYPE`): Acrylic for the heads-up glass, Mica for a
  window's background, Mica Alt for a title bar's. Acrylic must also show: an
  empty patch of the window changes as another test window takes the
  activation (Windows draws a solid colour for an inactive window's acrylic),
  where an opaque fill would not.
- `windows_clipboard_image_smoke.py` (`clipboard_image`): images other apps
  copy paste whole, every pixel compared: a 24-bit DIB, a 32-bit DIB whose
  fourth byte is unused (a GDI bitmap's, as Paint copies: opaque), a DIBV5
  with an alpha mask (as Firefox copies; arboard's decoder dropped these) and
  a PNG alone; text alone pastes no image; and an image zgx copies is there
  as a PNG, a DIBV5 with its alpha and the plain DIB Windows offers older
  apps. The user's clipboard is saved and restored as in the text smoke, and
  the copy check is skipped while cloud clipboard sync is on.
- `windows_vlist_smoke.py` (each framework's `vlist` adapter, `comparisons/README.md`):
  as the macOS runner checks the scene, the probe rows' heights agree within
  2 points in the one font file every adapter loads, and in `scroll`, `fling`
  and `jump` zgx's list shows no unfilled rows: 10 captures a second for 8 s,
  scanned inside the rows' left padding for runs of the window's background.
  The other frameworks' counts are reported. Its windows are put above the
  others while they run, never activated.
- `windows_caret_blink_probe.py` (`text_selection`, a release build): the
  caret blinks one frame a phase at the system's blink rate (counted with
  `ZGX_PRESENT_LOG`), the process's cycles a blink, and with the field still
  focused past the caret timeout, no frame in 10 s and the same cycles a
  second as the window at rest with nothing focused.
- `windows_contrast_smoke.py` (`text_selection`): a contrast theme switched on
  while the window runs, as the Left Alt+Left Shift+Print Screen shortcut does,
  turns its background into the theme's window colour, its text the theme's
  text colour and a double-clicked word's selection the theme's highlight; off
  again, the window takes back the colours it had. It switches the signed-in
  user's contrast theme for about ten seconds, repainting the whole desktop,
  and restores the user's settings after, the shortcut's own included; with a
  contrast theme already on, it checks that one and switches nothing.
- `windows_webview_smoke.py` (`web_view` built with `--features webview2`):
  the page loads and its title reaches the state, a click in the page reaches
  it, zgx's composer over the page takes clicks and typing and its script
  changes the page, the menu popover draws above the page and takes its
  clicks, the wheel scrolls the page, and after a resize the page still fills
  its node.
- `windows_touch_smoke.py`: synthetic touch and pen (no touchscreen needed):
  a tap clicks, a finger drags a slider, pans a list and flings it, a held
  finger opens a context menu or selects text by words, and a pen's stroke
  drags rather than pans.
- `windows_settings_smoke.py`: the window menu (Alt+Space) is dark in dark
  mode and light in light mode, and a scrollbar hides at rest, shows while
  the user scrolls and hides after with "Always show scrollbars" off, and
  shows at rest with it on. `--both` runs each setting both ways, and
  switches the app mode both ways while a window runs (`primary_selection`,
  whose theme follows the system's): its title bar and content must follow
  within seconds. The user's own settings are restored after.
- `windows_popup_smoke.py` (`popup_probe`): no popup window before the first
  popup; a menu opens past its window's bottom edge within milliseconds of
  the click and shows on screen; hovering and clicking, or pressing the
  trigger and releasing over an item, chooses it; the menu moves with its
  window and closes as the app deactivates and on a press on the title bar.
  It posts mouse messages to its own windows only, so it leaves the pointer
  alone. `--idle` adds the probe's idle spans (about a minute).
- `windows_drag_smoke.py` (`drag_probe` and `drop_target_probe`, a
  test-owned OLE drop target in another process placed against the probe):
  the chip dragged out onto the target, which receives its text and a file
  (`CF_HDROP`, as Explorer takes one) while the
  source hears it accepted; the target's description of the drop shown under
  the drag image (the white of Windows' label, absent when the source does
  not allow descriptions); the probe animating at the display's rate through
  `DoDragDrop`'s loop; dragged out and back, let go in its own window,
  Windows dropping nothing; Escape over the target cancelling, and the
  preview sliding back to the chip in its own window; a tab dropped onto the
  probe's other window, and torn off past both; text another app drags in
  (`drop_target_probe --source`, `CF_UNICODETEXT`) let go over the Words
  field's end landing there, copied, and refused over the chip. It never
  lets go over any
  window but its own and the target's: it checks the way is clear before each
  drag, and cancels with Escape if another window covers the target.

- `windows_tab_smoke.py` (`tab_windows`, and `drop_target_probe` as a
  test-owned backdrop): a tab torn off opens in a window of its own with the
  tab under the pointer; that window's only tab moves the window, following
  the pointer to the pixel; dropped on the other window's strip it merges
  there. It lays the windows out from the work area, so it fits at any
  scale (checked at 100% and 150%).
- `windows_window_smoke.py` (`window_probe`): a second window opens a step
  on from the first (`CW_USEDEFAULT`); a window with `frame_autosave` moved and
  closed reopens there at its size, left maximized reopens maximized, and at a
  saved frame no monitor shows opens on a monitor instead; `under_pointer`
  puts its point under the pointer. It leaves nothing in the registry.

`scripts/windows_uia_dump.ps1 -Hwnd <window>` prints a window's UI Automation
tree as Narrator reads it: control types, names, values, toggle and range
states, and text selections. Read that way (2026-10-24), the component
catalog and the dashboard example (`zgx-run crates/zgx-script/examples/...`)
give, on the first read, the names and values made from state that wait for
a screen reader (`accessible_label_when`): tabs, the select's value, the date
picker's days, the dashboard's instruments with their trends and its charts
named by what they show. A grid's cells are named by their text
(`vendor/accesskit_windows/ZGX_PATCH.md`).

UI Automation clients that are not screen readers attach to windows on most
desktops (the shell's touch keyboard host, Voice Access, Magnifier, test
tools). While no screen reader runs (none has set `SPI_GETSCREENREADER`, as
Narrator, NVDA and JAWS do), a window's tree reaches them paced: focus at
once, names, values and selection and geometry at most 100 ms late, so a
dashboard's live values do not rebuild the tree every frame. With a screen
reader running, names, values and selection go out at once. AccessKit
answers clients' reads from the tree it was last given, so a test reading a
value straight after an action that does not move focus may wait up to
100 ms for it.
