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
# 4. The previous release's zgx-desktop and zgx-gpu, as crates.io has them,
#    build against the zgx 2 packaged: an app on them that runs
#    `cargo update -p zgx` gets this zgx, and they call it in places a
#    doc(hidden) or return-type change breaks without semver-checks seeing
#    it (zgx-desktop 0.1.3 calls `zoom::wheel`).
# Before them: changelog.d/'s entries are well formed, and none is left out
# of a CHANGELOG.md that has this version's section (scripts/changelog.py);
# each fork whose version crates.io has is what crates.io has
# (scripts/check_forks.py; 2 and 3 are given vendor/, and would hide a change
# made without a version), and zgx's Thai word dictionary (crates/zgx/data/thaidict.bin) is
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
# The first Python that runs: Windows' runners name it python, and a
# Windows without Python has a python3 that only opens the Store (it is
# found, and exits 49: "Python was not found").
python=""
for candidate in python3 python; do
    if "$candidate" -c '' >/dev/null 2>&1; then
        python=$candidate
        break
    fi
done
if [ -z "$python" ]; then
    echo "no Python runs (python3 or python): the checks below need one" >&2
    exit 1
fi
echo "== the changelog's entries"
"$python" scripts/changelog.py check --release "$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
echo "== the forks against crates.io"
"$python" scripts/check_forks.py
echo "== zgx's vendored Thai dictionary"
"$python" scripts/extract_thai_dictionary.py --check
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
echo "== the previous release's zgx-desktop and zgx-gpu against this zgx"
# The newest zgx-desktop on crates.io below this version.
previous=$(curl -sfL -A zgx-release-check https://index.crates.io/zg/x-/zgx-desktop |
    grep -o '"vers":"[^"]*"' | cut -d'"' -f4 |
    { cat; echo "$zgx"; } | sort -V | awk -v this="$zgx" '$0 == this { print last; exit } { last = $0 }')
if [ -z "$previous" ]; then
    echo "no earlier zgx-desktop on crates.io"
    exit 0
fi
compat="$target/compat-app"
rm -rf "$compat"
mkdir -p "$compat/src"
{
    printf '[package]\nname = "zgx-compat-app"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n'
    printf '[workspace]\n\n[dependencies]\nzgx = "=%s"\nzgx-desktop = "=%s"\nzgx-gpu = "=%s"\n\n[patch.crates-io]\n' "$zgx" "$previous" "$previous"
    printf 'zgx = { path = "../package/zgx-%s" }\n' "$zgx"
    for fork in $forks; do
        printf 'zgx-%s = { path = "../package/zgx-%s-%s" }\n' "$fork" "$fork" "$(version "vendor/$fork/Cargo.toml")"
    done
} > "$compat/Cargo.toml"
printf 'use zgx_desktop as _;\nuse zgx_gpu as _;\nfn main() {}\n' > "$compat/src/main.rs"
cargo check --manifest-path "$compat/Cargo.toml" --target-dir "$target"
echo "zgx-desktop and zgx-gpu $previous build against zgx $zgx"
