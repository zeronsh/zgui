# zgx patch to accesskit_unix 0.22.1 and accesskit_atspi_common 0.19.1

Vendored from crates.io `accesskit_unix` 0.22.1 and `accesskit_atspi_common`
0.19.1 (`vendor/accesskit_atspi_common`), both MIT OR Apache-2.0 (the
published crates ship no license files), and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source.

## Why

AccessKit's Linux adapter leaves out two things Orca reads, which GTK and
Chromium give it:

- AT-SPI's `Table` and `TableCell` interfaces. A data grid read as a table
  with no size, its cells with no row or column, and no header over them:
  Orca can't say "row 3 of 501, Size", or move by row and column. zgx
  already gives AccessKit the counts and indices (`TablePlace`).
- The `expandable`, `expanded` and `collapsed` states. A tree item, a
  select or a date picker says nothing of whether it is open, though zgx
  sets AccessKit's `expanded`.

`accesskit_atspi_common` 0.21 and `accesskit_unix` 0.24 (the latest, of
2026-09) still lack both.

## What

1. `accesskit_atspi_common/src/table.rs`: a table's size (its
   `row_count`/`column_count`, rows not in the tree counted, else what its
   cells reach), a cell's position (its `row_index`/`column_index`, else its
   row's index and its place in the row), spans, the cell at a row and
   column, headers, selected rows. Tests at its end.
2. `accesskit_atspi_common/src/node.rs`: a table role (`Table`, `Grid`,
   `TreeGrid`, `ListGrid`) takes the `Table` interface, a cell or header in
   one `TableCell`; `PlatformNode`'s `table_*` methods; `expanded` as
   `Expandable` with `Expanded` or `Collapsed`, as Chromium's ATK states do.
   AccessKit's state diffing announces the change.
3. `accesskit_unix/src/atspi/interfaces/table.rs`: the two D-Bus interfaces,
   registered and unregistered with the others in `src/atspi/bus.rs`. A
   cell's index is its row times the table's columns, plus its column.
   Selecting rows or columns through `Table` is refused (a table's rows are
   selected through `Selection`, or the keyboard); `Caption` and `Summary`
   are null.
4. `accesskit_unix/Cargo.toml`: built alone, it takes the vendored
   `accesskit_atspi_common`.

5. `accesskit_atspi_common/src/node.rs`: a form field's error
   (`aria-invalid`, `aria-errormessage`), from the vendored
   accesskit_consumer (vendor/accesskit_consumer/ZGX_PATCH.md): the
   `invalid-entry` state while it is invalid, its error message node as an
   `ErrorMessage` relation (as Orca reads it), and the nodes that describe
   it as `DescribedBy`. The crate's `accesskit_consumer` dependency is the
   vendored one (`path`).

6. Another process's tree embedded under a node (a web page's, WebKitGTK
   drawing it in a process of its own; zgx-desktop's `webkitgtk`). A node
   whose author ID is `accesskit_atspi_common::EMBED`, a bus name and an
   object path (and a count, changed to embed again), each on a line:
   - `accesskit_unix/src/atspi/interfaces/accessible.rs`: lists that object
     as its last child (`ChildCount`, `GetChildAtIndex`, `GetChildren`);
     the author ID is no `AccessibleId`.
   - `accesskit_unix/src/atspi/bus.rs`: as the node is registered, and
     whenever its embedding changes (`ObjectEvent::Embedded`, from
     `accesskit_atspi_common/src/adapter.rs`), calls
     `org.a11y.atspi.Socket.Embedded` on the object from the adapter's own
     connection, as an AT-SPI socket asks the plug it embeds: the object
     takes the node for its parent, so a reader following focus into it
     finds the app's window above it.
   - `accesskit_atspi_common/src/adapter.rs` and `node.rs`: the node itself
     is never focused for AT-SPI (no `Focused` state or event): the
     embedded tree says what is focused, and a reader hearing the node
     focused after the page's field would leave the page.

zgx-desktop's `atspi_tests` (`src/accessibility.rs`, Linux) read a zgx
data grid and tree, and an invalid field, through the patched adapter.
Run the vendored tests with
`cargo test --manifest-path vendor/accesskit_atspi_common/Cargo.toml --target-dir target`.

Drop the patch when an upstream release does the same.
