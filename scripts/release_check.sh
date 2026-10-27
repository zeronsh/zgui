#!/bin/sh
# What a release publishes, checked as crates.io will build it, uploading
# nothing (`.github/workflows/release.yml`; see docs/releasing.md):
#
#     scripts/release_check.sh [--allow-dirty]
#
# 1. The forks zgx publishes as its own crates (vendor/harfrust,
#    cosmic-text, taffy, mlua as zgx-harfrust, zgx-cosmic-text, zgx-taffy,
#    zgx-mlua) are each packaged and built from their package, against
#    crates.io and the forks they depend on (zgx-cosmic-text shapes with
#    zgx-harfrust), given from vendor/.
# 2. Every published workspace crate is packaged and built from its package
#    (`cargo publish --workspace --dry-run`), in dependency order. That
#    build leaves out the workspace's `[patch.crates-io]`: winit, wgpu-hal
#    and AccessKit's consumer and adapters come from crates.io (their
#    `links` fallbacks built), as an app depending on
#    zgx gets them, so what only builds with a vendored patch fails here
#    (its fallbacks are what is built). The forks, not on crates.io until
#    the release publishes them first, are given from vendor/ as `[patch]`
#    configuration for this run alone.
# 3. A new app with no lockfile, resolving every other crate afresh from
#    crates.io (as `cargo add zgx-script` does), depends on what 1 and 2
#    packaged and runs a script headless: zgx's own lockfile holds its
#    dependencies where they were tested, which a new app's does not
#    (luau0-src 0.22.1 failed every Luau app at start, unseen by 1 and 2).
# Before them: zgx's Thai word dictionary (crates/zgx/data/thaidict.bin) is
# what scripts/extract_thai_dictionary.py extracts from the locked ICU4X
# data, and zgx's package carries its Unicode licence.
set -eu
cd "$(git rev-parse --show-toplevel)"
# `--allow-dirty`: check uncommitted changes too (a release itself is clean).
dirty=""
if [ "${1:-}" = --allow-dirty ]; then dirty=--allow-dirty; fi
forks="harfrust cosmic-text taffy mlua"
# Where packages are unpacked, the forks' beside the workspace's (3 reads
# them there).
target="${CARGO_TARGET_DIR:-target}"
set --
for fork in $forks; do
    set -- "$@" --config "patch.crates-io.zgx-$fork.path=\"vendor/$fork\""
done
for fork in $forks; do
    echo "== zgx-$fork (vendor/$fork)"
    # mlua builds for one Lua, chosen by a feature: zgx's, Luau.
    features=""
    if [ "$fork" = mlua ]; then features="--features luau"; fi
    # A fork is given only the forks it depends on: a patch it never uses
    # is written into its own lockfile (`[[patch.unused]]`, harfrust's and
    # taffy's are committed), which leaves the checkout dirty.
    deps=""
    if [ "$fork" = cosmic-text ]; then
        deps='patch.crates-io.zgx-harfrust.path="vendor/harfrust"'
    fi
    cargo package --manifest-path "vendor/$fork/Cargo.toml" $features $dirty \
        --target-dir "$target" ${deps:+--config "$deps"}
done
echo "== zgx's vendored Thai dictionary"
# (Windows' runners name it python.)
"$(command -v python3 || command -v python)" scripts/extract_thai_dictionary.py --check
cargo package -p zgx --list $dirty | grep -qx data/LICENSE-UNICODE
echo "== the workspace's published crates"
cargo publish --workspace --dry-run --locked $dirty "$@"
echo "== a new app, resolving afresh, runs a script"
version() { grep -m1 '^version' "$1" | cut -d'"' -f2; }
zgx=$(version Cargo.toml)
app="$target/fresh-app"
rm -rf "$app"
mkdir -p "$app/src"
{
    printf '[package]\nname = "zgx-fresh-app"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n'
    # A workspace of its own, not a member of zgx's when under its target/.
    printf '[workspace]\n\n[dependencies]\nzgx-script = "=%s"\n\n[patch.crates-io]\n' "$zgx"
    for crate in zgx zgx-components zgx-gpu zgx-desktop zgx-script zgx-run zgx-ghostty-sys; do
        printf '%s = { path = "../package/%s-%s" }\n' "$crate" "$crate" "$zgx"
    done
    for fork in $forks; do
        printf 'zgx-%s = { path = "../package/zgx-%s-%s" }\n' "$fork" "$fork" "$(version "vendor/$fork/Cargo.toml")"
    done
} > "$app/Cargo.toml"
cat > "$app/src/main.rs" <<'RUST'
// A Luau script loaded, mounted and clicked, headless.
fn main() {
    let mut app = zgx_script::testing::TestApp::inline(
        r#"local count = ui.state("count", 1)
return ui.column {
    ui.text { id = "count", "Count: {count}" },
    ui.button { id = "more", label = "More", on_click = function() count:set(count:get() + 1) end },
}"#,
    );
    app.click("more");
    assert_eq!(app.text("count"), "Count: 2");
    println!("a script ran: {}", app.text("count"));
}
RUST
cargo run --manifest-path "$app/Cargo.toml" --target-dir "$target"
