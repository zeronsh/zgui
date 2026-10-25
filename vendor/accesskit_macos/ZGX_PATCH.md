# zgx patch to accesskit_macos 0.26.3

Vendored from crates.io `accesskit_macos` 0.26.3 (MIT OR Apache-2.0; the
published crate ships no license files) and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source. The crate's tests are left out.

## Why

Headings: AccessKit 0.26 exposes `Role::Heading` with the role string `"Heading"`
rather than AppKit's `AXHeading`, so VoiceOver does not take zgx's
headings for headings: the rotor's Headings list is empty and VO-Command-H
moves past them. WebKit exposes headings as `AXHeading`, with the level as
their `AXValue`, which VoiceOver reads ("heading level 2").

## What

1. `Role::Heading` maps to `AXHeading` (`src/node.rs`, the role table).
2. A heading's `accessibilityValue` is its level when it has one, ahead of
   its text value (`src/node.rs`, `value`).
3. A grid's (or table's, tree grid's) `accessibilityRows` and
   `accessibilitySelectedRows` are its `Role::Row` descendants, through any
   containers between them (`src/node.rs`, `rows`, `selected_rows`,
   `table_rows`). AccessKit takes rows from `items`, which counts only
   item-like children, and a row is not one: a grid gave VoiceOver no rows,
   so its table navigation found nothing. Checked by
   `scripts/macos_accessibility_probe.py`.

4. A form field's error (`aria-invalid`, `aria-errormessage`): `AXInvalid`
   is `"true"` while the field is invalid, `"false"` otherwise, as WebKit
   gives it, and the error message's text follows the field's
   description in `accessibilityHelp` (`AXHelp`), as Safari reads
   `aria-errormessage` (`src/node.rs`, `description` and
   `accessibility_attribute_value`). It uses the vendored
   accesskit_consumer's `is_invalid` and `error_message`
   (vendor/accesskit_consumer/ZGX_PATCH.md).

Drop the patch when an upstream release does the same.
