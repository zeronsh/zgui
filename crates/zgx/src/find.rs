//! Find in page, as a browser's and AppKit's text finder: Command-F
//! (Control-F elsewhere) opens a bar at the window's top right, and what is
//! typed there is found in everything the window shows as text (text, rich
//! text, fields' values, and virtual lists' rows, mounted or not), each
//! match marked behind its text and the current one scrolled into view.
//! Enter and Command-G go to the next, with Shift the one before, wrapping;
//! Escape closes it, the current match selected where it can be. On macOS
//! Command-E finds the selection, and the app's Edit menu has a Find
//! submenu (`zgx-desktop`), unless the app binds those keys itself.
//!
//! Matching ignores case and accents (`find_text`). A virtual list's rows it
//! has not mounted are read by its row text (`View::row_text`, else what its
//! rows would show), a slice each frame until all are counted, so a list of
//! a million rows never stalls one; nothing runs once the count is done, or
//! while no find is open.
use crate::{
    compose::{View, ViewHandle},
    input::{EventPhase, InputEvent, Key},
    reactive::Signal,
    scene::{NodeId, NodeKind, Rect, TextMark},
    selection::RowText,
    style::{Styled, Styles},
    theme::tok,
    widgets::Ui,
};
use std::{
    ops::Range,
    rc::Rc,
    time::{Duration, Instant},
};

/// Where a match is.
#[derive(Clone, Debug, PartialEq)]
pub enum FoundAt {
    /// In a mounted text node (text or rich text).
    Text(NodeId),
    /// In a field's value (`node`, its editor).
    Editor(NodeId),
    /// In row `row` of the virtual list at `list`, mounted or not: its
    /// range is in the row's text.
    Row {
        /// The list's node.
        list: NodeId,
        /// The row, from 0.
        row: usize,
    },
}

/// A match: where it is, and its byte range there.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// Where it is.
    pub at: FoundAt,
    /// Its byte range in that text.
    pub range: Range<usize>,
}

/// What find shows: whether it is open, what it looks for, how many matches
/// there are (`done` once every row is counted) and which is current.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindStatus {
    /// The bar is open.
    pub open: bool,
    /// What is looked for.
    pub query: String,
    /// The matches found so far.
    pub count: usize,
    /// Every row of every list is counted.
    pub done: bool,
    /// The current match, from 0.
    pub current: Option<usize>,
}

/// A virtual list's row index from a mounted row's offset in its content.
pub(crate) type RowIndex = Rc<dyn Fn(f32) -> Option<usize>>;

/// How long a frame spends counting rows.
const SLICE: Duration = Duration::from_millis(4);

enum Segment {
    Found(Vec<Found>),
    List {
        list: NodeId,
        text: RowText,
        next: usize,
        found: Vec<Found>,
        done: bool,
    },
}
impl Segment {
    fn found(&self) -> &[Found] {
        match self {
            Segment::Found(found) | Segment::List { found, .. } => found,
        }
    }
}

/// An open find.
pub(crate) struct Session {
    query: String,
    folded: String,
    segments: Vec<Segment>,
    current: Option<usize>,
    /// Text nodes and editors showing marks.
    marked: Vec<NodeId>,
    editors_marked: Vec<NodeId>,
    /// The scene's content and layout when marks were last laid out.
    marked_at: Option<(u64, u64)>,
    status: Signal<FindStatus>,
    /// What is typed in the bar.
    typed: Signal<String>,
    bar: Option<ViewHandle>,
    /// What had focus before the bar.
    previous_focus: Option<NodeId>,
    /// The bar asked for this, to do once its event is done.
    asked: Option<Ask>,
}
#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Search,
    Next,
    Previous,
    Close,
}
impl Session {
    fn count(&self) -> usize {
        self.segments.iter().map(|s| s.found().len()).sum()
    }
    fn done(&self) -> bool {
        self.segments
            .iter()
            .all(|s| !matches!(s, Segment::List { done: false, .. }))
    }
    fn get(&self, mut index: usize) -> Option<&Found> {
        for segment in &self.segments {
            let found = segment.found();
            if index < found.len() {
                return found.get(index);
            }
            index -= found.len();
        }
        None
    }
    fn iter(&self) -> impl Iterator<Item = &Found> {
        self.segments.iter().flat_map(|s| s.found().iter())
    }
}

impl Ui {
    /// Whether Command-F (Control-F elsewhere) opens find in this window:
    /// on unless the app turns it off, or binds the key itself.
    pub fn set_find_in_page(&self, enabled: bool) {
        self.storage.find_off.set(!enabled);
        if !enabled {
            let mut ui = self.shared();
            ui.close_find();
        }
    }
    /// Open find's bar (or focus it, its text selected, if open). False if
    /// find is off (`set_find_in_page`).
    pub fn open_find(&mut self) -> bool {
        if self.storage.find_off.get() {
            return false;
        }
        if self.storage.find.borrow().is_none() {
            let status = self.signal(FindStatus {
                open: true,
                ..Default::default()
            });
            let typed = self.signal(String::new());
            let previous_focus = self.input.focused();
            *self.storage.find.borrow_mut() = Some(Session {
                query: String::new(),
                folded: String::new(),
                segments: Vec::new(),
                current: None,
                marked: Vec::new(),
                editors_marked: Vec::new(),
                marked_at: None,
                status: status.clone(),
                typed: typed.clone(),
                bar: None,
                previous_focus,
                asked: None,
            });
            let bar = self.mount(bar(self, status, typed));
            if let Some(session) = self.storage.find.borrow_mut().as_mut() {
                session.bar = Some(bar);
            }
            self.prepare_frame();
        }
        self.focus_find_field();
        true
    }
    fn focus_find_field(&mut self) {
        let field = self
            .storage
            .find
            .borrow()
            .as_ref()
            .and_then(|s| s.bar.as_ref()?.find(FIELD));
        if let Some(field) = field {
            self.input.focus(&self.scene, Some(field));
            if let Some(editor) = self.editor(field) {
                editor.select_all();
            }
        }
    }
    /// Close find: its marks go, and the current match is selected where it
    /// is selectable (as a browser leaves it), else focus goes back where
    /// it was.
    pub fn close_find(&mut self) {
        let Some(mut session) = self.storage.find.borrow_mut().take() else {
            return;
        };
        self.clear_marks(&session);
        session.status.set(FindStatus::default());
        if let Some(bar) = session.bar.take() {
            bar.unmount();
        }
        let current = session.current.and_then(|i| session.get(i).cloned());
        let selected = current.is_some_and(|found| self.select_found(&found));
        if !selected
            && let Some(previous) = session
                .previous_focus
                .filter(|id| self.scene.borrow().contains(*id))
        {
            self.input.focus(&self.scene, Some(previous));
        }
    }
    /// Find `query` (as typed: case and accents ignored), its first match
    /// current and in view.
    pub fn find_text(&mut self, query: &str) {
        if self.storage.find.borrow().is_none() && !self.open_find() {
            return;
        }
        let folded = crate::find_text::fold_query(query);
        let segments = if folded.is_empty() {
            Vec::new()
        } else {
            self.collect(&folded)
        };
        let typed = self.storage.find.borrow_mut().as_mut().map(|session| {
            session.query = query.to_owned();
            session.folded = folded;
            session.segments = segments;
            session.current = None;
            session.marked_at = None;
            session.typed.clone()
        });
        // The bar shows what is looked for (its own search then finds it
        // done: `sync`).
        if let Some(typed) = typed
            && typed.with_untracked(|t| t != query)
        {
            typed.set(query.to_owned());
        }
        // The first slice of the lists' rows now, so a short list is counted
        // at once.
        self.count_rows(Instant::now() + SLICE);
        let any = self
            .storage
            .find
            .borrow()
            .as_ref()
            .is_some_and(|s| s.count() > 0);
        if any {
            self.go_to(0);
        } else {
            self.after_find_change();
        }
    }
    /// Find what is selected (selected text, else a field's selection), as
    /// AppKit's Use Selection for Find (⌘E): false with nothing selected.
    pub fn find_selection(&mut self) -> bool {
        let field = self
            .storage
            .find
            .borrow()
            .as_ref()
            .and_then(|s| s.bar.as_ref()?.find(FIELD));
        let selected = match self.focused_editor().filter(|e| Some(e.node) != field) {
            Some(editor) => editor.copy(),
            None if self.has_text_selection() => self.selected_text(),
            None => String::new(),
        };
        let selected = selected.lines().next().unwrap_or_default().to_owned();
        if selected.is_empty() {
            return false;
        }
        self.find_text(&selected);
        true
    }
    /// The next match current, wrapping, and in view.
    pub fn find_next(&mut self) {
        self.step(1);
    }
    /// The match before current, wrapping, and in view.
    pub fn find_previous(&mut self) {
        self.step(-1);
    }
    fn step(&mut self, by: isize) {
        let (count, current) = match self.storage.find.borrow().as_ref() {
            Some(s) => (s.count(), s.current),
            None => return,
        };
        if count == 0 {
            return;
        }
        let next = match current {
            Some(i) => (i as isize + by).rem_euclid(count as isize) as usize,
            None if by < 0 => count - 1,
            None => 0,
        };
        self.go_to(next);
    }
    /// Count every list's rows now, for at most `within` (devtools answer
    /// with the whole count): whether all are counted.
    pub fn finish_find_count(&mut self, within: Duration) -> bool {
        let until = Instant::now() + within;
        while next_step(self).is_some() && Instant::now() < until {
            advance(self, Instant::now());
        }
        next_step(self).is_none()
    }
    /// What find shows.
    pub fn find_status(&self) -> FindStatus {
        self.storage
            .find
            .borrow()
            .as_ref()
            .map_or_else(FindStatus::default, status_of)
    }
    /// The first `limit` matches, in document order.
    pub fn find_results(&self, limit: usize) -> Vec<Found> {
        self.storage
            .find
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |s| s.iter().take(limit).cloned().collect())
    }
    /// Match `index`'s text with up to `around` characters each side, for
    /// telling matches apart.
    pub fn find_context(&self, found: &Found, around: usize) -> String {
        let Some(text) = self.found_text(&found.at) else {
            return String::new();
        };
        let range = found.range.clone();
        if range.end > text.len() || !text.is_char_boundary(range.start) {
            return String::new();
        }
        let before: String = {
            let chars: Vec<char> = text[..range.start].chars().rev().take(around).collect();
            chars.into_iter().rev().collect()
        };
        let after: String = text[range.end..].chars().take(around).collect();
        format!("{before}[{}]{after}", &text[range])
    }
    /// Where match `found` is in the window (mounted text), if it shows.
    pub fn find_rect(&self, found: &Found) -> Option<Rect> {
        let scene = self.scene.borrow();
        let node = match found.at {
            FoundAt::Text(node) => node,
            FoundAt::Editor(node) => node,
            FoundAt::Row { list, row } => self.row_texts(&scene, list, row).first()?.0,
        };
        if !scene.contains(node) {
            return None;
        }
        let range = match found.at {
            FoundAt::Text(_) => found.range.clone(),
            FoundAt::Editor(_) => return scene.visible_bounds(node),
            FoundAt::Row { .. } => return scene.visible_bounds(node),
        };
        let layout = self.selection_layout(&scene, node)?;
        let bounds = scene.bounds(node);
        layout
            .selection(range)
            .into_iter()
            .map(|r| Rect::new(bounds.x + r.x, bounds.y + r.y, r.width, r.height))
            .reduce(|a, b| a.union(b))
    }
    fn found_text(&self, at: &FoundAt) -> Option<String> {
        match at {
            FoundAt::Text(node) => {
                let scene = self.scene.borrow();
                crate::selection::text_of(&scene, *node).map(|t| t.to_string())
            }
            FoundAt::Editor(node) => Some(self.editor(*node)?.editor.borrow().text().into_owned()),
            FoundAt::Row { list, row } => {
                let text = crate::selection::list_row_text(self, *list)?;
                text(*row)
            }
        }
    }

    /// Every match of `folded` in the window's text, in document order:
    /// lists' rows left to count (`count_rows`).
    fn collect(&self, folded: &str) -> Vec<Segment> {
        let scene = self.scene.borrow();
        let bar = self
            .storage
            .find
            .borrow()
            .as_ref()
            .and_then(|s| s.bar.as_ref().map(ViewHandle::node));
        let mut segments = Vec::new();
        let mut found = Vec::new();
        let mut pending = vec![scene.root()];
        while let Some(id) = pending.pop() {
            if Some(id) == bar || !scene.contains(id) || !scene.shown_in_layout(id) {
                continue;
            }
            if self.is_editor(id) {
                if let Some(editor) = self.editor(id)
                    && !editor.is_secure()
                {
                    let text = editor.editor.borrow().text().into_owned();
                    found.extend(crate::find_text::matches(&text, folded).into_iter().map(
                        |range| Found {
                            at: FoundAt::Editor(id),
                            range,
                        },
                    ));
                }
                continue;
            }
            if let Some(text) = crate::selection::list_row_text(self, id) {
                segments.push(Segment::Found(std::mem::take(&mut found)));
                segments.push(Segment::List {
                    list: id,
                    text,
                    next: 0,
                    found: Vec::new(),
                    done: false,
                });
                continue;
            }
            if let Some(text) = crate::selection::text_of(&scene, id) {
                found.extend(
                    crate::find_text::matches(&text, folded)
                        .into_iter()
                        .map(|range| Found {
                            at: FoundAt::Text(id),
                            range,
                        }),
                );
            }
            pending.extend(scene.children(id).iter().rev().copied());
        }
        segments.push(Segment::Found(found));
        segments.retain(|s| !matches!(s, Segment::Found(f) if f.is_empty()));
        segments
    }
    /// Count lists' rows until `until`: whether any were counted.
    fn count_rows(&self, until: Instant) -> bool {
        let mut session = self.storage.find.borrow_mut();
        let Some(session) = session.as_mut() else {
            return false;
        };
        let folded = session.folded.clone();
        let mut counted = false;
        for segment in &mut session.segments {
            let Segment::List {
                text,
                next,
                found,
                done,
                list,
            } = segment
            else {
                continue;
            };
            while !*done {
                // A few rows between looks at the clock.
                for _ in 0..64 {
                    match text(*next) {
                        Some(row) => {
                            found.extend(crate::find_text::matches(&row, &folded).into_iter().map(
                                |range| Found {
                                    at: FoundAt::Row {
                                        list: *list,
                                        row: *next,
                                    },
                                    range,
                                },
                            ));
                            *next += 1;
                        }
                        None => {
                            *done = true;
                            break;
                        }
                    }
                }
                counted = true;
                if Instant::now() >= until {
                    return counted;
                }
            }
        }
        counted
    }
    /// Make match `index` current: in view, marked, and the bar told.
    fn go_to(&mut self, index: usize) {
        let found = {
            let mut session = self.storage.find.borrow_mut();
            let Some(session) = session.as_mut() else {
                return;
            };
            session.current = Some(index);
            session.marked_at = None;
            session.get(index).cloned()
        };
        if let Some(found) = found {
            self.reveal(&found);
        }
        self.after_find_change();
    }
    fn after_find_change(&mut self) {
        self.prepare_frame();
        self.refresh_marks();
        if let Some(session) = self.storage.find.borrow().as_ref() {
            let status = status_of(session);
            if session.status.with_untracked(|s| *s != status) {
                session.status.set(status);
            }
        }
    }
    /// Scroll `found` into view.
    fn reveal(&mut self, found: &Found) {
        match found.at {
            FoundAt::Text(node) => {
                if let Some(rect) = self.find_rect(found) {
                    self.scroll_rect_into_view(node, rect);
                }
            }
            FoundAt::Editor(node) => {
                // Scrolled to, its caret and selection left as they are, as
                // a browser's find leaves a field's until it closes.
                if let Some(editor) = self.editor(node) {
                    editor.reveal_offset(found.range.start);
                }
                let rect = self.scene.borrow().visible_bounds(node);
                if let Some(rect) = rect {
                    self.scroll_rect_into_view(node, rect);
                }
            }
            FoundAt::Row { list, row } => {
                let reveal = self.list_reveals().borrow().get(&list).cloned();
                if let Some(reveal) = reveal {
                    reveal.update(|(serial, at, how)| {
                        *serial = serial.wrapping_add(1);
                        *at = row;
                        *how = crate::compose::RowAlign::Center;
                    });
                }
                self.prepare_frame();
                let rect = {
                    let scene = self.scene.borrow();
                    scene.visible_bounds(list)
                };
                if let Some(rect) = rect {
                    self.scroll_rect_into_view(list, rect);
                }
            }
        }
    }
    /// Scroll the scroll views around `node` (innermost first) so `rect`
    /// (window points) shows, with a little room.
    fn scroll_rect_into_view(&mut self, node: NodeId, rect: Rect) {
        const ROOM: f32 = 24.;
        let viewports: Vec<NodeId> = {
            let scene = self.scene.borrow();
            scene
                .ancestors(node)
                .filter(|id| scene.is_scroll_content(*id))
                .filter_map(|content| scene.parent(content))
                .collect()
        };
        let mut rect = rect;
        for viewport in viewports {
            let Some(visible) = self.scene.borrow().visible_bounds(viewport) else {
                continue;
            };
            let along = |start: f32, size: f32, from: f32, length: f32| {
                if start < from {
                    (start - from - ROOM).max(start + size - from - length)
                } else if start + size > from + length {
                    (start + size - from - length + ROOM).min(start - from)
                } else {
                    0.
                }
            };
            let dy = along(rect.y, rect.height, visible.y, visible.height);
            let dx = along(rect.x, rect.width, visible.x, visible.width);
            if dx == 0. && dy == 0. {
                continue;
            }
            self.input.dispatch_to(
                &self.scene,
                viewport,
                InputEvent::Scroll {
                    x: 0.,
                    y: 0.,
                    delta_x: dx,
                    delta_y: dy,
                },
            );
            self.prepare_frame();
            rect = Rect::new(rect.x - dx, rect.y - dy, rect.width, rect.height);
        }
    }
    /// The current match selected, where it can be: whether it was.
    fn select_found(&mut self, found: &Found) -> bool {
        match found.at {
            FoundAt::Text(node) => self.select_text(
                crate::selection::TextPoint {
                    node,
                    offset: found.range.start,
                },
                crate::selection::TextPoint {
                    node,
                    offset: found.range.end,
                },
            ),
            FoundAt::Editor(node) => {
                let Some(editor) = self.editor(node) else {
                    return false;
                };
                self.input.focus(&self.scene, Some(node));
                editor.set_selection(found.range.start, found.range.end);
                true
            }
            FoundAt::Row { .. } => false,
        }
    }
    /// Mounted text nodes of row `row` of `list`, with each one's text.
    fn row_texts(
        &self,
        scene: &crate::scene::Scene,
        list: NodeId,
        row: usize,
    ) -> Vec<(NodeId, std::sync::Arc<str>)> {
        let Some(index) = self.storage.list_rows.borrow().get(&list).cloned() else {
            return Vec::new();
        };
        let Some(content) = scene
            .children(list)
            .first()
            .and_then(|viewport| scene.children(*viewport).first().copied())
        else {
            return Vec::new();
        };
        let Some(mounted) = scene
            .children(content)
            .iter()
            .copied()
            .find(|r| index(scene.transform(*r).y) == Some(row))
        else {
            return Vec::new();
        };
        let mut texts = Vec::new();
        let mut pending = vec![mounted];
        while let Some(id) = pending.pop() {
            if let Some(text) = crate::selection::text_of(scene, id) {
                texts.push((id, text));
            }
            pending.extend(scene.children(id).iter().rev().copied());
        }
        texts
    }
    /// Mark every match in mounted text, the current one apart.
    fn refresh_marks(&mut self) {
        let theme = self.theme_scope();
        let (matched, current_color) = (
            theme.color_untracked(tok::FIND_MATCH),
            theme.color_untracked(tok::FIND_CURRENT),
        );
        let radius = theme.number_untracked(tok::MARK_RADIUS);
        let Some((folded, current, marked, editors_marked, bar)) =
            self.storage.find.borrow().as_ref().map(|s| {
                (
                    s.folded.clone(),
                    s.current.and_then(|i| s.get(i).cloned()),
                    s.marked.clone(),
                    s.editors_marked.clone(),
                    s.bar.as_ref().map(ViewHandle::node),
                )
            })
        else {
            return;
        };
        // The current match, as mounted text: a row's, its place among its
        // row's matches found in the text nodes mounted for it.
        let current_text: Option<(NodeId, Range<usize>)> = match &current {
            Some(Found {
                at: FoundAt::Text(node),
                range,
            }) => Some((*node, range.clone())),
            Some(Found {
                at: FoundAt::Row { list, row },
                range,
            }) => {
                let scene = self.scene.borrow();
                let nth = crate::selection::list_row_text(self, *list)
                    .and_then(|text| text(*row))
                    .map_or(0, |text| {
                        crate::find_text::matches(&text, &folded)
                            .iter()
                            .take_while(|r| r.start < range.start)
                            .count()
                    });
                self.row_texts(&scene, *list, *row)
                    .into_iter()
                    .flat_map(|(node, text)| {
                        crate::find_text::matches(&text, &folded)
                            .into_iter()
                            .map(move |r| (node, r))
                    })
                    .nth(nth)
            }
            _ => None,
        };
        let mut now_marked = Vec::new();
        let mut now_editors = Vec::new();
        if !folded.is_empty() {
            let leaves: Vec<NodeId> = {
                let scene = self.scene.borrow();
                let mut leaves = Vec::new();
                let mut pending = vec![scene.root()];
                while let Some(id) = pending.pop() {
                    if Some(id) == bar || !scene.shown_in_layout(id) {
                        continue;
                    }
                    if self.is_editor(id) {
                        now_editors.push(id);
                        continue;
                    }
                    if matches!(
                        scene.kind(id),
                        NodeKind::Text { .. } | NodeKind::RichText { .. }
                    ) {
                        leaves.push(id);
                    }
                    pending.extend(scene.children(id).iter().rev().copied());
                }
                leaves
            };
            for node in leaves {
                let marks = {
                    let scene = self.scene.borrow();
                    let Some(text) = crate::selection::text_of(&scene, node) else {
                        continue;
                    };
                    let ranges = crate::find_text::matches(&text, &folded);
                    if ranges.is_empty() {
                        continue;
                    }
                    let Some(layout) = self.selection_layout(&scene, node) else {
                        continue;
                    };
                    let bounds = scene.layout_bounds(node);
                    let frame = Rect::new(0., 0., bounds.width, bounds.height);
                    // Each match one shape, over the lines it runs on.
                    let mut marks = Vec::new();
                    let mut ends = Vec::new();
                    for range in ranges {
                        let color = if current_text.as_ref() == Some(&(node, range.clone())) {
                            current_color
                        } else {
                            matched
                        };
                        marks.extend(
                            (layout.selection(range).into_iter())
                                .filter_map(|r| r.intersection(frame))
                                .map(|rect| TextMark { rect, color }),
                        );
                        ends.push(marks.len());
                    }
                    (marks, ends)
                };
                let (marks, ends) = marks;
                self.scene
                    .borrow_mut()
                    .set_text_mark_shapes(node, marks, ends, radius);
                now_marked.push(node);
            }
            for node in &now_editors {
                let Some(editor) = self.editor(*node) else {
                    continue;
                };
                if editor.is_secure() {
                    continue;
                }
                let text = editor.editor.borrow().text().into_owned();
                let ranges: Vec<_> = crate::find_text::matches(&text, &folded)
                    .into_iter()
                    .map(|range| {
                        let is_current = matches!(&current, Some(Found { at: FoundAt::Editor(n), range: r }) if n == node && *r == range);
                        (range, if is_current { current_color } else { matched })
                    })
                    .collect();
                editor.set_decorations(ranges);
            }
        }
        for node in marked {
            if !now_marked.contains(&node) {
                self.scene.borrow_mut().set_text_marks(node, Vec::new());
            }
        }
        for node in editors_marked {
            if !now_editors.contains(&node)
                && let Some(editor) = self.editor(node)
            {
                editor.set_decorations(Vec::new());
            }
        }
        let at = {
            let scene = self.scene.borrow();
            (scene.content_revision(), scene.layout_revision())
        };
        if let Some(session) = self.storage.find.borrow_mut().as_mut() {
            session.marked = now_marked;
            session.editors_marked = now_editors;
            session.marked_at = Some(at);
        }
    }
    fn clear_marks(&mut self, session: &Session) {
        for node in &session.marked {
            self.scene.borrow_mut().set_text_marks(*node, Vec::new());
        }
        for node in &session.editors_marked {
            if let Some(editor) = self.editor(*node) {
                editor.set_decorations(Vec::new());
            }
        }
    }
}

fn status_of(session: &Session) -> FindStatus {
    FindStatus {
        open: true,
        query: session.query.clone(),
        count: session.count(),
        done: session.done(),
        current: session.current,
    }
}

/// After a frame is laid out: what the bar asked for, and the marks again
/// where text mounted, moved or changed (a list's rows scrolled in).
pub(crate) fn sync(ui: &Ui) {
    let Some((asked, marked_at, bar_gone)) = ui.storage.find.borrow_mut().as_mut().map(|s| {
        (
            s.asked.take(),
            s.marked_at,
            s.bar
                .as_ref()
                .is_some_and(|bar| !ui.scene.borrow().contains(bar.node())),
        )
    }) else {
        return;
    };
    let mut ui = ui.shared();
    // The app rendered anew over the bar: find is closed.
    if bar_gone {
        if let Some(session) = ui.storage.find.borrow_mut().as_mut() {
            session.bar = None;
        }
        ui.close_find();
        return;
    }
    match asked {
        Some(Ask::Search) => {
            let typed = ui
                .storage
                .find
                .borrow()
                .as_ref()
                .map(|s| s.typed.with_untracked(String::clone))
                .filter(|typed| ui.find_status().query != *typed);
            if let Some(typed) = typed {
                ui.find_text(&typed);
            }
            return;
        }
        Some(Ask::Next) => return ui.find_next(),
        Some(Ask::Previous) => return ui.find_previous(),
        Some(Ask::Close) => return ui.close_find(),
        None => {}
    }
    let now = {
        let scene = ui.scene.borrow();
        (scene.content_revision(), scene.layout_revision())
    };
    if marked_at != Some(now) {
        ui.refresh_marks();
    }
}

/// While rows are left to count, now: the next slice is due.
pub(crate) fn next_step(ui: &Ui) -> Option<Instant> {
    let session = ui.storage.find.borrow();
    let session = session.as_ref()?;
    (!session.done()).then(Instant::now)
}

/// Count the next slice of rows: whether any were.
pub(crate) fn advance(ui: &Ui, now: Instant) -> bool {
    if next_step(ui).is_none() {
        return false;
    }
    let counted = ui.count_rows(now + SLICE);
    if counted {
        let mut ui = ui.shared();
        let first = {
            let session = ui.storage.find.borrow();
            session
                .as_ref()
                .is_some_and(|s| s.current.is_none() && s.count() > 0)
        };
        if first {
            ui.go_to(0);
        } else if let Some(session) = ui.storage.find.borrow().as_ref() {
            let status = status_of(session);
            if session.status.with_untracked(|s| *s != status) {
                session.status.set(status);
            }
        }
    }
    counted
}

/// The window's own find keys, for keys nothing else took: Command-F
/// (Control-F elsewhere) opens find, and while it is open Command-G
/// (Control-G, F3) goes to the next match, with Shift the one before.
pub(crate) fn after_dispatch(ui: &Ui, event: &InputEvent, prevented: bool) -> bool {
    let InputEvent::KeyDown { key, modifiers, .. } = event else {
        return false;
    };
    if prevented || ui.storage.find_off.get() {
        return false;
    }
    let letter = |l: &str| matches!(key, Key::Character(k) if k.eq_ignore_ascii_case(l));
    let shortcut = modifiers.primary_shortcut() && !modifiers.alt;
    let mut ui = ui.shared();
    if shortcut && !modifiers.shift && letter("f") {
        return ui.open_find();
    }
    // Use Selection for Find, as AppKit's.
    if cfg!(target_os = "macos") && shortcut && !modifiers.shift && letter("e") {
        return ui.find_selection();
    }
    if ui.storage.find.borrow().is_none() {
        return false;
    }
    let next = (shortcut && letter("g")) || (*key == Key::Function(3) && !modifiers.alt);
    if next {
        if modifiers.shift {
            ui.find_previous();
        } else {
            ui.find_next();
        }
        return true;
    }
    false
}

/// The find field's id in the bar.
const FIELD: &str = "zgx-find-field";

/// Find's bar: a field, how many matches, the one before and the next, and
/// close.
fn bar(ui: &Ui, status: Signal<FindStatus>, typed: Signal<String>) -> View {
    use crate::compose::{button, row, text, text_input, text_signal};
    let ask = |ui: &Ui, what: Ask| {
        let weak = ui.downgrade();
        move || {
            if let Some(ui) = weak.upgrade()
                && let Some(session) = ui.storage.find.borrow_mut().as_mut()
            {
                session.asked = Some(what);
            }
        }
    };
    let (search, next, previous, close) = (
        ask(ui, Ask::Search),
        ask(ui, Ask::Next),
        ask(ui, Ask::Previous),
        ask(ui, Ask::Close),
    );
    // What is typed is searched once its event is done.
    let typing = {
        let (typed, search) = (typed.clone(), search.clone());
        let last = std::cell::RefCell::new(String::new());
        move || {
            let now = typed.get();
            if *last.borrow() != now {
                *last.borrow_mut() = now;
                search();
            }
        }
    };
    let keys = {
        let (next, previous, close) = (next.clone(), previous.clone(), close.clone());
        move |cx: &mut crate::input::EventContext| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
                return;
            };
            match key {
                Key::Enter if modifiers.shift => previous(),
                Key::Enter => next(),
                Key::Escape => close(),
                _ => return,
            }
            cx.prevent_default();
            cx.stop_propagation();
        }
    };
    let shown = move || {
        let s = status.get();
        match (s.query.is_empty(), s.count, s.current) {
            (true, ..) => String::new(),
            (false, 0, _) if s.done => "No results".to_owned(),
            (false, count, current) => {
                let at = current.map_or(0, |c| c + 1);
                let more = if s.done { "" } else { "+" };
                format!("{at} of {count}{more}")
            }
        }
    };
    let small = |label: &'static str, name: &'static str, act: Rc<dyn Fn()>| {
        button()
            .accessible_label(name)
            .px(6.)
            .py(2.)
            .on_click(move || act())
            .child(text(label))
    };
    row()
        .absolute()
        .top(8.)
        .right(8.)
        .gap(6.)
        .p(6.)
        .items_center()
        .rounded(8.)
        .bg(tok::POPOVER_BG)
        .border(1.)
        .border_color(tok::BORDER_SUBTLE)
        .accessible_label("Find")
        .reactive_style(move || {
            typing();
            Styles::new()
        })
        .on_event(keys)
        .child(text_input("Find", typed).id(FIELD).w(180.))
        .child(
            text_signal(shown)
                .text_size(12.)
                .text_color(tok::TEXT_SECONDARY)
                .live(crate::semantics::Live::Polite),
        )
        .child(small("‹", "Previous", Rc::new(previous)))
        .child(small("›", "Next", Rc::new(next)))
        .child(small("✕", "Close", Rc::new(close)))
}
