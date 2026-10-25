# zgx patch to accesskit_consumer 0.38.0

Vendored from crates.io `accesskit_consumer` 0.38.0 (MIT OR Apache-2.0)
and applied through `[patch.crates-io]` in the workspace `Cargo.toml`.
Changes are marked "zgx patch" in the source.

## Why

A form field's error (`aria-invalid`, `aria-errormessage`): AccessKit's
`Node` carries `invalid`, `error_message` and `described_by`, but the
consumer gives the adapters no way to reach the nodes those relations
name (`NodeId::with_same_tree` is the crate's own), so no adapter told
the platform a field was invalid or what said why.

## What

1. `Node::is_invalid`, `Node::error_message` and `Node::described_by`
   (`src/node.rs`): the targets as nodes, any not in the tree left out.
2. `links = "zgx_accesskit_consumer"` and a build script printing
   `cargo:patched=1`: zgx-desktop reads it (`DEP_ZGX_ACCESSKIT_CONSUMER_PATCHED`,
   its `zgx_accesskit_patched` cfg) to give the error message as a relation
   only where the adapters read it, and to say it in the description
   otherwise (crates.io's adapters read neither).

The adapters' use of them is in vendor/accesskit_macos, accesskit_windows
and accesskit_atspi_common's ZGX_PATCH.md. Drop the patch when an
upstream release does the same.
