# Linux desktop support

zgui runs natively on Wayland and on X11 (winit picks Wayland when
`WAYLAND_DISPLAY` is set). [Validation](validation.md) lists what the native
Linux probes check on both: input, IME, clipboard and the primary selection,
scaling, cursors, accessibility (AT-SPI) and the desktop's settings.

## Title bars

`Titlebar::Inset` and `Titlebar::Hidden` are undecorated windows, as a GTK
header bar's window is (client-side decorations): the window manager or
compositor draws no frame, on X11 or Wayland, whatever `decorations` says.
Their edges resize them: a press within 5 points of an edge, or 16 along it
from a corner, starts the window manager's own resize (winit's
`drag_resize_window`, `_NET_WM_MOVERESIZE` on X11 and `xdg_toplevel.resize` on
Wayland), with the matching cursor; the content hears the pointer leave
meanwhile. A maximized or fullscreen window, or one of a fixed size, has no
bands.

`Inset` adds libadwaita's window buttons over the content's top corners,
centred in its 40-point strip: 24-point circles in the text colour at 10% over
the window's background, 15% under the pointer, 30% pressed, with the
symbols dimmed while the window is in the back. Which buttons, and on which
side, follow the desktop as GTK 4 does: GNOME's `button-layout` through the
settings portal (`appmenu:close` shows close alone, on the right;
`close,minimize,maximize:` puts all three on the left), live; without it,
minimize, maximize and close on the right. Close asks the app's close policy,
as the window manager's would. Mark the strip `View::window_drag` so it moves
the window; a double click on it maximizes or restores.
`window_buttons_smoke.py` checks it on X11: no frame, maximize and restore,
a corner drag, minimize, and close.
## The interface font

Text in the theme's generic family (`sans-serif`, which the default themes
use, and `system-ui`) takes the desktop's interface face, as GTK and Qt apps
do: GNOME's `font-name` (`Cantarell 11`, `Adwaita Sans 11`) or KDE's `font`,
read through the settings portal before the first window and given to the
renderer (`zgui_gpu::set_desktop_sans_serif`); a family the system lacks is
passed over. Without one, cosmic-text's default stands (Open Sans, else its
fallback list). A theme or view naming a family keeps it. The size in
`font-name` is not taken; GNOME's text scaling factor is (`text_scale`).
`desktop_font_smoke.py` renders the gallery under "DejaVu Sans 11" and
"DejaVu Serif 11" from a stand-in portal and finds the text changed.
## New windows fit the screen

A window manager shows a new window as large as it asks, past the screen's
edge (Openbox does; Mutter maximizes one near the screen's size), and a size
in points can be more pixels than the screen has at a high scale. Chromium
and GTK fit theirs first, and so does zgui: on X11, before a window first
shows and unless the app gave its bounds, its size shrinks to its monitor's
work area (the current desktop's `_NET_WORKAREA`, the screen less its panels),
its frame included (`_NET_REQUEST_FRAME_EXTENTS`, asked only of a window
manager that lists it and only when the window may not fit), though not below
`min_size`. It takes 0.2 ms when the window fits. On Wayland the compositor
bounds a new window's size (`xdg_toplevel.configure_bounds`) and winit keeps
to it: a 1200 by 800 window opens at 1024 by 640 on a 1024 by 640 sway output.
`work_area_smoke.py` checks X11 below a panel.

## Launching

A launcher hands a new app a token: `XDG_ACTIVATION_TOKEN` on Wayland,
`DESKTOP_STARTUP_ID` on X11. The app's first window takes it, as GTK's
does: on Wayland it activates the window (`xdg_activation_v1`), so GNOME
focuses it rather than announcing it "is ready"; on X11 it ends the launch's
startup notification, and with it the launcher's busy cursor. The token then
leaves the environment, so processes the app starts later do not take it as
theirs.

A launch of an app that already runs (one with an application ID) hands its
token over with its D-Bus `Activate` or `Open` (GApplication's
`platform_data`: `activation-token`, `desktop-startup-id`). A window the app
opens for it takes the token; opening none, the app's window in front (the
last focused) is raised with it: on Wayland through `xdg_activation_v1`
(winit's `focus_window` does nothing there), on X11 through the window
manager, restoring it if minimized and ending that launch's startup
notification. `launch_token_smoke.py` checks all four.

## Frame pacing and latency

On Wayland a window draws when the compositor's frame callback grants a
frame (winit withholds `RedrawRequested` until then), so at most one frame
is in flight and none queue up. On X11, which has no frame callbacks, a
timer at the monitor's refresh rate paces animation and presents go out
through the FIFO swapchain. A window with nothing changing does no work
on either.

`scripts/wayland_latency_probe.py` measures a Wayland client from its own
protocol log, under Weston on Xvfb (60 Hz, software rendering, debug build,
2026-10-23):

| | Key press to commit |
| --- | --- |
| At rest | median 1.9 ms, p90 2.4 ms |
| Animating at the display's rate | median 8.2 ms, p90 16.8 ms |

At rest the frame showing a key is committed as soon as it is drawn. While
animating, the key waits for the next frame callback, about half a refresh
on average, and is drawn in that frame (one that arrives while a frame is
being drawn waits for the one after). Weston then shows the commit at its next
repaint, up to a refresh later, as it would show any client's commit.
Animation commits came 16.5 ms apart (median, p90 17.4 ms), with none
longer than 1.5 refreshes in 504 frames. Idle, the client sent and received
no Wayland messages and used no CPU in 5 s. On X11 (Xvfb) it used no CPU
and woke 0 times idle, and about 6 times per frame animating.

Not measured: X11 presentation timing on real hardware. Xvfb has no vblank,
and software Vulkan presents through shared memory, not the Present
extension. Also open: drawing a frame later in the refresh, as macOS does,
would cut the animating case toward the at-rest one. On Wayland that needs
the compositor's refresh phase, from `wp_presentation` feedback.

## Input method candidates while scrolling

zgui gives the input method the caret's rectangle on every frame the caret
moves, scrolling included, so the candidate window opens at the caret
wherever it has scrolled to. Checked with IBus (libpinyin) over XIM on
2026-10-23: composing in a field inside a scroll view, then scrolling the
field 160 px up by wheel, the candidate window opened at the caret's
scrolled position on the next keystroke. IBus does not move a candidate
window that is already showing when only the caret moves, so it stays put
during the scroll itself, as it does for GTK apps. On Wayland each frame's
rectangle goes out at once (`zwp_text_input_v3.set_cursor_rectangle`, then a
commit), so a compositor that places the input method's popup from it can
follow the scroll. A caret scrolled out of the window's top is given as at
its top edge.

## Web views

`zgui::web_view` has no Linux implementation: a web view shows its labelled
placeholder and its controller still answers, as headless. This is a
deliberate gap, not an oversight. Here is why, and what an implementation
would take.

On macOS and Windows a page is a surface of the window's own compositor tree:
zgui renders a base plane under it and an overlay plane over it (a popover, a
composer), and the frame's presents and the page's geometry commit in one
transaction (Core Animation, DirectComposition), so the page moves with zgui
content. Linux needs the page as a surface zgui's own window controls, and
WebKitGTK cannot be one:

- WebKitGTK (`webkit2gtk-4.1` on GTK 3, `webkitgtk-6.0` on GTK 4) is a GTK
  widget. GTK draws it through its own display connection into a toplevel GTK
  owns. Wayland has no cross-client embedding (a subsurface belongs to the
  client that made its parent), and GTK cannot draw into a surface another
  connection made. So a WebKitGTK page cannot sit inside a winit window on
  Wayland, which is the default session on GNOME and KDE.
- On X11, GTK 3 can embed by reparenting a GTK window into the zgui window
  (XEmbed). GTK 4 removed `GtkSocket` and `GtkPlug`. wry's child web views do
  this, and are X11-only for this reason. It would make web views X11-only in
  zgui too, so an app would have to run under XWayland on Wayland desktops.
  zgui paint over a page would have to be a separate child window, which X
  cannot commit together with the page, so it would trail the page by a frame
  while scrolling. GTK's main loop would also need to run alongside winit's.

The route that works on both is WPE WebKit, WebKit's port for hosts without a
toolkit. Its fdo backend hands the host each rendered frame as a dmabuf:
- On Wayland, zgui would attach those frames to a subsurface of its window,
  between a base and an overlay subsurface, and commit the three together, as
  macOS and Windows commit theirs.
- On X11, the renderer would import them as textures.

The costs:
- zgui on Linux renders a single surface today, so the plane split would come
  first. It would serve any native view, terminals included.
- The host would forward pointer, keyboard, IME and touch input into the page,
  as WebView2's visual hosting does on Windows.
- The host would draw what the page asks for: cursors, `<select>` popups and
  dialogs.
- The page's accessibility tree would need its own bridge to AT-SPI.
- WPE is a separate library from WebKitGTK, so apps would depend on it.

That is a project of the size of the WebView2 host plus the Wayland plane
split. It is not started.
