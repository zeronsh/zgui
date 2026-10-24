# Releasing

Nothing is published yet. This is how a release would go, and what has to be settled first.

## Version scheme

- Every published crate shares one version, the workspace's (`[workspace.package] version` in `Cargo.toml`). Path dependencies between them name it (`version = "0.1.0"`), as crates.io needs.
- It follows [semver](https://semver.org) as Cargo reads it before 1.0: `0.MINOR.PATCH`, where a new minor may break apps (each break listed in `CHANGELOG.md` with what to check and how to migrate) and a patch does not.
- A release is a tag `v<version>` on `main` (`v0.2.0`), made once its changelog section is written: `CHANGELOG.md`'s "Unreleased" becomes `## 0.2.0 (date)`.
- The oldest Rust the workspace builds with is `rust-version` (1.95: `AtomicU64::try_update`). CI's Linux job checks it: its no-default-features check runs on that toolchain.

## What a release checks

`.github/workflows/release.yml` runs on a `v*` tag, or by hand (Actions → Release dry run). It checks that the tag is the workspace's version, then runs `cargo publish --workspace --dry-run --locked`: every published crate is packaged and built from its package, in dependency order, and nothing is uploaded. It never publishes.

Locally, the same without the build (a minute): `cargo publish --workspace --dry-run --no-verify`.

Published: `zgui`, `zgui-components`, `zgui-gpu`, `zgui-desktop`, `zgui-script`, `zgui-run` and `zgui-ghostty-sys` (its declarations only; its `link` feature builds Ghostty from a pinned source with Zig). Not published (`publish = false`): `zgui-demo` and `zgui-workload`, the benchmarks in `comparisons/`.

## Before the first publish

- **The `zgui` name is taken on crates.io** (by an unrelated project, `zortax/zgui`). The core crate needs another name (`zgui-core`, say: a rename of the package, its dependents' `zgui = { package = … }`, and the docs), or the name from its owner. `zgui-components`, `zgui-gpu`, `zgui-desktop`, `zgui-script`, `zgui-run` and `zgui-ghostty-sys` are free.
- **Vendored dependencies.** The workspace patches crates from crates.io (`[patch.crates-io]`: taffy, cosmic-text, the AccessKit adapters for macOS and Windows, wgpu-hal, winit, mlua), each with a fix zgui relies on (see their `ZGUI_PATCH.md` and the comments in `Cargo.toml`). A published crate builds against crates.io's versions, without them. Each fix has to land upstream, or ship as a published fork, before a release behaves as the repository does.
- **Credentials.** Publishing needs a crates.io token as a repository secret, and a publish step after the dry run, run by hand the first time. Neither exists yet, on purpose.
