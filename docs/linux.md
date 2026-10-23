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

## File dialogs

File choosers go through the desktop's portal (`org.freedesktop.portal.FileChooser`),
owned by their window and closed with it. Where no portal answers (a window
manager alone: Openbox, i3, a sway without xdg-desktop-portal), zgui shows
zenity's chooser, GTK's, as rfd and Chromium fall back to GTK's own: modal,
attached to its window on X11 with zenity 3, and ended when its window closes
or its request is dropped (the waiting thread sleeps on the process and the
request together, through a pidfd). Without zenity either, the request fails
with `BackendUnavailable`. `zenity_dialog_smoke.py` checks it.

## Popups past the window

On X11 a menu, submenu, popover, context menu, select or tooltip that opens
shows in a window of its own, as on macOS (`popup_window.rs`), so it reaches
past its window's edges and flips or clamps at the screen's: each monitor
less its panels (`_NET_WORKAREA`), given to the core as `Ui::set_popup_screens`.
The window is override-redirect, typed a popup menu
(`_NET_WM_WINDOW_TYPE_POPUP_MENU`), transient for its parent, with an ARGB
visual for its rounded corners and shadow, and never takes the keyboard: the
parent keeps it, and with it the popup's focus and keys. Its pointer input
goes to the parent's host in the parent's coordinates. A menu or popover
holds the pointer (an XI2 grab with owner events, as GTK's menus do): presses
in the app's windows arrive as ever, one anywhere else closes it; a tooltip
holds nothing. Popups close as the parent loses focus, too.

Nothing is made before the pointer first nears something that opens a popup
(`Ui::on_popup_soon`); then a window is made, unmapped, and up to three are
kept for reuse. A popup's window is mapped off screen, takes its first
frame there (an unmapped window's swapchain never completes a present), then
moves into place; closed, it moves off and is unmapped. On Wayland popups stay
inside their window: `xdg_popup` is not something winit exposes.
`popup_windows_smoke.py` checks a menu past its window's edge.

A drag's preview shows in such a window too, so it follows the pointer past
its window, over other windows. It is drawn once; as the pointer moves,
`PopupHost::follow` moves its window (`XMoveWindow`), drawing nothing, in it
or in the window it came from. Its input shape is empty, so the pointer goes
through it to what is under it. When nothing takes the drag, a thread with
its own connection slides it back to its source over 200 ms, easing out, then
hides it. Without a compositor, the X server asks each window that a moving
preview uncovers to repaint (Expose), and zgui repaints the whole window (winit
does not say which part). `drag_preview_smoke.py` checks the preview
under picom: it follows the pointer past the window with nothing drawn, and
slides back.

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

The same holds as a word is typed. IBus lays out its candidate window, and
moves it to the last spot it was given, each time the candidate list changes.
The spot for the new preedit's caret comes after that, from the app. When the
app is rendering a frame as the preedit arrives (a slow software renderer
under load), the spot can come after IBus's layout, and the window stays at
the previous caret until the next key. `native_ime_smoke.py` records whether
the window followed at once. If it did not, Down and Up make IBus lay it out
again, the preedit unchanged, and the window must then be at the caret.

## Popups on Wayland: what it would take

On X11 popups show in windows of their own (see the X11 popups PR, #179); on
Wayland they stay inside their window. The protocol's answer is `xdg_popup`,
and here is what building it would take, assessed before building.

**The protocol.** A popup is a `wl_surface` with an `xdg_surface` and its
`xdg_popup` role, made by `xdg_surface.get_popup(parent xdg_surface,
positioner)`. The `xdg_positioner` says its size, the anchor rectangle on the
parent (the trigger), the edge and gravity, and how the compositor may adjust
it (`constraint_adjustment`: flip, slide, resize). The compositor places it
and reports where in `xdg_popup.configure` (relative to the parent), which
the client acks before its first commit; `reposition` (version 3) moves an
open one when its anchor moves. `xdg_popup.grab(seat, serial)` makes it the
seat's grab: the compositor dismisses it (`popup_done`) on a press outside
the client's popups, and keyboard focus goes to the popup. A client cannot
learn screen bounds; flipping at the screen's edge is the compositor's.

**The blocker: the parent's `xdg_surface`.** winit 0.30.13 exposes the
window's `xdg_toplevel` (`WindowExtWayland::xdg_toplevel`), not its
`xdg_surface`. The protocol has no way from one to the other, and a second
`xdg_surface` on the window's `wl_surface` is a protocol error
(`already_constructed`). Its sctk window has the `xdg_surface`
(`XdgSurface::xdg_surface`), behind a private field.

**Options.**

- *Patch winit* (recommended): vendor winit 0.30.13 as wgpu-hal,
  cosmic-text and the accesskit adapters are, adding one method,
  `WindowExtWayland::xdg_surface`, beside `xdg_toplevel` (ten lines; a patch
  to offer upstream). The popup is then zgui's own, on the connection the
  data-device worker already keeps on winit's display (`wayland_data.rs`,
  which owns the seats, their pointers and the latest input serial): a
  `wl_surface` and `xdg_popup` (sctk 0.19's `Popup`, or wayland-protocols
  directly), a wgpu surface from its raw handles, its pointer events from
  the worker's `wl_pointer`, routed by surface to the parent's host as X11's
  are, fractional scale per surface (`wp_fractional_scale_v1` and the
  viewporter, as the window has).
- *Subsurfaces* (no patch): a `wl_subsurface` of the window's surface past
  its edges. Mutter, KWin and wlroots draw it, but nothing keeps it on the
  screen (the client knows no screens), it gets no grab or `popup_done`, and
  it moves only with its parent. Not what users of GTK or Chromium popups
  see; rejected.
- *A winit fork with popups of its own*: winit's popup API is unsettled
  upstream (its 0.31 rework); carrying it would cost more than the one
  method above.

**What changes in zgui.** The compositor, not zgui, has the last word on
where a popup goes: the core places it as now (with no screens to keep it
in, as on Wayland today), the host gives the positioner the anchor and the
placement's edge and gravity, and the configured rectangle comes back to the
core so drawing and hit testing follow it (a `Ui` hook for a popup's
placement; on X11 and macOS it is the core's own). The grab moves keyboard
focus to the popup surface, so either the worker's `wl_keyboard` routes keys
to the parent's host too (with winit's keymap), or menus start without the
grab and close as their window loses focus or takes a press, as X11's do
without one, the grab following once keys route.

**Size.** The winit patch; about 500 lines in a `wayland` module of
`popup_window.rs` (positioner, configure and ack, surfaces and renderer,
input routing, `popup_done`); the core's placement hook; a probe under a
headless sway, driving it with `scripts/wayland_pointer.py`'s virtual
pointer and checking the protocol in the client's `WAYLAND_DEBUG` log.
Risks: frame pacing for a surface winit does not own (its frame
callbacks are the worker's), and compositors' differing `constraint_adjustment`.

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
