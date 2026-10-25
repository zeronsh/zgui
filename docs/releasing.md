# Releasing

zgx 0.1.0 is on crates.io (2026-10-25, tag `v0.1.0`). This is how a release goes, and what it checks.

## Version scheme

- Every published crate shares one version, the workspace's (`[workspace.package] version` in `Cargo.toml`). Path dependencies between them name it (`version = "0.1.0"`), as crates.io needs.
- It follows [semver](https://semver.org) as Cargo reads it before 1.0: `0.MINOR.PATCH`, where a new minor may break apps (each break listed in `CHANGELOG.md` with what to check and how to migrate) and a patch does not.
- A release is a tag `v<version>` on `main` (`v0.2.0`), made once its changelog section is written: `CHANGELOG.md`'s "Unreleased" becomes `## 0.2.0 (date)`.
- The oldest Rust the workspace builds with is `rust-version` (1.95: `AtomicU64::try_update`). CI's Linux job checks it: its no-default-features check runs on that toolchain.

## What is published

zgx's crates: `zgx`, `zgx-components`, `zgx-gpu`, `zgx-desktop`, `zgx-script`, `zgx-run` and `zgx-ghostty-sys` (its declarations only; its `link` feature builds Ghostty from a pinned source with Zig). Not published (`publish = false`): `zgx-demo` and `zgx-workload`, the benchmarks in `comparisons/`.

And four forks, as zgx's own crates, published first:

| Crate | Fork of | Why | Notes |
| --- | --- | --- | --- |
| `zgx-harfrust` | harfrust 0.5.2 | a variable font's glyph advances worked out once for its instance, not for every run shaped | `vendor/harfrust/ZGX_PATCH.md` |
| `zgx-cosmic-text` | cosmic-text 0.19.0 | text shaping that streams without rebuilding every word's key; shapes with `zgx-harfrust` | `vendor/cosmic-text/ZGX_PATCH.md` |
| `zgx-taffy` | taffy 0.9.0 | auto margins that keep a flex line's gaps | `vendor/taffy/ZGX_PATCH.md` |
| `zgx-mlua` | mlua 0.12.2 | table walks and userdata without a copy per key | `vendor/mlua/ZGX_PATCH.md` |

Each keeps its upstream's license files, authors and library name (`harfrust`, `cosmic_text`, `taffy`, `mlua`), and its version (a fix of our own bumps the patch: `zgx-cosmic-text` is 0.19.1 since it shapes with `zgx-harfrust`). zgx's crates depend on them by path and version under their upstream names (`cosmic-text = { package = "zgx-cosmic-text", version = "0.19.1", path = "../../vendor/cosmic-text" }`, and `zgx-cosmic-text` on `zgx-harfrust` likewise), so code reads `cosmic_text::` as before. Only zgx's crates use them, so an app never sees two of one. Drop a fork, back to the upstream crate, once a release of it has the fix.

## What stays patched in the repository alone

The workspace's `[patch.crates-io]` keeps the rest: wgpu-hal (Metal command buffers made lazily and never empty, a Direct3D 12 adapter exposed without opening the others), winit (a Wayland window's `xdg_surface`, X11's `_NET_WM_SYNC_REQUEST` and `_NET_WM_USER_TIME`), and the AccessKit adapters for macOS, Windows and Linux (headings, grid rows, tables, tree items). Forking them would mean forking what depends on them too (wgpu, accesskit_winit). An app depending on zgx from crates.io gets their releases instead, and zgx builds and runs with them, without those fixes:

- **winit**: Wayland popups open in their window, as on a compositor without `xdg_popup`, instead of as popup surfaces of their own. The X11 changes are behaviour alone: a window manager's resize no longer waits for frames, and focus-stealing prevention sees no user time.
- **wgpu-hal**: Metal frames take upstream's command buffers (more per submission); on Windows, `request_adapter` ranks the adapters, opening a device on each.
- **AccessKit**: VoiceOver's rotor misses headings and its table navigation finds no grid rows; Narrator and Orca lose what their patches add. Everything else reads as before.

Where a patch adds an API zgx calls (winit's `xdg_surface`, wgpu-hal's `expose_preferred_adapter`), zgx detects it at build time: the vendored crate carries `links` metadata (`links = "zgx_winit"`, `"zgx_wgpu_hal"`) whose `patched` key reaches the build scripts of the crates depending on it (zgx-desktop, zgx-gpu), which set `zgx_winit_patched` and `zgx_wgpu_hal_patched`. With crates.io's crates neither is set, and the fallback above is compiled. Nothing to enable: a build in the repository has every patch, any other has none.

## Follow-ups: patches to drop

- **harfrust**: from 0.9 a shaper takes font functions of its own (`FontFuncs`, `ShapeOptions::font_funcs`), which could keep a variable instance's advances from cosmic-text. Upgrading cosmic-text's harfrust to it (0.5 → 0.9 changes its API) and keeping them there drops `zgx-harfrust`.
- **taffy**: the auto-margin gaps fix shipped upstream in taffy 0.14.0. Upgrading zgx's layout to it (0.9 → 0.14 changes taffy's API) drops `zgx-taffy` for crates.io's taffy.
- **winit**: native popups are on winit's master (the 0.31 betas). Upgrading to 0.31 drops the `xdg_surface` accessors (Wayland popups through winit's own), and with them `zgx_winit_patched`; the X11 changes are to check against it then.

## What a release checks

`.github/workflows/release.yml` runs on a `v*` tag, or by hand (Actions → Release dry run), on Linux, macOS and Windows (each has fallbacks of its own to build). It checks that the tag is the workspace's version, then runs `scripts/release_check.sh`:

1. Each fork is packaged and built from its package, against crates.io.
2. Every published workspace crate is packaged and built from its package, in dependency order (`cargo publish --workspace --dry-run`). That build leaves the workspace's `[patch.crates-io]` out: winit, wgpu-hal and AccessKit come from crates.io, as an app depending on zgx gets them, so what only builds with a vendored patch fails it. The forks, which crates.io has only once the release publishes them, are given from `vendor/` (`--config 'patch.crates-io.zgx-cosmic-text.path="vendor/cosmic-text"'`) for that run alone.

It never publishes. It doesn't run on pull requests: CI's time is unchanged. Locally, `scripts/release_check.sh --allow-dirty` checks uncommitted work.

## Publishing

Not automated, on purpose: publishing needs a crates.io token, and is done by hand, in this order, each after the release check passes on the tag:

1. The forks that changed since they were last published, their versions bumped, `zgx-harfrust` before `zgx-cosmic-text` (which depends on it): `cargo publish --manifest-path vendor/harfrust/Cargo.toml`, `vendor/cosmic-text`, `vendor/taffy`, `vendor/mlua --features luau`.
2. The workspace: `cargo publish --workspace --locked` (it orders the crates itself).

`cargo publish --workspace` can stop part way with cargo's own error "no packages ready to publish but … remain in plan … awaiting confirmation" (cargo 1.96, at 0.1.0, after `zgx-gpu`): what it uploaded is published (check crates.io), and the rest publish one at a time in dependency order, each waiting for the index: `cargo publish -p zgx-script --locked`, then `zgx-desktop`, then `zgx-run`.

Then tag the commit (`v<version>`), make a GitHub release from its changelog section, and check a new project builds from crates.io alone (`cargo add zgx zgx-desktop`).

A crate on crates.io can't be deleted, only yanked: a version published is published for good.
