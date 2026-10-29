# Releasing

zgx 0.1.5 is on crates.io (2026-10-28, tag `v0.1.5`; 0.1.0, the first, on 2026-10-25). This is how a release goes, and what it checks.

## Version scheme

- Every published crate shares one version, the workspace's (`[workspace.package] version` in `Cargo.toml`). Path dependencies between them name it (`version = "0.1.2"`), as crates.io needs.
- It follows [semver](https://semver.org) as Cargo reads it before 1.0: `0.MINOR.PATCH`, where a new minor may break apps (each break listed in `CHANGELOG.md` with what to check and how to migrate) and a patch does not.
- A release is a tag `v<version>` on `main` (`v0.2.0`), made once its changelog section is written: `CHANGELOG.md`'s "Unreleased" becomes `## 0.2.0 (date)`.
- A fix to a release already out (a patch with nothing else in it) branches from that release's tag, not from `main`, whose later changes would each need their semver check and dry run: `release-0.1.2` is `v0.1.1` and the fix, tagged `v0.1.2` on that branch and published from it, the fix then brought to `main` by its own pull request (as 0.1.2 was).
- The oldest Rust the workspace builds with is `rust-version` (1.95: `AtomicU64::try_update`). CI's Linux job checks it: its no-default-features check runs on that toolchain.

## What is published

zgx's crates: `zgx`, `zgx-components`, `zgx-gpu`, `zgx-3d`, `zgx-desktop`, `zgx-script`, `zgx-run` and `zgx-ghostty-sys` (its declarations only; its `link` feature builds Ghostty from a pinned source with Zig). Not published (`publish = false`): `zgx-demo` and `zgx-workload`, the benchmarks in `comparisons/`.

And four forks, as zgx's own crates, published first:

| Crate | Fork of | Why | Notes |
| --- | --- | --- | --- |
| `zgx-harfrust` | harfrust 0.5.2 | a variable font's glyph advances worked out once for its instance, not for every run shaped | `vendor/harfrust/ZGX_PATCH.md` |
| `zgx-cosmic-text` | cosmic-text 0.19.0 | text shaping that streams without rebuilding every word's key; shapes with `zgx-harfrust` | `vendor/cosmic-text/ZGX_PATCH.md` |
| `zgx-taffy` | taffy 0.9.0 | auto margins that keep a flex line's gaps; a node whose display changes lays its children out afresh (0.9.1) | `vendor/taffy/ZGX_PATCH.md` |
| `zgx-mlua` | mlua 0.12.2 | table walks and userdata without a copy per key; Luau 744's bytecode (version 13) loading, which mlua-sys 0.13 takes for text | `vendor/mlua/ZGX_PATCH.md` |

Each keeps its upstream's license files, authors and library name (`harfrust`, `cosmic_text`, `taffy`, `mlua`), and its version (a fix of our own bumps the patch: `zgx-cosmic-text` is 0.19.2, as it shapes with `zgx-harfrust` (0.19.1) and places glyphs inline (0.19.2); `zgx-mlua` is 0.12.5, its userdata checks inlined (0.12.4) and Luau 744's bytecode loading (0.12.5); `zgx-taffy` is 0.9.1, its children's caches cleared when a node's display changes; `zgx-harfrust` is 0.5.3, as 0.5.2, uploaded on 2026-10-26, never reached crates.io's index). zgx's crates depend on them by path and version under their upstream names (`cosmic-text = { package = "zgx-cosmic-text", version = "0.19.2", path = "../../vendor/cosmic-text" }`, and `zgx-cosmic-text` on `zgx-harfrust` likewise), so code reads `cosmic_text::` as before. Only zgx's crates use them, so an app never sees two of one. Drop a fork, back to the upstream crate, once a release of it has the fix.

## Vendored data

zgx carries ICU4X's Thai word dictionary for its `cjk-thai-words` feature (`crates/zgx/data/thaidict.bin`, 131,759 bytes). ICU4X's compiled data keeps Thai in one table with Burmese, Khmer and Lao, and linking it would add the other three (about 0.9 MB). Chinese and Japanese come from that compiled data as a dependency. The file is extracted from `icu_segmenter_data` 2.4, the version zgx's `icu_segmenter = "~2.4"` pins, by `scripts/extract_thai_dictionary.py`. It reads the locked crate from Cargo's registry, writes the same bytes from the same source, and with `--check` (the release check runs it) fails if the committed file differs.

- **Licence.** The data is Unicode's, under the Unicode License v3, a permissive one. So zgx's `license` is `MIT AND Unicode-3.0`, and its package ships the licence text as `data/LICENSE-UNICODE` (the release check fails without it). A release's metadata and semver check expect that field, which changed after 0.1.3.
- **Moving ICU4X on** (2.5, say):
  1. Bump `icu_segmenter`, `icu_provider` and `icu_collections` in `crates/zgx/Cargo.toml`, and `PINNED` in the script, together.
  2. Run `cargo update -p icu_segmenter`.
  3. Run the script, and copy that version's `LICENSE` from `icu_segmenter_data` over `data/LICENSE-UNICODE`.
  4. Run zgx's `words::` tests. They read Chinese, Japanese and Thai words, so they catch a dictionary that didn't load.

  If the data's layout changed (2.4 moved Thai from a char16 trie to an alphabet and a `ZeroTrieSimpleAscii`), the script stops and says so. Then the script and `words.rs`'s provider change with it.

## What stays patched in the repository alone

The workspace's `[patch.crates-io]` keeps the rest: wgpu-hal (Metal command buffers made lazily and never empty, a Direct3D 12 adapter exposed without opening the others), winit (a Wayland window's `xdg_surface`, X11's `_NET_WM_SYNC_REQUEST` and `_NET_WM_USER_TIME`), and AccessKit's consumer and adapters for macOS, Windows and Linux (headings, grid rows, tables, tree items, a field's error message). Forking them would mean forking what depends on them too (wgpu, accesskit_winit). An app depending on zgx from crates.io gets their releases instead, and zgx builds and runs with them, without those fixes:

- **winit**: Wayland popups open in their window, as on a compositor without `xdg_popup`, instead of as popup surfaces of their own. Text dragged in from other apps drops into fields on Wayland alone (zgx's own data device takes it; on macOS, Windows and X11 the platform's drag reaches winit's windows, whose hook is the patch's), and macOS's Services don't act on zgx's selected text. A Wayland input method isn't told a field's input mode (email, URL, number, phone): every field is plain text to it. On macOS a key a web page or terminal has the keyboard for is never the app's: neither its reserved shortcuts nor one the page leaves alone. The X11 changes are behaviour alone: a window manager's resize no longer waits for frames, and focus-stealing prevention sees no user time.
- **wgpu-hal**: Metal frames take upstream's command buffers (more per submission); on Windows, `request_adapter` ranks the adapters, opening a device on each.
- **AccessKit**: VoiceOver's rotor misses headings and its table navigation finds no grid rows; Narrator and Orca lose what their patches add. An invalid field's error is said in its description rather than as the platform's own invalid flag and error message (zgx-desktop sees which it has by the vendored consumer's `links` metadata, so patch the consumer with the adapters, never alone). Everything else reads as before.

Where a patch adds an API zgx calls (winit's `xdg_surface`, `text_drop` and `SelectionServices`, wgpu-hal's `expose_preferred_adapter`), zgx detects it at build time: the vendored crate carries `links` metadata (`links = "zgx_winit"`, `"zgx_wgpu_hal"`) whose `patched` key reaches the build scripts of the crates depending on it (zgx-desktop, zgx-gpu), which set `zgx_winit_patched` and `zgx_wgpu_hal_patched`. With crates.io's crates neither is set, and the fallback above is compiled. Nothing to enable: a build in the repository has every patch, any other has none.

## Follow-ups: patches to drop

- **harfrust**: from 0.9 a shaper takes font functions of its own (`FontFuncs`, `ShapeOptions::font_funcs`), which could keep a variable instance's advances from cosmic-text. Upgrading cosmic-text's harfrust to it (0.5 → 0.9 changes its API) and keeping them there drops `zgx-harfrust`.
- **taffy**: the auto-margin gaps fix shipped upstream in taffy 0.14.0. Upgrading zgx's layout to it (0.9 → 0.14 changes taffy's API) drops `zgx-taffy` for crates.io's taffy.
- **winit**: native popups are on winit's master (the 0.31 betas). Upgrading to 0.31 drops the `xdg_surface` accessors (Wayland popups through winit's own), and with them `zgx_winit_patched`; the X11 changes are to check against it then.

## What a release checks

`.github/workflows/release.yml` runs on a `v*` tag, or by hand (Actions → Release dry run), on Linux, macOS and Windows (each has fallbacks of its own to build). It checks that the tag is the workspace's version, then runs `scripts/release_check.sh`:

1. Each fork is packaged and built from its package, against crates.io.
2. Every published workspace crate is packaged and built from its package, in dependency order (`cargo publish --workspace --dry-run`). That build leaves the workspace's `[patch.crates-io]` out: winit, wgpu-hal and AccessKit come from crates.io, as an app depending on zgx gets them, so what only builds with a vendored patch fails it. The forks, which crates.io has only once the release publishes them, are given from `vendor/` (`--config 'patch.crates-io.zgx-cosmic-text.path="vendor/cosmic-text"'`) for that run alone.
3. A new app with no lockfile depends on what 1 and 2 packaged, every other crate resolved afresh from crates.io as `cargo add` resolves it, and runs a Luau script headless. zgx's lockfile holds its dependencies where they were tested; a new app's does not, and a new release of one (luau0-src 0.22.1, Luau 744) failed every Luau app at start while 1 and 2 passed.
4. The previous release's zgx-desktop and zgx-gpu, from crates.io, build against the zgx 2 packaged, as an app on them gets it from `cargo update -p zgx`. They call zgx where a change semver-checks passes over still breaks them: a return type (0.1.3's `PaintItem::matrix`, put back before it shipped) or a `doc(hidden)` function (`zoom::wheel`, put back for 0.1.4).

It never publishes. It doesn't run on pull requests: CI's time is unchanged. Locally, `scripts/release_check.sh --allow-dirty` checks uncommitted work. In Git Bash on Windows, coreutils' `link` comes before MSVC's on the path, and every build script fails to link ("link: extra operand"): set `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER` to MSVC's `link.exe` (GitHub's runners don't need it). Before a release, the check can run on machines of our own while CI's queue is long: 0.1.3's ran in the Linux container and on a Windows PC as well as macOS.

## Publishing

Not automated, on purpose: publishing needs a crates.io token, and is done by hand, in this order, each after the release check passes on the tag:

1. The forks that changed since they were last published, their versions bumped, `zgx-harfrust` before `zgx-cosmic-text` (which depends on it): `cargo publish --manifest-path vendor/harfrust/Cargo.toml`, `vendor/cosmic-text`, `vendor/taffy`, `vendor/mlua --features luau`.
2. The workspace: `cargo publish --workspace --locked` (it orders the crates itself).

`cargo publish --workspace` can stop part way with cargo's own error "no packages ready to publish but … remain in plan … awaiting confirmation" (cargo 1.96, at 0.1.0 and again at 0.1.2, each time after `zgx-gpu`; 0.1.1 went through): what it uploaded is published (check crates.io), and the rest publish one at a time in dependency order, each once the one before is in the index (`curl https://index.crates.io/zg/x-/zgx-gpu`): `cargo publish -p zgx-script --locked`, then `zgx-desktop`, then `zgx-run`.

crates.io's index can stop taking new versions for everyone while uploads still succeed (2026-10-26, about four hours from 04:46 UTC; its status page said operational): the API lists the version, the index (`https://index.crates.io/zg/x-/<crate>?cb=<anything>`, past its cache) answers 404, and nothing depending on it publishes. Check that other crates' newest versions are missing too before blaming ours; a version bump doesn't help (zgx-harfrust 0.5.2 and 0.5.3 both waited). Wait, and go on once it answers.

Then tag the commit (`v<version>`), make a GitHub release from its changelog section (its links absolute, `https://github.com/zeronsh/zgx/blob/v<version>/…`: a release resolves no relative ones), and check a new project, with no lockfile, builds from crates.io alone and runs: `cargo add zgx zgx-desktop zgx-script zgx-run`, a `main` calling `zgx_run::run` on an `app.luau`, launched to its window. Building is not enough: 0.1.0 and 0.1.1 built in a new project and panicked as their first script loaded (luau0-src 0.22.1, resolved afresh; the release check's third step now runs one).

A crate on crates.io can't be deleted, only yanked: a version published is published for good.
