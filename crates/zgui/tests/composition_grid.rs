//! The data grid: the keyboard (cells, the header, sorting, moving and
//! resizing columns), editing, what assistive technology hears, thousands
//! of rows, right to left, and the pointer.
use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::{GridCell, GridColumn, GridSort, GridSource, ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::{Direction, NodeId, NodeKind},
    semantics::{Role, SortOrder},
    widgets::Ui,
};

struct Fixture {
    ui: Ui,
    view: ViewHandle,
    columns: Signal<Vec<GridColumn>>,
    sort: Signal<Option<GridSort>>,
    current: Signal<Option<GridCell>>,
    /// Each row's name and age.
    people: Signal<Vec<(String, f64)>>,
    edits: Rc<RefCell<Vec<(usize, String, String)>>>,
    activated: Rc<RefCell<Vec<usize>>>,
}

fn fixture(rows: usize) -> Fixture {
    let mut ui = Ui::new(500., 300.);
    let columns = ui.signal(vec![
        GridColumn::new("name", "Name", 160.).sortable().editable(),
        GridColumn::new("age", "Age", 80.).sortable().align_end(),
        GridColumn::new("city", "City", 120.),
    ]);
    let sort = ui.signal(None::<GridSort>);
    let current = ui.signal(None::<GridCell>);
    let people = ui.signal(
        (0..rows)
            .map(|i| (format!("Person {i:04}"), (i * 7 % 90) as f64))
            .collect::<Vec<_>>(),
    );
    let edits = Rc::new(RefCell::new(Vec::new()));
    let activated = Rc::new(RefCell::new(Vec::new()));
    // The app sorts: the rows' order as `sort` says.
    let (order_people, order_sort) = (people.clone(), sort.clone());
    let order = move || {
        let people = order_people.get();
        let mut index: Vec<usize> = (0..people.len()).collect();
        if let Some(by) = order_sort.get() {
            index.sort_by(|&a, &b| {
                let o = if by.column == "age" {
                    people[a].1.total_cmp(&people[b].1)
                } else {
                    people[a].0.cmp(&people[b].0)
                };
                if by.descending { o.reverse() } else { o }
            });
        }
        index
    };
    let order = Rc::new(order);
    let (count, cell_people, cell_order) = (people.clone(), people.clone(), order.clone());
    let (edit_log, edit_people, edit_order) = (edits.clone(), people.clone(), order.clone());
    let log = activated.clone();
    let source = GridSource::new(
        move || count.with(Vec::len),
        move |r, column| {
            let i = cell_order()[r];
            cell_people.with(|p| match column {
                "name" => p[i].0.clone(),
                "age" => p[i].1.to_string(),
                _ => "Lisbon".into(),
            })
        },
    )
    .on_edit(move |r, column, text| {
        edit_log
            .borrow_mut()
            .push((r, column.to_owned(), text.clone()));
        let i = edit_order()[r];
        edit_people.update(|p| p[i].0 = text);
    })
    .on_activate(move |r| log.borrow_mut().push(r));
    let view = ui.mount(column().id("root").w(500.).h(300.).child(data_grid(
        "People",
        columns.clone(),
        sort.clone(),
        current.clone(),
        source,
    )));
    ui.prepare_frame();
    ui.prepare_frame();
    Fixture {
        ui,
        view,
        columns,
        sort,
        current,
        people,
        edits,
        activated,
    }
}
fn key_with(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
    ui.prepare_frame();
}
fn key(ui: &mut Ui, key: Key) {
    key_with(ui, key, Modifiers::default());
}
fn with_role(ui: &Ui, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(n) = pending.pop() {
        if semantics.get(n).is_some_and(|s| s.role == role) {
            out.push(n);
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    out
}
/// The text under `node`.
fn text_of(ui: &Ui, node: NodeId) -> String {
    let scene = ui.scene.borrow();
    let mut out = String::new();
    let mut pending = vec![node];
    while let Some(n) = pending.pop() {
        if let NodeKind::Text { text, .. } = scene.kind(n) {
            out.push_str(text);
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    out
}
fn at(row: usize, column: &str) -> Option<GridCell> {
    Some(GridCell::new(row, column))
}
fn focus_grid(f: &mut Fixture) -> NodeId {
    let grid = with_role(&f.ui, Role::Grid)[0];
    f.ui.input.focus(&f.ui.scene, Some(grid));
    f.ui.prepare_frame();
    grid
}
fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

#[test]
fn the_keyboard_moves_among_cells_and_thousands_stay_virtual() {
    let mut f = fixture(5000);
    let grid = focus_grid(&mut f);
    assert_eq!(f.current.get(), at(0, "name"), "focused: the first cell");
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(f.current.get(), at(0, "age"));
    key(&mut f.ui, Key::End);
    assert_eq!(f.current.get(), at(0, "city"));
    key(&mut f.ui, Key::ArrowDown);
    key(&mut f.ui, Key::ArrowDown);
    key(&mut f.ui, Key::Home);
    assert_eq!(f.current.get(), at(2, "name"));
    key(&mut f.ui, Key::PageDown);
    let paged = f.current.get().unwrap().row;
    assert!((5..12).contains(&paged), "a page down: row {paged}");
    key_with(&mut f.ui, Key::End, command());
    assert_eq!(f.current.get(), at(4999, "city"));
    f.ui.prepare_frame();
    // Scrolled to it, building only what shows.
    let cells = with_role(&f.ui, Role::Cell);
    assert!(cells.len() < 3 * 30, "{} cells built", cells.len());
    assert!(
        with_role(&f.ui, Role::Row)
            .iter()
            .any(|r| text_of(&f.ui, *r).contains("Person 4999")),
        "the last row shows"
    );
    key_with(&mut f.ui, Key::Home, command());
    assert_eq!(f.current.get(), at(0, "name"));
    assert_eq!(f.ui.input.focused(), Some(grid), "focus stays on the grid");
}

#[test]
fn up_from_the_first_row_reaches_the_header_which_sorts_moves_and_resizes() {
    let mut f = fixture(20);
    focus_grid(&mut f);
    key(&mut f.ui, Key::ArrowUp);
    key(&mut f.ui, Key::ArrowRight);
    key(&mut f.ui, Key::Enter);
    assert_eq!(
        f.sort.get(),
        Some(GridSort {
            column: "age".into(),
            descending: false
        })
    );
    key(&mut f.ui, Key::Character(" ".into()));
    assert!(f.sort.get().unwrap().descending, "then descending");
    key(&mut f.ui, Key::Enter);
    assert_eq!(f.sort.get(), None, "then none");
    // ⌘⇧→ moves the column on; Alt+→ widens it.
    key_with(
        &mut f.ui,
        Key::ArrowRight,
        Modifiers {
            shift: true,
            ..command()
        },
    );
    let ids = |f: &Fixture| {
        f.columns
            .get()
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&f), ["name", "city", "age"]);
    key_with(
        &mut f.ui,
        Key::ArrowRight,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    assert_eq!(f.columns.get()[2].width, 96.);
    // The headers follow, in order.
    let headers: Vec<String> = with_role(&f.ui, Role::ColumnHeader)
        .iter()
        .map(|h| f.ui.semantics.borrow().get(*h).unwrap().label.clone())
        .collect();
    assert_eq!(headers, ["Name", "City", "Age"]);
    key(&mut f.ui, Key::ArrowDown);
    assert_eq!(f.current.get(), at(0, "age"), "Down: back into the rows");
}

#[test]
fn enter_edits_keeps_and_moves_down_and_escape_drops() {
    let mut f = fixture(20);
    let grid = focus_grid(&mut f);
    key(&mut f.ui, Key::Enter);
    let field = f.ui.input.focused().unwrap();
    assert_ne!(field, grid);
    assert_eq!(
        f.ui.semantics.borrow().get(field).unwrap().role,
        Role::TextInput
    );
    f.ui.dispatch(InputEvent::Text("!".into()));
    key(&mut f.ui, Key::Enter);
    assert_eq!(
        *f.edits.borrow(),
        [(0, "name".to_owned(), "Person 0000!".to_owned())]
    );
    assert_eq!(f.people.get()[0].0, "Person 0000!");
    assert_eq!(f.current.get(), at(1, "name"), "Enter moves down");
    assert_eq!(f.ui.input.focused(), Some(grid), "focus back on the grid");
    // Typing over a cell starts afresh; Escape drops it.
    key(&mut f.ui, Key::Character("Z".into()));
    f.ui.dispatch(InputEvent::Text("Z".into()));
    f.ui.prepare_frame();
    key(&mut f.ui, Key::Escape);
    assert_eq!(f.edits.borrow().len(), 1, "dropped");
    assert_eq!(f.people.get()[1].0, "Person 0001");
    key(&mut f.ui, Key::Character("Z".into()));
    f.ui.dispatch(InputEvent::Text("Z".into()));
    key(&mut f.ui, Key::Enter);
    assert_eq!(f.people.get()[1].0, "Z");
    // A column that doesn't edit: Enter activates the row.
    key(&mut f.ui, Key::ArrowRight);
    key(&mut f.ui, Key::Enter);
    assert_eq!(*f.activated.borrow(), [2]);
}

#[test]
fn assistive_technology_hears_the_grid_its_places_and_its_sort() {
    let mut f = fixture(300);
    let grid = focus_grid(&mut f);
    key(&mut f.ui, Key::ArrowDown);
    key(&mut f.ui, Key::ArrowRight);
    let semantics = f.ui.semantics.borrow();
    let node = semantics.get(grid).unwrap();
    let table = node.table.as_deref().unwrap();
    assert_eq!((table.rows, table.columns), (Some(301), Some(3)));
    let active = node.active_descendant.expect("a current cell");
    let cell = semantics.get(active).unwrap();
    assert_eq!(cell.role, Role::Cell);
    let place = cell.table.as_deref().unwrap();
    assert_eq!((place.row, place.column), (Some(2), Some(1)));
    drop(semantics);
    assert_eq!(text_of(&f.ui, active), "7");
    f.sort.set(Some(GridSort {
        column: "name".into(),
        descending: true,
    }));
    f.ui.prepare_frame();
    let name = with_role(&f.ui, Role::ColumnHeader)[0];
    let semantics = f.ui.semantics.borrow();
    let header = semantics.get(name).unwrap().table.as_deref().unwrap();
    assert_eq!(
        (header.row, header.column, header.sort),
        (Some(0), Some(0), Some(SortOrder::Descending))
    );
    let row = with_role(&f.ui, Role::Row)[1];
    assert_eq!(
        semantics.get(row).unwrap().table.as_deref().unwrap().row,
        Some(1)
    );
}

#[test]
fn right_to_left_the_arrows_mirror() {
    let mut f = fixture(5);
    let root = f.view.find("root").unwrap();
    f.ui.scene
        .borrow_mut()
        .set_direction(root, Some(Direction::Rtl));
    f.ui.prepare_frame();
    focus_grid(&mut f);
    key(&mut f.ui, Key::ArrowLeft);
    assert_eq!(f.current.get(), at(0, "age"), "Left goes on, right to left");
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(f.current.get(), at(0, "name"));
}

#[test]
fn the_pointer_chooses_sorts_moves_and_resizes() {
    let mut f = fixture(20);
    let button = PointerButton::Primary;
    // As a window delivers presses: with their modifiers, which counts
    // clicks.
    let press = |ui: &mut Ui, x: f32, y: f32| {
        ui.dispatch_with_modifiers(
            InputEvent::PointerDown { x, y, button },
            Modifiers::default(),
        );
        ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, Modifiers::default());
        ui.prepare_frame();
    };
    // A cell: chosen; twice: activated (its column doesn't edit).
    let city = with_role(&f.ui, Role::Cell)
        .into_iter()
        .filter(|c| text_of(&f.ui, *c) == "Lisbon")
        .nth(3)
        .unwrap();
    let b = f.ui.scene.borrow().bounds(city);
    press(&mut f.ui, b.x + 10., b.y + 10.);
    assert_eq!(f.current.get(), at(3, "city"));
    let grid = with_role(&f.ui, Role::Grid)[0];
    assert_eq!(f.ui.input.focused(), Some(grid), "a press focuses the grid");
    press(&mut f.ui, b.x + 10., b.y + 10.);
    assert_eq!(*f.activated.borrow(), [3]);
    // A header's click sorts.
    let headers = with_role(&f.ui, Role::ColumnHeader);
    let age = f.ui.scene.borrow().bounds(headers[1]);
    press(&mut f.ui, age.x + 20., age.y + 10.);
    assert_eq!(f.sort.get().map(|s| s.column), Some("age".into()));
    // Its edge's drag widens it; a double click there puts it back.
    let edge_x = age.x + age.width - 2.;
    let y = age.y + 10.;
    f.ui.dispatch(InputEvent::PointerDown {
        x: edge_x,
        y,
        button,
    });
    f.ui.dispatch(InputEvent::PointerMove { x: edge_x + 30., y });
    f.ui.dispatch(InputEvent::PointerUp {
        x: edge_x + 30.,
        y,
        button,
    });
    f.ui.prepare_frame();
    assert_eq!(f.columns.get()[1].width, 110.);
    assert_eq!(
        f.sort.get().map(|s| s.descending),
        Some(false),
        "a resize doesn't sort"
    );
    let edge_x = edge_x + 30.;
    press(&mut f.ui, edge_x, y);
    press(&mut f.ui, edge_x, y);
    assert_eq!(f.columns.get()[1].width, 80.);
    // Dragging the Name header past Age's middle moves it after.
    let name = f.ui.scene.borrow().bounds(headers[0]);
    let (x, y) = (name.x + 40., name.y + 10.);
    f.ui.dispatch(InputEvent::PointerDown { x, y, button });
    f.ui.dispatch(InputEvent::PointerMove { x: x + 20., y });
    f.ui.dispatch(InputEvent::PointerMove { x: x + 130., y });
    f.ui.dispatch(InputEvent::PointerUp {
        x: x + 130.,
        y,
        button,
    });
    f.ui.prepare_frame();
    let ids: Vec<String> = f.columns.get().iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids, ["age", "name", "city"]);
    assert_eq!(
        f.sort.get().map(|s| s.descending),
        Some(false),
        "a move doesn't sort"
    );
}

/// The clipboard: the text and HTML last put on it.
#[derive(Default)]
struct Clipboard(RefCell<Option<(String, String)>>);
impl zgui::platform::Platform for Clipboard {
    fn clipboard_text(&self) -> Option<String> {
        self.0.borrow().as_ref().map(|c| c.0.clone())
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        *self.0.borrow_mut() = Some((text, String::new()));
        true
    }
    fn set_clipboard_rich_text(&self, text: String, html: String) -> bool {
        *self.0.borrow_mut() = Some((text, html));
        true
    }
    fn open_files(
        &self,
        _: zgui::platform::FilePicker,
    ) -> zgui::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn save_file(
        &self,
        _: zgui::platform::FilePicker,
    ) -> zgui::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(None) })
    }
}

#[test]
fn rows_are_selected_by_click_shift_and_command_and_copied_as_a_spreadsheet_does() {
    let mut ui = Ui::new(500., 300.);
    let clipboard = Rc::new(Clipboard::default());
    let columns = ui.signal(vec![
        GridColumn::new("name", "Name", 160.),
        GridColumn::new("note", "Note", 160.),
    ]);
    let (sort, current) = (ui.signal(None), ui.signal(None));
    let selection = ui.signal(GridSelection::default());
    let source = GridSource::new(
        || 50,
        |r, c| match c {
            "name" => format!("Row {r}"),
            _ if r == 2 => "tab\there & <b>".into(),
            _ => "ok".into(),
        },
    )
    .selection(selection.clone());
    ui.mount(provide(
        zgui::platform::PlatformService(clipboard.clone()),
        column()
            .w(500.)
            .h(300.)
            .child(data_grid("Rows", columns, sort, current.clone(), source)),
    ));
    ui.prepare_frame();
    ui.prepare_frame();
    let rows = |s: &GridSelection| s.rows().collect::<Vec<_>>();
    let cell = |ui: &Ui, row: usize| {
        with_role(ui, Role::Cell)
            .into_iter()
            .find(|c| text_of(ui, *c) == format!("Row {row}"))
            .unwrap()
    };
    let click = |ui: &mut Ui, row: usize, modifiers: Modifiers| {
        let b = ui.scene.borrow().bounds(cell(ui, row));
        let (x, y, button) = (b.x + 10., b.y + 10., PointerButton::Primary);
        ui.dispatch_with_modifiers(InputEvent::PointerDown { x, y, button }, modifiers);
        ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, modifiers);
        ui.prepare_frame();
    };
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    click(&mut ui, 1, Modifiers::default());
    assert_eq!(rows(&selection.get()), [1]);
    click(&mut ui, 4, shift);
    assert_eq!(
        rows(&selection.get()),
        [1, 2, 3, 4],
        "Shift: from the anchor"
    );
    click(&mut ui, 3, command());
    assert_eq!(rows(&selection.get()), [1, 2, 4], "⌘: one taken away");
    click(&mut ui, 6, command());
    assert_eq!(rows(&selection.get()), [1, 2, 4, 6], "⌘: one added");
    // The rows say so; the grid that several can be.
    let selected = with_role(&ui, Role::Row)
        .into_iter()
        .filter(|r| ui.semantics.borrow().get(*r).unwrap().selected == Some(true))
        .count();
    assert_eq!(selected, 4);
    let grid = with_role(&ui, Role::Grid)[0];
    assert!(
        ui.semantics
            .borrow()
            .get(grid)
            .unwrap()
            .table
            .as_deref()
            .unwrap()
            .multiselectable
    );
    // ⌘C: the rows, tab-separated (a cell with a tab quoted) and as HTML.
    key_with(&mut ui, Key::Character("c".into()), command());
    let (text, html) = clipboard.0.borrow().clone().unwrap();
    assert_eq!(
        text,
        "Row 1\tok\nRow 2\t\"tab\there & <b>\"\nRow 4\tok\nRow 6\tok"
    );
    assert!(
        html.starts_with("<table><tr><td>Row 1</td><td>ok</td></tr>"),
        "{html}"
    );
    assert!(
        html.contains("<td>tab\there &amp; &lt;b&gt;</td>"),
        "{html}"
    );
    // Shift and an arrow extend from the anchor (row 6); an arrow alone
    // selects the row it reaches; one row copies the current cell.
    key_with(&mut ui, Key::ArrowDown, shift);
    assert_eq!(rows(&selection.get()), [6, 7]);
    key(&mut ui, Key::ArrowDown);
    assert_eq!(rows(&selection.get()), [8]);
    assert_eq!(current.get(), at(8, "name"));
    key_with(&mut ui, Key::Character("c".into()), command());
    assert_eq!(clipboard.0.borrow().as_ref().unwrap().0, "Row 8");
    // ⌘A: all of them; Escape: the current row's alone again.
    key_with(&mut ui, Key::Character("a".into()), command());
    assert_eq!(selection.get().len(), 50);
    key(&mut ui, Key::Escape);
    assert_eq!(rows(&selection.get()), [8]);
}

#[test]
fn a_selection_toggles_and_joins_its_ranges() {
    let s = GridSelection::span(2, 4).toggled(5).toggled(9);
    assert_eq!(s.rows().collect::<Vec<_>>(), [2, 3, 4, 5, 9]);
    assert_eq!(s.anchor(), Some(9));
    let s = s.toggled(3);
    assert_eq!(s.rows().collect::<Vec<_>>(), [2, 4, 5, 9]);
    assert!(s.contains(4) && !s.contains(3) && !s.contains(6));
    assert_eq!(
        s.extended_to(7).rows().collect::<Vec<_>>(),
        [3, 4, 5, 6, 7],
        "from the anchor, 3"
    );
    assert_eq!(
        GridSelection::from_rows([7, 1, 2, 3, 7])
            .rows()
            .collect::<Vec<_>>(),
        [1, 2, 3, 7]
    );
}
