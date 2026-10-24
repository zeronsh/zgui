// zgui patch: AT-SPI's Table and TableCell interfaces (see ZGUI_PATCH.md),
// over `accesskit_atspi_common`'s table methods. Selecting rows or columns
// through them is refused: a table's rows are selected through its
// Selection interface, or the keyboard.

use accesskit_atspi_common::{NodeId, PlatformNode};
use zbus::{fdo, interface, names::OwnedUniqueName};

use crate::atspi::{ObjectId, OwnedObjectAddress};

pub(crate) struct TableInterface {
    bus_name: OwnedUniqueName,
    node: PlatformNode,
}

impl TableInterface {
    pub fn new(bus_name: OwnedUniqueName, node: PlatformNode) -> Self {
        Self { bus_name, node }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.node, error)
    }

    fn address(&self, node: Option<NodeId>) -> (OwnedObjectAddress,) {
        let id = node.map(|node| ObjectId::Node {
            adapter: self.node.adapter_id(),
            node,
        });
        super::optional_object_address(&self.bus_name, id)
    }
}

#[interface(name = "org.a11y.atspi.Table")]
impl TableInterface {
    #[zbus(property, name = "NRows")]
    fn n_rows(&self) -> fdo::Result<i32> {
        Ok(self.node.table_size().map_err(self.map_error())?.0)
    }

    #[zbus(property, name = "NColumns")]
    fn n_columns(&self) -> fdo::Result<i32> {
        Ok(self.node.table_size().map_err(self.map_error())?.1)
    }

    #[zbus(property)]
    fn caption(&self) -> OwnedObjectAddress {
        OwnedObjectAddress::null()
    }

    #[zbus(property)]
    fn summary(&self) -> OwnedObjectAddress {
        OwnedObjectAddress::null()
    }

    #[zbus(property, name = "NSelectedRows")]
    fn n_selected_rows(&self) -> fdo::Result<i32> {
        let rows = self.node.table_selected_rows().map_err(self.map_error())?;
        Ok(rows.len().try_into().unwrap_or(i32::MAX))
    }

    #[zbus(property, name = "NSelectedColumns")]
    fn n_selected_columns(&self) -> i32 {
        0
    }

    fn get_accessible_at(&self, row: i32, column: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let cell = self
            .node
            .table_cell_at(index(row)?, index(column)?)
            .map_err(self.map_error())?;
        Ok(self.address(cell))
    }

    fn get_index_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        self.node
            .table_index_at(index(row)?, index(column)?)
            .map_err(self.map_error())
    }

    fn get_row_at_index(&self, index: i32) -> fdo::Result<i32> {
        let at = self
            .node
            .table_position_at_index(self::index(index)?)
            .map_err(self.map_error())?;
        Ok(at.map_or(-1, |(row, _)| row))
    }

    fn get_column_at_index(&self, index: i32) -> fdo::Result<i32> {
        let at = self
            .node
            .table_position_at_index(self::index(index)?)
            .map_err(self.map_error())?;
        Ok(at.map_or(-1, |(_, column)| column))
    }

    fn get_row_description(&self, row: i32) -> fdo::Result<String> {
        self.node
            .table_description(index(row)?, false)
            .map_err(self.map_error())
    }

    fn get_column_description(&self, column: i32) -> fdo::Result<String> {
        self.node
            .table_description(index(column)?, true)
            .map_err(self.map_error())
    }

    fn get_row_extent_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        let extents = self
            .node
            .table_extents_at(index(row)?, index(column)?)
            .map_err(self.map_error())?;
        Ok(extents.map_or(0, |(_, _, rows, _, _)| rows))
    }

    fn get_column_extent_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        let extents = self
            .node
            .table_extents_at(index(row)?, index(column)?)
            .map_err(self.map_error())?;
        Ok(extents.map_or(0, |(_, _, _, columns, _)| columns))
    }

    fn get_row_header(&self, row: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let header = self
            .node
            .table_header(index(row)?, false)
            .map_err(self.map_error())?;
        Ok(self.address(header))
    }

    fn get_column_header(&self, column: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let header = self
            .node
            .table_header(index(column)?, true)
            .map_err(self.map_error())?;
        Ok(self.address(header))
    }

    fn get_selected_rows(&self) -> fdo::Result<Vec<i32>> {
        self.node.table_selected_rows().map_err(self.map_error())
    }

    fn get_selected_columns(&self) -> Vec<i32> {
        Vec::new()
    }

    fn is_row_selected(&self, row: i32) -> fdo::Result<bool> {
        let rows = self.node.table_selected_rows().map_err(self.map_error())?;
        Ok(rows.contains(&row))
    }

    fn is_column_selected(&self, _column: i32) -> bool {
        false
    }

    fn is_selected(&self, row: i32, column: i32) -> fdo::Result<bool> {
        self.node
            .table_is_selected(index(row)?, index(column)?)
            .map_err(self.map_error())
    }

    fn add_row_selection(&self, _row: i32) -> bool {
        false
    }

    fn add_column_selection(&self, _column: i32) -> bool {
        false
    }

    fn remove_row_selection(&self, _row: i32) -> bool {
        false
    }

    fn remove_column_selection(&self, _column: i32) -> bool {
        false
    }

    /// Cell `index`'s row, column, rows and columns spanned, and whether it
    /// is selected, after whether there is one.
    fn get_row_column_extents_at_index(
        &self,
        index: i32,
    ) -> fdo::Result<(bool, i32, i32, i32, i32, bool)> {
        let not_found = (false, -1, -1, 0, 0, false);
        let Some((row, column)) = self
            .node
            .table_position_at_index(self::index(index)?)
            .map_err(self.map_error())?
        else {
            return Ok(not_found);
        };
        let extents = self
            .node
            .table_extents_at(row as usize, column as usize)
            .map_err(self.map_error())?;
        Ok(
            extents.map_or(not_found, |(row, column, rows, columns, selected)| {
                (true, row, column, rows, columns, selected)
            }),
        )
    }
}

pub(crate) struct TableCellInterface {
    bus_name: OwnedUniqueName,
    node: PlatformNode,
}

impl TableCellInterface {
    pub fn new(bus_name: OwnedUniqueName, node: PlatformNode) -> Self {
        Self { bus_name, node }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.node, error)
    }

    fn addresses(&self, nodes: Vec<NodeId>) -> Vec<OwnedObjectAddress> {
        nodes
            .into_iter()
            .map(|node| {
                ObjectId::Node {
                    adapter: self.node.adapter_id(),
                    node,
                }
                .to_address(&self.bus_name)
            })
            .collect()
    }
}

#[interface(name = "org.a11y.atspi.TableCell")]
impl TableCellInterface {
    #[zbus(property)]
    fn column_span(&self) -> fdo::Result<i32> {
        Ok(self.node.table_cell_spans().map_err(self.map_error())?.1)
    }

    #[zbus(property)]
    fn row_span(&self) -> fdo::Result<i32> {
        Ok(self.node.table_cell_spans().map_err(self.map_error())?.0)
    }

    #[zbus(property)]
    fn position(&self) -> fdo::Result<(i32, i32)> {
        self.node.table_cell_position().map_err(self.map_error())
    }

    #[zbus(property)]
    fn table(&self) -> fdo::Result<OwnedObjectAddress> {
        let table = self.node.table_cell_table().map_err(self.map_error())?;
        Ok(ObjectId::Node {
            adapter: self.node.adapter_id(),
            node: table,
        }
        .to_address(&self.bus_name))
    }

    /// Whether it is in a table, then its row, column, rows and columns
    /// spanned.
    fn get_row_column_span(&self) -> fdo::Result<(bool, i32, i32, i32, i32)> {
        let (row, column) = self.node.table_cell_position().map_err(self.map_error())?;
        let (rows, columns) = self.node.table_cell_spans().map_err(self.map_error())?;
        Ok((true, row, column, rows, columns))
    }

    fn get_column_header_cells(&self) -> fdo::Result<Vec<OwnedObjectAddress>> {
        let headers = self
            .node
            .table_cell_headers(true)
            .map_err(self.map_error())?;
        Ok(self.addresses(headers))
    }

    fn get_row_header_cells(&self) -> fdo::Result<Vec<OwnedObjectAddress>> {
        let headers = self
            .node
            .table_cell_headers(false)
            .map_err(self.map_error())?;
        Ok(self.addresses(headers))
    }
}

fn index(index: i32) -> fdo::Result<usize> {
    index
        .try_into()
        .map_err(|_| fdo::Error::InvalidArgs("Index can't be negative.".into()))
}
