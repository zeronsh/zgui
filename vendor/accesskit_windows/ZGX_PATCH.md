# zgx patch to accesskit_windows 0.34.0

Vendored from crates.io `accesskit_windows` 0.34.0 (MIT OR Apache-2.0; the
published crate ships no license files) and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source.

## Why

AccessKit gives a heading's level to UI Automation only as the generic
`Level` property and the ARIA `level`, never as `HeadingLevel` (UIA
property 30173). Narrator's heading navigation (scan mode H, the headings
list) and Windows' other screen readers go by `HeadingLevel`, so zgx's
headings read as "heading" but cannot be moved between. Chromium and
WebView2 set it. 0.35.1 and upstream main still leave it out.

## What

1. `heading_level` in `src/node.rs`: a heading's level 1 to 9 as
   `HeadingLevel1` to `HeadingLevel9`, exposed as
   `UIA_HeadingLevelPropertyId`. Other nodes keep UIA's default,
   `HeadingLevel_None`.
2. A test, `src/tests/heading.rs`, reading it through the UIA client.

Run the crate's tests on Windows with
`cargo test --manifest-path vendor/accesskit_windows/Cargo.toml --locked`.
Upstream's `simple::focus` brings its window to the foreground, and fails
when Windows refuses that (a run from a background terminal).

Drop the patch when an upstream release does the same.
