# zgx patch to accesskit_macos 0.26.3

Vendored from crates.io `accesskit_macos` 0.26.3 (MIT OR Apache-2.0; the
published crate ships no license files) and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source. The crate's tests are left out.

## Why

AccessKit 0.26 exposes `Role::Heading` with the role string `"Heading"`
rather than AppKit's `AXHeading`, so VoiceOver does not take zgx's
headings for headings: the rotor's Headings list is empty and VO-Command-H
moves past them. WebKit exposes headings as `AXHeading`, with the level as
their `AXValue`, which VoiceOver reads ("heading level 2").

## What

1. `Role::Heading` maps to `AXHeading` (`src/node.rs`, the role table).
2. A heading's `accessibilityValue` is its level when it has one, ahead of
   its text value (`src/node.rs`, `value`).

Drop the patch when an upstream release does the same.
