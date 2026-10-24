# zgui patch to accesskit_unix 0.22.1 and accesskit_atspi_common 0.19.1

Vendored from crates.io `accesskit_unix` 0.22.1 and `accesskit_atspi_common`
0.19.1 (`vendor/accesskit_atspi_common`), both MIT OR Apache-2.0 (the
published crates ship no license files), and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgui patch" in the source.

## Why

AccessKit's Linux adapter leaves out two things Orca reads, which GTK and
Chromium give it:

- AT-SPI's `Table` and `TableCell` interfaces. A data grid read as a table
  with no size, its cells with no row or column, and no header over them:
  Orca can't say "row 3 of 501, Size", or move by row and column. zgui
  already gives AccessKit the counts and indices (`TablePlace`).
- The `expandable`, `expanded` and `collapsed` states. A tree item, a
  select or a date picker says nothing of whether it is open, though zgui
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

zgui-desktop's `atspi_tests` (`src/accessibility.rs`, Linux) read a zgui
data grid and tree through the patched adapter.
Run the vendored tests with
`cargo test --manifest-path vendor/accesskit_atspi_common/Cargo.toml --target-dir target`.

Drop the patch when an upstream release does the same.
