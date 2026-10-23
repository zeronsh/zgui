# Linux desktop support

zgui runs natively on Wayland and on X11 (winit picks Wayland when
`WAYLAND_DISPLAY` is set). [Validation](validation.md) lists what the native
Linux probes check on both: input, IME, clipboard and the primary selection,
scaling, cursors, accessibility (AT-SPI) and the desktop's settings.

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
