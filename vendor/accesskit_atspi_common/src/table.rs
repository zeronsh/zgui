// zgx patch: AT-SPI's Table and TableCell interfaces, which AccessKit's
// Unix adapter leaves out (see ZGX_PATCH.md). Orca reads a table's size, a
// cell's row and column, and the headers over a cell through them.
//
// Positions come from the AccessKit properties a toolkit sets: a table's
// `row_count` and `column_count` (rows not in the tree, a virtual list's,
// counted), a cell's or a row's `row_index`, a cell's `column_index`, and
// spans. A cell without indices takes its row's index and its place among
// the row's cells.

use accesskit::Role;
use accesskit_consumer::Node;

use crate::filters::filter;

pub(crate) fn is_table(node: &Node) -> bool {
    matches!(
        node.role(),
        Role::Table | Role::Grid | Role::TreeGrid | Role::ListGrid
    )
}

pub(crate) fn is_cell(node: &Node) -> bool {
    matches!(
        node.role(),
        Role::Cell | Role::GridCell | Role::ColumnHeader | Role::RowHeader
    )
}

/// The table a cell is in: its nearest table ancestor.
pub(crate) fn table_of<'a>(cell: &Node<'a>) -> Option<Node<'a>> {
    let mut current = cell.filtered_parent(&filter)?;
    loop {
        if is_table(&current) {
            return Some(current);
        }
        current = current.filtered_parent(&filter)?;
    }
}

/// The row a cell is in, if it is in one below its table.
fn row_of<'a>(cell: &Node<'a>) -> Option<Node<'a>> {
    let mut current = cell.filtered_parent(&filter)?;
    loop {
        if is_table(&current) {
            return None;
        }
        if current.role() == Role::Row {
            return Some(current);
        }
        current = current.filtered_parent(&filter)?;
    }
}

/// A cell's row and column.
pub(crate) fn position(cell: &Node) -> Option<(usize, usize)> {
    let row_node = row_of(cell);
    let row = cell
        .data()
        .row_index()
        .or_else(|| row_node.as_ref()?.data().row_index())?;
    let column = cell.data().column_index().or_else(|| {
        row_node?
            .filtered_children(filter)
            .filter(is_cell)
            .position(|other| other.id() == cell.id())
    })?;
    Some((row, column))
}

/// A cell's rows and columns: one each unless it spans more.
pub(crate) fn spans(cell: &Node) -> (usize, usize) {
    let data = cell.data();
    (
        data.row_span().unwrap_or(1).max(1),
        data.column_span().unwrap_or(1).max(1),
    )
}

/// Every node in `table` (not in a table within it) that `keep` keeps.
fn within<'a>(table: &Node<'a>, keep: impl Fn(&Node<'a>) -> bool) -> Vec<Node<'a>> {
    let mut found = Vec::new();
    let mut stack: Vec<Node<'a>> = table.filtered_children(filter).collect();
    stack.reverse();
    while let Some(node) = stack.pop() {
        if keep(&node) {
            found.push(node);
        }
        if !is_table(&node) {
            let first = stack.len();
            stack.extend(node.filtered_children(filter));
            stack[first..].reverse();
        }
    }
    found
}

/// A table's cells, in order.
pub(crate) fn cells<'a>(table: &Node<'a>) -> Vec<Node<'a>> {
    within(table, is_cell)
}

/// A table's rows and columns: those it says it has, else those its cells
/// reach.
pub(crate) fn size(table: &Node) -> (usize, usize) {
    let data = table.data();
    let (rows, columns) = (data.row_count(), data.column_count());
    if let (Some(rows), Some(columns)) = (rows, columns) {
        return (rows, columns);
    }
    let (mut reached_rows, mut reached_columns) = (0, 0);
    for cell in cells(table) {
        if let Some((row, column)) = position(&cell) {
            let (row_span, column_span) = spans(&cell);
            reached_rows = reached_rows.max(row + row_span);
            reached_columns = reached_columns.max(column + column_span);
        }
    }
    (
        rows.unwrap_or(reached_rows),
        columns.unwrap_or(reached_columns),
    )
}

/// The cell covering `row` and `column`, if it is in the tree.
pub(crate) fn cell_at<'a>(table: &Node<'a>, row: usize, column: usize) -> Option<Node<'a>> {
    cells(table).into_iter().find(|cell| {
        position(cell).is_some_and(|(r, c)| {
            let (row_span, column_span) = spans(cell);
            (r..r + row_span).contains(&row) && (c..c + column_span).contains(&column)
        })
    })
}

/// The headers of `column`, or (`column` false) of row `index`.
pub(crate) fn headers<'a>(table: &Node<'a>, index: usize, column: bool) -> Vec<Node<'a>> {
    let role = if column {
        Role::ColumnHeader
    } else {
        Role::RowHeader
    };
    cells(table)
        .into_iter()
        .filter(|cell| {
            cell.role() == role
                && position(cell).is_some_and(|(r, c)| {
                    let (row_span, column_span) = spans(cell);
                    if column {
                        (c..c + column_span).contains(&index)
                    } else {
                        (r..r + row_span).contains(&index)
                    }
                })
        })
        .collect()
}

/// The rows selected: a selected row's index, or a selected cell's row.
pub(crate) fn selected_rows(table: &Node) -> Vec<usize> {
    let mut rows: Vec<usize> = within(table, |node| {
        (node.role() == Role::Row || is_cell(node)) && node.is_selected() == Some(true)
    })
    .iter()
    .filter_map(|node| {
        if node.role() == Role::Row {
            node.data().row_index().or_else(|| {
                node.filtered_children(filter)
                    .find(is_cell)
                    .and_then(|cell| position(&cell))
                    .map(|(row, _)| row)
            })
        } else {
            position(node).map(|(row, _)| row)
        }
    })
    .collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// Whether the cell at `row` and `column` is selected, or its row is.
pub(crate) fn is_selected(table: &Node, row: usize, column: usize) -> bool {
    cell_at(table, row, column).is_some_and(|cell| {
        cell.is_selected() == Some(true)
            || row_of(&cell).is_some_and(|row| row.is_selected() == Some(true))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeWrapper;
    use accesskit::{Node as NodeData, NodeId, Tree as TreeData, TreeId, TreeUpdate};
    use accesskit_consumer::Tree;
    use atspi_common::{Interface, State};

    /// A grid of a header row and two rows of two cells, rows 1 and 2 of 40
    /// (the rest not in the tree); row 2 selected, the second header's
    /// cell spanning both columns of a third row. Then a tree with an open
    /// item and a closed one.
    fn tree() -> Tree {
        let node = |role, children: &[u64]| {
            let mut node = NodeData::new(role);
            node.set_children(children.iter().copied().map(NodeId).collect::<Vec<_>>());
            node
        };
        let named = |role, label: &str, row: Option<usize>, column: Option<usize>| {
            let mut node = NodeData::new(role);
            node.set_label(label);
            if let Some(row) = row {
                node.set_row_index(row);
            }
            if let Some(column) = column {
                node.set_column_index(column);
            }
            node
        };
        let mut grid = node(Role::Grid, &[2, 5, 8]);
        grid.set_label("Grid");
        grid.set_row_count(40);
        grid.set_column_count(2);
        let mut row_2 = node(Role::Row, &[9, 10]);
        row_2.set_row_index(2);
        row_2.set_selected(true);
        let mut wide = named(Role::Cell, "Wide", Some(3), Some(0));
        wide.set_column_span(2);
        let mut grid_wide = node(Role::Row, &[11]);
        grid_wide.set_row_index(3);
        let mut open = named(Role::TreeItem, "Open", None, None);
        open.set_expanded(true);
        let mut closed = named(Role::TreeItem, "Closed", None, None);
        closed.set_expanded(false);
        let nodes = vec![
            (NodeId(0), node(Role::Window, &[1, 20])),
            (NodeId(1), grid),
            (NodeId(2), node(Role::Row, &[3, 4])),
            (
                NodeId(3),
                named(Role::ColumnHeader, "Name", Some(0), Some(0)),
            ),
            (
                NodeId(4),
                named(Role::ColumnHeader, "Size", Some(0), Some(1)),
            ),
            // A row whose cells say only where they are in it.
            (NodeId(5), {
                let mut row = node(Role::Row, &[6, 7]);
                row.set_row_index(1);
                row
            }),
            (NodeId(6), named(Role::Cell, "a", None, None)),
            (NodeId(7), named(Role::Cell, "1", None, None)),
            (NodeId(8), node(Role::GenericContainer, &[30, 31])),
            (NodeId(30), row_2),
            (NodeId(31), grid_wide),
            (NodeId(9), named(Role::Cell, "b", Some(2), Some(0))),
            (NodeId(10), named(Role::Cell, "2", Some(2), Some(1))),
            (NodeId(11), wide),
            (NodeId(20), node(Role::Tree, &[21, 22])),
            (NodeId(21), open),
            (NodeId(22), closed),
        ];
        Tree::new(
            TreeUpdate {
                nodes,
                tree: Some(TreeData::new(NodeId(0))),
                tree_id: TreeId::ROOT,
                focus: NodeId(0),
            },
            true,
        )
    }

    fn find<'a>(tree: &'a Tree, label: &str) -> Node<'a> {
        let mut stack = vec![tree.state().root()];
        while let Some(node) = stack.pop() {
            if node.label().as_deref() == Some(label) {
                return node;
            }
            stack.extend(node.children());
        }
        panic!("no {label}")
    }

    fn label(node: Option<Node>) -> Option<String> {
        node?.label()
    }

    #[test]
    fn a_grid_says_its_size_and_where_its_cells_are() {
        let tree = tree();
        let grid = find(&tree, "Grid");
        assert!(is_table(&grid));
        assert_eq!(size(&grid), (40, 2), "rows not in the tree counted");
        assert_eq!(position(&find(&tree, "Size")), Some((0, 1)));
        // From the row's index and the cell's place in it.
        assert_eq!(position(&find(&tree, "1")), Some((1, 1)));
        assert_eq!(table_of(&find(&tree, "2")).map(|t| t.id()), Some(grid.id()));
        assert_eq!(label(cell_at(&grid, 2, 1)), Some("2".into()));
        assert_eq!(label(cell_at(&grid, 3, 1)), Some("Wide".into()), "a span");
        assert_eq!(spans(&find(&tree, "Wide")), (1, 2));
        assert_eq!(label(cell_at(&grid, 30, 0)), None, "not in the tree");
        let column = headers(&grid, 1, true);
        assert_eq!(column.len(), 1);
        assert_eq!(column[0].label().as_deref(), Some("Size"));
        assert!(headers(&grid, 1, false).is_empty(), "no row headers");
        assert_eq!(selected_rows(&grid), vec![2]);
        assert!(is_selected(&grid, 2, 0));
        assert!(!is_selected(&grid, 1, 0));
    }

    #[test]
    fn tables_and_cells_take_their_interfaces() {
        let tree = tree();
        let interfaces = |label| NodeWrapper(&find(&tree, label)).interfaces();
        assert!(interfaces("Grid").contains(Interface::Table));
        assert!(interfaces("2").contains(Interface::TableCell));
        assert!(interfaces("Name").contains(Interface::TableCell));
        assert!(!interfaces("Open").contains(Interface::TableCell));
    }

    #[test]
    fn what_opens_says_whether_it_is_open() {
        let tree = tree();
        let state = |label| NodeWrapper(&find(&tree, label)).state(true);
        assert!(state("Open").contains(State::Expandable));
        assert!(state("Open").contains(State::Expanded));
        assert!(state("Closed").contains(State::Expandable));
        assert!(state("Closed").contains(State::Collapsed));
        assert!(!state("Closed").contains(State::Expanded));
        assert!(!state("Grid").contains(State::Expandable));
    }
}
