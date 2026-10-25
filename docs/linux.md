# Linux desktop support

zgx runs natively on Wayland and on X11 (winit picks Wayland when
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
the window; a double click on it does what GNOME's
`action-double-click-titlebar` says, as GTK's header bars do (on X11 the
session's XSETTINGS `Gtk/TitlebarDoubleClick` first): maximize or restore by
default, minimize, the window menu, or nothing (`lower` and `toggle-shade`,
which winit cannot ask for, do nothing). As GTK's and
Chromium's, the move starts once the pointer, held, moves past the drag
threshold from the press: the window manager or compositor takes the pointer when asked to move
the window, so a click (the first of a double click) stays a click.
`window_buttons_smoke.py` checks it on X11: no frame, maximize and restore,
a corner drag, a double click and a drag on the strip, minimize, and close.
## The interface font

Text in the theme's generic family (`sans-serif`, which the default themes
use, and `system-ui`) takes the desktop's interface face, as GTK and Qt apps
do: GNOME's `font-name` (`Cantarell 11`, `Adwaita Sans 11`) or KDE's `font`,
read through the settings portal before the first window and given to the
renderer (`zgx_gpu::set_desktop_sans_serif`); on X11 the session's XSETTINGS
`Gtk/FontName` comes first, as GTK takes it there (XFCE's interface font),
read as the first window is made, before its renderer loads the fonts. A
family the system lacks is passed over. Without one, cosmic-text's default stands (Open Sans, else its
fallback list). A theme or view naming a family keeps it. The size in
`font-name` is not taken; GNOME's text scaling factor is (`text_scale`).
`desktop_font_smoke.py` renders the gallery under "DejaVu Sans 11" and
"DejaVu Serif 11" from a stand-in portal and finds the text changed, then
under the portal's sans with xsettingsd's `Gtk/FontName` "DejaVu Serif 11",
and finds the serif window.
## New windows fit the screen

A window manager shows a new window as large as it asks, past the screen's
edge (Openbox does; Mutter maximizes one near the screen's size), and a size
in points can be more pixels than the screen has at a high scale. Chromium
and GTK fit theirs first, and so does zgx: on X11, before a window first
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
owned by their window and closed with it: modal, and attached to it (the
request's `parent_window`). A chooser dismissed any way is no choice
(`Ok(None)`): its Cancel button, and Escape or its window's close button,
which GTK's portal answers as "ended another way". `portal_dialog_smoke.py`
checks it through xdg-desktop-portal and its GTK backend. Where no portal answers (a window
manager alone: Openbox, i3, a sway without xdg-desktop-portal), zgx shows
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
moves into place; closed, it moves off and is unmapped. On Wayland a popup is
an `xdg_popup` instead ([Popups on Wayland](#popups-on-wayland)).
`popup_windows_smoke.py` checks a menu past its window's edge, at a scale of
1 and of 2 (its window in the screen's pixels, where the scene put it, drawn
to its far corner and chosen from).

A drag's preview shows in such a window too, so it follows the pointer past
its window, over other windows. It is drawn once; as the pointer moves,
`PopupHost::follow` moves its window (`XMoveWindow`), drawing nothing, in it
or in the window it came from. Its input shape is empty, so the pointer goes
through it to what is under it. When nothing takes the drag, a thread with
its own connection slides it back to its source over 200 ms, easing out, then
hides it. Without a compositor, the X server asks each window that a moving
preview uncovers to repaint (Expose), and zgx repaints the whole window (winit
does not say which part). `drag_preview_smoke.py` checks the preview
under picom: it follows the pointer past the window with nothing drawn, and
slides back.

A drag carrying what other apps take (`drag_export`) is theirs past the
app's windows, over XDND (version 5). The button pressed in the window
grabs the pointer until it is released, so the window goes on hearing every
move; while the drag is X11's the manager hands those to the drag rather
than the window (`native_drag/x11.rs`). A session owns `XdndSelection` with
an input-only window on a connection of its own. Over another app's window
that says it takes drops (`XdndAware`, through any `XdndProxy`) it sends
`XdndEnter` and `XdndPosition`, the next position waiting for the target's
`XdndStatus`; released where the target accepted, `XdndDrop`, and a thread
on the session's connection hands the target the data it asks for and ends
the drag when the target says it finished (taken, or after 3 s, taken if
it asked for the data). Released elsewhere, or with Escape, the target
hears `XdndLeave`. The app's own windows are never targets (winit's say
they take files): over them the drag is zgx's. The preview stays its own
window, moved as the pointer moves and never drawn again. A conversion
larger than one X request (with BIG-REQUESTS, usually 16 MB) is refused;
there is no INCR transfer. `xdnd_source_smoke.py` drags text onto GTK
windows of its own (`xdnd_target.py`).

Drags go between the app's windows, as on macOS: the host asks where each
window's content is on the root (`XTranslateCoordinates`, kept as the window
moves) and which window is in front at the pointer. The X server finds that
by stacking order, down from the root through the window manager's frame,
past windows the pointer goes through: drag previews and a compositor's
overlay. Over another app's window covering one of the app's, the drag is
not the app's window's. A tab torn off (released over no window of the app's)
opens with the point it was grabbed by under the pointer. That point is
`XQueryPointer`'s, less the frame the window manager reports
(`_NET_REQUEST_FRAME_EXTENTS`), set as the user's position (`USPosition`) so
window managers keep it. A window's only tab moves the window itself
(`drag_moves_window`): its frame moves by request, which the window manager
carries out, placed from the pointer the server reports (`XQueryPointer`),
and what is below it is found among the root's windows in stacking order,
past the moving window and override-redirect ones, as Chromium finds it.
`drag_windows_smoke.py` checks all four.

A window with no position opens a step on from the app's front window, as
on macOS and Windows: down and right by its title bar's height, as the
program's position (`PPosition`). A window's frame is kept under its
`frame_autosave` name in `$XDG_STATE_HOME/zgx/<executable>/frames`, half a
second after it last moved or resized and as it closes, and reopened there,
maximized if it was left so. A window opened maximized, saved so or asked
to, has `_NET_WM_STATE` set before it maps: winit asks the window manager
with a message, which it ignores for a window it does not manage yet. On
Wayland the compositor places windows, so a saved frame keeps its size and
maximized state alone. `window_frames_smoke.py` checks both.

## Graphics

zgx draws through Vulkan. Where no Vulkan driver is (VMs and older GPUs
without one, systems that ship none), it draws through GL over EGL instead,
asking only for what older GPUs give (wgpu's downlevel limits); before, such
an app did not start ("Failed to create surface for any enabled backend").
The GL instance is made only once Vulkan finds no driver, so a Vulkan system
loads no EGL. `gl_fallback_smoke.py` draws the gallery both ways.

Where no font is installed, or none fontconfig finds (a minimal system, a
container), text shapes to nothing and everything else draws; the first text
says once on stderr why it does not show. Before, shaping panicked every
frame and the window drew nothing. `no_fonts_smoke.py` runs the gallery
with a fontconfig that lists only an empty directory.
On an X screen of 16 bits (some VNC servers, older setups) or 30 (deep
colour), a window takes a TrueColor visual of 24 bits, or of 32 (the one
compositing adds), rather than the screen's own (`x11_visual`): Mesa's Vulkan
offers no format to present to a 16- or 30-bit window, so the renderer
panicked and no window showed, and GL drew a 32-bit image into it as its
own. `x11_depths_smoke.py` runs the gallery on screens of each depth and
compares their pixels with a 24-bit screen's.

## Window icons

A window's Wayland app_id and X11 class are its app's desktop file ID:
what docks and task switchers match to `id.desktop` for its name and icon,
and how they group its windows. It is `Application::desktop_id`, else the
application id (`Application::application_id`), else the executable's
name. zgx-run gives each script its `zgx.json`'s `app_id`, so two script
apps are two apps, not one "zgx-run".

On X11 a window carries its app's icon (`_NET_WM_ICON`, which taskbars,
docks and window switchers show), from the icon theme by that desktop file
ID, as GTK takes a window's default icon from the program's name: the
hicolor theme under `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`, a 64 pixel PNG
first, then other sizes, then the scalable SVG drawn at 64, then `pixmaps`.
It is looked up and decoded once. Desktops that match windows to an
installed desktop file by its class show that file's icon anyway; this one
shows for an app run with none installed, and where a panel reads the
window's own. On Wayland the compositor takes the desktop file's icon, and
a window has none of its own. `window_icon_smoke.py` checks the class and
icon, by the executable's name and by a desktop file ID.

## Resizing on X11

A window manager resizing a zgx window waits for each frame
(`_NET_WM_SYNC_REQUEST`, as GTK, Chromium and Qt): the vendored winit names
the protocol and a SYNC counter on the window, keeps the window manager's
request, and sets the counter as zgx presents the frame at the new size
(`Window::pre_present_notify`), or at once when no new size comes. Without
it the window manager resized as fast as the pointer moved, ahead of the
frames. `resize_sync_smoke.py` resizes window_probe's window with Openbox's
Alt and right button drag: the counter, at 0 before, reaches Openbox's last
request, with a frame drawn for each step.

## Focus stealing on X11

Each window carries the time of the user's last key or button press in the
app (`_NET_WM_USER_TIME`, as GTK sets it): a press sets it on its window,
and a window opened later carries it from the moment it maps. Window
managers weigh it against focus stealing, each by its own policy: Mutter
and KWin compare it with the focused window's, Openbox keeps the focus
where the user worked within the last half second. Before any press a
window carries none, so an app's first window is focused as it launches.
`user_time_smoke.py` checks the times.

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

zgx gives the input method the caret's rectangle on every frame the caret
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

## Drags out on Wayland

A drag carrying what other apps take (`drag_export`) is the compositor's past
the window it came from: the data worker, on winit's display, makes a
`wl_data_source` with what it carries (as X11's XDND source offers it) and
starts the drag from the press that began it (`wl_data_device.start_drag`).
Its icon is the preview drawn once, offscreen, into shared memory, the point
it was grabbed by under the pointer. The compositor has the pointer then, and
says where it goes only to the window under it: over any of the app's
windows the worker hears the drag as that window's data device (which
window, from the surfaces it routes file drops by), and the window's
destinations hear where it is. It accepts the drag there, as one of its own
types, copied, so a release drops it rather than cancels, and drops it on
those destinations itself; it holds the offer until the drag leaves
(destroyed sooner, wlroots cancels the drag) and finishes it on the drop. A
target of another app taking it finishes it (`dnd_finished`), and the
source hears it accepted; a drag nothing takes is cancelled. Windows have no
place on screen on Wayland, so a drag leaving its window becomes the
compositor's even carrying nothing to other apps (a tab), offered only as
`application/x-zgx-drag`, which no other app takes: that is how it reaches
the app's other windows, and let go past every window it is torn off, as on
X11. A window's only
tab (`drag_moves_window`) has the compositor move its window, as a title
bar's drag does (`xdg_toplevel.move`): the compositor holds the pointer from
there until it is let go, so zgx's drag ends as the move begins, and the
tab cannot merge into another window's strip (where that is, only the
compositor knows). `wayland_drag_out_smoke.py` drags onto GTK windows of its
own under sway, drags a tab onto the other window's strip and off past every
window, and drags a window's only tab.

## Popups on Wayland

Menus, popovers, selects and tooltips show past the window as `xdg_popup`s of
it (`popup_window/wayland.rs`), as GTK's and Chromium's do. winit gives a
window's `xdg_toplevel`, not the `xdg_surface` a popup's parent must be, so
zgx vendors winit 0.30.13 with one method more, `WindowExtWayland::xdg_surface`,
and where the window geometry starts (`xdg_window_geometry_origin`, above the
content by the title bar winit draws where the compositor draws none, as
Weston): see `vendor/winit/ZGX_PATCH.md`.

zgx's objects live on an event queue of its own on winit's display: its own
`wl_compositor`, `xdg_wm_base`, `wp_viewporter` and a `wl_pointer` per seat.
winit reads the socket for both queues, and zgx's is dispatched where the
event loop turns, so it costs no wakeup. A popup is a `wl_surface` with an
`xdg_surface` and `xdg_popup`, made as it shows and destroyed as it hides,
drawn by a renderer of its own on the window's device
(`GpuRenderer::for_raw_surface`). Its positioner anchors it on its trigger
(`Scene::popup_anchor`, the menu's button or the submenu's item), on the side
the scene put it, in the window geometry's coordinates, and lets the
compositor flip it to the other side at the output's edge, or slide it on.
The gap from the trigger widens the anchor rectangle rather than going in the
offset, which wlroots keeps as it is through a flip. Pointer input over it
reaches the window's host in the window's coordinates, by where the scene
drew it, so a flipped menu's items take clicks as before. A submenu is a
popup of its menu's popup (the one holding its item), placed against that
menu's surface, so it opens beside the menu wherever the compositor put it;
popups go children first, as the protocol requires. The scene places
popups unbounded (the app cannot learn the outputs), so a menu past the
window's bottom edge opens downward, past it, unless the output ends first. Opening one (configured, its surface
made) took 3.8 to 5.8 ms and its first frame 2.2 to 4.5 ms, in a debug build
on lavapipe: within a frame, so popups are not pooled.

The popup takes no grab: keys stay the window's, which drives menus as
before, and a press on another app takes the window's focus, which closes
them, as without a grab on X11. Drag previews stay inside the window (a popup
moved with the pointer needs the compositor's configure each move).
`wayland_popups_smoke.py` clicks popup_probe's menu open under sway, past
the window's edge where the scene put it, chooses its last item below the
window, closes it with a press in the window, and, the window moved near the
output's bottom, sees it flip above its trigger and take a click there, and
the submenus example's submenu open beside its flipped menu; on outputs
scaled 2× and 1.5×, the menu's buffer is in the output's pixels, where the
scene put it, and takes a click; under Weston, the menu sits the same on the content below winit's title bar.

A popup taller (or wider) than the output has room for is cut to it by the
compositor (the positioner lets it resize, after flipping and sliding), which
says so in its configure. The host passes that size to the scene
(`Ui::set_popup_limit`), which lays the panel out no bigger, a menu
scrolling its items as it does in an X11 or macOS screen, and the popup
shows anew at that size. The probe opens a menu of 60 items on a 960-point
output, sees it cut to 960, scrolls it and chooses its last item.

A popup the compositor flipped to its trigger's other side (its configure
puts it across the trigger from where it asked) is reported to the scene
once while it is open (`Ui::set_popup_flipped`), which places a popover with
an arrow on that side, the arrow on its edge facing the trigger, and it
shows anew there. Menus, which have no arrow, are left where the compositor
put them.

Not yet: the grab (`xdg_popup.grab`, which moves the keyboard to the popup,
so keys must come through a `wl_keyboard` of zgx's own with winit's
keymap).

## Web views

`zgx::web_view` has no Linux implementation: a web view shows its labelled
placeholder and its controller still answers, as headless. This is a
deliberate gap, not an oversight. Here is why, and what an implementation
would take.

On macOS and Windows a page is a surface of the window's own compositor tree:
zgx renders a base plane under it and an overlay plane over it (a popover, a
composer), and the frame's presents and the page's geometry commit in one
transaction (Core Animation, DirectComposition), so the page moves with zgx
content. Linux needs the page as a surface zgx's own window controls, and
WebKitGTK cannot be one:

- WebKitGTK (`webkit2gtk-4.1` on GTK 3, `webkitgtk-6.0` on GTK 4) is a GTK
  widget. GTK draws it through its own display connection into a toplevel GTK
  owns. Wayland has no cross-client embedding (a subsurface belongs to the
  client that made its parent), and GTK cannot draw into a surface another
  connection made. So a WebKitGTK page cannot sit inside a winit window on
  Wayland, which is the default session on GNOME and KDE.
- On X11, GTK 3 can embed by reparenting a GTK window into the zgx window
  (XEmbed). GTK 4 removed `GtkSocket` and `GtkPlug`. wry's child web views do
  this, and are X11-only for this reason. It would make web views X11-only in
  zgx too, so an app would have to run under XWayland on Wayland desktops.
  zgx paint over a page would have to be a separate child window, which X
  cannot commit together with the page, so it would trail the page by a frame
  while scrolling. GTK's main loop would also need to run alongside winit's.

The route that works on both is WPE WebKit, WebKit's port for hosts without a
toolkit. Its fdo backend hands the host each rendered frame as a dmabuf:
- On Wayland, zgx would attach those frames to a subsurface of its window,
  between a base and an overlay subsurface, and commit the three together, as
  macOS and Windows commit theirs.
- On X11, the renderer would import them as textures.

The costs:
- zgx on Linux renders a single surface today, so the plane split would come
  first. It would serve any native view, terminals included.
- The host would forward pointer, keyboard, IME and touch input into the page,
  as WebView2's visual hosting does on Windows.
- The host would draw what the page asks for: cursors, `<select>` popups and
  dialogs.
- The page's accessibility tree would need its own bridge to AT-SPI.
- WPE is a separate library from WebKitGTK, so apps would depend on it.

That is a project of the size of the WebView2 host plus the Wayland plane
split. It is not started.
