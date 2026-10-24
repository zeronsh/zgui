//! A data grid: rows of cells under a header of columns, as WAI-ARIA's grid
//! and a spreadsheet's table. The grid is one Tab stop and keeps focus, its
//! current cell the active descendant. The arrows move (mirrored right to
//! left), Home and End go to the row's ends (with ⌘ or Ctrl, the grid's),
//! Page Up and Down a page. Enter or F2 edits an editable cell, as typing
//! over it does; Enter keeps the edit and moves down, Escape drops it;
//! Delete clears the cell. Up from the first row reaches the header: Enter
//! or Space sorts its column, ⌘⇧ (Ctrl+Shift) with an arrow moves it, Alt
//! with an arrow narrows or widens it. With the pointer, a header's click
//! sorts, a drag moves its column and its edge's drag resizes it (a double
//! click puts it back). The rows are virtual: a grid of a hundred thousand
//! builds only what shows.
use crate::{
    compose::{
        Context, NodeHandle, View, column, component, div, keyed, row, scroll_x, switch,
        text_input, text_signal, virtual_list,
    },
    cursor::Cursor,
    input::{EventContext, EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Direction, NodeId},
    semantics::{Role, SortOrder, TablePlace},
    style::{Styled, Styles},
    text_layout::TextAlign,
    theme::tok,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

/// A column of a [`data_grid`].
#[derive(Clone, Debug, PartialEq)]
pub struct GridColumn {
    /// Unique among the columns: what cells and the sort name it by.
    pub id: String,
    /// Its header.
    pub title: String,
    /// Its width in points.
    pub width: f32,
    /// What resizing leaves it at least.
    pub min_width: f32,
    /// Its header sorts by it.
    pub sortable: bool,
    /// Its cells are edited.
    pub editable: bool,
    /// Its cells sit at the end of the line, as numbers do.
    pub align_end: bool,
}
impl GridColumn {
    /// A column `width` wide.
    pub fn new(id: impl Into<String>, title: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width,
            min_width: 40.,
            sortable: false,
            editable: false,
            align_end: false,
        }
    }
    /// That its header sorts by.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }
    /// Whose cells are edited.
    pub fn editable(mut self) -> Self {
        self.editable = true;
        self
    }
    /// Whose cells sit at the end of the line (numbers).
    pub fn align_end(mut self) -> Self {
        self.align_end = true;
        self
    }
}

/// A grid's sort: by a column, and whether largest (last) first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridSort {
    /// The column's id.
    pub column: String,
    /// Largest (last) first.
    pub descending: bool,
}

/// A cell of a grid: its row, from 0 as the grid shows them, and its
/// column's id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GridCell {
    /// Its row, from 0.
    pub row: usize,
    /// Its column's id.
    pub column: String,
}
impl GridCell {
    /// Row `row`'s cell in column `column`.
    pub fn new(row: usize, column: impl Into<String>) -> Self {
        Self {
            row,
            column: column.into(),
        }
    }
}

type Edit = Rc<dyn Fn(usize, &str, String)>;
type CellText = Rc<dyn Fn(usize, &str) -> String>;

/// Where a [`data_grid`]'s rows come from: how many there are and each
/// cell's text (both followed: a signal they read changing shows the
/// change), and what an edit and an activation do.
pub struct GridSource {
    rows: Rc<dyn Fn() -> usize>,
    cell: CellText,
    edit: Option<Edit>,
    activate: Option<Rc<dyn Fn(usize)>>,
}
impl GridSource {
    /// `rows` rows, row `r`'s cell in column `c` showing `cell(r, c)`.
    pub fn new(
        rows: impl Fn() -> usize + 'static,
        cell: impl Fn(usize, &str) -> String + 'static,
    ) -> Self {
        Self {
            rows: Rc::new(rows),
            cell: Rc::new(cell),
            edit: None,
            activate: None,
        }
    }
    /// An edit kept: its row, column and text. Without one nothing edits.
    pub fn on_edit(mut self, edit: impl Fn(usize, &str, String) + 'static) -> Self {
        self.edit = Some(Rc::new(edit));
        self
    }
    /// A row activated: a double click on it, or Enter where nothing edits.
    pub fn on_activate(mut self, activate: impl Fn(usize) + 'static) -> Self {
        self.activate = Some(Rc::new(activate));
        self
    }
}

/// A row's height.
const ROW: f32 = 28.;
/// The resize handle's width at a header's edge.
const EDGE: f32 = 6.;
/// How far Alt and an arrow resize.
const RESIZE_STEP: f32 = 16.;

/// What the grid's parts share.
struct Grid {
    columns: Signal<Vec<GridColumn>>,
    /// The columns' ids in order: what the header's and the rows' cells
    /// follow, unchanged as a column is resized.
    order: Signal<Vec<String>>,
    sort: Signal<Option<GridSort>>,
    current: Signal<Option<GridCell>>,
    /// The cursor is on the header row: its column's header is current.
    header: Signal<bool>,
    editing: Signal<Option<GridCell>>,
    draft: Signal<String>,
    focused: Signal<bool>,
    source: GridSource,
    /// The rows' scroll, and the columns'.
    offset: Signal<f32>,
    scroll: Signal<f32>,
    node: RefCell<Option<NodeHandle>>,
    /// The cells and headers built, for the active descendant and their
    /// places.
    cells: RefCell<HashMap<GridCell, NodeId>>,
    headers: RefCell<HashMap<String, NodeId>>,
    /// The widths the columns came with (a double click on an edge).
    defaults: HashMap<String, f32>,
}

impl Grid {
    fn order(&self) -> Vec<String> {
        self.columns
            .with_untracked(|cs| cs.iter().map(|c| c.id.clone()).collect())
    }
    fn column_untracked(&self, id: &str) -> Option<GridColumn> {
        self.columns
            .with_untracked(|cs| cs.iter().find(|c| c.id == id).cloned())
    }
    fn handle(&self) -> Option<NodeHandle> {
        self.node.borrow().clone()
    }
    fn rtl(&self) -> bool {
        self.handle().is_some_and(|handle| {
            handle
                .scene()
                .is_some_and(|scene| scene.borrow().direction(handle.node()) == Direction::Rtl)
        })
    }
    /// Edit `at`, starting from `draft` (else its text): whether its column
    /// edits.
    fn start(&self, at: GridCell, draft: Option<String>) -> bool {
        if self.source.edit.is_none()
            || !self
                .column_untracked(&at.column)
                .is_some_and(|c| c.editable)
        {
            return false;
        }
        self.draft
            .set(draft.unwrap_or_else(|| (self.source.cell)(at.row, &at.column)));
        self.header.set(false);
        self.current.set(Some(at.clone()));
        self.editing.set(Some(at));
        true
    }
    /// End the edit, keeping it or not, and focus the grid again (not when
    /// focus left for something else).
    fn commit(&self, keep: bool, refocus: bool) {
        let Some(at) = self.editing.with_untracked(Clone::clone) else {
            return;
        };
        self.editing.set(None);
        let text = self.draft.with_untracked(Clone::clone);
        if keep
            && let Some(edit) = &self.source.edit
            && text != (self.source.cell)(at.row, &at.column)
        {
            edit(at.row, &at.column, text);
        }
        if refocus && let Some(handle) = self.handle() {
            handle.focus();
        }
    }
    fn cycle_sort(&self, id: &str) {
        if !self.column_untracked(id).is_some_and(|c| c.sortable) {
            return;
        }
        let next = match self.sort.with_untracked(Clone::clone) {
            Some(s) if s.column == id && !s.descending => Some(GridSort {
                column: id.to_owned(),
                descending: true,
            }),
            Some(s) if s.column == id => None,
            _ => Some(GridSort {
                column: id.to_owned(),
                descending: false,
            }),
        };
        self.sort.set(next);
    }
    fn move_column(&self, from: usize, to: usize) {
        let mut next = self.columns.with_untracked(Clone::clone);
        if from == to || from >= next.len() || to >= next.len() {
            return;
        }
        let moved = next.remove(from);
        next.insert(to, moved);
        self.columns.set(next);
    }
    fn resize(&self, id: &str, width: f32) {
        let mut next = self.columns.with_untracked(Clone::clone);
        if let Some(c) = next.iter_mut().find(|c| c.id == id) {
            let width = width.max(c.min_width).round();
            if c.width == width {
                return;
            }
            c.width = width;
            self.columns.set(next);
        }
    }
    /// The grid's size: its rows (the header's too) and columns.
    fn describe_size(&self, rows: usize, columns: usize) {
        let Some(handle) = self.handle() else { return };
        if let Some(ui) = handle.weak_ui().upgrade() {
            ui.semantics.borrow_mut().update(handle.node(), |node| {
                node.table = Some(Box::new(TablePlace {
                    rows: Some(rows + 1),
                    columns: Some(columns),
                    ..Default::default()
                }));
            });
        }
    }
    /// The grid's active descendant: the current cell, or its header.
    fn describe_active(&self) {
        let Some(handle) = self.handle() else { return };
        let target = self.current.with_untracked(|at| {
            let at = at.as_ref()?;
            if self.header.with_untracked(|h| *h) {
                self.headers.borrow().get(&at.column).copied()
            } else {
                self.cells.borrow().get(at).copied()
            }
        });
        if let Some(ui) = handle.weak_ui().upgrade() {
            ui.semantics
                .borrow_mut()
                .update(handle.node(), |node| node.active_descendant = target);
        }
    }
    /// The current cell in view: the rows' and the columns' scroll moved.
    fn reveal(&self) {
        let (Some(handle), Some(at)) = (self.handle(), self.current.with_untracked(Clone::clone))
        else {
            return;
        };
        let Some(scene) = handle.scene() else { return };
        let bounds = scene.borrow().bounds(handle.node());
        // Not laid out yet: no view to bring it into.
        if bounds.height <= 0. {
            return;
        }
        if !self.header.with_untracked(|h| *h) {
            // The body: the grid less its header and the line under it.
            let height = (bounds.height - ROW - 1.).max(ROW);
            let top = at.row as f32 * ROW;
            let now = self.offset.with_untracked(|o| *o);
            if top < now {
                self.offset.set(top);
            } else if top + ROW > now + height {
                self.offset.set(top + ROW - height);
            }
        }
        let place = self.columns.with_untracked(|cs| {
            let i = cs.iter().position(|c| c.id == at.column)?;
            Some((cs[..i].iter().map(|c| c.width).sum::<f32>(), cs[i].width))
        });
        if let Some((start, width)) = place {
            let now = self.scroll.with_untracked(|s| *s);
            if start < now {
                self.scroll.set(start);
            } else if start + width > now + bounds.width {
                self.scroll.set((start + width - bounds.width).min(start));
            }
        }
    }
}

/// A cell or header built, by its place or column.
enum Built {
    Cell(GridCell),
    Header(String),
}
/// Removes a built cell's or header's entry as it goes.
struct Registered {
    grid: Weak<Grid>,
    built: Built,
    node: NodeId,
}
impl Drop for Registered {
    fn drop(&mut self) {
        let Some(grid) = self.grid.upgrade() else {
            return;
        };
        match &self.built {
            Built::Cell(at) => {
                let mut cells = grid.cells.borrow_mut();
                if cells.get(at) == Some(&self.node) {
                    cells.remove(at);
                }
            }
            Built::Header(id) => {
                let mut headers = grid.headers.borrow_mut();
                if headers.get(id) == Some(&self.node) {
                    headers.remove(id);
                }
            }
        }
    }
}

/// A grid of `source`'s rows under `columns`, two-way (resizing and moving
/// a column change it), sorted as `sort` says: a header sets it (ascending,
/// descending, then none) and the app gives its rows in that order.
/// `current` is the current cell, two-way; `label` names the grid.
pub fn data_grid(
    label: impl Into<String>,
    columns: Signal<Vec<GridColumn>>,
    sort: Signal<Option<GridSort>>,
    current: Signal<Option<GridCell>>,
    source: GridSource,
) -> View {
    let label = label.into();
    component(move |cx| {
        let runtime = cx.runtime();
        let defaults = columns.with_untracked(|cs| {
            cs.iter()
                .map(|c| (c.id.clone(), c.width))
                .collect::<HashMap<_, _>>()
        });
        let order = runtime.signal(
            columns.with_untracked(|cs| cs.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        );
        {
            let (columns, order) = (columns.clone(), order.clone());
            cx.retain(runtime.effect(move || {
                order.set(columns.with(|cs| cs.iter().map(|c| c.id.clone()).collect()));
            }));
        }
        let grid = Rc::new(Grid {
            columns,
            order,
            sort,
            current,
            header: runtime.signal(false),
            editing: runtime.signal(None),
            draft: runtime.signal(String::new()),
            focused: runtime.signal(false),
            source,
            offset: runtime.signal(0.),
            scroll: runtime.signal(0.),
            node: RefCell::default(),
            cells: RefCell::default(),
            headers: RefCell::default(),
            defaults,
        });
        // The current cell in view, and the active descendant.
        {
            let grid = grid.clone();
            cx.retain(runtime.effect(move || {
                grid.current.with(|_| ());
                grid.header.with(|_| ());
                grid.reveal();
                grid.describe_active();
            }));
        }
        {
            let grid = grid.clone();
            cx.retain(runtime.effect(move || {
                let rows = (grid.source.rows)();
                let columns = grid.columns.with(Vec::len);
                grid.describe_size(rows, columns);
            }));
        }
        // Each built cell's and header's column, as columns move; each
        // header's sort.
        {
            let grid = grid.clone();
            let last: RefCell<Option<(Vec<String>, Option<GridSort>)>> = RefCell::default();
            cx.retain(runtime.effect(move || {
                let order = grid.order.get();
                let sort = grid.sort.get();
                let now = Some((order, sort));
                if *last.borrow() == now {
                    return;
                }
                *last.borrow_mut() = now.clone();
                let Some((order, sort)) = now else { return };
                let Some(ui) = grid.handle().and_then(|h| h.weak_ui().upgrade()) else {
                    return;
                };
                let column_of = |id: &str| order.iter().position(|o| o == id);
                let mut semantics = ui.semantics.borrow_mut();
                for (at, node) in grid.cells.borrow().iter() {
                    semantics.update(*node, |n| {
                        n.table = Some(Box::new(TablePlace {
                            row: Some(at.row + 1),
                            column: column_of(&at.column),
                            ..Default::default()
                        }));
                    });
                }
                for (id, node) in grid.headers.borrow().iter() {
                    semantics.update(*node, |n| {
                        n.table = Some(Box::new(header_place(column_of(id), &sort, id)));
                    });
                }
            }));
        }

        let header = {
            let (grid, ids) = (grid.clone(), grid.clone());
            keyed(
                move || ids.order.get(),
                move |id: String, _: &mut Context| header_cell(grid.clone(), id),
            )
            .flex_row()
            .h(ROW)
            .shrink_0()
            .role(Role::Row)
            .bg(tok::SURFACE_RAISED)
        };
        let rows = {
            let grid = grid.clone();
            move |index: Signal<usize>, _: usize, cx: &mut Context| {
                grid_row(grid.clone(), index.with_untracked(|i| *i), cx)
            }
        };
        let count = grid.clone();
        let body = virtual_list(
            grid.offset.clone(),
            ROW,
            6,
            move || (count.source.rows)(),
            |i| i,
            rows,
        )
        .grow();

        let (width, keys, mount) = (grid.clone(), grid.clone(), grid.clone());
        div()
            .role(Role::Grid)
            .accessible_label(label.clone())
            .focusable(true)
            .h_full()
            .on_mount(move |handle| {
                *mount.node.borrow_mut() = Some(handle);
                // What the effects found before the grid was there.
                let (rows, columns) = (
                    (mount.source.rows)(),
                    mount.columns.with_untracked(Vec::len),
                );
                mount.describe_size(rows, columns);
                mount.reveal();
                mount.describe_active();
            })
            .on_event(move |cx| grid_keys(&keys, cx))
            .child(
                scroll_x(grid.scroll.clone()).h_full().child(
                    column()
                        .h_full()
                        .shrink_0()
                        .reactive_style(move || {
                            Styles::new().w(width
                                .columns
                                .with(|cs| cs.iter().map(|c| c.width).sum::<f32>()))
                        })
                        .child(header)
                        .child(
                            div()
                                .h(1.)
                                .shrink_0()
                                .bg(tok::BORDER_SUBTLE)
                                .accessible_hidden(),
                        )
                        .child(body),
                ),
            )
    })
}

/// A header's place: row 0, its column, and its sort.
fn header_place(column: Option<usize>, sort: &Option<GridSort>, id: &str) -> TablePlace {
    TablePlace {
        row: Some(0),
        column,
        sort: sort.as_ref().filter(|s| s.column == id).map(|s| {
            if s.descending {
                SortOrder::Descending
            } else {
                SortOrder::Ascending
            }
        }),
        ..Default::default()
    }
}

/// The grid's keys, its focus, and its presses.
fn grid_keys(grid: &Rc<Grid>, cx: &mut EventContext) {
    if cx.phase == EventPhase::Capture {
        return;
    }
    let own = cx.target == cx.current_target;
    let editing = grid.editing.with_untracked(Option::is_some);
    match &cx.event {
        InputEvent::Focus if own => {
            grid.focused.set(true);
            if grid.current.with_untracked(Option::is_none)
                && let Some(first) = grid.order().into_iter().next()
            {
                grid.header.set((grid.source.rows)() == 0);
                grid.current.set(Some(GridCell::new(0, first)));
            }
            return;
        }
        InputEvent::Blur if own => {
            grid.focused.set(false);
            return;
        }
        // Typed before the cell's field took focus: into it.
        InputEvent::Text(text) if own && editing => {
            let text = text.clone();
            grid.draft.update(|d| d.push_str(&text));
            cx.stop_propagation();
            return;
        }
        InputEvent::PointerDown { .. } if !editing => {
            cx.focus();
            return;
        }
        _ => {}
    }
    // While a cell edits, its field has the keys.
    if editing {
        return;
    }
    let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
        return;
    };
    let order = grid.order();
    if order.is_empty() {
        return;
    }
    let rows = (grid.source.rows)();
    let (r, c) = match grid.current.with_untracked(Clone::clone) {
        Some(at) => (
            at.row.min(rows.saturating_sub(1)),
            order.iter().position(|o| *o == at.column).unwrap_or(0),
        ),
        None => (0, 0),
    };
    let (back, on) = if grid.rtl() {
        (Key::ArrowRight, Key::ArrowLeft)
    } else {
        (Key::ArrowLeft, Key::ArrowRight)
    };
    let command = modifiers.meta || modifiers.control;
    let last_row = rows.saturating_sub(1);
    let last_col = order.len() - 1;
    let go = |r: usize, c: usize| {
        grid.current.set(Some(GridCell::new(r, order[c].clone())));
    };
    let at = GridCell::new(r, order[c].clone());
    if grid.header.with_untracked(|h| *h) {
        match key {
            k if (*k == on || *k == back) && command && modifiers.shift => {
                let to = if *k == on {
                    (c + 1).min(last_col)
                } else {
                    c.saturating_sub(1)
                };
                grid.move_column(c, to);
            }
            k if (*k == on || *k == back) && modifiers.alt => {
                if let Some(column) = grid.column_untracked(&at.column) {
                    let by = if *k == on { RESIZE_STEP } else { -RESIZE_STEP };
                    grid.resize(&at.column, column.width + by);
                }
            }
            k if *k == on => go(r, (c + 1).min(last_col)),
            k if *k == back => go(r, c.saturating_sub(1)),
            Key::Home => go(r, 0),
            Key::End => go(r, last_col),
            Key::ArrowDown if rows > 0 => {
                grid.header.set(false);
            }
            Key::Enter => grid.cycle_sort(&at.column),
            Key::Character(s) if s == " " => grid.cycle_sort(&at.column),
            _ => return,
        }
        cx.prevent_default();
        cx.stop_propagation();
        return;
    }
    // A page: the rows that show, less one.
    let page = grid
        .handle()
        .and_then(|h| h.scene().map(|s| s.borrow().bounds(h.node()).height))
        .map_or(10, |h| {
            (((h - ROW) / ROW).max(0.) as usize)
                .saturating_sub(1)
                .max(1)
        });
    match key {
        Key::ArrowUp if r == 0 => {
            grid.header.set(true);
        }
        _ if rows == 0 => return,
        Key::ArrowDown => go((r + 1).min(last_row), c),
        Key::ArrowUp => go(r - 1, c),
        k if *k == on => go(r, (c + 1).min(last_col)),
        k if *k == back => go(r, c.saturating_sub(1)),
        Key::Home if command => go(0, 0),
        Key::End if command => go(last_row, last_col),
        Key::Home => go(r, 0),
        Key::End => go(r, last_col),
        Key::PageDown => go((r + page).min(last_row), c),
        Key::PageUp => go(r.saturating_sub(page), c),
        Key::Function(2) => {
            grid.start(at, None);
        }
        Key::Enter => {
            if !grid.start(at, None)
                && let Some(activate) = &grid.source.activate
            {
                activate(r);
            }
        }
        Key::Delete | Key::Backspace => {
            if grid
                .column_untracked(&at.column)
                .is_some_and(|col| col.editable)
                && let Some(edit) = &grid.source.edit
            {
                edit(at.row, &at.column, String::new());
            }
        }
        // Typing over a cell edits it: what is typed goes on into its field.
        Key::Character(_) if !command && !modifiers.alt => {
            grid.start(at, Some(String::new()));
            return;
        }
        _ => return,
    }
    cx.prevent_default();
    cx.stop_propagation();
}

/// Row `r`: its cells, striped, and marked while the cursor is in it.
fn grid_row(grid: Rc<Grid>, r: usize, cx: &mut Context) -> View {
    let runtime = cx.runtime();
    // The cursor's column while it is in this row, and the editing one:
    // only this row's cells follow them.
    let here = runtime.signal(None::<String>);
    let editing_here = runtime.signal(None::<String>);
    {
        let (grid, here, editing_here) = (grid.clone(), here.clone(), editing_here.clone());
        cx.retain(runtime.effect(move || {
            let header = grid.header.get();
            let now = grid.current.with(|at| {
                at.as_ref()
                    .filter(|at| at.row == r && !header)
                    .map(|at| at.column.clone())
            });
            if here.with_untracked(|h| *h != now) {
                here.set(now);
            }
            let edit = grid.editing.with(|at| {
                at.as_ref()
                    .filter(|at| at.row == r)
                    .map(|at| at.column.clone())
            });
            if editing_here.with_untracked(|e| *e != edit) {
                editing_here.set(edit);
            }
        }));
    }
    let cells = {
        let (grid, ids, here) = (grid.clone(), grid.clone(), here.clone());
        keyed(
            move || ids.order.get(),
            move |id: String, _: &mut Context| {
                grid_cell(grid.clone(), r, id, here.clone(), editing_here.clone())
            },
        )
        .flex_row()
    };
    let (selected, styled) = (here.clone(), here);
    row()
        .role(Role::Row)
        .h(ROW)
        .selected_when(move || selected.with(Option::is_some))
        .reactive_style(move || {
            Styles::new().bg(if styled.with(Option::is_some) {
                tok::SELECTION_ITEM
            } else if r % 2 == 1 {
                tok::SURFACE_SUNKEN
            } else {
                tok::SURFACE_BASE
            })
        })
        .on_mount(move |handle| {
            if let Some(ui) = handle.weak_ui().upgrade() {
                ui.semantics.borrow_mut().update(handle.node(), |n| {
                    n.table = Some(Box::new(TablePlace {
                        row: Some(r + 1),
                        ..Default::default()
                    }));
                });
            }
        })
        .child(cells)
}

/// Row `r`'s cell in column `id`: its text, or the field editing it.
fn grid_cell(
    grid: Rc<Grid>,
    r: usize,
    id: String,
    here: Signal<Option<String>>,
    editing_here: Signal<Option<String>>,
) -> View {
    let (field, shown, editing_id, shown_id) = (grid.clone(), grid.clone(), id.clone(), id.clone());
    let body = switch(
        move || editing_here.with(|e| e.as_deref() == Some(&*editing_id)),
        move |editing, _| {
            if editing {
                return cell_editor(field.clone(), r);
            }
            let (text, align) = (shown.clone(), shown.clone());
            let (text_id, align_id) = (shown_id.clone(), shown_id.clone());
            text_signal(move || (text.source.cell)(r, &text_id))
                .w_full()
                .truncate()
                .reactive_style(move || {
                    let end = align
                        .columns
                        .with(|cs| cs.iter().any(|c| c.id == align_id && c.align_end));
                    Styles::new().text_align(if end {
                        TextAlign::End
                    } else {
                        TextAlign::Start
                    })
                })
        },
    );
    let (styled, mount, press) = (grid.clone(), grid.clone(), grid);
    let (styled_id, mount_id, press_id) = (id.clone(), id.clone(), id);
    div()
        .role(Role::Cell)
        .shrink_0()
        .h(ROW)
        .px(8.)
        .flex_row()
        .items_center()
        .reactive_style(move || {
            let width = styled.columns.with(|cs| {
                cs.iter()
                    .find(|c| c.id == styled_id)
                    .map_or(100., |c| c.width)
            });
            let styles = Styles::new().w(width);
            if here.with(|h| h.as_deref() == Some(&*styled_id)) {
                let ring = if styled.focused.get() {
                    tok::FOCUS_RING
                } else {
                    tok::BORDER_STRONG
                };
                styles.border(2.).border_color(ring).border_over(true)
            } else {
                styles.border(0.)
            }
        })
        .on_mount(move |handle| {
            let at = GridCell::new(r, mount_id.clone());
            mount.cells.borrow_mut().insert(at.clone(), handle.node());
            if let Some(ui) = handle.weak_ui().upgrade() {
                let column = mount.order().iter().position(|o| *o == mount_id);
                ui.semantics.borrow_mut().update(handle.node(), |n| {
                    n.table = Some(Box::new(TablePlace {
                        row: Some(r + 1),
                        column,
                        ..Default::default()
                    }));
                });
            }
            let current = mount.current.with_untracked(|c| c.as_ref() == Some(&at));
            handle.retain(Registered {
                grid: Rc::downgrade(&mount),
                built: Built::Cell(at),
                node: handle.node(),
            });
            if current {
                mount.describe_active();
            }
        })
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            let InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } = cx.event
            else {
                return;
            };
            let at = GridCell::new(r, press_id.clone());
            // A press in the field editing this cell is the field's; one
            // elsewhere keeps that edit first.
            if press.editing.with_untracked(|e| e.as_ref() == Some(&at)) {
                return;
            }
            press.commit(true, true);
            if cx.click_count() >= 2 {
                if !press.start(at, None)
                    && let Some(activate) = &press.source.activate
                {
                    activate(r);
                }
            } else {
                press.header.set(false);
                press.current.set(Some(at));
            }
        })
        // The text (or field) fills the cell: aligned, and truncated at its
        // edge.
        .child(body.grow().min_w(0.))
}

/// The field editing a cell of row `r`: focused as it comes. Enter keeps
/// its text and moves down, Escape drops it; focus leaving keeps it.
fn cell_editor(grid: Rc<Grid>, r: usize) -> View {
    let mount = grid.clone();
    text_input("Edit cell", grid.draft.clone())
        .w_full()
        .h(ROW - 6.)
        .px(4.)
        .on_mount(move |handle| {
            handle.focus();
            let end = mount.draft.with_untracked(String::len);
            handle.set_selection(end, end);
        })
        // Before the editor, whose Enter and Escape are otherwise its own.
        .on_event(move |cx| {
            if cx.phase == EventPhase::Bubble {
                return;
            }
            match &cx.event {
                InputEvent::KeyDown {
                    key: Key::Enter, ..
                } => {
                    let column = grid
                        .editing
                        .with_untracked(|e| e.as_ref().map(|e| e.column.clone()));
                    grid.commit(true, true);
                    if let Some(column) = column
                        && r + 1 < (grid.source.rows)()
                    {
                        grid.current.set(Some(GridCell::new(r + 1, column)));
                    }
                }
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                } => grid.commit(false, true),
                InputEvent::Blur if cx.target == cx.current_target => {
                    grid.commit(true, false);
                    return;
                }
                _ => return,
            }
            cx.prevent_default();
            cx.stop_propagation();
        })
}

/// Column `id`'s header: its title and sort. A click sorts, a drag moves
/// the column; its edge's drag resizes it, a double click there puts its
/// width back.
fn header_cell(grid: Rc<Grid>, id: String) -> View {
    let resizing: Rc<Cell<Option<(f32, f32)>>> = Rc::default();
    let (edge_grid, edge_id) = (grid.clone(), id.clone());
    let edge = div()
        .absolute()
        .top(0.)
        .bottom(0.)
        .end(0.)
        .w(EDGE)
        .cursor(Cursor::ColResize)
        .accessible_hidden()
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            match cx.event {
                InputEvent::PointerDown {
                    x,
                    button: PointerButton::Primary,
                    ..
                } => {
                    if cx.click_count() >= 2 {
                        if let Some(width) = edge_grid.defaults.get(&edge_id) {
                            edge_grid.resize(&edge_id, *width);
                        }
                    } else if let Some(column) = edge_grid.column_untracked(&edge_id) {
                        resizing.set(Some((x, column.width)));
                        cx.capture_pointer();
                    }
                    cx.prevent_default();
                    cx.stop_propagation();
                }
                InputEvent::PointerMove { x, .. } => {
                    if let Some((from, width)) = resizing.get() {
                        let by = if edge_grid.rtl() { from - x } else { x - from };
                        edge_grid.resize(&edge_id, width + by);
                        cx.stop_propagation();
                    }
                }
                InputEvent::PointerUp { .. } | InputEvent::PointerCancel
                    if resizing.take().is_some() =>
                {
                    cx.release_pointer();
                    cx.stop_propagation();
                }
                _ => {}
            }
        });

    // A press: where it began, and whether it has moved the column.
    let dragging: Rc<Cell<Option<(f32, bool)>>> = Rc::default();
    let (title, label, styled, arrow) = (grid.clone(), grid.clone(), grid.clone(), grid.clone());
    let (title_id, label_id, styled_id, arrow_id) =
        (id.clone(), id.clone(), id.clone(), id.clone());
    let (mount, mount_id) = (grid.clone(), id.clone());
    let (press, press_id) = (grid, id);
    div()
        .role(Role::ColumnHeader)
        .relative()
        .shrink_0()
        .h(ROW)
        .px(8.)
        .gap(4.)
        .flex_row()
        .items_center()
        .accessible_label_when(move || {
            label
                .columns
                .with(|cs| {
                    cs.iter()
                        .find(|c| c.id == label_id)
                        .map(|c| c.title.clone())
                })
                .unwrap_or_default()
        })
        .reactive_style(move || {
            let (width, end) = styled.columns.with(|cs| {
                cs.iter()
                    .find(|c| c.id == styled_id)
                    .map_or((100., false), |c| (c.width, c.align_end))
            });
            let current = styled.header.get()
                && styled.focused.get()
                && styled
                    .current
                    .with(|c| c.as_ref().is_some_and(|c| c.column == styled_id));
            // A number's column has its title at the end too.
            let styles = Styles::new().w(width);
            let styles = if end {
                styles.justify_end()
            } else {
                styles.justify_start()
            };
            if current {
                styles
                    .border(2.)
                    .border_color(tok::FOCUS_RING)
                    .border_over(true)
            } else {
                styles.border(0.)
            }
        })
        .on_mount(move |handle| {
            mount
                .headers
                .borrow_mut()
                .insert(mount_id.clone(), handle.node());
            if let Some(ui) = handle.weak_ui().upgrade() {
                let column = mount.order().iter().position(|o| *o == mount_id);
                let sort = mount.sort.with_untracked(Clone::clone);
                ui.semantics.borrow_mut().update(handle.node(), |n| {
                    n.table = Some(Box::new(header_place(column, &sort, &mount_id)));
                });
            }
            handle.retain(Registered {
                grid: Rc::downgrade(&mount),
                built: Built::Header(mount_id.clone()),
                node: handle.node(),
            });
        })
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            match cx.event {
                InputEvent::PointerDown {
                    x,
                    button: PointerButton::Primary,
                    ..
                } => {
                    dragging.set(Some((x, false)));
                    cx.capture_pointer();
                }
                InputEvent::PointerMove { x, .. } => {
                    let Some((start, moved)) = dragging.get() else {
                        return;
                    };
                    let rtl = press.rtl();
                    let dx = if rtl { start - x } else { x - start };
                    if !moved && dx.abs() < 6. {
                        return;
                    }
                    dragging.set(Some((start, true)));
                    // Its middle past a neighbour's, it takes that place:
                    // its place among the others is how many middles it is
                    // past.
                    let cs = press.columns.with_untracked(Clone::clone);
                    let Some(from) = cs.iter().position(|c| c.id == press_id) else {
                        return;
                    };
                    let before: f32 = cs[..from].iter().map(|c| c.width).sum();
                    let middle = before + cs[from].width / 2. + dx;
                    let (mut start_of, mut to) = (0., 0);
                    for (i, c) in cs.iter().enumerate() {
                        if i != from && middle > start_of + c.width / 2. {
                            to += 1;
                        }
                        start_of += c.width;
                    }
                    if to != from {
                        press.move_column(from, to);
                        // Measured from its new place from now on.
                        let after: f32 = press
                            .columns
                            .with_untracked(|cs| cs[..to].iter().map(|c| c.width).sum());
                        let shift = if rtl { before - after } else { after - before };
                        dragging.set(Some((start + shift, true)));
                    }
                    cx.stop_propagation();
                }
                InputEvent::PointerUp { .. } => {
                    let Some((_, moved)) = dragging.take() else {
                        return;
                    };
                    cx.release_pointer();
                    // A click, not a drag: it sorts.
                    if !moved {
                        press.cycle_sort(&press_id);
                        let row = press
                            .current
                            .with_untracked(|c| c.as_ref().map_or(0, |c| c.row));
                        press.header.set(true);
                        press
                            .current
                            .set(Some(GridCell::new(row, press_id.clone())));
                    }
                }
                InputEvent::PointerCancel if dragging.take().is_some() => {
                    cx.release_pointer();
                }
                _ => {}
            }
        })
        .child(
            text_signal(move || {
                title
                    .columns
                    .with(|cs| {
                        cs.iter()
                            .find(|c| c.id == title_id)
                            .map(|c| c.title.clone())
                    })
                    .unwrap_or_default()
            })
            .font_weight(600)
            .truncate(),
        )
        .child(
            text_signal(move || match arrow.sort.get() {
                Some(s) if s.column == arrow_id && s.descending => "▼".into(),
                Some(s) if s.column == arrow_id => "▲".into(),
                _ => String::new(),
            })
            .text_size(9.)
            .text_color(tok::TEXT_SECONDARY)
            .shrink_0()
            .accessible_hidden(),
        )
        .child(edge)
}
