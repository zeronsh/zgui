# zgui patch to winit 0.30.13

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

Drop the patch when a winit release exposes the same (its popup work, 0.31).
