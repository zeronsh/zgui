# Native window controls

`WindowContext::window` is a cloneable, thread-safe `WindowHandle`. Its methods queue requests onto the application thread:

```rust,no_run
# fn configure(cx: &mut zgx_desktop::WindowContext) {
let window = cx.window.clone();
window.set_title("Notes — saved");
window.set_inner_size(800., 600.); // logical client dimensions
window.set_maximized(true);
window.set_minimized(false);
window.set_visible(true);
window.request_focus();
# }
```

Title, size, visibility, and requested minimize/maximize state survive suspension and native recreation. Commands also work while a newly opened window is awaiting construction. A focus request made without a native window is delivered once after creation. Closed handles ignore subsequent requests. Updating the title invalidates the accessible window title as well.

Size requests reject zero, negative, nonfinite, or dimensions exceeding `u32::MAX`. The operating system can constrain any request. Observe `WindowContext::viewport` for actual logical client dimensions; receiving a command is not confirmation that the window manager accepted it. Native resize and scale events update the size used for recreation.

Winit currently does not implement dynamic visibility or focus requests on Wayland, and Wayland cannot programmatically unminimize. Focus requests on other platforms can also be rejected by focus-stealing prevention. Minimize/maximize recreation state records application requests; it does not promise to mirror every window-manager action.

Run the interactive example with `cargo run -p zgx-desktop --example window_controls`. On X11 with a window manager, append `-- --smoke-test` to request title, resize, minimize/restore, maximize/restore, hide/show, and close. The example checks actual viewport resize acknowledgments. External X11 property and map-state observations additionally verified minimization, maximization, restoration, hiding and showing; see `platform-validation/window-controls-x11.json`. This does not constitute native macOS or Wayland control validation.

Confirmed hidden, minimized, zero-sized or occluded surfaces skip frame preparation and GPU submission. Pending damage remains until restoration; application tasks and model updates continue. Unknown visibility is treated as drawable, so unsupported compositor queries do not blank the window. A redraw request cannot override explicit native occlusion. `hidden_updates` and `scripts/hidden_updates_smoke.py` check actual X11 unmapped states and latest restored pixels after 40 timed model updates. This does not imply that all reactive application computations stop while hidden.

## Title bars, glass and drag regions

`WindowOptions::titlebar` styles the title bar: `Titlebar::Standard`, `Titlebar::Inset` (transparent and untitled, content running under it, the traffic lights inset in a 40-point strip, Electron's `hiddenInset`) or `Titlebar::Hidden` (no title bar and no window buttons, the window keeping its corners and shadow). On Windows, `Inset` puts Windows 11's caption buttons (minimize, maximize, close; 46 by 32 points each) over the content's top right, which Windows treats as its own: tooltips, and snap layouts on the maximize button. Both are frameless windows that keep the system shadow and, on Windows 11, rounded corners, and resize from bands 5 points in from their edges. On Linux both are undecorated windows, as GTK's header bars are, resizing from the same bands through the window manager, and `Inset` draws libadwaita's window buttons in the order and on the side the desktop asks ([Linux](linux.md#title-bars)). `WindowOptions::glass` puts a frosted desktop behind the window, cut to its rounded corners: `Glass::new(GlassMaterial::Hud)` (the default), any `GlassMaterial`, and `.plain()` for only the blur, untinted and unsaturated, under an app's own tint. Glass makes the window transparent. On Windows 11 (22H2 and later) the glass is the system backdrop nearest the material: Acrylic (a blur) for `Hud`, `Menu`, `Popover`, `ToolTip`, `Sheet`, `FullScreenUi` and `Selection`; Mica for `WindowBackground`, `UnderWindowBackground`, `UnderPageBackground`, `ContentBackground` and `Sidebar`; Mica Alt for `Titlebar` and `HeaderView`. `.plain()` has no Windows equivalent. Older Windows shows a plain transparent window. Both apply as the window is made, before it shows, keeping the content size asked for; `WindowContext::window_info.corner_radius` then holds the window's corner radius, so content inset from its edge stays concentric with it.

Content running under an `Inset` or `Hidden` title bar should stay clear of the window's own buttons. `WindowContext::window_info.titlebar_area` holds where it may draw, in points, as the web's Window Controls Overlay gives it (`getTitlebarAreaRect`, `env(titlebar-area-*)`): the strip the title bar leaves, less the window's buttons. `window_info.controls` holds the buttons themselves. Both follow resizes, scale and fullscreen; both are `None` for a standard title bar and in fullscreen (where a window shows no buttons). On Windows the strip is 32 points tall, with the caption buttons (138 points) at its right for `Inset`, and the whole strip for `Hidden`. macOS and Linux do not report them yet (`None`). Luau reads them as `ui.window.titlebar_area` and `ui.window.controls` once the host passes them in (`ScriptHost::window_chrome`; `zgx run` does).

`View::window_drag()` (Luau `window_drag = true`) makes a view a drag region, as `-webkit-app-region: drag`: a primary press on it, or on anything inside it that takes no input, moves the window, and a double click does what a double click on the title bar does (macOS follows the system setting: zoom, minimize or nothing; elsewhere the window toggles maximized). Focusable or pointer-capturing descendants, such as buttons and inputs, keep their presses, and pressing to move the window leaves focus where it was. A secondary click on a drag region opens the window's menu (restore, move, size, minimize, maximize, close) as a right click on a title bar does, on Windows and Wayland; macOS has no such menu.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Notes".into(),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(|cx| {
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .bg(rgba(0x1214168c))
                    // The strip under the traffic lights moves the window.
                    .child(div().w_full().h(40.).window_drag()),
            );
        })
}
```


`WindowHandle::set_glass` replaces or removes the native backdrop without
restarting the window. Create a window with `transparent = true` when its
backdrop will change. `GlassMaterial::Liquid` uses macOS 26's public
`NSGlassEffectView` in an `NSGlassEffectContainerView`, regular style; older
macOS uses an active, within-window popover material. Windows uses Acrylic.
`Glass::tint`, `corner_radius`, `backdrop` and `fill` configure Liquid Glass's
native tint, geometry and optional color layers. Classic material layers paint
backdrop, tint, then fill; `GlassMaterial::None` retains those layers and an
optional independent underlay without requesting blur. `GlassInnerShadow`
retains its own fixed/concentric radius, insets and surface-clipping choice.
Normal, color and overlay use Core Image compositing. Additive lighter/darker
use the source application's exact `plusL`/`plusD` Core Animation tokens through
the public `compositingFilter` property; those token values are undocumented.
A GPU background
cannot blend with a separate native glass view. Per-node `backdrop_blur` remains
a separate renderer effect and is not native Liquid Glass. Typed per-node
surface recipes use separate native backdrop, fill, stroke and shadow planes
on macOS, including decoration-only surfaces. Their source alpha and geometry
opacity remain separate. Per-node borders, fills and shadow paths use circular
corners to match the GPU and native content clips. This is a framework-consistent
approximation of the source application's continuous corners; visual parity has
not been verified. Root window glass keeps its native continuous corners.

`WindowOptions::theme` and `WindowHandle::set_theme` choose a native light/dark
appearance; `None` follows the system. Glass and appearance requests survive
native window recreation. `WindowInfo::focused` follows native window focus.

On macOS, `python3 scripts/macos_appearance_smoke.py` runs native structure and
Ghostty color fixtures: replacing/removing materials, regular style and corner
radius, real compositing filters, native dark/light/automatic appearance, and a
shell variable retained across live terminal color changes. It skips on Linux.
