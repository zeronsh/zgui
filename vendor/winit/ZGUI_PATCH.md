# zgui patches to winit 0.30.13

Vendored from crates.io `winit` 0.30.13 (Apache-2.0, `LICENSE`) and applied
through `[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgui patch" in the source. Its examples, tests and docs are left out.

## Why

On Wayland a popup past the window's edge is an `xdg_popup`, made with
`xdg_surface.get_popup(parent, positioner)`: its parent is the window's
`xdg_surface`. winit exposes the window's `xdg_toplevel`, from which the
protocol has no way back to the `xdg_surface`, and a second `xdg_surface` on
the window's `wl_surface` is a protocol error (`already_constructed`). The
positioner's rectangles are relative to the window geometry, which starts
above the surface by the title bar winit draws where the compositor draws
none (Weston): zgui must know where.

## What

1. `WindowExtWayland::xdg_surface`, beside `xdg_toplevel`
   (`src/platform/wayland.rs`; `src/platform_impl/linux/wayland/window/mod.rs`).
2. `WindowExtWayland::xdg_window_geometry_origin`: where
   `set_window_geometry` puts the geometry on the surface (the frame's
   location, `(0, 0)` without one; `window/state.rs`).

Drop these when a winit release exposes the same (its popup work, 0.31).

## X11: `_NET_WM_SYNC_REQUEST`

A window manager resizing a window (dragging its border) resized it as fast
as the pointer moved, ahead of the frames drawn for each size, which then
lag and show garbage at the edges under a compositor. GTK, Chromium and Qt
take part in `_NET_WM_SYNC_REQUEST`: the window manager waits, before its
next resize, for the window to say it drew the last.

3. A window gets a SYNC counter (`_NET_WM_SYNC_REQUEST_COUNTER`) and names
   the protocol in `WM_PROTOCOLS` (`window.rs`; x11rb's `sync` feature,
   `Cargo.toml`). The counter goes with the window (`mod.rs`).
4. The window manager's request (a `WM_PROTOCOLS` client message) is kept
   (`event_processor.rs`), and the counter set to its value at the next
   `Window::pre_present_notify`, which zgui calls as it presents a frame,
   or at once when the configure that follows brings no new size (no frame
   follows it).

## X11: `_NET_WM_USER_TIME`

Window managers weigh a new window against focus stealing by the time of
the user's last input in its app (EWMH's `_NET_WM_USER_TIME`), which GTK
sets on its windows: one the app opens on its own, long after the user
turned to another app, need not take the focus. winit set none.

5. The connection keeps the last key or button press's time (`xdisplay.rs`,
   from `event_processor.rs`), and each press sets it on its window.
6. A new window carries it before it maps (`window.rs`); before any press,
   none, so an app's first window is focused as it launches.
