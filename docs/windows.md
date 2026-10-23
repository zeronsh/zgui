# Windows desktop support

Use stable Rust with the `x86_64-pc-windows-msvc` target and Visual Studio Build
Tools (Desktop development with C++, including the Windows SDK). From PowerShell:

```powershell
cargo run -p zgui-desktop --example components --locked
cargo run -p zgui-desktop --example platform_services --locked
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

zgui leaves the global allocator to the application. Windows' default heap
is slower than mimalloc under zgui's allocation patterns. On an
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

## Presentation

wgpu runs on Direct3D 12 alone (`WGPU_BACKEND` still overrides, among the
backends built in). Every Windows 10 and 11 machine has a Direct3D 12
adapter: where the GPU's driver has no Direct3D 12, Windows' WARP renders in
software. GPUs that old (Intel before Skylake, AMD before GCN, NVIDIA before
Kepler) have no Windows Vulkan driver either, so Vulkan is a feature
(`vulkan`, on zgui-desktop, zgui-script or zgui-gpu), off by default, for a
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

`ZGUI_PRESENT_LOG=1` logs each shown present as DXGI reports it. The probes
measure the same things on any machine:

```powershell
cargo build --release -p zgui-desktop --example gallery --example input_latency --example animation_workload
python scripts/windows_idle_probe.py target/release/examples/gallery.exe
python scripts/windows_latency_probe.py target/release/examples/input_latency.exe --click 100,44 --region 24,24,520,40 --env ZGUI_ANIMATE=1
python scripts/windows_pacing_probe.py target/release/examples/animation_workload.exe
```

Resizing shows a frame drawn for each new size as soon as a window drawn
with GDI does, with no black or stretched frames on the way.
`scripts/windows_resize_probe.py` resizes `resize_probe` (edged in red, so a
frame for the new size shows red at its new right and bottom edges) and
captures it until that frame shows. `--reference` measures a plain GDI window
painting the same thing in `WM_PAINT`. Measured on an RTX 4090 at 60 Hz, at
100%:

| Change | GDI window | zgui | zgui, 3000 labels |
| --- | --- | --- | --- |
| `SetWindowPos` from another process | 33 ms; 10 black and 9 stale captures first | 16 ms; 1 stale | 22 ms; 1 black, 3 stale |
| Live resize, the right edge dragged (`--drag`) | 34 ms a step, none | 34 ms, none | 34 ms, none |
| Maximize and restore | 319 ms (Windows' animation) | 310 ms | 312 ms |
| Snap (Win+Left) | 297 ms (Windows' animation) | 298 ms | 308 ms |

```powershell
cargo build --release -p zgui-desktop --example resize_probe
python scripts/windows_resize_probe.py target/release/examples/resize_probe.exe --drag
python scripts/windows_resize_probe.py --reference --drag
```

## Title bars

`Titlebar::Inset` and `Titlebar::Hidden` are frameless windows that keep the
system shadow and Windows 11's rounded corners, resizing from bands 5 points in
from their edges. `Inset` adds Windows 11's caption buttons over the content's
top right, drawn by zgui and reported to Windows as its own (`WM_NCHITTEST`),
so they get the system's tooltips and the maximize button's snap layouts; the
buttons minimize, maximize, restore and close through the system's commands.
The title bar and the window menu (Alt+Space) follow the system's light or
dark mode. Where windows run right to left (the user's language, or
`ZGUI_DIRECTION=rtl`), `Inset`'s buttons sit at the top left, Close outermost,
as Windows lays out a mirrored window's, and `WindowInfo::titlebar_area` and
`controls` mirror with them. A `Standard` title bar keeps the system's own
layout, its buttons at the right: winit makes its windows unmirrored, and
Windows mirrors a caption only for a window made so (`WS_EX_LAYOUTRTL`).

Windows present through a DirectComposition visual, so `WindowOptions::transparent`
windows are transparent where zgui paints nothing, and `WindowOptions::glass`
puts the Windows 11 system backdrop behind them: Acrylic for the heads-up and
transient materials, Mica for window and page backgrounds, Mica Alt for title
bars (see [window controls](window-controls.md)). Presentation through the visual
measured the same as a window's own swap chain: every frame shown, the same
keystroke latency and idle cost.
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
scale, on whichever monitor it lands: zgui keeps it on its trigger's screen,
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
popup has none. `ZGUI_POPUP_WINDOWS=0` keeps popups inside their window, as
they are where Direct3D 12 is not available.

## Drags

A drag's preview shows in a window of its own, over every window, and moves
with the pointer without drawing again. It goes from one of the app's
windows to another (their destinations hear it, a tab torn off past them
hears `on_drag_outside`), exactly between windows on monitors of different
scales: their places are compared in the screen's pixels, each window's
points its scale apart (`popup_window::global_scale`).

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
takes it. Back over the app's windows the drag is zgui's again: let go there,
zgui's destinations take it and Windows drops nothing. Escape, or letting go
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
- The wheel's lines and characters to scroll at a time (Bluetooth & devices,
  Mouse): a notch scrolls each line or character at 100/3 pixels, 100 for
  the default 3, as Chromium does; "one screen at a time" scrolls by the
  window's height less an eighth.

An app theme or `theme_scope` that sets a colour or duration token itself
keeps its own value; the text size scales the font tokens of every theme.

## Web views

`zgui::web_view` pages are WebView2, with the `webview2` feature of
`zgui-desktop` (off by default; without it nothing below is compiled and web
views render placeholders):

```powershell
cargo run -p zgui-desktop --features webview2 --example web_view
```

With the feature, each window owns its DirectComposition tree: the window
renderer presents into its bottom visual, each page's visual sits above it in
paint order inside visuals clipping it to its node (rounded corners included,
antialiased), and zgui paint over a page (a popover, a composer) goes on an
overlay plane presenting into the top visual. A frame's presents and the pages'
geometry commit together, so pages move with zgui content. Presentation is
otherwise unchanged; the probes above measured the same with and without the
feature (every frame shown, 12.7 against 12.8 ms keystroke latency, the same
idle cost).

Pages are visual-hosted (`ICoreWebView2CompositionController`): the window's
procedure routes the pointer as `PlaneSplit::route` decides, so zgui content
drawn over a page takes its clicks, and a click on zgui content takes the
keyboard back from a page. Navigation, its state and events, script evaluation,
messages (`window.zgui.postMessage`), user scripts, app schemes, the navigation
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
  `https://zgui-file.invalid/` along with the files of its folder (WebView2
  asks the app for no `file:` request), so its origin is that host.

As with WebKit without delegates for them, pages save no downloads and get no
permission (camera, microphone, location, notifications, clipboard reads).

`scripts/windows_webview_smoke.py` checks the page against zgui content
around and over it.

## Touch and pen

Windows gives a window touch and pen as pointer messages, not a mouse's, and
zgui takes them as its pointer. A pen is a mouse. A finger presses where it
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

## Shipping a zgui app on Windows

zgui adds no packaging tooling; these are the pieces a Windows build needs,
and how they fit what zgui does at run time.

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

**Size.** zgui's comparison app (`component_workload`, every image format)
is 12.1 MiB x64 built as it ships: `lto = "fat"` and `codegen-units = 1`,
the workspace's `dist` profile, which an app sets in its own `[profile.release]`
or a profile like it (thin LTO is 0.7 MiB larger). What else changes it,
measured on that app:
- Image formats (zgui-desktop's features, see
  [images-and-animation.md](images-and-animation.md)): SVG is 0.75 MiB,
  JPEG 0.22, PNG 0.13. GIF's decoder is linked only into an app that shows a
  GIF.
- The `vulkan` feature adds 1.1 MiB (see Presentation).
- `panic = "abort"` saves 3.2 MiB, a quarter. It is the app's choice, not
  zgui's default: zgui contains a panic in an event handler, an effect or a
  task by unwinding it ([application.md](application.md)), and with abort
  such a panic ends the app instead.
- `opt-level = "s"` saves 3.0 MiB; measure the app's own frames before
  shipping it, since zgui's hot paths are measured at `opt-level = 3`.
- `strip` saves nothing on Windows: MSVC writes symbols to the `.pdb` beside
  the executable, which you keep for crash dumps rather than ship.

**Resources.** Embed them from a build script, for example with
`winresource`:
- An icon: zgui's windows take the executable's first icon group for the
  title bar, taskbar and Alt+Tab. Without one, Windows' default shows.
- Version information (product name, company, version): Explorer's
  Properties and Task Manager show it.
- An application manifest that declares the Common Controls 6 dependency.
  Without it, the system message boxes zgui's prompts use draw in the old
  style. zgui sets per-monitor DPI awareness itself at startup, so the
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
  the file or URL as an argument (`"C:\…\app.exe" "%1"`). zgui reads existing
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
  identity of its own. zgui still sets the application ID as the process's
  explicit AppUserModelID, so check that pinning and the taskbar behave as
  you expect in your package; zgui's CI does not build MSIX packages.

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
cargo build -p zgui-desktop --example gallery --example dpi_transition --example native_ime --example selectable_text --example platform_services --example drag_drop --example scrollbars --example workbench --example form --example selectable_text --example popup_probe --example drag_probe --example drop_target_probe --example window_probe
python scripts/windows_desktop_smoke.py target/debug/examples/gallery.exe
python scripts/windows_dpi_smoke.py target/debug/examples/dpi_transition.exe
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean
python scripts/windows_platform_smoke.py target/debug/examples
python scripts/windows_settings_smoke.py target/debug/examples/scrollbars.exe --both
python scripts/windows_touch_smoke.py target/debug/examples
python scripts/windows_popup_smoke.py target/debug/examples
python scripts/windows_drag_smoke.py target/debug/examples
python scripts/windows_window_smoke.py target/debug/examples
```

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
  synthesized on/off key.
- `windows_platform_smoke.py`: selected display text copied as text and HTML
  and read by another process, text from another process pasted into an
  editor, the file picker and the message box (each owned by its window, the
  window disabled while it shows), and a file dragged in from another process
  through OLE drag and drop, as from Explorer.
- `windows_webview_smoke.py` (`web_view` built with `--features webview2`):
  the page loads and its title reaches the state, a click in the page reaches
  it, zgui's composer over the page takes clicks and typing and its script
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
  shows at rest with it on. `--both` runs each setting both ways (the user's
  own restored after).
- `windows_popup_smoke.py` (`popup_probe`): no popup window before the first
  popup; a menu opens past its window's bottom edge within milliseconds of
  the click and shows on screen; hovering and clicking, or pressing the
  trigger and releasing over an item, chooses it; the menu moves with its
  window and closes as the app deactivates and on a press on the title bar.
  It posts mouse messages to its own windows only, so it leaves the pointer
  alone. `--idle` adds the probe's idle spans (about a minute).
- `windows_drag_smoke.py` (`drag_probe` and `drop_target_probe`, a
  test-owned OLE drop target in another process placed against the probe):
  the chip dragged out onto the target, which receives its text while the
  source hears it accepted; the target's description of the drop shown under
  the drag image (the white of Windows' label, absent when the source does
  not allow descriptions); the probe animating at the display's rate through
  `DoDragDrop`'s loop; dragged out and back, let go in its own window,
  Windows dropping nothing; Escape over the target cancelling, and the
  preview sliding back to the chip in its own window; a tab dropped onto the
  probe's other window, and torn off past both. It never lets go over any
  window but its own and the target's: it checks the way is clear before each
  drag, and cancels with Escape if another window covers the target.

- `windows_window_smoke.py` (`window_probe`): a second window opens a step
  on from the first (`CW_USEDEFAULT`); a window with `frame_autosave` moved and
  closed reopens there at its size, left maximized reopens maximized, and at a
  saved frame no monitor shows opens on a monitor instead; `under_pointer`
  puts its point under the pointer. It leaves nothing in the registry.

`scripts/windows_uia_dump.ps1 -Hwnd <window>` prints a window's UI Automation
tree as Narrator reads it: control types, names, values, toggle and range
states, and text selections.
