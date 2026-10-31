//! The terminal zgx draws: a [`Session`] behind a `zgx::terminal::Terminal`
//! controller, shown by [`terminal_view`].
//!
//! Each row of the screen is a canvas drawing what [`crate::paint`] worked
//! out for it: when output arrives, only the rows the screen model marks
//! damaged are worked out again, and only those that differ draw again.
//! Keys go to the program through a hidden editor at the cursor, so input
//! methods compose there and place their candidates at the cursor on every
//! platform; what they commit is sent, never kept.
use crate::{
    GridSize, Session, SessionEvent, SessionOptions, keys,
    paint::{self, CursorAt, Look, Metrics, RowPaint},
};
use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionRange, SelectionType},
    term::TermMode,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::Duration,
};
use zgx::{
    compose::{NodeHandle, TaskRunner, TaskToken, prelude::*},
    input::{EventContext, EventPhase, InputEvent, Key, Modifiers, PointerButton},
    platform::PlatformService,
    reactive::{Runtime, Signal},
    semantics::Live,
    terminal::{Terminal, TerminalBackend, TerminalCommand, TerminalEvent, WeakTerminal},
};

/// Around the grid, in points (as the Ghostty backend's padding).
const PAD_X: f32 = 10.;
const PAD_Y: f32 = 6.;
/// The font size when the session's options give none.
const FONT_SIZE: f32 = 13.;
/// How long after output the foreground process is checked again: a
/// command starting or ending shows in its output, and a burst of output
/// costs one check.
const PROCESS_CHECK: Duration = Duration::from_millis(150);

/// A session drawn by zgx, behind a terminal controller (its backend).
pub struct EngineTerminal(Rc<Engine>);

struct Engine {
    term: WeakTerminal,
    runtime: Runtime,
    session: RefCell<Option<Session>>,
    /// The task reading the session as it wakes; cancelled with the engine.
    task: RefCell<Option<TaskToken>>,
    /// The window's that last showed it, and where the session's wakes go:
    /// a view mounting in another window takes them over, so they don't end
    /// with the first window's tasks.
    runner: RefCell<TaskRunner>,
    wake: Arc<Mutex<Option<zgx::task::LatestSender<()>>>>,
    platform: RefCell<Option<Rc<PlatformService>>>,
    /// Each viewport row's paint, as its canvas draws it.
    rows: RefCell<Vec<Signal<Rc<RowPaint>>>>,
    /// How many rows show, and the metrics they are laid out by (rows are
    /// made again when these change).
    layout: Signal<(u16, u32)>,
    metrics: Cell<Metrics>,
    generation: Cell<u32>,
    font: zgx::text_layout::FontStyle,
    look: Look,
    /// Where the hidden editor sits: the cursor's cell, in points.
    caret: Signal<(f32, f32)>,
    /// An input method's composition, drawn at the cursor.
    preedit: Signal<String>,
    focused: Cell<bool>,
    /// The hidden editor, or the terminal's node (a press focuses it first),
    /// has keyboard focus; the two settle into `focused` once a dispatch is
    /// over, so focus passing between them reports nothing.
    input_has: Cell<bool>,
    root_has: Cell<bool>,
    settling: Cell<bool>,
    /// Views showing it now.
    views: Cell<u32>,
    /// The latest view's nodes (a view mounting in another split before
    /// the last unmounts takes them over), and which view that is.
    root: RefCell<Option<NodeHandle>>,
    input: RefCell<Option<NodeHandle>>,
    latest_view: Cell<u64>,
    /// Focus asked for before a view showed it.
    pending_focus: Cell<bool>,
    /// Every row must be worked out again (a view mounted, the font changed).
    full: Cell<bool>,
    last_cursor: Cell<Option<usize>>,
    last_selection: RefCell<Option<SelectionRange>>,
    last_offset: Cell<usize>,
    last_focus: Cell<bool>,
    /// The content box last laid out, in points.
    size: Cell<(f32, f32)>,
    /// A press being dragged: selecting, or reported to the program.
    pressed: Cell<Option<Press>>,
    /// Wheel travel short of a whole line.
    scroll_rest: Cell<f32>,
    pid: Cell<u32>,
    process_check: Cell<bool>,
    process_task: RefCell<Option<TaskToken>>,
    /// What the app's resolver last saw in front ([`crate::front`]), the
    /// app's count of changes last seen, and its polling.
    front: RefCell<Option<crate::Foreground>>,
    resolved: Cell<bool>,
    seen_changes: Cell<u64>,
    polling: Cell<bool>,
    poll_task: RefCell<Option<TaskToken>>,
    exited: Cell<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Press {
    Selecting,
    Reporting(PointerButton),
}

impl EngineTerminal {
    fn new(term: &Terminal, runtime: Runtime, runner: TaskRunner) -> std::io::Result<Self> {
        let options = term.options();
        let font = paint::font();
        let engine = Rc::new(Engine {
            term: term.downgrade(),
            runtime: runtime.clone(),
            session: RefCell::new(None),
            task: RefCell::new(None),
            runner: RefCell::new(runner),
            wake: Arc::new(Mutex::new(None)),
            platform: RefCell::new(None),
            rows: RefCell::new(Vec::new()),
            layout: runtime.signal((24, 0)),
            metrics: Cell::new(Metrics {
                font_size: options.font_size.unwrap_or(FONT_SIZE),
                cell_width: 0.,
                cell_height: 0.,
            }),
            generation: Cell::new(0),
            font,
            look: Look::default(),
            caret: runtime.signal((PAD_X, PAD_Y)),
            preedit: runtime.signal(String::new()),
            focused: Cell::new(false),
            input_has: Cell::new(false),
            root_has: Cell::new(false),
            settling: Cell::new(false),
            views: Cell::new(0),
            root: RefCell::new(None),
            input: RefCell::new(None),
            latest_view: Cell::new(0),
            pending_focus: Cell::new(false),
            full: Cell::new(true),
            last_cursor: Cell::new(None),
            last_selection: RefCell::new(None),
            last_offset: Cell::new(0),
            last_focus: Cell::new(false),
            size: Cell::new((0., 0.)),
            pressed: Cell::new(None),
            scroll_rest: Cell::new(0.),
            pid: Cell::new(0),
            process_check: Cell::new(false),
            process_task: RefCell::new(None),
            front: RefCell::new(None),
            resolved: Cell::new(false),
            seen_changes: Cell::new(crate::front::changes()),
            polling: Cell::new(false),
            poll_task: RefCell::new(None),
            exited: Cell::new(false),
        });
        crate::front::register(Arc::downgrade(&engine.wake));
        let wake = engine.wake.clone();
        let session = Session::spawn(
            &SessionOptions {
                command: options.command.clone(),
                working_directory: options.working_directory.clone().or_else(home_directory),
                env: options.env.clone(),
                ..SessionOptions::default()
            },
            GridSize::new(80, 24),
            move || {
                if let Some(wake) = &*wake.lock().unwrap() {
                    wake.send(());
                }
            },
        )?;
        session.set_palette(engine.look.palette.clone());
        *engine.session.borrow_mut() = Some(session);
        engine.listen();
        engine.check_process();
        Ok(Self(engine))
    }
}

fn home_directory() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    home.map(Into::into)
}

impl Engine {
    fn with_session<R>(&self, f: impl FnOnce(&Session) -> R) -> Option<R> {
        self.session.borrow().as_ref().map(f)
    }
    fn controller(&self) -> Option<Terminal> {
        self.term.upgrade()
    }

    /// Read the session as it wakes, on the runner's thread of tasks (again
    /// after a view mounts with another runner).
    fn listen(self: &Rc<Self>) {
        let (wake, mut woken) = zgx::task::latest::<()>();
        *self.wake.lock().unwrap() = Some(wake);
        let weak = Rc::downgrade(self);
        let task = self.runner.borrow().spawn(Box::pin(async move {
            while woken.recv().await.is_some() {
                let Some(engine) = weak.upgrade() else { return };
                engine.refresh();
            }
        }));
        // The task before ends with its token.
        *self.task.borrow_mut() = Some(task);
        // A wake the last task didn't read: the session won't wake again
        // until it is read.
        self.refresh();
    }

    /// Read what the session reported and draw what changed.
    fn refresh(self: &Rc<Self>) {
        let Some(events) = self.with_session(Session::take_events) else {
            return;
        };
        let term = self.controller();
        // The app said what is in front may have changed.
        let changes = crate::front::changes();
        let mut recheck = changes != self.seen_changes.replace(changes);
        for event in events {
            let Some(term) = &term else { break };
            match event {
                SessionEvent::Title(title) => {
                    term.update_state(|state| state.title = title.clone());
                    term.emit(TerminalEvent::TitleChanged { title });
                }
                SessionEvent::Pwd(pwd) => {
                    term.update_state(|state| state.pwd = pwd.clone());
                    term.emit(TerminalEvent::PwdChanged { pwd });
                    recheck = true;
                }
                SessionEvent::Bell => {
                    if !term.focused() {
                        term.update_state(|state| state.bell = true);
                    }
                    term.emit(TerminalEvent::Bell);
                }
                SessionEvent::Progress(progress) => term.update_state(|state| {
                    state.progress = progress.fraction();
                    state.progress_error = matches!(progress, crate::Progress::Error(_));
                }),
                SessionEvent::CommandFinished {
                    exit_code,
                    duration_ms,
                } => {
                    term.emit(TerminalEvent::CommandFinished {
                        exit_code,
                        duration_ms,
                    });
                    recheck = true;
                }
                SessionEvent::Notification { title, body } => {
                    term.emit(TerminalEvent::Notification { title, body })
                }
                SessionEvent::ClipboardStore(text) => {
                    if let Some(platform) = &*self.platform.borrow() {
                        platform.0.set_clipboard_text(text);
                    }
                }
                // Reading the clipboard needs the user's word, which this
                // can't ask yet: the program hears nothing back.
                SessionEvent::ClipboardLoad(_) => {}
                SessionEvent::Exited(code) => {
                    self.exited.set(true);
                    term.update_state(|state| {
                        state.exited = Some(code);
                        state.pid = 0;
                        state.process.clear();
                    });
                    term.emit(TerminalEvent::Exited { code });
                    term.emit(TerminalEvent::CloseRequested {
                        process_alive: false,
                    });
                }
            }
        }
        if self.views.get() > 0 {
            self.paint();
        }
        if recheck {
            self.check_process();
        } else {
            self.schedule_process_check();
        }
    }

    /// Work out the rows that changed and hand the canvases those that
    /// differ.
    fn paint(&self) {
        let session = self.session.borrow();
        let Some(session) = session.as_ref() else {
            return;
        };
        let metrics = self.metrics.get();
        if metrics.cell_width <= 0. {
            return;
        }
        let focused = self.focused.get();
        let mut changed = Vec::new();
        let caret;
        {
            let mut term = session.term();
            let rows = term.screen_lines();
            let offset = term.grid().display_offset();
            let mode = *term.mode();
            let point = term.grid().cursor.point;
            let cursor_row = (offset == 0 && mode.contains(TermMode::SHOW_CURSOR))
                .then(|| (point.line.0 + offset as i32) as usize)
                .filter(|row| *row < rows);
            let shape = term.cursor_style().shape;
            caret = (
                PAD_X + point.column.0 as f32 * metrics.cell_width,
                PAD_Y + (point.line.0 + offset as i32).max(0) as f32 * metrics.cell_height,
            );
            let selection = term.selection.as_ref().and_then(|s| s.to_range(&term));
            let mut all = self.full.replace(false)
                || offset != self.last_offset.replace(offset)
                || focused != self.last_focus.replace(focused)
                || selection != *self.last_selection.borrow();
            *self.last_selection.borrow_mut() = selection;
            let mut damaged = vec![false; rows];
            match term.damage() {
                alacritty_terminal::term::TermDamage::Full => all = true,
                alacritty_terminal::term::TermDamage::Partial(lines) => {
                    for line in lines {
                        if let Some(row) = damaged.get_mut(line.line) {
                            *row = true;
                        }
                    }
                }
            }
            let was = self.last_cursor.replace(cursor_row);
            for row in [was, cursor_row].into_iter().flatten() {
                if let Some(row) = damaged.get_mut(row) {
                    *row = true;
                }
            }
            for (row, damaged) in damaged.into_iter().enumerate() {
                if !(all || damaged) {
                    continue;
                }
                let cursor = (cursor_row == Some(row)).then_some(CursorAt {
                    column: point.column.0,
                    shape,
                    focused,
                });
                changed.push((
                    row,
                    paint::row(&term, row, &self.look, metrics, cursor, selection.as_ref()),
                ));
            }
            term.reset_damage();
        }
        let rows = self.rows.borrow();
        self.runtime.batch(|| {
            for (row, paint) in changed {
                if let Some(signal) = rows.get(row)
                    && signal.with_untracked(|shown| **shown != paint)
                {
                    signal.set(Rc::new(paint));
                }
            }
            self.caret.set(caret);
        });
    }

    /// The row signal at `row`, made on first use.
    fn row(&self, row: usize) -> Signal<Rc<RowPaint>> {
        let mut rows = self.rows.borrow_mut();
        while rows.len() <= row {
            rows.push(self.runtime.signal(Rc::new(RowPaint::default())));
        }
        rows[row].clone()
    }

    /// Fit the grid to the view's `size` (its content box, in points).
    fn fit(&self, size: (f32, f32)) {
        self.size.set(size);
        let metrics = self.metrics.get();
        if metrics.cell_width <= 0. || size.0 <= 0. || size.1 <= 0. {
            return;
        }
        let columns = ((size.0 - 2. * PAD_X) / metrics.cell_width).floor().max(1.) as u16;
        let rows = ((size.1 - 2. * PAD_Y) / metrics.cell_height)
            .floor()
            .max(1.) as u16;
        let scale = 2.;
        self.with_session(|session| {
            session.resize(GridSize {
                columns,
                rows,
                cell_width: (metrics.cell_width * scale) as u16,
                cell_height: (metrics.cell_height * scale) as u16,
            })
        });
        if let Some(term) = self.controller() {
            let (was_columns, was_rows) = term
                .state()
                .with_untracked(|state| (state.columns, state.rows));
            if (was_columns, was_rows) != (columns, rows) {
                term.update_state(|state| {
                    state.columns = columns;
                    state.rows = rows;
                });
            }
        }
        self.full.set(true);
        let layout = (rows, self.generation.get());
        if self.layout.with_untracked(|shown| *shown != layout) {
            self.layout.set(layout);
        }
        self.paint();
    }

    /// Measure the cell for the font size (on mounting, and as it changes).
    fn measure(&self, scene: &zgx::scene::Scene) {
        let mut metrics = self.metrics.get();
        let (width, height) =
            scene.measure_text_with_font("0000000000", metrics.font_size, None, &self.font);
        metrics.cell_width = width / 10.;
        metrics.cell_height = height.ceil();
        if metrics != self.metrics.get() {
            self.metrics.set(metrics);
            self.generation.set(self.generation.get().wrapping_add(1));
        }
    }

    fn set_font_size(&self, size: f32) {
        let size = size.clamp(6., 72.);
        let mut metrics = self.metrics.get();
        if (metrics.font_size - size).abs() < 0.01 {
            return;
        }
        metrics.font_size = size;
        self.metrics.set(metrics);
        if let Some(scene) = self.root.borrow().as_ref().and_then(NodeHandle::scene) {
            self.measure(&scene.borrow());
        }
        self.fit(self.size.get());
    }

    fn write(&self, bytes: Vec<u8>) {
        self.with_session(|session| {
            // Typing goes to the bottom, where the program is.
            let mut term = session.term();
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            drop(term);
            session.write(bytes);
        });
        self.paint();
    }

    fn mode(&self) -> TermMode {
        self.with_session(|session| *session.term().mode())
            .unwrap_or(TermMode::empty())
    }

    fn paste(&self, text: &str) {
        if !text.is_empty() {
            let bytes = keys::paste(text, self.mode());
            self.write(bytes);
        }
    }

    fn copy(&self) -> bool {
        let text = self
            .with_session(|session| session.term().selection_to_string())
            .flatten();
        match (text, &*self.platform.borrow()) {
            (Some(text), Some(platform)) if !text.is_empty() => platform.0.set_clipboard_text(text),
            _ => false,
        }
    }

    fn paste_clipboard(&self) {
        let text = self
            .platform
            .borrow()
            .as_ref()
            .and_then(|platform| platform.0.clipboard_text());
        if let Some(text) = text {
            self.paste(&text);
        }
    }

    fn focus_changed(&self, focused: bool) {
        if self.focused.replace(focused) == focused {
            return;
        }
        if self.mode().contains(TermMode::FOCUS_IN_OUT) {
            self.with_session(|session| {
                session.write(if focused {
                    b"\x1b[I".as_slice()
                } else {
                    b"\x1b[O"
                })
            });
        }
        if let Some(term) = self.controller() {
            term.update_state(|state| {
                state.focused = focused;
                if focused {
                    state.bell = false;
                }
            });
        }
        self.paint();
    }

    /// Focus came to (`has`) or left the editor or the terminal's node.
    fn focus_event(self: &Rc<Self>, root: bool, has: bool) {
        if root {
            self.root_has.set(has);
        } else {
            self.input_has.set(has);
        }
        if self.settling.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        zgx::native_view::post(move || {
            if let Some(engine) = weak.upgrade() {
                engine.settling.set(false);
                engine.focus_changed(engine.input_has.get() || engine.root_has.get());
            }
        });
    }

    /// A keyboard event at the editor, or at the terminal's node while it
    /// holds focus on the way to the editor.
    fn keyboard(self: &Rc<Self>, root: bool, cx: &mut EventContext) {
        match cx.event.clone() {
            InputEvent::KeyDown { key, modifiers, .. } => self.key(&key, modifiers, cx),
            InputEvent::Text(text) | InputEvent::ImeCommit(text) => {
                if self.preedit.with_untracked(|shown| !shown.is_empty()) {
                    self.preedit.set(String::new());
                }
                self.write(text.into_bytes());
                cx.prevent_default();
            }
            InputEvent::ImePreedit { text, .. } => {
                if self.preedit.with_untracked(|shown| *shown != text) {
                    self.preedit.set(text);
                }
            }
            InputEvent::Focus => {
                self.focus_event(root, true);
                if root {
                    self.focus(true);
                }
            }
            InputEvent::Blur => self.focus_event(root, false),
            _ => {}
        }
    }

    /// The cell under (`x`, `y`) in the view, and which half of it.
    fn cell_at(&self, x: f32, y: f32) -> Option<(Point, Side, usize)> {
        let metrics = self.metrics.get();
        if metrics.cell_width <= 0. {
            return None;
        }
        self.with_session(|session| {
            let term = session.term();
            let column = ((x - PAD_X) / metrics.cell_width).max(0.);
            let row =
                (((y - PAD_Y) / metrics.cell_height).max(0.) as usize).min(term.screen_lines() - 1);
            let side = if column.fract() < 0.5 {
                Side::Left
            } else {
                Side::Right
            };
            let column = (column as usize).min(term.columns() - 1);
            let line = Line(row as i32 - term.grid().display_offset() as i32);
            (Point::new(line, Column(column)), side, row)
        })
    }

    /// A pointer event over the grid at view point (`x`, `y`).
    fn pointer(
        self: &Rc<Self>,
        event: &InputEvent,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        clicks: u8,
        cx: &mut EventContext,
    ) {
        let Some((point, side, row)) = self.cell_at(x, y) else {
            return;
        };
        let mode = self.mode();
        // Shift selects even where the program reads the mouse.
        let reported = mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift;
        let report = |mouse: keys::Mouse| {
            if let Some(bytes) = keys::mouse(mouse, point.column.0, row, modifiers, mode) {
                self.with_session(|session| session.write(bytes));
            }
        };
        match *event {
            InputEvent::PointerDown { button, .. } => {
                // The node takes focus now (keys typed at once reach it),
                // the editor after the press.
                if !self.focused.get() {
                    cx.focus();
                }
                self.focus(true);
                if reported {
                    report(keys::Mouse::Press(button));
                    self.pressed.set(Some(Press::Reporting(button)));
                    cx.capture_pointer();
                    cx.prevent_default();
                    return;
                }
                if button != PointerButton::Primary {
                    return;
                }
                if modifiers.primary_shortcut() && self.open_link(point) {
                    cx.prevent_default();
                    return;
                }
                let kind = match clicks {
                    2 => SelectionType::Semantic,
                    n if n >= 3 => SelectionType::Lines,
                    _ => SelectionType::Simple,
                };
                self.with_session(|session| {
                    let mut term = session.term();
                    match (&mut term.selection, modifiers.shift) {
                        (Some(selection), true) => selection.update(point, side),
                        _ => term.selection = Some(Selection::new(kind, point, side)),
                    }
                });
                self.pressed.set(Some(Press::Selecting));
                cx.capture_pointer();
                cx.prevent_default();
                self.paint();
            }
            InputEvent::PointerMove { .. } => match self.pressed.get() {
                Some(Press::Selecting) => {
                    self.with_session(|session| {
                        let mut term = session.term();
                        if let Some(selection) = &mut term.selection {
                            selection.update(point, side);
                        }
                    });
                    self.paint();
                }
                Some(Press::Reporting(button)) => report(keys::Mouse::Move(Some(button))),
                None if reported => report(keys::Mouse::Move(None)),
                None => {}
            },
            InputEvent::PointerUp { button, .. } => match self.pressed.take() {
                Some(Press::Reporting(_)) => report(keys::Mouse::Release(button)),
                Some(Press::Selecting) => {
                    // A click without a drag leaves nothing selected.
                    self.with_session(|session| {
                        let mut term = session.term();
                        if term.selection.as_ref().is_some_and(|s| s.is_empty()) {
                            term.selection = None;
                        }
                    });
                    self.paint();
                }
                None => {}
            },
            _ => {}
        }
    }

    /// Open the hyperlink (OSC 8) at `point`, if there is one.
    fn open_link(&self, point: Point) -> bool {
        let uri = self
            .with_session(|session| {
                let term = session.term();
                term.grid()[point]
                    .hyperlink()
                    .map(|link| link.uri().to_owned())
            })
            .flatten();
        match (uri, &*self.platform.borrow()) {
            (Some(uri), Some(platform)) => platform.0.open_url(&uri),
            _ => false,
        }
    }

    fn scroll(&self, x: f32, y: f32, delta_y: f32, modifiers: Modifiers) {
        let metrics = self.metrics.get();
        if metrics.cell_height <= 0. {
            return;
        }
        let travel = self.scroll_rest.get() - delta_y / metrics.cell_height;
        let lines = travel.trunc() as i32;
        self.scroll_rest.set(travel.fract());
        if lines == 0 {
            return;
        }
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift {
            if let Some((point, _, row)) = self.cell_at(x, y) {
                let mut bytes = Vec::new();
                for _ in 0..lines.unsigned_abs().min(20) {
                    if let Some(report) = keys::mouse(
                        keys::Mouse::Wheel(lines > 0),
                        point.column.0,
                        row,
                        modifiers,
                        mode,
                    ) {
                        bytes.extend(report);
                    }
                }
                self.with_session(|session| session.write(bytes));
            }
            return;
        }
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !modifiers.shift {
            // Full-screen programs scroll by the arrows.
            let arrow = if lines > 0 {
                Key::ArrowUp
            } else {
                Key::ArrowDown
            };
            if let Some(once) = keys::encode(&arrow, Modifiers::default(), mode) {
                let bytes = once.repeat(lines.unsigned_abs() as usize);
                self.with_session(|session| session.write(bytes));
            }
            return;
        }
        self.with_session(|session| session.term().scroll_display(Scroll::Delta(lines)));
        self.paint();
    }

    fn focus(&self, focus: bool) {
        let input = self.input.borrow().clone();
        match input {
            // At the next safe point: from inside an event's dispatch, what
            // the dispatch does after (a press's focus) would undo it.
            Some(input) if focus => zgx::native_view::post(move || {
                input.focus();
            }),
            Some(_) => {
                if self.focused.get()
                    && let Some(root) = self.root.borrow().as_ref()
                {
                    // Focus goes to the terminal's node, which takes no keys.
                    root.focus();
                }
            }
            None => self.pending_focus.set(focus),
        }
    }

    /// A key, at the hidden editor: sent to the program, or left to the app
    /// (`⌘` chords on macOS, shortcuts it binds).
    fn key(&self, key: &Key, modifiers: Modifiers, cx: &mut EventContext) {
        // An input method composing takes the keys.
        if self.preedit.with_untracked(|text| !text.is_empty()) {
            return;
        }
        let character = match key {
            Key::Character(c) => Some(c.to_lowercase()),
            _ => None,
        };
        let copy_paste_chord = if cfg!(target_os = "macos") {
            modifiers.meta && !modifiers.control && !modifiers.alt
        } else {
            modifiers.control && modifiers.shift && !modifiers.alt && !modifiers.meta
        };
        if copy_paste_chord {
            match character.as_deref() {
                Some("c") => {
                    if self.copy() {
                        cx.prevent_default();
                        cx.stop_propagation();
                    }
                    return;
                }
                Some("v") => {
                    self.paste_clipboard();
                    cx.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }
        if let Some(bytes) = keys::encode(key, modifiers, self.mode()) {
            self.write(bytes);
            cx.prevent_default();
            cx.stop_propagation();
        }
    }

    /// Run a Ghostty binding action's name (those apps use).
    fn binding(&self, action: &str) {
        let (name, argument) = action.split_once(':').unwrap_or((action, ""));
        let page = self
            .with_session(|session| session.term().screen_lines() as i32)
            .unwrap_or(1);
        let scroll = |by: Scroll| {
            self.with_session(|session| session.term().scroll_display(by));
            self.paint();
        };
        match name {
            "copy_to_clipboard" => {
                self.copy();
            }
            "paste_from_clipboard" | "paste_from_selection" => self.paste_clipboard(),
            "select_all" => {
                self.with_session(|session| {
                    let mut term = session.term();
                    let top = term.topmost_line();
                    let bottom = term.bottommost_line();
                    let last = term.last_column();
                    let mut selection = Selection::new(
                        SelectionType::Simple,
                        Point::new(top, Column(0)),
                        Side::Left,
                    );
                    selection.update(Point::new(bottom, last), Side::Right);
                    term.selection = Some(selection);
                });
                self.paint();
            }
            "clear_screen" => {
                self.with_session(|session| {
                    let mut term = session.term();
                    term.grid_mut().clear_history();
                    term.selection = None;
                    drop(term);
                    // The program draws its screen (a shell's prompt) again.
                    session.write(b"\x0c".as_slice());
                });
                self.full.set(true);
                self.paint();
            }
            "scroll_to_top" => scroll(Scroll::Top),
            "scroll_to_bottom" => scroll(Scroll::Bottom),
            "scroll_page_up" => scroll(Scroll::PageUp),
            "scroll_page_down" => scroll(Scroll::PageDown),
            "scroll_page_fractional" => {
                let fraction: f32 = argument.parse().unwrap_or(0.);
                scroll(Scroll::Delta(-(fraction * page as f32).round() as i32));
            }
            "scroll_page_lines" => scroll(Scroll::Delta(-argument.parse::<i32>().unwrap_or(0))),
            "increase_font_size" => {
                let by: f32 = argument.parse().unwrap_or(1.);
                self.set_font_size(self.metrics.get().font_size + by);
            }
            "decrease_font_size" => {
                let by: f32 = argument.parse().unwrap_or(1.);
                self.set_font_size(self.metrics.get().font_size - by);
            }
            "reset_font_size" => {
                let size = self
                    .controller()
                    .and_then(|term| term.options().font_size)
                    .unwrap_or(FONT_SIZE);
                self.set_font_size(size);
            }
            "text" => self.write(unescape(argument)),
            "csi" => self.write([b"\x1b[".as_slice(), argument.as_bytes()].concat()),
            "esc" => self.write([b"\x1b".as_slice(), argument.as_bytes()].concat()),
            _ => {}
        }
    }

    /// Ask again what is in front, after the output settles.
    fn schedule_process_check(self: &Rc<Self>) {
        if self.process_check.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        let task = self.runner.borrow().spawn(Box::pin(async move {
            zgx::timer::sleep(PROCESS_CHECK).await;
            if let Some(engine) = weak.upgrade() {
                engine.process_check.set(false);
                engine.check_process();
            }
        }));
        *self.process_task.borrow_mut() = Some(task);
    }

    /// What runs in front: the controller's process name, pid and status,
    /// as the app's resolver sees it when it does.
    fn check_process(self: &Rc<Self>) {
        let Some(pid) = self.with_session(Session::foreground_pid) else {
            return;
        };
        if self.exited.get() {
            return;
        }
        // Seen through (an attach client to a session server): what is
        // behind it changes while the client stays in front, so ask each
        // time.
        let resolved = if pid == 0 {
            None
        } else {
            crate::front::resolve(pid)
        };
        self.resolved.set(resolved.is_some());
        if resolved.is_some() && !crate::front::pushed() {
            self.poll();
        }
        let (name, pid, status) = match resolved {
            Some(front) => {
                if self.front.borrow().as_ref() == Some(&front) {
                    return;
                }
                *self.front.borrow_mut() = Some(front.clone());
                self.pid.set(pid);
                (front.name, front.pid, front.status)
            }
            None => {
                let was_resolved = self.front.borrow_mut().take().is_some();
                if pid == self.pid.replace(pid) && !was_resolved {
                    return;
                }
                let name = if pid == 0 {
                    String::new()
                } else {
                    crate::process_name(pid)
                };
                (name, pid, None)
            }
        };
        if let Some(term) = self.controller() {
            let changed = term
                .state()
                .with_untracked(|state| state.process != name || state.pid != pid);
            term.update_state(|state| {
                state.process = name.clone();
                state.pid = pid;
                state.status = status;
            });
            if changed {
                term.emit(TerminalEvent::ProcessChanged { process: name, pid });
            }
        }
    }

    /// Ask the resolver twice a second while it resolves this terminal and
    /// the app doesn't say when what it sees changes.
    fn poll(self: &Rc<Self>) {
        if self.polling.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        let task = self.runner.borrow().spawn(Box::pin(async move {
            loop {
                zgx::timer::sleep(Duration::from_millis(500)).await;
                let Some(engine) = weak.upgrade() else { return };
                if !engine.resolved.get() || crate::front::pushed() || engine.exited.get() {
                    engine.polling.set(false);
                    return;
                }
                engine.check_process();
            }
        }));
        *self.poll_task.borrow_mut() = Some(task);
    }
}

/// A binding's `text:` argument, with Zig-style escapes (`\n`, `\x1b`).
fn unescape(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let (byte, used) = match bytes[i + 1] {
                b'n' => (b'\n', 2),
                b'r' => (b'\r', 2),
                b't' => (b'\t', 2),
                b'\\' => (b'\\', 2),
                b'x' if i + 4 <= bytes.len() => {
                    match std::str::from_utf8(&bytes[i + 2..i + 4])
                        .ok()
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    {
                        Some(byte) => (byte, 4),
                        None => (b'\\', 1),
                    }
                }
                _ => (b'\\', 1),
            };
            out.push(byte);
            i += used;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

impl TerminalBackend for EngineTerminal {
    fn run(&self, command: TerminalCommand) {
        let engine = &self.0;
        match command {
            TerminalCommand::Focus(focus) => engine.focus(focus),
            TerminalCommand::Input(text) => engine.write(text.into_bytes()),
            TerminalCommand::Paste(text) => engine.paste(&text),
            TerminalCommand::Binding(action) => engine.binding(&action),
            TerminalCommand::Close => {
                engine.task.borrow_mut().take();
                engine.process_task.borrow_mut().take();
                engine.poll_task.borrow_mut().take();
                // Dropping the session hangs up its program.
                engine.session.borrow_mut().take();
            }
        }
    }
    fn needs_confirm_close(&self) -> bool {
        self.0
            .with_session(|session| {
                if session.exited().is_some() {
                    return false;
                }
                let pid = session.foreground_pid();
                match (pid != 0).then(|| crate::front::resolve(pid)).flatten() {
                    Some(front) => !front.idle,
                    None => !session.is_idle(),
                }
            })
            .unwrap_or(false)
    }
    fn snapshot(&self, max_width: u32) -> Option<std::sync::Arc<zgx::image::ImageData>> {
        self.0.root.borrow().as_ref()?.snapshot(max_width)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Drop for EngineTerminal {
    fn drop(&mut self) {
        self.0.task.borrow_mut().take();
        self.0.process_task.borrow_mut().take();
        self.0.poll_task.borrow_mut().take();
        self.0.session.borrow_mut().take();
    }
}

/// `term` drawn by zgx (its session started on first showing): lay it out
/// like any node (give it a size or let it grow, round and clip it). Its
/// default background is transparent: the node's own shows through.
pub fn terminal_view(term: &Terminal) -> View {
    let term = term.clone();
    component(move |cx| {
        let engine = match term.backend() {
            Some(backend) => {
                let engine = backend
                    .as_any()
                    .downcast_ref::<EngineTerminal>()
                    .map(|engine| engine.0.clone());
                if let (Some(engine), Some(runner)) = (&engine, cx.try_service::<TaskRunner>()) {
                    *engine.runner.borrow_mut() = (*runner).clone();
                    engine.listen();
                }
                engine
            }
            None if term.is_closed() => None,
            None => {
                let started = cx
                    .try_service::<TaskRunner>()
                    .map(|runner| EngineTerminal::new(&term, cx.runtime(), (*runner).clone()));
                match started {
                    Some(Ok(engine)) => {
                        let shared = engine.0.clone();
                        term.attach(Rc::new(engine));
                        Some(shared)
                    }
                    Some(Err(error)) => {
                        return message(format!("Terminal · couldn't start: {error}"));
                    }
                    None => None,
                }
            }
        };
        let Some(engine) = engine else {
            return message("Terminal".into());
        };
        if let Some(platform) = cx.try_service::<PlatformService>() {
            *engine.platform.borrow_mut() = Some(platform);
        }
        engine.views.set(engine.views.get() + 1);
        engine.full.set(true);
        let id = engine.latest_view.get() + 1;
        engine.latest_view.set(id);
        cx.retain(Shown(Rc::downgrade(&engine), id));
        view(engine)
    })
}

/// Keeps a view's count while it is mounted.
struct Shown(Weak<Engine>, u64);
impl Drop for Shown {
    fn drop(&mut self) {
        if let Some(engine) = self.0.upgrade() {
            engine.views.set(engine.views.get().saturating_sub(1));
            if engine.latest_view.get() == self.1 {
                engine.input.borrow_mut().take();
                engine.root.borrow_mut().take();
            }
        }
    }
}

fn message(text: String) -> View {
    div().items_center().justify_center().child(
        text_signal(move || text.clone())
            .text_size(12.)
            .text_color(rgba(0xa0a0_a0ff)),
    )
}

fn view(engine: Rc<Engine>) -> View {
    let rows = {
        let engine = engine.clone();
        keyed(
            {
                let layout = engine.layout.clone();
                move || {
                    let (rows, generation) = layout.get();
                    (0..rows).map(|row| (row, generation)).collect::<Vec<_>>()
                }
            },
            move |(row, _), _cx| row_view(&engine, usize::from(row)),
        )
    };
    let input = {
        let value = engine.runtime.signal(String::new());
        let caret = engine.caret.clone();
        let height = engine.metrics.get().cell_height.max(1.);
        let keys = engine.clone();
        let mounted = engine.clone();
        text_input("Terminal input", value)
            .accessible_hidden()
            .absolute()
            .w(1.)
            .h(height)
            .opacity(0.)
            .reactive_style(move || {
                let (x, y) = caret.get();
                Styles::new().left(x).top(y)
            })
            .on_event(move |cx| {
                if cx.phase != EventPhase::Target {
                    return;
                }
                keys.keyboard(false, cx);
            })
            .on_mount(move |handle| {
                *mounted.input.borrow_mut() = Some(handle.clone());
                if mounted.pending_focus.take() {
                    handle.focus();
                }
            })
    };
    let preedit = {
        let text = engine.preedit.clone();
        let caret = engine.caret.clone();
        let metrics = engine.metrics.get();
        let font_color = paint::color(engine.look.palette.foreground);
        let shown = engine.preedit.clone();
        text_signal(move || text.get())
            .absolute()
            .text_size(metrics.font_size)
            .font_family(zgx::text_layout::FontFamily::Monospace)
            .text_color(font_color)
            .bg(rgba(0x2020_20ff))
            .reactive_style(move || {
                let (x, y) = caret.get();
                let hidden = shown.with(String::is_empty);
                Styles::new()
                    .left(x)
                    .top(y)
                    .opacity(if hidden { 0. } else { 1. })
            })
    };
    let pointer = engine.clone();
    let mounted = engine.clone();
    div()
        .focusable(true)
        .overflow_hidden()
        .cursor(Cursor::Text)
        .live(Live::Polite)
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            if cx.phase == EventPhase::Target
                && matches!(
                    cx.event,
                    InputEvent::KeyDown { .. }
                        | InputEvent::Text(_)
                        | InputEvent::ImeCommit(_)
                        | InputEvent::ImePreedit { .. }
                        | InputEvent::Focus
                        | InputEvent::Blur
                )
            {
                pointer.keyboard(true, cx);
                return;
            }
            let modifiers = cx.pointer_modifiers();
            let clicks = cx.click_count();
            let event = cx.event.clone();
            let origin = pointer
                .root
                .borrow()
                .as_ref()
                .and_then(NodeHandle::observe_bounds)
                .map(|bounds| bounds.with_untracked(|b| (b.x, b.y)))
                .unwrap_or((0., 0.));
            match event {
                InputEvent::PointerDown { x, y, .. }
                | InputEvent::PointerMove { x, y }
                | InputEvent::PointerUp { x, y, .. } => {
                    pointer.pointer(&event, x - origin.0, y - origin.1, modifiers, clicks, cx);
                    if let (InputEvent::PointerMove { .. }, Some(term)) =
                        (&event, pointer.controller())
                    {
                        term.set_pointer(Some((x - origin.0, y - origin.1)));
                    }
                }
                InputEvent::PointerLeave => {
                    if let Some(term) = pointer.controller() {
                        term.set_pointer(None);
                    }
                }
                InputEvent::Scroll { x, y, delta_y, .. } => {
                    pointer.scroll(x - origin.0, y - origin.1, delta_y, modifiers);
                    cx.prevent_default();
                    cx.stop_propagation();
                }
                _ => {}
            }
        })
        .child(rows)
        .child(preedit)
        .child(input)
        .on_mount(move |handle| {
            let engine = mounted;
            if let Some(scene) = handle.scene() {
                engine.measure(&scene.borrow());
            }
            *engine.root.borrow_mut() = Some(handle.clone());
            if let Some(size) = handle.observe_size() {
                let fit = engine.clone();
                let effect = engine.runtime.effect(move || {
                    let size = size.get();
                    fit.fit(size);
                });
                handle.retain(effect);
            }
        })
}

fn row_view(engine: &Rc<Engine>, row: usize) -> View {
    let paint = engine.row(row);
    let metrics = engine.metrics.get();
    let font = engine.font.clone();
    let label = paint.clone();
    canvas(move |_| paint::draw(&paint.get(), metrics, &font))
        .absolute()
        .left(PAD_X)
        .top(PAD_Y + row as f32 * metrics.cell_height)
        .w_full()
        .h(metrics.cell_height)
        .accessible_label_when(move || label.with(|paint| paint.text.clone()))
}

/// Whether `term`'s backend is drawn by zgx (this crate's).
pub fn is_engine_drawn(term: &Terminal) -> bool {
    term.backend()
        .is_some_and(|backend| backend.as_any().is::<EngineTerminal>())
}
