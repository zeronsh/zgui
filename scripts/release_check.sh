#!/bin/sh
# What a release publishes, checked as crates.io will build it, uploading
# nothing (`.github/workflows/release.yml`; see docs/releasing.md):
#
#     scripts/release_check.sh [--allow-dirty]
#
# 1. The forks zgx publishes as its own crates (vendor/harfrust,
#    cosmic-text, taffy, mlua as zgx-harfrust, zgx-cosmic-text, zgx-taffy,
#    zgx-mlua) are each packaged and built from their package, against
#    crates.io and the forks before them (zgx-cosmic-text shapes with
#    zgx-harfrust), given from vendor/ as below.
# 2. Every published workspace crate is packaged and built from its package
#    (`cargo publish --workspace --dry-run`), in dependency order. That
#    build leaves out the workspace's `[patch.crates-io]`: winit, wgpu-hal
#    and AccessKit's consumer and adapters come from crates.io (their
#    `links` fallbacks built), as an app depending on
#    zgx gets them, so what only builds with a vendored patch fails here
#    (its fallbacks are what is built). The forks, not on crates.io until
#    the release publishes them first, are given from vendor/ as `[patch]`
#    configuration for this run alone.
set -eu
cd "$(git rev-parse --show-toplevel)"
# `--allow-dirty`: check uncommitted changes too (a release itself is clean).
dirty=""
if [ "${1:-}" = --allow-dirty ]; then dirty=--allow-dirty; fi
forks="harfrust cosmic-text taffy mlua"
set --
for fork in $forks; do
    set -- "$@" --config "patch.crates-io.zgx-$fork.path=\"vendor/$fork\""
done
for fork in $forks; do
    echo "== zgx-$fork (vendor/$fork)"
    # mlua builds for one Lua, chosen by a feature: zgx's, Luau.
    features=""
    if [ "$fork" = mlua ]; then features="--features luau"; fi
    cargo package --manifest-path "vendor/$fork/Cargo.toml" $features $dirty "$@"
done
echo "== the workspace's published crates"
cargo publish --workspace --dry-run --locked $dirty "$@"
