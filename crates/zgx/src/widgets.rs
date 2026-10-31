//! Retained widgets with reactive values, keyboard input and accessibility semantics.
use crate::{
    input::{EventPhase, InputBinding, InputDispatcher, InputEvent, Key, PointerButton},
    reactive::{Effect, Runtime, Signal},
    scene::{Color, Insets, Layout, NodeId, NodeKind, Rect, Scene, Style, Transform},
    semantics::{Role, SemanticNode, Semantics},
    text::{Text, TextValue},
    text_edit::{ClauseStyle, TextEditor},
    text_layout::{FontStyle, TextAffinity, TextLayout, TextPosition},
    theme::{ColorToken, ThemeScope, Themes, tok},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

pub use crate::editor_lines::{Highlighter, LineColors, LineSpans, SpanHighlighter, SpanStyle};

#[derive(Default)]
struct Owned {
    /// Most nodes own one effect: inline, it costs no allocation.
    effects: smallvec::SmallVec<[Effect; 1]>,
    bindings: Vec<InputBinding>,
    resources: Vec<Box<dyn Any>>,
}
/// An editor's box size, style, shaper revision and content insets.
type EditorGeometry = (Rect, Style, u64, Insets);
/// What an editor is bound to: a string, or a [`TextValue`] (which holds a
/// long document once, shared with the editor, and which an edit updates
/// without copying it). Either converts from its signal.
#[derive(Clone)]
pub enum EditorValue {
    /// A string, copied in and out.
    String(Signal<String>),
    /// A text value, a long one shared.
    Text(Signal<TextValue>),
}
impl From<Signal<String>> for EditorValue {
    fn from(value: Signal<String>) -> Self {
        EditorValue::String(value)
    }
}
impl From<Signal<TextValue>> for EditorValue {
    fn from(value: Signal<TextValue>) -> Self {
        EditorValue::Text(value)
    }
}
impl EditorValue {
    /// Its text (a copy).
    pub fn get(&self) -> String {
        match self {
            EditorValue::String(value) => value.get(),
            EditorValue::Text(value) => value.with(|v| v.to_cow().into_owned()),
        }
    }
    /// Set its text.
    pub fn set(&self, text: String) {
        match self {
            EditorValue::String(value) => {
                value.set(text);
            }
            EditorValue::Text(value) => {
                value.set(text.into());
            }
        }
    }
    /// Read it, tracked (an effect reading it runs again when it changes).
    pub(crate) fn track(&self) {
        match self {
            EditorValue::String(value) => value.with(|_| ()),
            EditorValue::Text(value) => value.with(|_| ()),
        }
    }
    /// Its text, untracked, as a value (a long one shared).
    fn value_untracked(&self) -> TextValue {
        match self {
            EditorValue::String(value) => TextValue::Flat(value.with_untracked(String::clone)),
            EditorValue::Text(value) => value.with_untracked(TextValue::clone),
        }
    }
    /// What `editor` holds now, after an edit (its borrow released before
    /// the value's subscribers run: the editor's own binding is one).
    fn write(&self, editor: &RefCell<TextEditor>) {
        match self {
            EditorValue::String(value) => {
                let next = editor.borrow().text().into_owned();
                value.set(next);
            }
            EditorValue::Text(value) => {
                let next = editor.borrow().value();
                value.set(next);
            }
        }
    }
    /// Its text (tracked) into `editor`, in canonical form: whether the
    /// editor's changed, and the canonical text if it isn't the value's.
    fn apply(&self, editor: &RefCell<TextEditor>, multiline: bool) -> (bool, Option<String>) {
        let flat = |current: &str| {
            let canonical = normalize_text(current, multiline);
            let changed = editor.borrow_mut().set_text_ref(canonical.as_ref());
            match canonical {
                std::borrow::Cow::Owned(canonical) => (changed, Some(canonical)),
                std::borrow::Cow::Borrowed(_) => (changed, None),
            }
        };
        match self {
            EditorValue::String(value) => value.with(|current| flat(current)),
            EditorValue::Text(value) => value.with(|current| match current {
                TextValue::Flat(current) => flat(current),
                TextValue::Long(text) if !is_canonical(text, multiline) => flat(&text.to_cow()),
                TextValue::Long(_) => (editor.borrow_mut().set_value(current), None),
            }),
        }
    }
}
/// Whether a long text needs no `normalize_text`.
fn is_canonical(text: &Text, multiline: bool) -> bool {
    text.chunks()
        .all(|chunk| !chunk.contains('\r') && (multiline || !chunk.contains('\n')))
}
/// A mounted text editor (a `text_input` or `text_area`): its editing
/// model and the value it is bound to, for hosts, tests and commands.
#[derive(Clone)]
pub struct EditorHandle {
    multiline: bool,
    read_only: Rc<Cell<bool>>,
    /// Retained caret geometry for native IME candidate-window placement.
    pub caret: NodeId,
    /// The editor's root node.
    pub node: NodeId,
    /// The editing model: text, selection, undo and composition.
    pub editor: Rc<RefCell<TextEditor>>,
    refresh: Rc<dyn Fn()>,
    typography: Rc<RefCell<(Color, f32, FontStyle)>>,
    wrap: Rc<Cell<bool>>,
    line_numbers: Rc<Cell<bool>>,
    preferred_x: Rc<Cell<Option<f32>>>,
    text: NodeId,
    scene: Rc<RefCell<Scene>>,
    /// What the last refresh laid the text out for: the box's size, its
    /// style, the shaper revision and its content insets (which a border
    /// in its kind changes too).
    geometry: Rc<RefCell<Option<EditorGeometry>>>,
    position: Rc<Cell<TextPosition>>,
    hint: Rc<EditorHint>,
}
/// What an editor shows while it is empty, whether focus selects all, and
/// the value it is bound to (here, once, for its handle and handlers).
struct EditorHint {
    value: EditorValue,
    /// The placeholder and its colour (`None`: the theme's `muted`).
    placeholder: RefCell<Option<(std::sync::Arc<str>, Option<Color>)>>,
    /// The caret's colour and width, if not the text's and the app's
    /// (`EditorHandle::set_caret_style`).
    caret: Cell<(Option<Color>, Option<f32>)>,
    select_on_focus: Cell<bool>,
    /// A wheel over it stays with it at its edges (`View::overscroll_contain`).
    contain: Cell<bool>,
    /// A password field: its text shows masked and never leaves it.
    secure: Cell<bool>,
    /// What kind of text it takes (`View::input_mode`).
    input_mode: Cell<crate::text_edit::InputMode>,
    /// Misspelled words (`View::spellcheck`), as byte ranges: marked with a
    /// dotted line. Usually none.
    misspelled: RefCell<Vec<std::ops::Range<usize>>>,
    /// The clause an input method is converting (its composition's cursor
    /// range), in the composition layer's points, and that layer: where its
    /// candidates show.
    clause: Cell<Option<Rect>>,
    composition_layer: Cell<Option<NodeId>>,
    /// A selection set by the program shows the next time it is drawn,
    /// focused or not (a search's match, chosen from its field).
    reveal: Cell<bool>,
    /// A place to scroll into view the next time it is drawn, its caret and
    /// selection left as they are (find in page's match in a field), and
    /// where in the view (`scroll_to_line`).
    reveal_at: Cell<Option<(usize, crate::compose::RowAlign)>>,
    /// The byte at a point (`EditorHandle::offset_at`).
    point: std::cell::OnceCell<Rc<dyn Fn(f32, f32) -> usize>>,
    /// What a code editor adds (`CodeHint`), made when first used: an
    /// editor without it carries a pointer's room.
    code: std::cell::OnceCell<Box<CodeHint>>,
    /// The theme colours its default placeholder, gutter or marks have
    /// shown in (`THEMED_*`), and what redraws it when one changes, made
    /// when the first shows.
    themed: Cell<u8>,
    theme_watch: Cell<Option<Box<(u8, Effect)>>>,
}
/// Stops an editor's theme watch (`EditorHint::watch_theme`) with its node.
struct ThemeUnwatch(Weak<EditorHint>);
impl Drop for ThemeUnwatch {
    fn drop(&mut self) {
        if let Some(hint) = self.0.upgrade() {
            drop(hint.theme_watch.take());
        }
    }
}
/// Theme colours an editor draws in only for what it may not have: a
/// default placeholder (`text.tertiary`), line numbers (and
/// `text.secondary`), a composition's underline and spelling marks.
const THEMED_TERTIARY: u8 = 1;
const THEMED_SECONDARY: u8 = 2;
const THEMED_ACCENT: u8 = 4;
const THEMED_DANGER: u8 = 8;
/// What a code editor adds to an editor's hint: its long text's lines,
/// marks behind its selection, its current line, Tab's indent, and who is
/// told where its caret is.
/// What an underline's node shows: its width in quarter points, its colour
/// and how it is drawn.
type UnderlineDrawn = (u32, Color, crate::rich_text::LineStyle);
/// A range of a text area's text underlined (`View::underlines`): a spelling
/// mistake's or a diagnostic's wave.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorUnderline {
    /// Its UTF-8 byte range.
    pub range: std::ops::Range<usize>,
    /// Its colour.
    pub color: Color,
    /// How it is drawn.
    pub line: crate::rich_text::LineStyle,
}
/// A row's underline, `width` points long, as what a canvas draws: shapes
/// and lines, drawn without a raster.
fn underline_canvas(
    width: f32,
    color: Color,
    line: crate::rich_text::LineStyle,
) -> crate::canvas::Canvas {
    use crate::{
        canvas::{Canvas, Path, Stroke},
        rich_text::LineStyle,
    };
    let mut canvas = Canvas::new();
    let rect = |canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32| {
        canvas.fill(Path::rectangle(Rect::new(x, y, width, height)), color);
    };
    match line {
        LineStyle::Double => {
            rect(&mut canvas, 0., 0., width, 1.);
            rect(&mut canvas, 0., 2., width, 1.);
        }
        LineStyle::Dotted => {
            for k in 0..(width / 2.).ceil() as u32 {
                if let Ok(dot) = Path::ellipse(Rect::new(k as f32 * 2., 0.5, 1.2, 1.2)) {
                    canvas.fill(dot, color);
                }
            }
        }
        LineStyle::Dashed => {
            for k in 0..(width / 5.).ceil() as u32 {
                let x = k as f32 * 5.;
                rect(&mut canvas, x, 1., 3f32.min(width - x), 1.);
            }
        }
        LineStyle::Wavy => {
            let mut wave = Path::builder();
            wave.move_to(0., 1.5);
            for k in 1..=(width / 1.5).ceil() as u32 {
                let x = (k as f32 * 1.5).min(width);
                wave.line_to(x, 1.5 + (x * std::f32::consts::TAU / 6.).sin());
            }
            if let Ok(wave) = wave.build() {
                canvas.stroke(
                    wave,
                    color,
                    Stroke {
                        width: 1.,
                        ..Default::default()
                    },
                );
            }
        }
        _ => rect(&mut canvas, 0., 1., width, 1.),
    }
    canvas
}
/// An editor's decorations (`shaped`, each shape's marks ending at one of
/// `ends`) as marks on a node of their own, last in their layer, over the
/// current line; made when the first shows.
#[cold]
#[inline(never)]
fn show_decorations(
    s: &mut crate::scene::Scene,
    code: &CodeHint,
    (width, height): (f32, f32),
    (shaped, ends): (Vec<crate::scene::TextMark>, Vec<usize>),
    radius: f32,
) {
    let Some(layer) = code.decoration_layer.get() else {
        return;
    };
    let id = match code.decoration_text.get() {
        Some(id) => id,
        None => {
            let kind = NodeKind::Text {
                text: "".into(),
                color: Color(0, 0, 0, 0),
                font_size: 1.,
            };
            let id = s.append(layer, kind, Style::default());
            code.decoration_text.set(Some(id));
            id
        }
    };
    if s.children(layer).last() != Some(&id) {
        let mut order = s.children(layer).to_vec();
        order.retain(|child| *child != id);
        order.push(id);
        s.reorder_children(layer, &order);
    }
    // Padded: flush takes its ink from its box, shaping nothing.
    let mut style = fixed(width, height);
    style.padding = 1.;
    s.set_style(id, style);
    s.set_text_mark_shapes(id, shaped, ends, radius);
}
#[derive(Default)]
struct CodeHint {
    /// Ranges of the text marked with a colour behind them (a search's
    /// matches), by where they start.
    decorations: RefCell<Vec<(std::ops::Range<usize>, Color)>>,
    /// Ranges of the text underlined, by where they start.
    underlines: RefCell<Vec<EditorUnderline>>,
    /// What draws them: a node a row of each, what each shows, and so
    /// only what changed is made again.
    underline_nodes: RefCell<Vec<(NodeId, UnderlineDrawn)>>,
    /// The colour behind the caret's row (a code editor's current line).
    current_line: Cell<Option<Color>>,
    /// What Tab inserts (a code editor's indent), instead of moving focus.
    indent: RefCell<Option<std::sync::Arc<str>>>,
    /// A long text's lines (`editor_lines`), made when one first shows, and
    /// the layer of a text node a line in view, by line number.
    lines: RefCell<Option<Rc<crate::editor_lines::LineCache>>>,
    line_layer: Cell<Option<NodeId>>,
    line_nodes: RefCell<Vec<(usize, NodeId)>>,
    /// Marks behind the selection (the current line, decorations), made
    /// when the first shows.
    decoration_layer: Cell<Option<NodeId>>,
    decoration_nodes: RefCell<Vec<NodeId>>,
    /// Carries the decorations as marks drawn as shapes, made when one
    /// first shows.
    decoration_text: Cell<Option<NodeId>>,
    /// Told where the caret is when it moves (`EditorHandle::on_caret`),
    /// and whether one listens (the slot is empty while it runs).
    on_caret: RefCell<Option<CaretListener>>,
    caret_listened: Cell<bool>,
    /// Where it was last drawn, and last told.
    caret_drawn: Cell<Option<Caret>>,
    caret_told: Cell<Option<Caret>>,
    /// Told the first line in view when it changes
    /// (`EditorHandle::on_top_line`), as `on_caret`.
    on_top_line: RefCell<Option<TopLineListener>>,
    top_line_listened: Cell<bool>,
    top_line_drawn: Cell<Option<usize>>,
    top_line_told: Cell<Option<usize>>,
}
impl Drop for CodeHint {
    /// A long text's last layout and its line cache hold each other.
    fn drop(&mut self) {
        if let Some(lines) = self.lines.get_mut().take() {
            lines.forget_last();
        }
    }
}
impl EditorHint {
    fn new(value: EditorValue) -> Self {
        Self {
            value,
            placeholder: RefCell::default(),
            caret: Cell::default(),
            select_on_focus: Cell::default(),
            contain: Cell::default(),
            secure: Cell::default(),
            input_mode: Cell::default(),
            misspelled: RefCell::default(),
            clause: Cell::default(),
            composition_layer: Cell::default(),
            reveal: Cell::default(),
            reveal_at: Cell::default(),
            point: std::cell::OnceCell::new(),
            code: std::cell::OnceCell::default(),
            themed: Cell::default(),
            theme_watch: Cell::default(),
        }
    }
    /// Redraw `root` when a theme colour it has shown in changes: one
    /// effect, made again only when it shows in another, and stopped when
    /// `root` goes (a handle may outlive it).
    fn watch_theme(
        hint: &Rc<Self>,
        storage: &Weak<UiStorage>,
        root: NodeId,
        theme: &Rc<ThemeScope>,
    ) {
        let themed = hint.themed.get();
        if themed == 0 {
            return;
        }
        let watch = hint.theme_watch.take();
        if watch.as_ref().is_some_and(|watch| watch.0 == themed) {
            hint.theme_watch.set(watch);
            return;
        }
        let Some(strong) = storage.upgrade() else {
            return;
        };
        if watch.is_none() {
            let mut owned = strong.owned.borrow_mut();
            let Some(owned) = owned.get_mut(&root) else {
                return;
            };
            owned
                .resources
                .push(Box::new(ThemeUnwatch(Rc::downgrade(hint))));
        }
        drop(watch);
        let runtime = strong.runtime.clone();
        drop(strong);
        let (storage, theme) = (storage.clone(), theme.clone());
        let mut first = true;
        let effect = runtime.effect(move || {
            for (bit, token) in [
                (THEMED_TERTIARY, tok::TEXT_TERTIARY),
                (THEMED_SECONDARY, tok::TEXT_SECONDARY),
                (THEMED_ACCENT, tok::ACCENT),
                (THEMED_DANGER, tok::DANGER),
            ] {
                if themed & bit != 0 {
                    theme.color(token);
                }
            }
            if std::mem::take(&mut first) {
                return;
            }
            let Some(storage) = storage.upgrade() else {
                return;
            };
            let handle = storage.editors.borrow().get(&root).cloned();
            if let Some(handle) = handle {
                storage.runtime.untracked(|| handle.refresh());
            }
        });
        hint.theme_watch.set(Some(Box::new((themed, effect))));
    }
    fn code(&self) -> Option<&CodeHint> {
        self.code.get().map(|code| &**code)
    }
    fn code_made(&self) -> &CodeHint {
        self.code.get_or_init(Default::default)
    }
    /// Its long text's line cache, made the first time it is needed.
    fn line_cache(&self) -> Rc<crate::editor_lines::LineCache> {
        self.code_made()
            .lines
            .borrow_mut()
            .get_or_insert_with(Default::default)
            .clone()
    }
    fn highlighted(&self) -> bool {
        self.code()
            .and_then(|code| code.lines.borrow().clone())
            .is_some_and(|lines| lines.highlighted())
    }
}
/// What an editor tells its first line in view (`EditorHandle::on_top_line`).
type TopLineListener = Box<dyn FnMut(usize)>;
/// What an editor tells where its caret is (`EditorHandle::on_caret`).
type CaretListener = Box<dyn FnMut(Caret)>;
/// Where an editor's caret is (`EditorHandle::on_caret`): its line and
/// column, from 1 (the column in characters), and the selection as UTF-8
/// byte offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    /// The caret's line, from 1.
    pub line: usize,
    /// Its column in characters, from 1.
    pub column: usize,
    /// Where the selection starts (equal to `focus` for a caret).
    pub anchor: usize,
    /// Where the caret is.
    pub focus: usize,
}
impl EditorHandle {
    /// The value it is bound to; edits set it.
    pub fn value(&self) -> &EditorValue {
        &self.hint.value
    }
    /// Show the platform's definition of the selected text over it (Look
    /// Up); whether it did. A password's text is never looked up.
    pub fn look_up(&self, platform: &crate::platform::PlatformService) -> bool {
        let text = self.copy();
        if text.trim().is_empty() {
            return false;
        }
        let start = self.editor.borrow().selection().range().start;
        let Some((at, size)) = self.scene.borrow().text_baseline(self.text, start) else {
            return false;
        };
        platform.0.look_up(&text, at, size)
    }
    /// Where an input method's candidate window goes: under the clause it
    /// is converting (Japanese, Chinese), as a native field puts it, in the
    /// window's points (through any transform or zoom the editor shows
    /// under); `None` while there is none (the caret, then).
    pub fn composition_area(&self) -> Option<Rect> {
        let clause = self.hint.clause.get()?;
        let layer = self.hint.composition_layer.get()?;
        let scene = self.scene.borrow();
        if !scene.contains(layer) {
            return None;
        }
        let at = scene.bounds(layer);
        Some(scene.window_rect(
            layer,
            Rect::new(
                at.x + clause.x,
                at.y + clause.y,
                clause.width,
                clause.height,
            ),
        ))
    }
    /// Where an input method's candidates go, in the window's points: under
    /// the clause it converts (`composition_area`), else at the caret.
    pub fn input_method_area(&self) -> Rect {
        self.composition_area()
            .unwrap_or_else(|| self.scene.borrow().transformed_bounds(self.caret))
    }
    /// Where bytes `range` of the text it shows (its committed text with
    /// any composition in place of the selection) are, in the window's
    /// points: the box around them across every line they run on, a
    /// caret's place for an empty range. `None` where its text has no
    /// lines to ask (a password's). Shapes the text: for an input method
    /// asking where a range is (Windows' `GetTextExt`), not every frame.
    pub fn text_area(&self, range: std::ops::Range<usize>) -> Option<Rect> {
        self.scene.borrow().text_range_area(self.text, range)
    }
    /// The selection as UTF-8 byte offsets into the committed text,
    /// `(anchor, focus)`: equal for a caret, `focus` where the caret is.
    pub fn selection(&self) -> (usize, usize) {
        let selection = self.editor.borrow().selection();
        (selection.anchor, selection.focus)
    }
    /// Scroll byte `at` into view the next time it is drawn, its caret and
    /// selection left as they are.
    pub(crate) fn reveal_offset(&self, at: usize) {
        self.hint
            .reveal_at
            .set(Some((at, crate::compose::RowAlign::Nearest)));
        self.refresh();
    }
    /// Select from `anchor` to `focus` (UTF-8 byte offsets, clamped to the
    /// text and moved back to a grapheme boundary), as a drag would.
    pub fn set_selection(&self, anchor: usize, focus: usize) {
        let focus = {
            let mut editor = self.editor.borrow_mut();
            editor.set_selection(anchor, focus);
            editor.selection().focus
        };
        self.position.set(TextPosition {
            byte_offset: focus,
            affinity: TextAffinity::After,
        });
        self.preferred_x.set(None);
        self.hint.reveal.set(true);
        self.refresh();
    }
    /// Select the whole text.
    /// Mark `ranges` (UTF-8 byte ranges of the text) misspelled.
    pub(crate) fn set_misspellings(&self, ranges: Vec<std::ops::Range<usize>>) {
        if *self.hint.misspelled.borrow() == ranges {
            return;
        }
        *self.hint.misspelled.borrow_mut() = ranges;
        self.refresh();
    }
    /// The misspelled word the selection is in (or the caret touches).
    pub(crate) fn misspelling_at_selection(&self) -> Option<std::ops::Range<usize>> {
        let (anchor, focus) = self.selection();
        let (start, end) = (anchor.min(focus), anchor.max(focus));
        self.hint
            .misspelled
            .borrow()
            .iter()
            .find(|range| range.start <= start && end <= range.end)
            .cloned()
    }
    pub(crate) fn misspellings(&self) -> Vec<std::ops::Range<usize>> {
        self.hint.misspelled.borrow().clone()
    }
    pub(crate) fn text(&self) -> String {
        self.editor.borrow().text().into_owned()
    }
    /// Select all the text.
    pub fn select_all(&self) {
        let end = self.editor.borrow().text().len();
        self.set_selection(0, end);
    }
    /// Select all whenever the editor takes keyboard focus (a click, Tab,
    /// a program), as a browser's address bar does: the first click selects
    /// everything, a second places the caret. A window coming back to the
    /// front keeps the selection it had.
    pub fn set_select_on_focus(&self, select: bool) {
        self.hint.select_on_focus.set(select);
    }
    /// Whether it is a password's (`set_secure`).
    pub fn is_secure(&self) -> bool {
        self.hint.secure.get()
    }
    /// A password field: its text shows as a bullet per character (each in
    /// that character's place, so carets and selections work), and copying
    /// or cutting it gives nothing.
    pub fn set_secure(&self, secure: bool) {
        self.hint.secure.set(secure);
    }
    /// What kind of text it takes, as HTML's `inputmode` says it: a hint
    /// to the platform's input (Windows' touch keyboard and input scope, a
    /// Wayland input method's content purpose), not a check of the text.
    pub fn set_input_mode(&self, mode: crate::text_edit::InputMode) {
        self.hint.input_mode.set(mode);
    }
    /// `set_input_mode`'s; `InputMode::Text` until set.
    pub fn input_mode(&self) -> crate::text_edit::InputMode {
        self.hint.input_mode.get()
    }
    /// Text shown, never edited, while the editor is empty: in its own
    /// typography, coloured `color` (`None`: the theme's `muted`). It is
    /// not part of the value and does not change the editor's size;
    /// assistive technology reads it as the placeholder.
    pub fn set_placeholder(&self, text: Option<&str>, color: Option<Color>) {
        let next = text
            .filter(|text| !text.is_empty())
            .map(|text| (std::sync::Arc::<str>::from(text), color));
        let mut current = self.hint.placeholder.borrow_mut();
        if *current == next {
            return;
        }
        *current = next;
        drop(current);
        self.refresh();
    }
    /// The placeholder, if any.
    pub fn placeholder(&self) -> Option<String> {
        self.hint
            .placeholder
            .borrow()
            .as_ref()
            .map(|(text, _)| text.to_string())
    }
    /// Whether user editing is blocked. Selection, copying and model writes remain available.
    pub fn is_read_only(&self) -> bool {
        self.read_only.get()
    }
    /// Keep a wheel gesture that starts over it at its edges, as CSS's
    /// `overscroll-behavior: contain`: the scroll views around it don't
    /// take what it can't scroll (`View::overscroll_contain`).
    pub fn set_overscroll_contain(&self, contain: bool) {
        self.hint.contain.set(contain);
    }
    /// Change editing policy without moving focus or changing committed text/history.
    /// Entering read-only mode cancels an existing composition.
    pub fn set_read_only(&self, read_only: bool) {
        if self.read_only.replace(read_only) == read_only {
            return;
        }
        if read_only {
            self.editor.borrow_mut().cancel_preedit();
        }
        self.refresh();
    }

    /// Enable soft wrapping for multiline controls. Single-line controls stay unwrapped.
    pub fn set_wrap(&self, wrap: bool) {
        let wrap = wrap && self.multiline;
        if self.wrap.replace(wrap) != wrap {
            self.preferred_x.set(None);
            self.refresh();
        }
    }
    /// Number a multiline editor's lines in a gutter at its left (a code
    /// editor's): each logical line's number beside its first row, the
    /// caret's brighter, only those in view built. Single-line controls
    /// have none.
    pub fn set_line_numbers(&self, numbered: bool) {
        let numbered = numbered && self.multiline;
        if self.line_numbers.replace(numbered) != numbered {
            self.refresh();
        }
    }
    /// Colour a multiline editor's lines with `highlighter` (a code
    /// editor's syntax): asked only for the lines that show, and again
    /// only for those that change (see [`Highlighter`]). Its colours change
    /// no metrics, so carets and selections stay where they were.
    pub fn set_highlighter(&self, highlighter: Option<Highlighter>) {
        self.set_span_highlighter(highlighter.map(crate::editor_lines::span_highlighter));
    }
    /// Style a multiline editor's lines with `highlighter`, as
    /// `set_highlighter` colours them, with weights, italics, strikes and
    /// washes too (see [`SpanStyle`]). A line whose spans set a weight or
    /// italics is shaped with them, so its carets, hits and wrapping follow
    /// what shows; other lines are shaped as their plain text.
    pub fn set_span_highlighter(&self, highlighter: Option<SpanHighlighter>) {
        if !self.multiline {
            return;
        }
        self.hint.line_cache().set_highlighter(highlighter);
        self.refresh();
    }
    /// The caret's colour (`None`: the text's, as by default) and width in
    /// points (`None`: the app's, `Ui::set_caret_width`).
    pub fn set_caret_style(&self, color: Option<Color>, width: Option<f32>) {
        let width = width.filter(|w| w.is_finite() && *w >= 0.);
        if self.hint.caret.replace((color, width)) != (color, width) {
            self.refresh();
        }
    }
    /// Ask the highlighter for the lines anew (what it gives changed: a
    /// theme's colours).
    pub fn rehighlight(&self) {
        if let Some(lines) = self.hint.code().and_then(|c| c.lines.borrow().clone()) {
            lines.rehighlight();
        }
        self.refresh();
    }
    /// Mark `ranges` of the text (UTF-8 byte ranges) with a colour behind
    /// them, a search's matches: only those in view are drawn.
    pub fn set_decorations(&self, mut ranges: Vec<(std::ops::Range<usize>, Color)>) {
        ranges.sort_by_key(|(range, _)| range.start);
        let code = self.hint.code_made();
        if *code.decorations.borrow() == ranges {
            return;
        }
        *code.decorations.borrow_mut() = ranges;
        self.refresh();
    }
    /// Underline `ranges` of the text (a diagnostic's wave): only those in
    /// view are drawn.
    pub fn set_underlines(&self, mut ranges: Vec<EditorUnderline>) {
        ranges.sort_by_key(|underline| underline.range.start);
        let code = self.hint.code_made();
        if *code.underlines.borrow() == ranges {
            return;
        }
        *code.underlines.borrow_mut() = ranges;
        self.refresh();
    }
    /// Call `told` with where the caret is whenever it moves or the
    /// selection changes (a status bar's line and column), after the
    /// editor is drawn: it may change what it likes.
    pub fn on_caret(&self, told: impl FnMut(Caret) + 'static) {
        let code = self.hint.code_made();
        *code.on_caret.borrow_mut() = Some(Box::new(told));
        code.caret_listened.set(true);
        code.caret_told.set(None);
        self.refresh();
    }
    /// Call `told` with the first line in view, from 1, whenever it changes
    /// (an outline following the text), after the editor is drawn.
    pub fn on_top_line(&self, told: impl FnMut(usize) + 'static) {
        let code = self.hint.code_made();
        *code.on_top_line.borrow_mut() = Some(Box::new(told));
        code.top_line_listened.set(true);
        code.top_line_told.set(None);
        self.refresh();
    }
    /// Scroll line `line` (from 0) to `align` of the view, its caret and
    /// selection left as they are; past the last line, the end.
    pub fn scroll_to_line(&self, line: usize, align: crate::compose::RowAlign) {
        let at = self.editor.borrow().line_start(line);
        self.hint.reveal_at.set(Some((at, align)));
        self.refresh();
    }
    /// A colour behind the caret's row (a code editor's current line).
    pub fn set_current_line(&self, color: Option<Color>) {
        if self.hint.code_made().current_line.replace(color) != color {
            self.refresh();
        }
    }
    /// What Tab inserts in a multiline editor (a code editor's indent:
    /// `"    "`, `"\t"`), instead of moving focus on; with lines selected
    /// it indents each, and Shift+Tab takes an indent off. `None`: Tab
    /// moves focus, as in any field.
    pub fn set_indent(&self, indent: Option<&str>) {
        *self.hint.code_made().indent.borrow_mut() =
            indent.filter(|_| self.multiline).map(Into::into);
    }
    /// Update rendering and editing metrics together, including selection and IME geometry.
    pub fn set_typography(&self, color: Color, size: f32, font: FontStyle) {
        self.set_text_style(color, size, font, self.wrap.get());
    }
    pub(crate) fn set_text_style(&self, color: Color, size: f32, mut font: FontStyle, wrap: bool) {
        let wrap = wrap && self.multiline;
        let wrap_changed = self.wrap.replace(wrap) != wrap;
        font.weight = font.weight.clamp(1, 1000);
        let next = (color, size.max(1.), font);
        let previous = self.typography.borrow().clone();
        if previous == next && !wrap_changed {
            return;
        }
        let metrics_changed = wrap_changed || previous.1 != next.1 || previous.2 != next.2;
        *self.typography.borrow_mut() = next;
        // A placeholder showing keeps its own colour: draw it again.
        if metrics_changed || self.hint.placeholder.borrow().is_some() {
            self.preferred_x.set(None);
            // A mount drawing this editor later draws it in this style.
            if !input_draw_pending(self.node, &self.scene) {
                self.refresh();
            }
        } else {
            let mut scene = self.scene.borrow_mut();
            if !scene.contains(self.text) {
                return;
            }
            if let NodeKind::Text {
                text, font_size, ..
            } = scene.kind(self.text).clone()
            {
                scene.set_kind(
                    self.text,
                    NodeKind::Text {
                        text,
                        font_size,
                        color,
                    },
                );
            }
            scene.set_kind(self.caret, NodeKind::Rect(color));
        }
    }
    fn refresh_if_resized(&self) {
        let scene = self.scene.borrow();
        if !scene.contains(self.node) {
            return;
        }
        let bounds = scene.bounds(self.node);
        let next = (
            Rect::new(0., 0., bounds.width, bounds.height),
            scene.style(self.node),
            scene.text_shaper_revision(),
            scene.content_insets(self.node),
        );
        let changed = self.geometry.borrow().as_ref() != Some(&next);
        drop(scene);
        if changed {
            self.preferred_x.set(None);
            self.refresh();
        }
    }

    /// Lay out and repaint the editor from its model again.
    pub fn refresh(&self) {
        (self.refresh)();
    }
    /// The selected text, for the clipboard; empty in a password field.
    pub fn copy(&self) -> String {
        // A password field's text never leaves it.
        if self.hint.secure.get() {
            return String::new();
        }
        self.editor.borrow().selected_text().into_owned()
    }
    /// Copy the selection and delete it when editable. Read-only cut is copy-only.
    pub fn cut(&self) -> String {
        if self.is_read_only() {
            return self.copy();
        }
        self.preferred_x.set(None);
        let text = self.copy();
        if !text.is_empty() {
            self.editor.borrow_mut().insert("");
            self.hint.value.write(&self.editor);
            self.refresh();
        }
        text
    }
    /// Replace the selection with `text`, as typed: what a text service
    /// (Windows' emoji panel, dictation, touch keyboard) inserts. A
    /// single-line editor joins its lines; a read-only one ignores it.
    pub fn insert_text(&self, text: &str) -> bool {
        if self.is_read_only() {
            return false;
        }
        self.preferred_x.set(None);
        let composing = self.editor.borrow().preedit().is_some();
        let changed = self
            .editor
            .borrow_mut()
            .insert(&normalize_text(text, self.multiline));
        self.edited(changed, composing);
        changed
    }
    /// Replace the committed text in `range` (UTF-8 byte offsets) with
    /// `text`, one undo step, the caret after it: an input method converting
    /// text again, or correcting it. A read-only editor ignores it.
    pub fn replace_text(&self, range: std::ops::Range<usize>, text: &str) -> bool {
        if self.is_read_only() {
            return false;
        }
        self.preferred_x.set(None);
        let composing = self.editor.borrow().preedit().is_some();
        let changed = self
            .editor
            .borrow_mut()
            .replace_range(range, &normalize_text(text, self.multiline));
        self.edited(changed, composing);
        changed
    }
    /// After an edit (`composing`: a composition showed before it): the
    /// model told and the editor drawn again if its text changed, or drawn
    /// again alone if only its composition ended (the same text put in).
    fn edited(&self, changed: bool, composing: bool) {
        if changed {
            self.hint.value.write(&self.editor);
            self.refresh();
        } else if composing && self.editor.borrow().preedit().is_none() {
            self.refresh();
        }
    }
    /// Show `text` as the input method's composition at the selection, as
    /// `TextEditor::set_preedit_clauses` does (empty clears it). A read-only
    /// editor shows none.
    pub fn set_composition(
        &self,
        text: &str,
        cursor: Option<(usize, usize)>,
        clauses: &[(std::ops::Range<usize>, crate::text_edit::ClauseStyle)],
    ) {
        if self.is_read_only() {
            return;
        }
        self.editor
            .borrow_mut()
            .set_preedit_clauses(text, cursor, clauses);
        self.refresh();
    }
    /// End the composition, inserting `text` (what the input method
    /// committed) as typed. False when nothing changed.
    pub fn commit_composition(&self, text: &str) -> bool {
        if self.is_read_only() {
            self.editor.borrow_mut().cancel_preedit();
            return false;
        }
        let changed = self
            .editor
            .borrow_mut()
            .commit_preedit(&normalize_text(text, self.multiline));
        if changed {
            self.hint.value.write(&self.editor);
        }
        self.refresh();
        changed
    }
    /// Drop the composition, inserting nothing.
    pub fn cancel_composition(&self) {
        self.editor.borrow_mut().cancel_preedit();
        self.refresh();
    }
    /// Replace the selection with `text`, as the person pasting it would. A
    /// single-line editor joins its lines; a read-only one ignores it.
    pub fn paste(&self, text: &str) {
        if self.is_read_only() {
            return;
        }
        self.preferred_x.set(None);
        // A step of its own, even of one character: not the typing's.
        self.editor
            .borrow_mut()
            .insert_pasted(&normalize_text(text, self.multiline));
        self.hint.value.write(&self.editor);
        self.refresh();
    }
    /// Undo its last step, as ⌘Z does; whether there was one to undo.
    pub fn undo(&self) -> bool {
        self.step(TextEditor::undo)
    }
    /// Redo the last step undone, as ⌘⇧Z does; whether there was one.
    pub fn redo(&self) -> bool {
        self.step(TextEditor::redo)
    }
    fn step(&self, step: fn(&mut TextEditor) -> bool) -> bool {
        if self.is_read_only() || !step(&mut self.editor.borrow_mut()) {
            return false;
        }
        self.preferred_x.set(None);
        self.hint.value.write(&self.editor);
        self.hint.reveal.set(true);
        self.refresh();
        true
    }
    /// The byte the text has at `x`, `y` (the window's points): where a
    /// click there would put the caret.
    pub fn offset_at(&self, x: f32, y: f32) -> Option<usize> {
        self.hint.point.get().map(|point| point(x, y))
    }
}
/// What `View::on_paste` gives an editor: handed what the person pasted
/// (none where the clipboard holds no text: an image, say), true if it took
/// the paste.
pub(crate) type PasteHook = Rc<dyn Fn(Option<&str>) -> bool>;
/// The layout and notification passes a frame takes to settle.
const LAYOUT_PASSES: usize = 64;
/// How much of a point a size may differ by and be the same (`resized`).
const SIZE_NOISE: f32 = 1. / 1024.;
/// The passes it takes once layout has failed to settle
/// (`Ui::contain_layout_feedback`): each change is tried again, cheaply.
const RETRY_LAYOUT_PASSES: usize = 16;
/// Layout notifications repeatedly changed their own allocation without settling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutFeedbackError {
    /// How many layout passes ran before giving up.
    pub passes: usize,
}
impl std::fmt::Display for LayoutFeedbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "layout allocation did not settle after {} passes; check size-dependent view mutations",
            self.passes
        )
    }
}
impl std::error::Error for LayoutFeedbackError {}
struct FramePreparationGuard<'a>(&'a Cell<bool>);
impl Drop for FramePreparationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// One retained document. Application code keeps returned node IDs as stable widget handles.
pub struct Ui {
    /// The reactive runtime its signals and effects live in.
    pub runtime: Runtime,
    /// The retained scene: nodes, styles and layout.
    pub scene: Rc<RefCell<Scene>>,
    /// Routes input to nodes and keeps focus.
    pub input: InputDispatcher,
    /// The accessibility tree hosts project to the platform.
    pub semantics: Rc<RefCell<Semantics>>,
    pub(crate) storage: Rc<UiStorage>,
}
// One focused-editor snapshot per document. Large or unaccounted custom layouts
// remain usable but are never retained by this reuse entry.
/// Subtrees up to this many nodes are freed as they are removed; larger
/// ones a chunk at a time after frames (`Ui::reclaim`).
const RECLAIM_NOW: usize = 256;
/// Nodes freed between looks at the clock (`Ui::reclaim`).
const RECLAIM_CHUNK: usize = 64;
/// Removed nodes left to free beyond which a removal frees the excess at
/// once, whatever it takes: memory stays bounded however often large
/// subtrees go.
const RECLAIM_BACKLOG: usize = 1 << 18;
/// What freeing removed nodes may take of a frame being prepared. A chunk
/// can overrun it while the system allocator returns pages (6–9 ms on
/// macOS over the first); mimalloc doesn't (zgx-desktop's `mimalloc`).
pub const RECLAIM_BUDGET: std::time::Duration = std::time::Duration::from_millis(1);
const EDITOR_CACHE_TEXT_LIMIT: usize = 64 * 1024;
const EDITOR_CACHE_WEIGHT_LIMIT: usize = 1024 * 1024;
struct EditorLayoutEntry {
    node: NodeId,
    text: String,
    size: u32,
    width: Option<u32>,
    font: FontStyle,
    revision: u64,
    layout: Rc<dyn TextLayout>,
}

/// A numbered editor's gutter: space before the numbers, and between them
/// and the text.
const GUTTER_INSET: f32 = 4.;
const GUTTER_GAP: f32 = 14.;
/// How many times `byte` occurs in `bytes`.
fn memchr_count(byte: u8, bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == byte).count()
}

/// An editor's text style as its text shows: `Start` is its right when the
/// editor runs right to left. Its inner nodes lay out left to right (they
/// are placed from this layout), so the scene doesn't resolve it for them.
fn editor_font<'a>(
    scene: &Scene,
    node: NodeId,
    font: &'a FontStyle,
) -> std::borrow::Cow<'a, FontStyle> {
    use crate::text_layout::TextAlign;
    let rtl = || scene.contains(node) && scene.direction(node) == crate::scene::Direction::Rtl;
    let align = match font.align {
        TextAlign::Start if rtl() => TextAlign::Right,
        TextAlign::End if rtl() => TextAlign::Left,
        TextAlign::End => TextAlign::Right,
        _ => return std::borrow::Cow::Borrowed(font),
    };
    std::borrow::Cow::Owned(FontStyle {
        align,
        ..font.clone()
    })
}

/// A long text's layout by its lines, the editor's last if the text is the
/// same.
#[allow(clippy::too_many_arguments)]
fn long_layout(
    lines: &Rc<crate::editor_lines::LineCache>,
    scene: &Scene,
    text: impl FnOnce() -> Text,
    revision: Option<u64>,
    size: f32,
    width: Option<f32>,
    font: &FontStyle,
) -> Rc<crate::editor_lines::LinesLayout> {
    lines.layout(
        text,
        revision,
        size,
        width,
        font,
        scene.text_shaper_revision(),
        || (scene.line_shaper(size, font), scene.rich_line_shaper()),
    )
}
/// `e`'s committed text laid out (`editor_layout`): a long one by its
/// lines, from the chunks it shares, known by its revision (a pointer's
/// move, a drag, a key, none of which copies it).
#[allow(clippy::too_many_arguments)]
fn committed_layout(
    storage: &Weak<UiStorage>,
    scene: &Scene,
    node: NodeId,
    e: &TextEditor,
    size: f32,
    width: Option<f32>,
    font: &FontStyle,
) -> Rc<dyn TextLayout> {
    let lines = storage.upgrade().and_then(|s| {
        let editors = s.editors.borrow();
        let hint = &editors.get(&node)?.hint;
        (e.long_text().is_some() || hint.highlighted()).then(|| hint.line_cache())
    });
    match lines {
        Some(lines) => {
            let font = editor_font(scene, node, font);
            let text = || {
                e.long_text()
                    .cloned()
                    .unwrap_or_else(|| Text::new(&e.text()))
            };
            long_layout(&lines, scene, text, Some(e.revision()), size, width, &font)
        }
        None => editor_layout(storage, scene, node, &e.text(), size, width, font),
    }
}
fn editor_layout(
    storage: &Weak<UiStorage>,
    scene: &Scene,
    node: NodeId,
    text: &str,
    size: f32,
    width: Option<f32>,
    font: &FontStyle,
) -> Rc<dyn TextLayout> {
    let font = &*editor_font(scene, node, font);
    let storage = storage.upgrade();
    if crate::editor_lines::is_long(text)
        && let Some(lines) = storage
            .as_ref()
            .and_then(|s| s.editors.borrow().get(&node).map(|e| e.hint.line_cache()))
    {
        return long_layout(&lines, scene, || Text::new(text), None, size, width, font);
    }
    let eligible = storage
        .as_ref()
        .is_some_and(|s| s.input.focused() == Some(node))
        && text.len() <= EDITOR_CACHE_TEXT_LIMIT;
    let revision = scene.text_shaper_revision();
    let width_bits = width.map(f32::to_bits);
    if eligible {
        let cache = storage.as_ref().unwrap().editor_layout.borrow();
        if let Some(entry) = cache.as_ref()
            && entry.node == node
            && entry.size == size.to_bits()
            && entry.width == width_bits
            && entry.font == *font
            && entry.revision == revision
            && entry.text == text
        {
            return entry.layout.clone();
        }
    }
    // Drop stale focused snapshots even when their replacement is too large.
    if let Some(storage) = &storage {
        let old = {
            let mut cache = storage.editor_layout.borrow_mut();
            if storage.input.focused() == Some(node)
                || cache.as_ref().is_some_and(|entry| entry.node == node)
            {
                cache.take()
            } else {
                None
            }
        };
        drop(old);
    }
    let layout: Rc<dyn TextLayout> = scene.shape_text_with_font(text, size, width, font).into();
    if eligible {
        let key_weight = std::mem::size_of::<EditorLayoutEntry>()
            .saturating_add(text.len())
            .saturating_add(font.storage_bytes())
            .saturating_add(2 * std::mem::size_of::<usize>());
        if layout
            .cache_weight()
            .is_some_and(|weight| weight.saturating_add(key_weight) <= EDITOR_CACHE_WEIGHT_LIMIT)
            && storage.as_ref().unwrap().input.focused() == Some(node)
        {
            let old = storage
                .as_ref()
                .unwrap()
                .editor_layout
                .borrow_mut()
                .replace(EditorLayoutEntry {
                    node,
                    text: text.to_owned(),
                    size: size.to_bits(),
                    width: width_bits,
                    font: font.clone(),
                    revision,
                    layout: layout.clone(),
                });
            drop(old);
        }
    }
    layout
}

type InteractionRefresh = Rc<RefCell<Option<Weak<dyn Fn()>>>>;

struct EditorInteraction {
    owner: NodeId,
    generation: u64,
    gesture: Rc<EditorDrag>,
    can_scroll: Rc<dyn Fn() -> bool>,
    tick: Rc<dyn Fn(f64)>,
}
struct InteractionDeadline {
    job: Rc<EditorInteraction>,
    last: Instant,
    next: Instant,
}
#[derive(Default)]
struct EditorDrag {
    mode: Cell<Option<EditorPointerSelection>>,
    pointer: Cell<(f32, f32)>,
    generation: Cell<u64>,
    /// The words or paragraphs the last presses selected by, and the
    /// selection they made: a shift-click extending it, while it stands,
    /// goes by those units too, as AppKit's text views do.
    units: Cell<Option<(EditorPointerSelection, (usize, usize))>>,
    /// A press on the selection, where and on what byte: moved far enough
    /// it drags the selected text; released, the caret goes there.
    text_press: Cell<Option<(f32, f32, usize)>>,
    /// The selection while a drag of text shows where it would drop (the
    /// caret there): back as it leaves, or before the drop.
    drop_restore: Cell<Option<crate::text_edit::Selection>>,
    /// A drop focuses the editor: the text it dropped stays selected, a
    /// field that selects all as it is focused or not.
    dropped: Cell<bool>,
}
impl EditorDrag {
    fn cancel(&self) {
        self.mode.set(None);
        self.text_press.set(None);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}
/// The host's picture of a region of what the window last showed.
type Snapshotter = Rc<dyn Fn(Rect, u32) -> Option<std::sync::Arc<crate::image::ImageData>>>;
type PopupSnapshotter =
    Rc<dyn Fn(NodeId, Rect, u32) -> Option<std::sync::Arc<crate::image::ImageData>>>;
/// The host's recorder and player of wheel input (`Ui::on_wheel_tape`).
type WheelTaper = Rc<dyn Fn(WheelTape) -> Result<String, String>>;
/// What devtools ask of the host's wheel input (`Ui::wheel_tape`): to record
/// what the window receives, or to play a recording back as if received.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum WheelTape {
    /// Start recording (what was recorded before is dropped).
    Record,
    /// Stop: the recording, as text (`zgx_desktop`'s tape format).
    Stop,
    /// Play `tape` back, its events at their times, at `at` (in points).
    Play { tape: String, at: (f32, f32) },
}
pub(crate) struct UiStorage {
    runtime: Runtime,
    pub(crate) themes: Rc<Themes>,
    /// The theme scope of the element being mounted (`Ui::enter_theme`).
    mount_theme: RefCell<Option<Rc<ThemeScope>>>,
    text_geometry_revision: Signal<u64>,
    presented: Signal<bool>,
    /// Whether the window is the active one, as a signal, made the first
    /// time something observes it (`Ui::observe_window_active`): an app
    /// that never does pays nothing. The selection keeps the flag itself.
    window_active: RefCell<Option<Signal<bool>>>,
    /// Where popups may show (`Ui::set_popup_screens`).
    popup_screens: Signal<Vec<Rect>>,
    /// The sizes the platform cut popups to, by their roots
    /// (`Ui::set_popup_limit`).
    popup_limits: Signal<Vec<(NodeId, (f32, f32))>>,
    /// Popups the platform showed on their triggers' other sides, by their
    /// roots (`Ui::set_popup_flipped`).
    popup_flips: Signal<Vec<(NodeId, (bool, bool))>>,
    /// Open signals of anchored popups, by their popup roots
    /// (`Ui::dismiss_popups`).
    popups: RefCell<Vec<(NodeId, Signal<bool>)>>,
    /// Portals' owners and overlays, in the order they mounted (`Ui::portals`).
    portals: RefCell<Vec<(NodeId, NodeId)>>,
    /// The host's hook for a popup about to open (`Ui::on_popup_soon`), and
    /// whether it was called and not yet heard (`popup_soon_heard`).
    popup_soon: RefCell<Option<Rc<dyn Fn()>>>,
    popup_soon_sent: Cell<bool>,
    /// The host's way to picture what the window last showed (`Ui::on_snapshot`).
    snapshotter: RefCell<Option<Snapshotter>>,
    popup_snapshotter: RefCell<Option<PopupSnapshotter>>,
    /// The window's zoom (`zoom`), 1 at 100%.
    pub(crate) zoom: Cell<f64>,
    /// The app turned zoom off (`set_zoomable`).
    pub(crate) zoom_off: Cell<bool>,
    /// Who is told the zoom changed (`on_zoom`): the host, a script.
    pub(crate) zoom_host: RefCell<Vec<Rc<dyn Fn()>>>,
    /// A Control-wheel's notches short of a whole one (`zoom_wheel`).
    pub(crate) zoom_wheel: Cell<f32>,
    /// Nodes kept at their own size whatever zooms are around them
    /// (`Ui::set_zoom_fixed`).
    pub(crate) fixed_zooms: RefCell<Vec<NodeId>>,
    /// The host's wheel recorder and player (`Ui::on_wheel_tape`).
    wheel_taper: RefCell<Option<WheelTaper>>,
    /// The display's refresh period (`Ui::set_refresh_interval`).
    refresh_interval: Cell<std::time::Duration>,
    /// The host shows drag previews in windows of their own (`set_popup_previews`).
    popup_previews: Cell<bool>,
    pub(crate) scene: Rc<RefCell<Scene>>,
    input: InputDispatcher,
    semantics: Rc<RefCell<Semantics>>,
    owned: RefCell<crate::scene::NodeMap<Owned>>,
    /// Nodes a large removal took out of the scene, to be freed a chunk
    /// at a time after frames (`Ui::reclaim`): freeing a page of tens of
    /// thousands of nodes takes tens of milliseconds at once.
    dying: RefCell<std::collections::VecDeque<NodeId>>,
    /// A large subtree was removed since the last frame was prepared.
    removed: Cell<bool>,
    editors: RefCell<crate::scene::NodeMap<EditorHandle>>,
    /// What takes a person's paste, for the few editors asked to (`View::on_paste`).
    paste_hooks: RefCell<crate::scene::NodeMap<PasteHook>>,
    editor_layout: RefCell<Option<EditorLayoutEntry>>,
    editor_layout_revision: Cell<Option<u64>>,
    bounds_geometry_revision: Cell<Option<u64>>,
    preparing_frame: Cell<bool>,
    /// A host keeps going past layout that doesn't settle
    /// (`Ui::contain_layout_feedback`).
    contains_layout: Cell<bool>,
    /// Layout that didn't settle, held as it was: the scene's layout and
    /// geometry revisions then (`Ui::hold_layout`).
    unsettled: Cell<Option<(u64, u64)>>,
    /// Where focus falls once the frame settles, if nothing took it: the
    /// nearest focusable ancestor of a removed focused node (`Ui::remove`).
    refocus: Cell<Option<NodeId>>,
    content_sizes: RefCell<crate::scene::NodeMap<Signal<(f32, f32)>>>,
    /// Nodes' laid-out sizes (`observe_size`): read only when layout runs.
    observed_sizes: RefCell<crate::scene::NodeMap<Signal<(f32, f32)>>>,
    observed_bounds: RefCell<crate::scene::NodeMap<Signal<Rect>>>,
    observed_visibility: RefCell<crate::scene::NodeMap<Signal<bool>>>,
    /// Run once after the next frame preparation's layout.
    after_layout: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// Run after every layout pass that changes the layout, by owning node.
    layout_listeners: RefCell<crate::scene::NodeMap<Vec<LayoutListener>>>,
    mount_initialization: RefCell<Option<Rc<MountInitialization>>>,
    /// Views mounting now, nested (`Ui::mounting`).
    mounting: Cell<u32>,
    /// Elements mounting now, outermost first (`Ui::open_element`).
    open_elements: RefCell<Vec<NodeId>>,
    interaction: RefCell<Option<InteractionDeadline>>,
    advancing_interaction: Cell<bool>,
    window_gesture: Cell<Option<WindowGesture>>,
    /// An editor a middle press put its caret in, for the host to paste the
    /// primary selection into (`Ui::take_primary_paste`).
    primary_paste: Cell<Option<NodeId>>,
    /// A middle press pastes the primary selection (`Ui::set_primary_paste`).
    primary_paste_on: Cell<bool>,
    /// Display text's selection (`View::selectable`).
    selection: RefCell<crate::selection::State>,
    /// Virtual lists' scroll-to-row requests, by list (`NodeHandle::scroll_to_row`).
    list_reveals: RefCell<std::collections::HashMap<NodeId, Signal<crate::compose::RowReveal>>>,
    /// Find in page, while open (`find`).
    pub(crate) find: RefCell<Option<crate::find::Session>>,
    /// The app turned find in page off.
    pub(crate) find_off: Cell<bool>,
    /// Virtual lists' rows' indices from their places, by list.
    pub(crate) list_rows: RefCell<std::collections::HashMap<NodeId, crate::find::RowIndex>>,
    /// What the last primary press went to: the keyboard scrolls the scroll
    /// view around it (`scroll_keys`).
    last_pressed: Cell<Option<NodeId>>,
    /// Elastic scrolling (`Ui::set_elastic_scrolling`): off, never read.
    elastic: RefCell<crate::elastic::State>,
    /// Scrollbars hide until their view scrolls (`Ui::set_overlay_scrollbars`).
    overlay_scrollbars: Signal<bool>,
    /// Editors' caret width in points (`Ui::set_caret_width`).
    caret_width: Rc<Cell<f32>>,
    /// The focused editor's caret blinking (`Ui::set_caret_blink`).
    caret_blink: Rc<crate::caret_blink::Blink>,
    /// Assistive technology reads the semantics (`Ui::set_assistive_technology`).
    assistive: Signal<bool>,
    /// The scene's pixel scale as frames last found it (`Ui::pixel_scale`),
    /// and the same in a cell: what a frame compares, borrowing nothing.
    pixel_scale: Signal<f32>,
    pixel_scale_seen: Cell<f32>,
    /// Text nodes whose labels (and the buttons they name) wait for
    /// assistive technology: changed while none read the semantics.
    pending_labels: RefCell<crate::scene::NodeMap<()>>,
    /// At how many pending labels the removed nodes' are dropped: twice as
    /// many as were left last time, so they stay as many as text nodes live.
    pending_prune: Cell<usize>,
    /// `assistive`'s value, read where a text changes (`defer_label`).
    listening: Cell<bool>,
    /// Overlay scrollbars shown, to fade once their view stops scrolling.
    scrollbar_fades: RefCell<Vec<std::rc::Weak<crate::compose_scrollbar::Fade>>>,
    /// Input is being dispatched: what scrolls now, the user scrolled.
    dispatching: Cell<bool>,
    /// A platform's wheel is being dispatched (`Ui::wheel_in_window`).
    wheel_in_window: Cell<bool>,
    /// What hears every event the document gets (`Ui::on_document_event`):
    /// usually nothing.
    document_listeners: RefCell<Vec<(u64, DocumentListener)>>,
}
type DocumentListener = Rc<RefCell<dyn FnMut(&InputEvent)>>;
/// Ends a document listener as its owner goes.
struct DocumentListening {
    storage: std::rc::Weak<UiStorage>,
    token: u64,
}
impl Drop for DocumentListening {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.upgrade()
            && let Ok(mut listeners) = storage.document_listeners.try_borrow_mut()
        {
            listeners.retain(|(token, _)| *token != self.token);
        }
    }
}
/// A press in a window drag region (`View::window_drag`), for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowGesture {
    /// Move the window with the pointer: the press is still down.
    Drag,
    /// The second press of a double click: zoom or minimize the window, as
    /// the platform's setting for its title bar says.
    DoubleClick,
    /// A secondary click: the window's menu (restore, move, size, minimize,
    /// maximize, close), as a right click on a title bar shows it on Windows
    /// and Linux. macOS has no such menu.
    Menu,
}
type LayoutListener = Rc<RefCell<Box<dyn FnMut()>>>;
/// A handle to a `Ui` that does not keep it alive (`Ui::downgrade`).
#[derive(Clone)]
pub struct WeakUi {
    storage: Weak<UiStorage>,
}
impl WeakUi {
    /// The `Ui`, if it still exists.
    pub fn upgrade(&self) -> Option<Ui> {
        self.storage.upgrade().map(Ui::from_storage)
    }
    /// What the `Ui` is made from, if it still exists: for a path that
    /// needs only its scene or themes, without a `Ui`'s handles made and
    /// dropped (`Ui::from_storage` makes one).
    pub(crate) fn upgrade_storage(&self) -> Option<Rc<UiStorage>> {
        self.storage.upgrade()
    }
}
/// Restores the previous mount theme scope (`Ui::enter_theme`), unless
/// entering changed nothing (the usual case: a child of an element mounts
/// in its parent's scope).
pub(crate) struct ThemeGuard {
    restore: Option<(Weak<UiStorage>, Option<Rc<ThemeScope>>)>,
}
impl Drop for ThemeGuard {
    fn drop(&mut self) {
        if let Some((storage, previous)) = self.restore.take()
            && let Some(storage) = storage.upgrade()
        {
            let restored = storage.mount_theme.replace(previous);
            drop(restored);
        }
    }
}
/// Ownership cleanup for the initial bindings of one public component mount.
/// It does not roll back model writes or catch later reactive update failures.
pub(crate) struct MountInitialization {
    ui: WeakUi,
    root: Cell<Option<NodeId>>,
    failed: Cell<bool>,
}
impl MountInitialization {
    pub(crate) fn set_root(&self, root: NodeId) {
        self.root.set(Some(root));
    }
    pub(crate) fn enter(self: &Rc<Self>) -> MountInitializationScope {
        let previous = self
            .ui
            .upgrade()
            .and_then(|ui| ui.storage.mount_initialization.replace(Some(self.clone())));
        MountInitializationScope {
            token: self.clone(),
            previous,
        }
    }
    fn fail(&self) {
        if self.failed.replace(true) {
            return;
        }
        if let Some(mut ui) = self.ui.upgrade()
            && let Some(root) = self.root.get()
        {
            ui.remove(root);
        }
    }
}
pub(crate) struct MountInitializationScope {
    token: Rc<MountInitialization>,
    previous: Option<Rc<MountInitialization>>,
}
impl Drop for MountInitializationScope {
    fn drop(&mut self) {
        if let Some(ui) = self.token.ui.upgrade() {
            let old = ui
                .storage
                .mount_initialization
                .replace(self.previous.take());
            drop(old);
        }
    }
}
struct InitialBindingGuard {
    token: Rc<MountInitialization>,
    scope: Option<MountInitializationScope>,
    completed: bool,
}
impl Drop for InitialBindingGuard {
    fn drop(&mut self) {
        // User resource destructors can mount new siblings during cleanup.
        // They must not inherit the failed initialization scope.
        drop(self.scope.take());
        if !self.completed && std::thread::panicking() {
            self.token.fail();
        }
    }
}

impl Drop for UiStorage {
    fn drop(&mut self) {
        let owned = std::mem::take(self.owned.get_mut());
        let editors = std::mem::take(self.editors.get_mut());
        for id in owned.keys() {
            self.input.unregister(*id);
        }
        drop(owned);
        drop(editors);
        self.input.validate_drag(&self.scene);
    }
}
impl Ui {
    /// A `width` by `height` document on a runtime of its own.
    pub fn new(width: f32, height: f32) -> Self {
        Self::with_runtime(Runtime::new(), width, height)
    }
    /// A document on `runtime`, which other documents may share: an app's
    /// windows share one, so the state one shows the others follow (a
    /// settings window's switch, every window's theme).
    pub fn with_runtime(runtime: Runtime, width: f32, height: f32) -> Self {
        // `ZGX_ASSISTIVE=0`: headless as a desktop starts, no reader (the
        // gate's scenes, which measure what users get).
        let listening = crate::env::var_os("ZGX_ASSISTIVE").is_none_or(|v| v != "0");
        let storage = Rc::new(UiStorage {
            themes: Themes::of(&runtime),
            mount_theme: RefCell::new(None),
            text_geometry_revision: runtime.signal(0),
            presented: runtime.signal(true),
            window_active: RefCell::default(),
            overlay_scrollbars: runtime.signal(false),
            caret_width: Rc::new(Cell::new(1.)),
            caret_blink: Rc::default(),
            assistive: runtime.signal(listening),
            pixel_scale: runtime.signal(0.),
            pixel_scale_seen: Cell::new(0.),
            pending_labels: RefCell::default(),
            pending_prune: Cell::new(1024),
            listening: Cell::new(listening),
            popup_screens: runtime.signal(Vec::new()),
            popup_limits: runtime.signal(Vec::new()),
            popup_flips: runtime.signal(Vec::new()),
            popups: RefCell::new(Vec::new()),
            portals: RefCell::new(Vec::new()),
            popup_soon: RefCell::new(None),
            popup_soon_sent: Cell::new(false),
            snapshotter: RefCell::new(None),
            popup_snapshotter: RefCell::new(None),
            zoom: Cell::new(1.),
            zoom_off: Cell::new(false),
            zoom_host: RefCell::new(Vec::new()),
            zoom_wheel: Cell::new(0.),
            fixed_zooms: RefCell::new(Vec::new()),
            wheel_taper: RefCell::new(None),
            refresh_interval: Cell::new(std::time::Duration::from_micros(16_667)),
            popup_previews: Cell::new(false),
            runtime,
            scene: Rc::new(RefCell::new(Scene::new(width, height))),
            input: InputDispatcher::new(),
            semantics: Rc::new(RefCell::new(Semantics::new())),
            owned: RefCell::default(),
            dying: RefCell::default(),
            removed: Cell::new(false),
            editors: RefCell::default(),
            paste_hooks: RefCell::default(),
            editor_layout: RefCell::new(None),
            editor_layout_revision: Cell::new(None),
            bounds_geometry_revision: Cell::new(None),
            preparing_frame: Cell::new(false),
            contains_layout: Cell::new(false),
            unsettled: Cell::new(None),
            refocus: Cell::new(None),
            content_sizes: RefCell::default(),
            observed_sizes: RefCell::default(),
            observed_bounds: RefCell::default(),
            observed_visibility: RefCell::default(),
            after_layout: RefCell::new(Vec::new()),
            layout_listeners: RefCell::default(),
            mount_initialization: RefCell::new(None),
            mounting: Cell::new(0),
            open_elements: RefCell::default(),
            interaction: RefCell::new(None),
            advancing_interaction: Cell::new(false),
            window_gesture: Cell::new(None),
            primary_paste: Cell::new(None),
            primary_paste_on: Cell::new(true),
            selection: Default::default(),
            list_reveals: Default::default(),
            find: RefCell::new(None),
            find_off: Cell::new(false),
            list_rows: Default::default(),
            last_pressed: Cell::new(None),
            elastic: Default::default(),
            scrollbar_fades: RefCell::new(Vec::new()),
            dispatching: Cell::new(false),
            wheel_in_window: Cell::new(false),
            document_listeners: RefCell::new(Vec::new()),
        });
        storage.input.set_semantics(&storage.semantics);
        Self::from_storage(storage)
    }
    pub(crate) fn from_storage(storage: Rc<UiStorage>) -> Self {
        Self {
            runtime: storage.runtime.clone(),
            scene: storage.scene.clone(),
            input: storage.input.clone(),
            semantics: storage.semantics.clone(),
            storage,
        }
    }
    /// The runtime's themes: switch, load and register themes, declare app
    /// tokens (see [`crate::theme`]).
    pub fn theme(&self) -> &Rc<Themes> {
        &self.storage.themes
    }
    /// The theme scope widgets mounting now style themselves from: the
    /// enclosing `theme_scope`'s, else the root.
    pub fn theme_scope(&self) -> Rc<ThemeScope> {
        self.storage
            .mount_theme
            .borrow()
            .clone()
            .unwrap_or_else(|| self.storage.themes.root().clone())
    }
    /// Mount under `scope` until the guard drops.
    pub(crate) fn enter_theme(&self, scope: &Rc<ThemeScope>) -> ThemeGuard {
        let current = self.storage.mount_theme.borrow();
        if current
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, scope))
        {
            return ThemeGuard { restore: None };
        }
        drop(current);
        ThemeGuard {
            restore: Some((
                Rc::downgrade(&self.storage),
                self.storage.mount_theme.replace(Some(scope.clone())),
            )),
        }
    }
    pub(crate) fn begin_mount_initialization(&self) -> Rc<MountInitialization> {
        Rc::new(MountInitialization {
            ui: self.downgrade(),
            root: Cell::new(None),
            failed: Cell::new(false),
        })
    }
    pub(crate) fn shared(&self) -> Self {
        Self::from_storage(self.storage.clone())
    }
    pub(crate) fn selection_state(&self) -> &RefCell<crate::selection::State> {
        &self.storage.selection
    }
    /// Virtual lists' scroll-to-row requests, by the list's node.
    pub(crate) fn list_reveals(
        &self,
    ) -> &RefCell<std::collections::HashMap<NodeId, Signal<crate::compose::RowReveal>>> {
        &self.storage.list_reveals
    }
    /// What the last primary press went to, while it is mounted.
    pub(crate) fn last_pressed(&self) -> Option<NodeId> {
        self.storage
            .last_pressed
            .get()
            .filter(|id| self.scene.borrow().contains(*id))
    }
    /// After the document had `event`: what the selection and keyboard
    /// scrolling make of it.
    fn after_dispatch(
        &self,
        event: (
            crate::selection::SelectionEvent,
            crate::scroll_keys::ScrollKey,
            bool,
        ),
        result: &mut crate::input::DispatchResult,
    ) {
        let (selection, scroll, press) = event;
        if press {
            self.storage.last_pressed.set(result.target);
        }
        self.selection_after_dispatch(selection, result);
        self.scroll_after_dispatch(scroll, result);
    }
    /// After the document had key `event`: find in page's keys, if nothing
    /// took it.
    fn find_after_dispatch(&self, event: &InputEvent, result: &mut crate::input::DispatchResult) {
        if crate::find::after_dispatch(self, event, result.default_prevented) {
            result.default_prevented = true;
        }
    }
    /// A handle that does not keep the document alive.
    pub fn downgrade(&self) -> WeakUi {
        WeakUi {
            storage: Rc::downgrade(&self.storage),
        }
    }
    pub(crate) fn retain(&mut self, owner: NodeId, resource: impl Any) {
        self.storage
            .owned
            .borrow_mut()
            .entry(owner)
            .or_default()
            .resources
            .push(Box::new(resource));
    }
    /// The document's root node.
    pub fn root(&self) -> NodeId {
        self.scene.borrow().root()
    }
    /// A signal holding `value`, on this document's runtime.
    pub fn signal<T: 'static>(&self, value: T) -> Signal<T> {
        self.runtime.signal(value)
    }
    /// Append a container laid out by `layout` under `parent`.
    pub fn container(&mut self, parent: NodeId, layout: Layout, style: Style) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Container(layout), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
    /// Append a line of text under `parent`, in the theme's text colour and
    /// size, which assistive technology reads as a label.
    pub fn label(&mut self, parent: NodeId, text: impl Into<String>, style: Style) -> NodeId {
        let text = text.into();
        let theme = self.theme_scope();
        let n = self.scene.borrow_mut().append(
            parent,
            NodeKind::Text {
                text: text.as_str().into(),
                color: theme.color_untracked(tok::TEXT_PRIMARY),
                font_size: theme.number_untracked(tok::FONT_SIZE_MD),
            },
            style,
        );
        self.semantics
            .borrow_mut()
            .set(n, SemanticNode::new(Role::Label, text));
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
    /// A [`Ui::label`] whose text follows `value`.
    pub fn label_signal(&mut self, parent: NodeId, value: Signal<String>, style: Style) -> NodeId {
        let n = self.label(parent, "", style);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        self.bind(n, move || {
            value.with(|v| {
                scene.borrow_mut().set_text(n, v.as_str());
                a.borrow_mut().set_label(n, v);
            });
        });
        n
    }
    /// Run `f` now and whenever what it reads changes, for as long as `owner`
    /// is mounted.
    pub fn bind(&mut self, owner: NodeId, f: impl FnMut() + 'static) {
        self.bind_marked(owner, false, f);
    }
    /// [`Ui::bind`]. `mount_pure`: `f` reads no layout, so a mount's
    /// deferred input draws need not be made before it runs
    /// (`watch_geometry`).
    pub(crate) fn bind_marked(
        &mut self,
        owner: NodeId,
        mount_pure: bool,
        mut f: impl FnMut() + 'static,
    ) {
        assert!(
            self.scene.borrow().contains(owner),
            "cannot bind a removed UI node"
        );
        let mut initial = self.storage.mount_initialization.borrow().clone();
        let callback: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(move || {
            let Some(token) = initial.take() else {
                f();
                return;
            };
            if token.failed.get()
                || token
                    .ui
                    .upgrade()
                    .is_none_or(|ui| !ui.scene.borrow().contains(owner))
            {
                return;
            }
            let scope = token.enter();
            let mut guard = InitialBindingGuard {
                token,
                scope: Some(scope),
                completed: false,
            };
            f();
            guard.completed = true;
        }));
        self.runtime.clone().batch(|| {
            let effect = self.runtime.effect_callback(mount_pure, callback);
            #[cfg(feature = "trace")]
            if crate::trace::enabled() {
                crate::trace::name(effect.number(), format!("@{} binding", owner.number()));
            }
            self.storage
                .owned
                .borrow_mut()
                .entry(owner)
                .or_default()
                .effects
                .push(effect);
        });
    }
    /// [`Ui::bind`] for a text input's model or typography: `mount_pure`,
    /// as it reads no geometry before its draw lays out (`watch_geometry`).
    pub(crate) fn bind_input(&mut self, owner: NodeId, f: impl FnMut() + 'static) {
        self.bind_marked(owner, true, f);
        INPUT_DRAWS.with_borrow_mut(|draws| {
            if let Some(draws) = &mut **draws
                && draws.storage.as_ptr() == Rc::as_ptr(&self.storage)
            {
                draws.bound = true;
            }
        });
    }
    /// Listen for input at `node` for as long as it is mounted. `focusable`
    /// applies when this is the node's first listener.
    pub fn on_event(
        &mut self,
        node: NodeId,
        focusable: bool,
        f: impl FnMut(&mut crate::input::EventContext) + 'static,
    ) {
        let b = if self.input.has_listeners(node) {
            self.input.listen(node, f)
        } else {
            let mut options = self.input.options(node).unwrap_or_default();
            options.focusable = focusable;
            self.input.register(node, options, f)
        };
        self.storage
            .owned
            .borrow_mut()
            .entry(node)
            .or_default()
            .bindings
            .push(b);
    }
    fn rect(&mut self, parent: NodeId, color: Color, w: f32, h: f32, x: f32, y: f32) -> NodeId {
        let mut s = self.scene.borrow_mut();
        let id = s.append(parent, NodeKind::Rect(color), fixed(w, h));
        s.set_transform(id, Transform { x, y });
        id
    }
    /// A control's root and background. The background shows `rest`, or
    /// what the returned state selects (`HOVERED`, `ACCENTED`), from the
    /// theme, and follows theme changes.
    fn control(
        &mut self,
        parent: NodeId,
        width: f32,
        height: f32,
        rest: ColorToken,
    ) -> (NodeId, NodeId, Signal<u8>) {
        let root = self.container(parent, Layout::Overlay, fixed(width, height));
        let theme = self.theme_scope();
        let bg = self.rect(root, theme.color_untracked(rest), width, height, 0., 0.);
        let state = self.signal(RESTING);
        let scene = self.scene.clone();
        let read = state.clone();
        self.bind(root, move || {
            let color = match read.get() {
                RESTING => rest,
                HOVERED => tok::BUTTON_HOVER,
                _ => tok::ACCENT,
            };
            let kind = surface(&theme, theme.color(color));
            scene.borrow_mut().set_kind(bg, kind);
        });
        (root, bg, state)
    }
    /// Append a button labelled `label`, `width` wide, that runs `clicked`
    /// when activated by pointer or keyboard.
    pub fn button(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        width: f32,
        mut clicked: impl FnMut() + 'static,
    ) -> NodeId {
        let label = label.into();
        let height = self.theme_scope().number_untracked(tok::CONTROL_HEIGHT);
        let (root, _, state) = self.control(parent, width, height, tok::BUTTON_BG);
        let text = self.label(root, &label, fixed((width - 20.).max(0.), height - 8.));
        self.scene
            .borrow_mut()
            .set_transform(text, Transform { x: 10., y: 6. });
        self.semantics.borrow_mut().remove(text);
        self.semantics
            .borrow_mut()
            .set(root, SemanticNode::new(Role::Button, label));
        let focused = Rc::new(RefCell::new(false));
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            match cx.event {
                InputEvent::Activate if !cx.default_prevented() => clicked(),
                InputEvent::PointerEnter => {
                    state.set(HOVERED);
                }
                InputEvent::PointerLeave => {
                    state.set(if *focused.borrow() { ACCENTED } else { RESTING });
                }
                InputEvent::Focus => {
                    *focused.borrow_mut() = true;
                    state.set(ACCENTED);
                }
                InputEvent::Blur => {
                    *focused.borrow_mut() = false;
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    /// Append a checkbox labelled `label` that toggles `value`.
    pub fn checkbox(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<bool>,
        width: f32,
    ) -> NodeId {
        let label = label.into();
        let height = self.theme_scope().number_untracked(tok::CONTROL_HEIGHT);
        let (root, _, state) = self.control(parent, width, height, tok::INPUT_BG);
        // The box, drawn as the platform draws one, then the label.
        let size = crate::compose_checkbox::SIZE;
        let theme = self.theme_scope();
        let (indicator, mark) = {
            let mut scene = self.scene.borrow_mut();
            let indicator = scene.append(
                root,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: Default::default(),
                },
                fixed(size, size),
            );
            scene.set_transform(
                indicator,
                Transform {
                    x: 8.,
                    y: ((height - size) / 2.).round(),
                },
            );
            let mark = scene.append(
                indicator,
                NodeKind::Canvas(Default::default()),
                fixed(size, size),
            );
            (indicator, mark)
        };
        let text = self.label(root, label.clone(), fixed((width - 40.).max(0.), 28.));
        self.scene
            .borrow_mut()
            .set_transform(text, Transform { x: 32., y: 6. });
        self.semantics.borrow_mut().remove(text);
        self.semantics
            .borrow_mut()
            .set(root, SemanticNode::new(Role::CheckBox, &label));
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        let v = value.clone();
        self.bind(root, move || {
            let checked = v.get();
            let (fill, edge) = if checked {
                (theme.color(tok::ACCENT), theme.color(tok::ACCENT))
            } else {
                (theme.color(tok::INPUT_BG), theme.color(tok::BORDER_STRONG))
            };
            let mut scene = scene.borrow_mut();
            scene.set_kind(
                indicator,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: crate::scene::QuadStyle {
                        fill,
                        radius: 4.,
                        border_color: edge,
                        border_width: 1.,
                        ..Default::default()
                    },
                },
            );
            let drawn = crate::compose_checkbox::mark(
                size,
                false,
                if checked { 1. } else { 0. },
                theme.color(tok::ACCENT_TEXT),
            );
            scene.set_kind(mark, NodeKind::Canvas(std::sync::Arc::new(drawn)));
            drop(scene);
            a.borrow_mut().update(root, |s| s.checked = Some(checked));
        });
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            match cx.event {
                InputEvent::Activate if !cx.default_prevented() => {
                    value.update(|v| *v = !*v);
                }
                InputEvent::Focus => {
                    state.set(HOVERED);
                }
                InputEvent::Blur => {
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    /// Values are normalized to the finite range: NaN becomes its minimum,
    /// infinities and out-of-range finite values clamp to the nearest endpoint.
    pub fn slider(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<f32>,
        range: std::ops::RangeInclusive<f32>,
        width: f32,
    ) -> NodeId {
        let min = *range.start();
        let max = *range.end();
        assert!(
            min.is_finite() && max.is_finite() && min < max,
            "slider range must be finite and increasing"
        );
        let span = max as f64 - min as f64;
        // `rtl`: measured from the right, where it starts.
        let at_position = move |position: f32, origin: f32, width: f32, rtl: bool| {
            let from_start = if rtl {
                origin as f64 + width as f64 - position as f64
            } else {
                position as f64 - origin as f64
            };
            let fraction = ((from_start - 8.) / (width as f64 - 26.).max(1.)).clamp(0., 1.);
            (min as f64 + fraction * span).clamp(min as f64, max as f64) as f32
        };
        let theme = self.theme_scope();
        let height = theme.number_untracked(tok::CONTROL_HEIGHT);
        let (root, bg, state) = self.control(parent, width, height, tok::INPUT_BG);
        let dimensions = self.observe_content_size(root);
        let fill = self.rect(root, theme.color_untracked(tok::ACCENT), 0., 4., 8., 16.);
        let knob = self.rect(
            root,
            theme.color_untracked(tok::TEXT_PRIMARY),
            10.,
            20.,
            8.,
            8.,
        );
        self.theme_rects(
            root,
            theme,
            &[(fill, tok::ACCENT), (knob, tok::TEXT_PRIMARY)],
        );
        let mut sem = SemanticNode::new(Role::Slider, label);
        sem.min = Some(min as f64);
        sem.max = Some(max as f64);
        self.semantics.borrow_mut().set(root, sem);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        let v = value.clone();
        self.bind(root, move || {
            let raw = v.get();
            let val = if raw.is_nan() {
                min
            } else {
                raw.clamp(min, max)
            };
            if raw != val {
                v.set(val);
            }
            let (width, height) = dimensions.get();
            let x = ((val as f64 - min as f64) / span * (width as f64 - 26.).max(1.)) as f32;
            let mut s = scene.borrow_mut();
            // Right to left it fills from the right; the scene lays each
            // part in at the right, so their translations are from there.
            let rtl = s.direction(root) == crate::scene::Direction::Rtl;
            s.set_style(bg, fixed(width, height));
            s.set_style(fill, fixed(x, 4.));
            s.set_transform(
                fill,
                Transform {
                    x: if rtl { -8. } else { 8. },
                    y: (height - 4.).max(0.) / 2.,
                },
            );
            s.set_transform(
                knob,
                Transform {
                    x: if rtl { -8. - x } else { x + 8. },
                    y: (height - 20.).max(0.) / 2.,
                },
            );
            a.borrow_mut()
                .update(root, |n| n.numeric_value = Some(val as f64));
        });
        let scene = self.scene.clone();
        let mut dragging = false;
        let rtl = move |scene: &Scene| scene.direction(root) == crate::scene::Direction::Rtl;
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            if cx.default_prevented()
                && !matches!(
                    cx.event,
                    InputEvent::PointerUp { .. }
                        | InputEvent::PointerCancel
                        | InputEvent::Blur
                        | InputEvent::Focus
                )
            {
                return;
            }
            match &cx.event {
                InputEvent::SetNumericValue(v) if v.is_finite() => {
                    value.set((*v as f32).clamp(min, max));
                    cx.prevent_default();
                }
                InputEvent::SetValue(v) => {
                    if let Ok(v) = v.parse::<f32>()
                        && v.is_finite()
                    {
                        value.set(v.clamp(min, max));
                        cx.prevent_default();
                    }
                }
                InputEvent::Increment | InputEvent::Decrement => {
                    let direction = if matches!(cx.event, InputEvent::Increment) {
                        1.
                    } else {
                        -1.
                    };
                    value.set((value.get() + direction * (span / 100.) as f32).clamp(min, max));
                    cx.prevent_default();
                }

                InputEvent::PointerDown {
                    x,
                    button: PointerButton::Primary,
                    ..
                } => {
                    dragging = true;
                    let x = *x;
                    cx.capture_pointer();
                    let (origin, width) = slider_content_bounds(&scene.borrow(), root);
                    let rtl = rtl(&scene.borrow());
                    value.set(at_position(x, origin, width, rtl));
                }
                InputEvent::PointerMove { x, .. } if dragging => {
                    let (origin, width) = slider_content_bounds(&scene.borrow(), root);
                    let rtl = rtl(&scene.borrow());
                    value.set(at_position(*x, origin, width, rtl));
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    dragging = false;
                    cx.release_pointer();
                }
                InputEvent::KeyDown { key, .. } => {
                    let step = (span / 100.) as f32;
                    // Left and right go the way it shows.
                    let rtl = rtl(&scene.borrow());
                    let right = if rtl { -step } else { step };
                    let next = match key {
                        Key::ArrowLeft => Some(value.get() - right),
                        Key::ArrowRight => Some(value.get() + right),
                        Key::ArrowDown => Some(value.get() - step),
                        Key::ArrowUp => Some(value.get() + step),
                        Key::Home => Some(min),
                        Key::End => Some(max),
                        _ => None,
                    };
                    if let Some(n) = next {
                        value.set(n.clamp(min, max));
                        cx.prevent_default();
                    }
                }
                InputEvent::Focus => {
                    state.set(HOVERED);
                }
                InputEvent::Blur => {
                    dragging = false;
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    /// Append a progress bar `width` wide showing `value` from 0 to 1 (NaN
    /// shows as 0).
    pub fn progress(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<f32>,
        width: f32,
    ) -> NodeId {
        let (root, bg, _) = self.control(parent, width, 8., tok::INPUT_BG);
        let dimensions = self.observe_content_size(root);
        let theme = self.theme_scope();
        let fill = self.rect(root, theme.color_untracked(tok::ACCENT), 0., 8., 0., 0.);
        self.theme_rects(root, theme, &[(fill, tok::ACCENT)]);
        let mut sem = SemanticNode::new(Role::Progress, label);
        sem.min = Some(0.);
        sem.max = Some(1.);
        self.semantics.borrow_mut().set(root, sem);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        self.bind(root, move || {
            let raw = value.get();
            let n = if raw.is_nan() { 0. } else { raw.clamp(0., 1.) };
            if raw != n {
                value.set(n);
            }
            let (width, height) = dimensions.get();
            let mut scene = scene.borrow_mut();
            scene.set_style(bg, fixed(width, height));
            scene.set_style(fill, fixed(width * n, height));
            a.borrow_mut()
                .update(root, |s| s.numeric_value = Some(n as f64));
        });
        root
    }
    /// Disable `node` or enable it again: a disabled node takes no input or
    /// focus, and assistive technology reports it disabled.
    pub fn set_disabled(&mut self, node: NodeId, disabled: bool) {
        self.input.set_disabled(&self.scene, node, disabled);
        // Cleanup and observer callbacks may remove the node or change its state.
        if !self.scene.borrow().contains(node) {
            return;
        }
        let disabled = self
            .input
            .options(node)
            .is_some_and(|options| options.disabled);
        if self.semantics.borrow().get(node).is_none() {
            self.semantics
                .borrow_mut()
                .set(node, SemanticNode::new(Role::Group, ""));
        }
        self.semantics
            .borrow_mut()
            .update(node, |n| n.disabled = disabled);
        let mut s = self.scene.borrow_mut();
        let mut effects = s.effects(node);
        effects.opacity = if disabled { 0.45 } else { 1. };
        s.set_effects(node, effects);
    }
    /// Observe allocated content dimensions without subscribing to model changes.
    /// The registration belongs to the retained node and is removed with it.
    pub(crate) fn observe_content_size(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.content_sizes.borrow().get(&node) {
            return signal.clone();
        }
        // While views mount, the frame's layout pass gives it its size,
        // once for the whole tree: laying out here would do it per node,
        // and inside the mount's recursion.
        if self.storage.mounting.get() == 0 {
            self.scene.borrow_mut().prepare_layout();
        }
        let size = allocated_content_size(&self.scene.borrow(), node);
        let signal = self.signal(size);
        self.storage
            .content_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// How many views are mounting now, each inside the last (see
    /// `observe_content_size`).
    pub(crate) fn mount_depth(&self) -> u32 {
        self.storage.mounting.get()
    }
    pub(crate) fn set_mount_depth(&self, depth: u32) {
        self.storage.mounting.set(depth);
    }
    /// `node`'s children are mounting, until `close_element`.
    pub(crate) fn open_element(&self, node: NodeId) {
        self.storage.open_elements.borrow_mut().push(node);
    }
    /// `node` is mounted, with whatever opened inside it and never closed
    /// (a child's mount that failed).
    pub(crate) fn close_element(&self, node: NodeId) {
        let mut open = self.storage.open_elements.borrow_mut();
        while let Some(top) = open.pop() {
            if top == node {
                break;
            }
        }
    }
    /// The innermost element whose children are mounting.
    pub(crate) fn innermost_open_element(&self) -> Option<NodeId> {
        self.storage.open_elements.borrow().last().copied()
    }
    /// Register newly mounted intrinsic content without forcing a layout for
    /// every row. The sentinel is replaced by the next prepared layout.
    pub(crate) fn observe_content_size_deferred(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.content_sizes.borrow().get(&node) {
            return signal.clone();
        }
        let signal = self.signal((f32::NAN, f32::NAN));
        self.storage
            .content_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// Observe the node's laid-out size (its border box), published after
    /// each layout that runs: unlike `observe_bounds`, nothing an ancestor
    /// does (scrolling, translating) costs it anything. The registration
    /// belongs to the node and goes with it.
    /// How many nodes each geometry observer list holds (content sizes,
    /// sizes, bounds, visibility): for diagnostics, and tests that what
    /// unmounts lets go.
    pub fn observer_counts(&self) -> [usize; 4] {
        [
            self.storage.content_sizes.borrow().len(),
            self.storage.observed_sizes.borrow().len(),
            self.storage.observed_bounds.borrow().len(),
            self.storage.observed_visibility.borrow().len(),
        ]
    }
    pub(crate) fn observe_size(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.observed_sizes.borrow().get(&node) {
            return signal.clone();
        }
        let bounds = self.scene.borrow().layout_bounds(node);
        let signal = self.signal((bounds.width, bounds.height));
        self.storage
            .observed_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// Observe world bounds, including ancestor translations without layout work.
    /// Like content-size observations, registrations are owned by their nodes.
    pub(crate) fn observe_bounds(&self, node: NodeId) -> Signal<Rect> {
        if let Some(signal) = self.storage.observed_bounds.borrow().get(&node) {
            return signal.clone();
        }
        // Current bounds, possibly before this frame's layout: the frame's
        // layout publishes them. Laying out here, mid-mount, would measure
        // text before its inherited styles apply.
        let signal = self.signal(self.scene.borrow().bounds(node));
        self.storage
            .observed_bounds
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.bounds_geometry_revision.set(None);
        signal
    }
    /// Run `f` once after the next frame preparation lays out, before the
    /// frame is drawn, whether or not the layout changed.
    pub(crate) fn after_layout(&self, f: Box<dyn FnOnce()>) {
        self.storage.after_layout.borrow_mut().push(f);
    }
    /// Run `f` after every layout pass that changes the layout, until `node`
    /// is removed.
    pub(crate) fn on_layout(&self, node: NodeId, f: Box<dyn FnMut()>) {
        self.storage
            .layout_listeners
            .borrow_mut()
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(f)));
    }
    /// Whether any after-layout work ran.
    fn run_after_layout(&self) -> bool {
        let pending = std::mem::take(&mut *self.storage.after_layout.borrow_mut());
        if pending.is_empty() {
            return false;
        }
        self.runtime.batch(|| {
            for f in pending {
                f();
            }
        });
        true
    }
    fn run_layout_listeners(&self) {
        let listeners: Vec<(NodeId, LayoutListener)> = {
            let listeners = self.storage.layout_listeners.borrow();
            if listeners.is_empty() {
                return;
            }
            listeners
                .iter()
                .flat_map(|(node, list)| list.iter().map(|f| (*node, f.clone())))
                .collect()
        };
        self.runtime.batch(|| {
            for (node, listener) in listeners {
                // One an earlier listener removed stays quiet; one already
                // running (it reentered a layout pass) is not called again.
                if self.scene.borrow().contains(node)
                    && let Ok(mut f) = listener.try_borrow_mut()
                {
                    f();
                }
            }
        });
    }
    /// Inform retained media that its native window can currently present.
    /// Headless hosts default to presented; hidden/suspended hosts should set false.
    pub fn set_presented(&self, presented: bool) {
        if presented != self.is_presented() {
            crate::profile::note(|| {
                if presented {
                    "the window presents again".into()
                } else {
                    "the window stopped presenting (hidden, minimized or covered): it draws no \
                     frames, and input waits for it"
                        .into()
                }
            });
        }
        self.storage.presented.set(presented);
    }
    /// Draw scrollbars as macOS's overlay ones (and GTK's) are: hidden until
    /// the user scrolls their view, then fading out a second after it stops,
    /// unless the pointer is on one. Off (the default), a scrollbar shows
    /// whenever its view has something to scroll. The desktop host follows
    /// the system's setting (macOS's "Show scroll bars").
    pub fn set_overlay_scrollbars(&self, overlay: bool) {
        self.storage.overlay_scrollbars.set(overlay);
    }
    pub(crate) fn overlay_scrollbars(&self) -> Signal<bool> {
        self.storage.overlay_scrollbars.clone()
    }
    /// How wide editors draw their caret, in points: the system's text
    /// cursor thickness, which the desktop host gives (Windows'
    /// `SPI_GETCARETWIDTH`, which people who lose a thin caret raise); 1
    /// until it does. Held between 1 and 20; NaN leaves it as it was.
    pub fn set_caret_width(&self, points: f32) {
        if points.is_nan() {
            return;
        }
        let points = points.clamp(1., 20.);
        if self.storage.caret_width.replace(points) != points {
            let editors: Vec<_> = self.storage.editors.borrow().values().cloned().collect();
            for editor in editors {
                editor.refresh();
            }
        }
    }
    /// Editors' caret width in points (`Ui::set_caret_width`).
    pub fn caret_width(&self) -> f32 {
        self.storage.caret_width.get()
    }
    /// Whether assistive technology (a screen reader, a switch control)
    /// reads this document's semantics. While none does, the names and
    /// values views give it as functions of state (`label`, `value`), and
    /// the labels of changing text, are not worked out as that state
    /// changes: they are only ever read by it.
    /// Turned on, they are worked out at once, from the state as it is, as
    /// the effects run (before the tree is read). On by default (tests and
    /// headless documents read the semantics; `ZGX_ASSISTIVE=0` starts one
    /// off, as the perf gate runs); a desktop host turns it off
    /// until a reader asks for its tree, and off again when it leaves.
    pub fn set_assistive_technology(&self, listening: bool) {
        if listening {
            // Before the tree is read: what text changed while none listened.
            let pending = std::mem::take(&mut *self.storage.pending_labels.borrow_mut());
            for (id, ()) in pending {
                crate::compose::label_from_text(self, id);
            }
        }
        self.storage.listening.set(listening);
        self.storage.assistive.set(listening);
    }
    /// Whether text node `id`'s label waits for assistive technology: none
    /// reads the semantics, so it is worked out once one does
    /// (`set_assistive_technology`).
    #[inline(always)]
    pub(crate) fn defer_label(&self, id: crate::scene::NodeId) -> bool {
        if self.storage.listening.get() {
            return false;
        }
        self.pend_label(id);
        true
    }
    /// How many text nodes' labels wait for a reader (`defer_label`): for
    /// tests.
    #[doc(hidden)]
    pub fn pending_label_count(&self) -> usize {
        self.storage.pending_labels.borrow().len()
    }
    /// `defer_label` with no reader: out of line, the listening path stays
    /// a load.
    #[cold]
    #[inline(never)]
    fn pend_label(&self, id: crate::scene::NodeId) {
        let mut pending = self.storage.pending_labels.borrow_mut();
        pending.insert(id, ());
        // A long session with no reader removes many (a list's rows): drop
        // theirs, now and then.
        if pending.len() >= self.storage.pending_prune.get() {
            let scene = self.scene.borrow();
            pending.retain(|id, ()| scene.contains(*id));
            self.storage
                .pending_prune
                .set((pending.len() * 2).max(1024));
        }
    }
    /// Whether assistive technology reads the semantics (see
    /// `set_assistive_technology`).
    pub fn assistive_technology(&self) -> bool {
        self.storage
            .assistive
            .with_untracked(|listening| *listening)
    }
    pub(crate) fn assistive(&self) -> Signal<bool> {
        self.storage.assistive.clone()
    }
    /// The scene's pixel scale as this frame found it: what places scroll
    /// content on whole device pixels reads it, to place it again from the
    /// offset it was asked for when the scale changes (the scene alone
    /// keeps only where it put it).
    pub(crate) fn pixel_scale(&self) -> Signal<f32> {
        self.storage.pixel_scale.clone()
    }
    /// Whether input is being dispatched, so a scroll now is the user's.
    pub(crate) fn is_dispatching(&self) -> bool {
        self.storage.dispatching.get()
    }
    pub(crate) fn scrollbar_fades(
        &self,
    ) -> &RefCell<Vec<std::rc::Weak<crate::compose_scrollbar::Fade>>> {
        &self.storage.scrollbar_fades
    }
    /// Hear every event the document is given, before it goes where it goes
    /// (and one that goes nowhere: a key with nothing focused), while
    /// `owner` is mounted: a browser's `document.addEventListener`. For what
    /// answers input anywhere, such as a tooltip any key closes.
    pub fn on_document_event(
        &mut self,
        owner: NodeId,
        listener: impl FnMut(&InputEvent) + 'static,
    ) {
        let guard = self.listen_document(listener);
        self.retain(owner, guard);
    }
    /// `on_document_event` for as long as the returned guard is kept.
    pub(crate) fn listen_document(
        &self,
        listener: impl FnMut(&InputEvent) + 'static,
    ) -> impl Drop + 'static {
        thread_local! {
            static NEXT: Cell<u64> = const { Cell::new(0) };
        }
        let token = NEXT.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        self.storage
            .document_listeners
            .borrow_mut()
            .push((token, Rc::new(RefCell::new(listener))));
        DocumentListening {
            storage: Rc::downgrade(&self.storage),
            token,
        }
    }
    /// Give `event` to the document's listeners (`on_document_event`).
    fn tell_document(&self, event: &InputEvent) {
        // One at a time, the list free while each runs (it may add or end
        // listeners); nothing allocated per event.
        let mut index = 0;
        loop {
            let Some(listener) = self
                .storage
                .document_listeners
                .borrow()
                .get(index)
                .map(|(_, f)| f.clone())
            else {
                return;
            };
            if let Ok(mut f) = listener.try_borrow_mut() {
                f(event);
            }
            index += 1;
        }
    }
    /// Stretch a scroll view a trackpad gesture pushes past its edge, and
    /// spring it back as the fingers lift, as macOS's scroll views and
    /// Safari's pages do; momentum reaching an edge bounces. The offset an
    /// app reads stays in range. The host turns it on (macOS's does) and
    /// reports the gesture's phases (`InputEvent::ScrollGesture`); a mouse
    /// wheel never stretches. Off (the default), nothing of it runs.
    pub fn set_elastic_scrolling(&self, elastic: bool) {
        self.storage.elastic.borrow_mut().enabled = elastic;
    }
    /// Take a trackpad's swipe between pages as navigation, as Safari does:
    /// a gesture that starts across where nothing under it scrolls that way
    /// (each scroll view at its edge), over a view that takes navigation
    /// (`View::on_navigate`), stretches and scrolls nothing. The host hears
    /// of it (`take_navigation_swipe`), has the platform track it, and
    /// steps back or forward as it completes (`InputDispatcher::navigate`).
    /// It reads elastic scrolling's gesture phases (`set_elastic_scrolling`).
    /// The host turns it on (macOS's, with the system's "Swipe between
    /// pages" set to two fingers); off (the default), nothing of it runs.
    pub fn set_navigation_swipes(&self, swipes: bool) {
        self.storage.elastic.borrow_mut().swipes = swipes;
    }
    /// Whether a trackpad gesture just became a swipe between pages
    /// (`set_navigation_swipes`), once: the host has the platform track it.
    pub fn take_navigation_swipe(&self) -> bool {
        std::mem::take(&mut self.storage.elastic.borrow_mut().asked)
    }
    pub(crate) fn elastic_state(&self) -> &RefCell<crate::elastic::State> {
        &self.storage.elastic
    }
    pub(crate) fn observe_presentation(&self) -> Signal<bool> {
        self.storage.presented.clone()
    }
    /// Whether the window shows (`Ui::set_presented`: not hidden, minimized
    /// or covered), as it changes: an app tells someone of what arrives
    /// while they can't see it.
    pub fn observe_shown(&self) -> Signal<bool> {
        self.storage.presented.clone()
    }
    /// Whether the window is the active one, its keys going to it
    /// (`Ui::set_window_active`), as it changes.
    pub fn observe_window_active(&self) -> Signal<bool> {
        let active = !self.storage.selection.borrow().inactive;
        self.storage
            .window_active
            .borrow_mut()
            .get_or_insert_with(|| self.runtime.signal(active))
            .clone()
    }
    /// The window's activity signal, if something observes it.
    pub(crate) fn window_active_observed(&self) -> Option<Signal<bool>> {
        self.storage.window_active.borrow().clone()
    }
    /// Whether the window can present now (`Ui::set_presented`), untracked:
    /// a hidden, minimized or covered window draws no frames.
    pub fn is_presented(&self) -> bool {
        self.storage
            .presented
            .with_untracked(|presented| *presented)
    }
    pub(crate) fn caret_blink(&self) -> &crate::caret_blink::Blink {
        &self.storage.caret_blink
    }
    /// Let anchored popups (popovers, menus, submenus, context menus,
    /// tooltips) show past the window, within `screens`: the visible part of
    /// each screen, in window coordinates. Each flips and clamps to the
    /// screen its trigger is on instead of the window, and its subtree
    /// becomes a popup root (`Scene::set_popup`) the host shows in a window
    /// of its own. None (the default, and on hosts without popup windows):
    /// they stay inside the window.
    /// The host shows a drag's preview in a window of its own and moves
    /// that window with the pointer (macOS): previews are then popup roots,
    /// over every window and app, drawn once.
    pub fn set_popup_previews(&self, on: bool) {
        self.storage.popup_previews.set(on);
    }
    pub(crate) fn popup_previews(&self) -> bool {
        self.storage.popup_previews.get()
            && self
                .storage
                .popup_screens
                .with_untracked(|screens| !screens.is_empty())
    }
    /// Where popups may show beyond the window: each screen's usable area,
    /// in window points from the content's top left (the desktop host sets
    /// it). Empty keeps popups inside the window.
    pub fn set_popup_screens(&self, screens: Vec<Rect>) {
        if self
            .storage
            .popup_screens
            .with_untracked(|current| *current != screens)
        {
            self.storage.popup_screens.set(screens);
        }
    }
    pub(crate) fn popup_screens(&self) -> Signal<Vec<Rect>> {
        self.storage.popup_screens.clone()
    }
    /// The size (width, height, in points) the platform cut popup `root` to
    /// where the screen has no room for it, or `None` once it has closed: a
    /// Wayland compositor, which keeps a popup on its output, is the only
    /// judge of where that is. The popup's panel is laid out no bigger, a
    /// menu scrolling its items, as it does in a screen
    /// (`Ui::set_popup_screens`).
    pub fn set_popup_limit(&self, root: NodeId, size: Option<(f32, f32)>) {
        let current = self.storage.popup_limits.with_untracked(|limits| {
            limits
                .iter()
                .find(|(limited, _)| *limited == root)
                .map(|(_, size)| *size)
        });
        if current != size {
            self.storage.popup_limits.update(|limits| {
                limits.retain(|(limited, _)| *limited != root);
                limits.extend(size.map(|size| (root, size)));
            });
        }
    }
    pub(crate) fn popup_limits(&self) -> Signal<Vec<(NodeId, (f32, f32))>> {
        self.storage.popup_limits.clone()
    }
    /// Whether the platform showed popup `root` on the other side of its
    /// trigger than the scene put it, across and down (a Wayland compositor
    /// flips a popup with no room on its side of the output, which the app
    /// cannot know beforehand); `(false, false)` once it has closed. A
    /// popover's arrow is drawn on the side facing its trigger, so the scene
    /// places such a popover on that other side.
    pub fn set_popup_flipped(&self, root: NodeId, flipped: (bool, bool)) {
        let current = self.storage.popup_flips.with_untracked(|flips| {
            flips
                .iter()
                .find(|(flipped, _)| *flipped == root)
                .map_or((false, false), |(_, flipped)| *flipped)
        });
        if current != flipped {
            self.storage.popup_flips.update(|flips| {
                flips.retain(|(flipped, _)| *flipped != root);
                if flipped != (false, false) {
                    flips.push((root, flipped));
                }
            });
        }
    }
    pub(crate) fn popup_flips(&self) -> Signal<Vec<(NodeId, (bool, bool))>> {
        self.storage.popup_flips.clone()
    }
    /// How the host pictures what the window last showed: `take(rect,
    /// max_width)` gives `rect` (in points, the window's coordinates) at
    /// most `max_width` pixels wide, its height in proportion, or `None`.
    /// What [`Ui::snapshot`] and `NodeHandle::snapshot` use; without it they
    /// give nothing.
    pub fn on_snapshot(
        &self,
        take: impl Fn(Rect, u32) -> Option<std::sync::Arc<crate::image::ImageData>> + 'static,
    ) {
        *self.storage.snapshotter.borrow_mut() = Some(Rc::new(take));
    }
    /// How the host pictures a popup shown in a surface of its own: of
    /// popup root `root`, `Rect` in the scene's points, as `on_snapshot`
    /// does the window. `None` where it shows no such surface: the window's
    /// picture has it then.
    pub fn on_popup_snapshot(
        &self,
        take: impl Fn(NodeId, Rect, u32) -> Option<std::sync::Arc<crate::image::ImageData>> + 'static,
    ) {
        *self.storage.popup_snapshotter.borrow_mut() = Some(Rc::new(take));
    }
    /// A picture of `node` as the window last showed it (a thumbnail of a
    /// pane, say): its box, at most `max_width` pixels wide. Native views
    /// in it (a page, a terminal) are the platform's to draw and show as
    /// what zgx drew under them. `None` if the host takes no pictures, or
    /// the node is not laid out or is hidden (its box is not what shows).
    pub fn snapshot(
        &self,
        node: NodeId,
        max_width: u32,
    ) -> Option<std::sync::Arc<crate::image::ImageData>> {
        let take = self.storage.snapshotter.borrow().clone()?;
        let bounds = {
            let scene = self.scene.borrow();
            if !scene.layout_visible(node) {
                return None;
            }
            scene.bounds(node)
        };
        if bounds.width <= 0. || bounds.height <= 0. {
            return None;
        }
        // In a popup past the window: what its own surface shows.
        let root = {
            let scene = self.scene.borrow();
            scene
                .popups()
                .iter()
                .copied()
                .find(|root| scene.is_within(node, *root))
        };
        let popup = self.storage.popup_snapshotter.borrow().clone();
        if let (Some(root), Some(popup)) = (root, popup)
            && let Some(picture) = popup(root, bounds, max_width.max(1))
        {
            return Some(picture);
        }
        take(bounds, max_width.max(1))
    }
    /// The display's refresh period, as the host knows it (60 Hz until
    /// told): what the engine times itself steps by (a stretched view
    /// springing back, overlay scrollbars fading), so they move every
    /// refresh, on a 120 Hz display as on a 60 Hz one.
    pub fn set_refresh_interval(&self, interval: std::time::Duration) {
        if interval >= std::time::Duration::from_millis(2)
            && interval <= std::time::Duration::from_millis(100)
        {
            self.storage.refresh_interval.set(interval);
        }
    }
    /// The display's refresh period (`Ui::set_refresh_interval`).
    pub fn refresh_interval(&self) -> std::time::Duration {
        self.storage.refresh_interval.get()
    }
    /// How the host records and plays back wheel input for devtools (`zgx
    /// inspect`'s gesture replays): its input path, the trackpad's phases,
    /// resampling and edges included, runs as for the platform's events.
    #[doc(hidden)]
    pub fn on_wheel_tape(&self, tape: impl Fn(WheelTape) -> Result<String, String> + 'static) {
        *self.storage.wheel_taper.borrow_mut() = Some(Rc::new(tape));
    }
    /// Ask the host's wheel recorder or player (`on_wheel_tape`); an error
    /// if the host has none.
    #[doc(hidden)]
    pub fn wheel_tape(&self, request: WheelTape) -> Result<String, String> {
        let taper = self.storage.wheel_taper.borrow().clone();
        match taper {
            Some(taper) => taper(request),
            None => Err("this host records and plays no wheel input".into()),
        }
    }
    /// Call `soon` as the pointer reaches something that opens a popup (a
    /// popover's or menu's trigger, a view with a tooltip or context menu),
    /// once until the host hears it (`popup_soon_heard`): a host showing
    /// popups in windows of their own readies one then, not before. Set
    /// before the window's views mount; without it nothing listens.
    pub fn on_popup_soon(&self, soon: impl Fn() + 'static) {
        *self.storage.popup_soon.borrow_mut() = Some(Rc::new(soon));
    }
    /// The host heard `on_popup_soon`'s call: the next may come.
    pub fn popup_soon_heard(&self) {
        self.storage.popup_soon_sent.set(false);
    }
    pub(crate) fn popup_soon_enabled(&self) -> bool {
        self.storage.popup_soon.borrow().is_some()
    }
    pub(crate) fn popup_soon(&self) {
        if self.storage.popup_soon_sent.replace(true) {
            return;
        }
        let soon = self.storage.popup_soon.borrow().clone();
        if let Some(soon) = soon {
            soon();
        }
    }
    /// Listen on `node` for the pointer reaching it (moving over it or
    /// pressing it): a popup it opens may follow (`on_popup_soon`). Only a
    /// node listening already: a bare one (a context menu's point) stays out
    /// of hit testing.
    pub(crate) fn listen_popup_soon(&mut self, node: NodeId) {
        if !self.popup_soon_enabled() || !self.input.has_listeners(node) {
            return;
        }
        let weak = self.downgrade();
        let binding = self.input.listen(node, move |cx| {
            if cx.phase != crate::input::EventPhase::Capture
                && matches!(
                    cx.event,
                    crate::input::InputEvent::PointerMove { .. }
                        | crate::input::InputEvent::PointerDown { .. }
                )
                && let Some(ui) = weak.upgrade()
            {
                ui.popup_soon();
            }
        });
        self.retain(node, binding);
    }
    pub(crate) fn register_popup(&self, root: NodeId, open: Signal<bool>) {
        let scene = self.scene.borrow();
        let mut popups = self.storage.popups.borrow_mut();
        popups.retain(|(node, _)| scene.contains(*node));
        popups.push((root, open));
    }
    pub(crate) fn add_portal(&self, owner: NodeId, overlay: NodeId) {
        self.storage.portals.borrow_mut().push((owner, overlay));
    }
    pub(crate) fn remove_portal(&self, overlay: NodeId) {
        self.storage
            .portals
            .borrow_mut()
            .retain(|(_, other)| *other != overlay);
    }
    /// Each portal's owner and overlay, in the order they mounted: a
    /// modal's, popover's or menu's panel shows in an overlay under the
    /// document root (or an outer portal's), outside the subtree of the
    /// node that owns it. Tools reading a view as it was written (an
    /// agent's targets, its tree) find the panel under its owner here.
    pub fn portals(&self) -> Vec<(NodeId, NodeId)> {
        self.storage.portals.borrow().clone()
    }
    /// Close every open popup shown past the window, as AppKit closes menus
    /// and transient popovers when the app or its window loses focus.
    pub fn dismiss_popups(&self) {
        let open: Vec<Signal<bool>> = {
            let scene = self.scene.borrow();
            self.storage
                .popups
                .borrow()
                .iter()
                .filter(|(node, _)| scene.is_popup(*node))
                .map(|(_, open)| open.clone())
                .collect()
        };
        for open in open {
            if open.with_untracked(|open| *open) {
                open.set(false);
            }
        }
    }
    pub(crate) fn observe_visibility(&self, node: NodeId) -> Signal<bool> {
        if let Some(signal) = self.storage.observed_visibility.borrow().get(&node) {
            return signal.clone();
        }
        let signal = self.signal(false);
        self.storage
            .observed_visibility
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.bounds_geometry_revision.set(None);
        signal
    }
    #[cold]
    #[inline(never)]
    fn take_paste_hooks(&self, ids: &[NodeId]) -> Vec<PasteHook> {
        let mut hooks = self.storage.paste_hooks.borrow_mut();
        ids.iter().filter_map(|id| hooks.remove(id)).collect()
    }
    /// `hook` hears pastes into editor `node` (only an editor: a hook goes
    /// as its editor does).
    pub(crate) fn set_paste_hook(&self, node: NodeId, hook: PasteHook) {
        debug_assert!(self.is_editor(node), "on_paste is for editors");
        if self.is_editor(node) {
            self.storage.paste_hooks.borrow_mut().insert(node, hook);
        }
    }
    /// Whether `node` has `View::on_paste`: it may want a paste with no
    /// text in it.
    pub(crate) fn takes_paste(&self, node: NodeId) -> bool {
        self.storage.paste_hooks.borrow().contains_key(&node)
    }
    /// The person pasted into `editor` (a key, a menu, a middle click):
    /// its `View::on_paste` hears first, and unless it takes the paste,
    /// `text` goes in. A read-only editor takes none. Hosts call this with
    /// no text where the clipboard holds something else (an image), and
    /// nothing borrowed that the hook may use: it may read the clipboard.
    pub fn paste_into(&self, editor: &EditorHandle, text: Option<&str>) {
        if editor.is_read_only() {
            return;
        }
        let hook = self.storage.paste_hooks.borrow().get(&editor.node).cloned();
        if hook.is_some_and(|hook| hook(text)) {
            return;
        }
        if let Some(text) = text {
            editor.paste(text);
        }
    }
    /// Prepare allocated control geometry before rendering a frame or dispatching input.
    /// Native hosts call this automatically. Headless hosts should call it before `Scene::flush`.
    pub fn prepare_frame(&self) {
        self.try_prepare_frame()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    /// Settle geometry, reporting cyclic or unbounded allocation feedback instead
    /// of hanging the UI thread. At most 64 layout/notification passes are allowed.
    /// Reentrant calls defer notifications to the active outer preparation.
    /// An error does not roll back application mutations; correct the dependency
    /// and call again. Under [`contain_layout_feedback`](Self::contain_layout_feedback),
    /// as in a window, this is reported and the frame goes on instead.
    pub fn try_prepare_frame(&self) -> Result<(), LayoutFeedbackError> {
        if self.storage.preparing_frame.get() {
            return Ok(());
        }
        // A new pixel scale (the window moved to another display) places
        // scroll content afresh from the offsets it was scrolled to.
        let scale = self.scene.borrow().pixel_scale();
        if self.storage.pixel_scale_seen.get() != scale {
            self.storage.pixel_scale_seen.set(scale);
            self.storage.pixel_scale.set(scale);
        }
        self.prepare_frame_once()?;
        // Find in page's marks where text mounted or moved, and what its
        // bar asked for.
        if self.storage.find.borrow().is_some() {
            crate::find::sync(self);
        }
        // Whether the host draws the caret, now that it is laid out.
        crate::caret_blink::settle(self);
        Ok(())
    }
    /// Go on past layout that doesn't settle, rather than return
    /// [`LayoutFeedbackError`]: the layout stands as the last pass left it,
    /// nothing hears its geometry until something else changes it (so the
    /// feedback stops, and idle stays idle), and [`issues::on_issue`]
    /// hears of it once, naming the nodes that still moved. Then a frame
    /// tries again, with fewer passes, whenever the layout has changed.
    /// Window hosts call this: a layout that doesn't converge never ends an
    /// app.
    ///
    /// [`issues::on_issue`]: crate::issues::on_issue
    pub fn contain_layout_feedback(&self) {
        self.storage.contains_layout.set(true);
    }
    fn prepare_frame_once(&self) -> Result<(), LayoutFeedbackError> {
        if self.storage.preparing_frame.replace(true) {
            return Ok(());
        }
        let _preparation = FramePreparationGuard(&self.storage.preparing_frame);
        // What the last frames' removals left to free, a little at a time:
        // a chunk only in a frame that removed a large subtree, which has
        // spent its share.
        if self.storage.removed.replace(false) {
            self.reclaim(std::time::Duration::ZERO);
        } else {
            self.reclaim(RECLAIM_BUDGET);
        }
        let held = self.storage.unsettled.get();
        if let Some(held) = held
            && self.layout_still_held(held)
        {
            return Ok(());
        }
        let passes = if held.is_some() {
            RETRY_LAYOUT_PASSES
        } else {
            LAYOUT_PASSES
        };
        for _ in 0..passes {
            self.scene.borrow_mut().prepare_layout();
            self.input.validate_drag(&self.scene);
            let (layout_revision, geometry_revision) = {
                let scene = self.scene.borrow();
                (scene.layout_revision(), scene.geometry_revision())
            };
            let layout_changed = self.storage.editor_layout_revision.get() != Some(layout_revision);
            let geometry_changed =
                self.storage.bounds_geometry_revision.get() != Some(geometry_revision);
            if !layout_changed && !geometry_changed {
                if !self.run_after_layout() {
                    // Settled: a removed focused node's focus falls to its
                    // ancestor, unless something (a list revealing a row)
                    // focused since.
                    if let Some(id) = self.storage.refocus.take()
                        && self.input.focused().is_none()
                        && self.scene.borrow().contains(id)
                    {
                        self.input.focus(&self.scene, Some(id));
                    }
                    self.sync_selection();
                    self.storage.unsettled.set(None);
                    return Ok(());
                }
                continue;
            }
            let hidden_focus = self
                .input
                .focused()
                .is_some_and(|id| !self.scene.borrow().layout_visible(id));
            if hidden_focus {
                self.input.focus(&self.scene, None);
            }
            self.input.refresh_hover(&self.scene);
            self.storage
                .editor_layout_revision
                .set(Some(layout_revision));
            self.storage
                .bounds_geometry_revision
                .set(Some(geometry_revision));
            let (sizes, bounds): (Vec<_>, Vec<_>) = {
                let scene = self.scene.borrow();
                let sizes = if layout_changed {
                    let content = self.storage.content_sizes.borrow();
                    let own = self.storage.observed_sizes.borrow();
                    content
                        .iter()
                        .filter(|(node, _)| scene.contains(**node))
                        .map(|(node, signal)| {
                            (*node, signal.clone(), allocated_content_size(&scene, *node))
                        })
                        .chain(own.iter().filter(|(node, _)| scene.contains(**node)).map(
                            |(node, signal)| {
                                let b = scene.layout_box(*node);
                                (*node, signal.clone(), (b.width, b.height))
                            },
                        ))
                        .collect()
                } else {
                    Vec::new()
                };
                let bounds = if geometry_changed {
                    self.storage
                        .observed_bounds
                        .borrow()
                        .iter()
                        .filter(|(node, _)| scene.contains(**node))
                        .map(|(node, signal)| (*node, signal.clone(), scene.world_bounds(*node)))
                        .collect()
                } else {
                    Vec::new()
                };
                (sizes, bounds)
            };
            let visibility: Vec<_> = if geometry_changed {
                let scene = self.scene.borrow();
                self.storage
                    .observed_visibility
                    .borrow()
                    .iter()
                    .filter(|(node, _)| scene.contains(**node))
                    .map(|(node, signal)| {
                        (*node, signal.clone(), scene.visible_bounds(*node).is_some())
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // Publish outside every storage/scene borrow: geometry subscribers can
            // mount, dispose, translate, or reenter frame preparation.
            self.runtime.batch(|| {
                for (node, signal, size) in sizes {
                    if self.scene.borrow().contains(node)
                        && signal.with_untracked(|told| resized(*told, size))
                    {
                        signal.set(size);
                    }
                }
                for (node, signal, visible) in visibility {
                    if self.scene.borrow().contains(node) {
                        signal.set(visible);
                    }
                }
                for (node, signal, bounds) in bounds {
                    if self.scene.borrow().contains(node) {
                        signal.set(bounds);
                    }
                }
            });
            if layout_changed {
                let editors: Vec<_> = self.storage.editors.borrow().values().cloned().collect();
                for editor in editors {
                    editor.refresh_if_resized();
                }
                self.run_layout_listeners();
            }
            self.run_after_layout();
            // Internal geometry bindings are constrained by their viewport.
            // A second pass also prepares editors mounted by virtual rows.
        }
        if self.storage.contains_layout.get() {
            self.hold_layout(passes, held.is_some());
            return Ok(());
        }
        Err(LayoutFeedbackError { passes })
    }
    /// Whether layout held unsettled (`hold_layout`) stands: laid out, as a
    /// frame needs, it is as it was held.
    #[cold]
    fn layout_still_held(&self, held: (u64, u64)) -> bool {
        self.scene.borrow_mut().prepare_layout();
        self.input.validate_drag(&self.scene);
        let scene = self.scene.borrow();
        (scene.layout_revision(), scene.geometry_revision()) == held
    }
    /// Layout didn't settle in `passes`: keep what the last one left, and
    /// let what hears geometry wait, which ends the feedback. Reported
    /// unless this was already a retry of the same trouble.
    #[cold]
    #[inline(never)]
    fn hold_layout(&self, passes: usize, retry: bool) {
        self.scene.borrow_mut().prepare_layout();
        self.input.validate_drag(&self.scene);
        let moved = self.nodes_still_moving();
        let held = {
            let scene = self.scene.borrow();
            (scene.layout_revision(), scene.geometry_revision())
        };
        self.storage.unsettled.set(Some(held));
        self.sync_selection();
        if retry {
            return;
        }
        let error = LayoutFeedbackError { passes };
        let what = if moved.is_empty() {
            "layout callbacks kept asking for another pass".to_string()
        } else {
            let nodes: Vec<String> = moved.iter().map(|n| format!("@{}", n.number())).collect();
            format!("still moving: {}", nodes.join(" "))
        };
        // The app goes on, and the cause can be the engine's (#1027): a
        // developer's to see, not an end user's.
        crate::issues::report_kind(
            &self.runtime,
            crate::issues::IssueKind::Degraded,
            &format!("{error} ({what}); the layout stays as it is until something changes it"),
        );
    }
    /// The nodes whose geometry differs from what was last told of it, at
    /// most eight: those the last pass moved, and what reads them moved it.
    fn nodes_still_moving(&self) -> Vec<NodeId> {
        let scene = self.scene.borrow();
        let sizes = {
            let content = self.storage.content_sizes.borrow();
            let own = self.storage.observed_sizes.borrow();
            let content = content.iter().filter(|(node, signal)| {
                scene.contains(**node)
                    && signal.with_untracked(|told| {
                        resized(*told, allocated_content_size(&scene, **node))
                    })
            });
            let own = own.iter().filter(|(node, signal)| {
                scene.contains(**node)
                    && signal.with_untracked(|told| {
                        let bounds = scene.layout_box(**node);
                        resized(*told, (bounds.width, bounds.height))
                    })
            });
            content
                .chain(own)
                .map(|(node, _)| *node)
                .collect::<Vec<_>>()
        };
        let bounds = self.storage.observed_bounds.borrow();
        let bounds = bounds.iter().filter(|(node, signal)| {
            scene.contains(**node)
                && signal.with_untracked(|bounds| *bounds != scene.world_bounds(**node))
        });
        let mut moved = sizes;
        moved.extend(bounds.map(|(node, _)| *node));
        moved.sort_by_key(|node| node.number());
        moved.dedup();
        moved.truncate(8);
        moved
    }
    /// Recompute editor geometry after changing the font backend or display scale.
    pub(crate) fn observe_text_geometry_revision(&self) -> Signal<u64> {
        self.storage.text_geometry_revision.clone()
    }
    /// Text's lines moved in a box that did not (`Scene::set_text_balance`):
    /// what follows text geometry (`observe_text_geometry_revision`) looks
    /// again.
    pub(crate) fn text_geometry_moved(&self) {
        (self.storage.text_geometry_revision)
            .update(|revision| *revision = revision.wrapping_add(1));
    }
    /// Lay out every editor's text again, after the font backend or display
    /// scale changed.
    pub fn refresh_text_geometry(&self) {
        let revision = self.storage.text_geometry_revision.get();
        self.storage
            .text_geometry_revision
            .set(revision.wrapping_add(1));
        let old = self.storage.editor_layout.borrow_mut().take();
        drop(old);
        let editors: Vec<_> = self.storage.editors.borrow().values().cloned().collect();
        for editor in editors {
            editor.refresh();
        }
    }
    /// Whether `id` is a text editor's root (an input or text area).
    pub fn is_editor(&self, id: NodeId) -> bool {
        self.storage.editors.borrow().contains_key(&id)
    }
    /// The editor that has keyboard focus, if one does.
    pub fn focused_editor(&self) -> Option<EditorHandle> {
        self.input
            .focused()
            .and_then(|id| self.storage.editors.borrow().get(&id).cloned())
    }
    /// The editor whose root is `node`, if it is one.
    pub fn editor(&self, node: NodeId) -> Option<EditorHandle> {
        self.storage.editors.borrow().get(&node).cloned()
    }
    /// The editor whose root is `node` or inside it, if there is one.
    pub fn editor_within(&self, node: NodeId) -> Option<EditorHandle> {
        if let Some(editor) = self.editor(node) {
            return Some(editor);
        }
        let scene = self.scene.borrow();
        self.storage
            .editors
            .borrow()
            .values()
            .find(|editor| scene.contains(editor.node) && scene.is_within(editor.node, node))
            .cloned()
    }
    /// What the last press in a `View::window_drag` region asked of the
    /// window, once: the desktop host takes it after each pointer press.
    pub fn take_window_gesture(&self) -> Option<WindowGesture> {
        self.storage.window_gesture.take()
    }
    /// The editor whose caret the last middle press placed, once: the
    /// desktop host pastes X11's and Wayland's primary selection there.
    pub fn take_primary_paste(&self) -> Option<NodeId> {
        self.storage.primary_paste.take()
    }
    /// Whether a middle press in an editor pastes the primary selection
    /// there (X11's and Wayland's): on until the desktop host says the
    /// person turned it off (GNOME's `gtk-enable-primary-paste`), as GTK
    /// follows it. Off, a middle press does nothing in an editor.
    pub fn set_primary_paste(&self, on: bool) {
        self.storage.primary_paste_on.set(on);
    }
    pub(crate) fn request_window_gesture(&self, gesture: WindowGesture) {
        self.storage.window_gesture.set(Some(gesture));
    }
    /// Deliver `event` after preparing the frame's layout. Panics if layout
    /// doesn't settle; [`Ui::try_dispatch`] returns the error instead.
    pub fn dispatch(&mut self, event: InputEvent) -> crate::input::DispatchResult {
        self.try_dispatch(event)
            .unwrap_or_else(|error| panic!("{error}"))
    }
    /// Dispatch after fallible geometry preparation. On allocation feedback,
    /// the event is not delivered and may be retried after repairing the model.
    pub fn try_dispatch(
        &mut self,
        event: InputEvent,
    ) -> Result<crate::input::DispatchResult, LayoutFeedbackError> {
        self.try_prepare_frame()?;
        let _span = crate::profile::span(crate::profile::Phase::Input);
        let after = (
            crate::selection::SelectionEvent::of(&event, Default::default()),
            crate::scroll_keys::ScrollKey::of(&event),
            matches!(
                event,
                InputEvent::PointerDown {
                    button: crate::input::PointerButton::Primary,
                    ..
                }
            ),
        );
        self.storage.dispatching.set(true);
        self.tell_document(&event);
        let result = self.route(event, after, |ui, event| {
            ui.input.dispatch(&ui.scene, event)
        });
        self.storage.dispatching.set(false);
        Ok(result)
    }
    /// Dispatch native-style pointer input with modifiers and click counting.
    /// Ordinary `dispatch` retains unmodified, single-click pointer behavior.
    pub fn dispatch_with_modifiers(
        &mut self,
        event: InputEvent,
        modifiers: crate::input::Modifiers,
    ) -> crate::input::DispatchResult {
        self.try_dispatch_with_modifiers(event, modifiers)
            .unwrap_or_else(|error| panic!("{error}"))
    }
    /// Fallible counterpart of `dispatch_with_modifiers`; failed geometry
    /// preparation leaves the event and click sequence unprocessed.
    pub fn try_dispatch_with_modifiers(
        &mut self,
        event: InputEvent,
        modifiers: crate::input::Modifiers,
    ) -> Result<crate::input::DispatchResult, LayoutFeedbackError> {
        self.try_prepare_frame()?;
        let _span = crate::profile::span(crate::profile::Phase::Input);
        let after = (
            crate::selection::SelectionEvent::of(&event, modifiers),
            crate::scroll_keys::ScrollKey::of(&event),
            matches!(
                event,
                InputEvent::PointerDown {
                    button: crate::input::PointerButton::Primary,
                    ..
                }
            ),
        );
        self.storage.dispatching.set(true);
        self.tell_document(&event);
        let result = self.route(event, after, |ui, event| {
            ui.input
                .dispatch_with_modifiers(&ui.scene, event, modifiers)
        });
        self.storage.dispatching.set(false);
        Ok(result)
    }
    /// Whether the wheel being dispatched is a platform's, in the window's
    /// points, rather than one the engine made in a scroll view's own.
    pub(crate) fn wheel_in_window(&self) -> bool {
        self.storage.wheel_in_window.get()
    }
    /// Give `event` to the nodes with `dispatch`, with elastic scrolling
    /// around it when that is on, then what the selection and keyboard
    /// scrolling make of it.
    fn route(
        &self,
        event: InputEvent,
        after: (
            crate::selection::SelectionEvent,
            crate::scroll_keys::ScrollKey,
            bool,
        ),
        dispatch: impl FnOnce(&Self, InputEvent) -> crate::input::DispatchResult,
    ) -> crate::input::DispatchResult {
        let elastic = self.storage.elastic.borrow().enabled;
        let event = match event {
            InputEvent::ScrollGesture(phase) => {
                if elastic {
                    crate::elastic::gesture(self, phase);
                }
                return Default::default();
            }
            event @ InputEvent::Scroll { .. } if elastic && !crate::zoom::wheel_zooms() => {
                match crate::elastic::before_scroll(self, event) {
                    Some(event) => event,
                    None => return Default::default(),
                }
            }
            event => event,
        };
        let scroll =
            (elastic && !crate::zoom::wheel_zooms() && matches!(event, InputEvent::Scroll { .. }))
                .then(|| event.clone());
        let key = matches!(event, InputEvent::KeyDown { .. }).then(|| event.clone());
        // A platform's wheel, in the window's points; the engine's own
        // (`InputDispatcher::dispatch_to`: keys, find, a reveal) are in a
        // scroll view's.
        let wheel = matches!(event, InputEvent::Scroll { .. });
        self.storage.wheel_in_window.set(wheel);
        let mut result = dispatch(self, event);
        self.storage.wheel_in_window.set(false);
        self.after_dispatch(after, &mut result);
        if let Some(key) = key {
            self.find_after_dispatch(&key, &mut result);
            // The window's zoom keys, for keys nothing took.
            if !result.default_prevented && crate::zoom::after_dispatch(self, &key) {
                result.default_prevented = true;
            }
        }
        if let Some(event) = scroll {
            crate::elastic::after_scroll(self, &event, &result);
        }
        result
    }
    fn interaction_valid(&self, job: &EditorInteraction) -> bool {
        job.gesture.mode.get().is_some()
            && job.gesture.generation.get() == job.generation
            && self.input.captured() == Some(job.owner)
            && self.input.focused() == Some(job.owner)
            && self.input.is_enabled(&self.scene.borrow(), job.owner)
    }
    /// Next deadline of a timed interaction: a key sequence waiting, a drag
    /// resting on a destination toward springing it open, editor drag
    /// autoscroll. Idle windows schedule nothing.
    pub fn next_interaction_deadline(&self) -> Option<Instant> {
        [
            self.input.next_key_deadline(),
            self.input.next_spring_deadline(),
            self.next_editor_interaction_deadline(),
            self.next_selection_deadline(),
            crate::compose_scrollbar::next_fade(self),
            crate::elastic::next_step(self),
            crate::caret_blink::next(self),
            crate::find::next_step(self),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    fn next_editor_interaction_deadline(&self) -> Option<Instant> {
        let pending = self
            .storage
            .interaction
            .borrow()
            .as_ref()
            .map(|p| (p.job.clone(), p.next));
        let (job, deadline) = pending?;
        let owned = self.interaction_valid(&job);
        if owned && (job.can_scroll)() {
            Some(deadline)
        } else {
            if !owned && job.gesture.generation.get() == job.generation {
                job.gesture.cancel();
            }
            let old = self.storage.interaction.borrow_mut().take();
            drop(old);
            None
        }
    }
    /// Advance due captured-editor interactions using an explicit monotonic time.
    /// Returns whether a tick ran; stalls contribute at most 50 ms of motion.
    pub fn advance_interactions(&mut self, now: Instant) -> Result<bool, LayoutFeedbackError> {
        if self.storage.advancing_interaction.get() {
            return Ok(false);
        }
        let keys = self.input.advance_key_sequence(&self.scene, now);
        if keys {
            self.try_prepare_frame()?;
        }
        // A selection drag held past a scroll viewport's edge.
        let keys = if self.advance_selection_scroll(now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };
        // A stretched scroll view springing back.
        let keys = if crate::elastic::advance(self, now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };
        // A drag resting on a destination long enough to spring it open.
        let keys = if self.input.advance_spring(&self.scene, now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };
        // Overlay scrollbars fading out.
        let keys = if crate::compose_scrollbar::advance_fades(self, now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };
        // The focused editor's caret blinking.
        let keys = if crate::caret_blink::advance(self, now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };
        // Find in page counting a list's rows.
        let keys = if crate::find::advance(self, now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };

        if self
            .next_editor_interaction_deadline()
            .is_none_or(|deadline| deadline > now)
        {
            return Ok(keys);
        }
        self.try_prepare_frame()?;
        // Geometry refresh can stop the gesture before the scheduled tick.
        if self
            .next_editor_interaction_deadline()
            .is_none_or(|deadline| deadline > now)
        {
            return Ok(keys);
        }
        let pending = self.storage.interaction.borrow_mut().take().unwrap();
        let elapsed = now
            .saturating_duration_since(pending.last)
            .min(Duration::from_millis(50));
        let storage = self.storage.clone();
        storage.advancing_interaction.set(true);
        let _guard = FramePreparationGuard(&storage.advancing_interaction);
        (pending.job.tick)(elapsed.as_secs_f64());
        let prepared = self.try_prepare_frame();
        let valid = self.interaction_valid(&pending.job);
        if !valid && pending.job.gesture.generation.get() == pending.job.generation {
            pending.job.gesture.cancel();
        }
        if valid && (pending.job.can_scroll)() && self.storage.interaction.borrow().is_none() {
            *self.storage.interaction.borrow_mut() = Some(InteractionDeadline {
                job: pending.job,
                last: now,
                next: now + Duration::from_millis(16),
            });
        }
        prepared?;
        Ok(true)
    }
    /// Cancel captured gestures and their deadlines, for example when a host hides.
    pub fn cancel_interactions(&mut self) {
        self.input.cancel_pending_keys();
        self.input.cancel_drag(&self.scene);
        if self.storage.interaction.borrow().is_none() && self.input.captured().is_none() {
            return;
        }
        let old = self.storage.interaction.borrow_mut().take();
        if let Some(pending) = &old {
            pending.job.gesture.cancel();
        }
        drop(old);
        self.input.dispatch(&self.scene, InputEvent::PointerCancel);
    }
    /// Whether nodes removed (`Ui::remove`) still hold what they held, to
    /// be freed over the next frames: frames call `reclaim` as they are
    /// prepared, so a large removal costs a few of them a little each
    /// instead of one of them a lot, and a host asks for frames while this
    /// holds.
    pub fn reclaiming(&self) -> bool {
        !self.storage.dying.borrow().is_empty()
    }
    /// Free what removed nodes held, a chunk at least and for at most
    /// `budget` past it: whether some is left for the next call.
    pub fn reclaim(&self, budget: std::time::Duration) -> bool {
        // Most frames free nothing: no look at the clock for them.
        if !self.reclaiming() {
            self.shrink_storage();
            return false;
        }
        let started = std::time::Instant::now();
        loop {
            // A chunk at a time between looks at the clock.
            let chunk: Vec<NodeId> = {
                let mut dying = self.storage.dying.borrow_mut();
                let count = RECLAIM_CHUNK.min(dying.len());
                dying.drain(..count).collect()
            };
            if chunk.is_empty() {
                self.shrink_storage();
                return false;
            }
            self.free_removed(&chunk);
            if started.elapsed() >= budget {
                return self.reclaiming();
            }
        }
    }
    /// Free what `ids`, nodes a large removal took out of the scene, held.
    fn free_removed(&self, ids: &[NodeId]) {
        let owned: Vec<_> = {
            let mut storage = self.storage.owned.borrow_mut();
            ids.iter().filter_map(|id| storage.remove(id)).collect()
        };
        let mut listeners = Vec::new();
        for id in ids {
            for sizes in [&self.storage.content_sizes, &self.storage.observed_sizes] {
                if !sizes.borrow().is_empty() {
                    sizes.borrow_mut().remove(id);
                }
            }
            if !self.storage.observed_bounds.borrow().is_empty() {
                self.storage.observed_bounds.borrow_mut().remove(id);
            }
            if !self.storage.observed_visibility.borrow().is_empty() {
                self.storage.observed_visibility.borrow_mut().remove(id);
            }
            if !self.storage.layout_listeners.borrow().is_empty() {
                listeners.extend(self.storage.layout_listeners.borrow_mut().remove(id));
            }
            self.input.unregister(*id);
        }
        {
            let mut semantics = self.semantics.borrow_mut();
            for id in ids {
                semantics.forget(*id);
            }
        }
        {
            let mut scene = self.scene.borrow_mut();
            for id in ids {
                scene.free_dead(*id);
            }
        }
        // Outside every borrow: their captures may reenter.
        drop(owned);
        drop(listeners);
    }
    /// A page of thousands of nodes gone: the maps give back their room.
    fn shrink_storage(&self) {
        crate::shrink_sparse(&mut self.storage.owned.borrow_mut());
        crate::shrink_sparse(&mut self.storage.editors.borrow_mut());
        crate::shrink_sparse(&mut self.storage.content_sizes.borrow_mut());
        crate::shrink_sparse(&mut self.storage.observed_sizes.borrow_mut());
        crate::shrink_sparse(&mut self.storage.observed_bounds.borrow_mut());
    }
    /// What removing `node` ends before its subtree goes: an interaction
    /// or a cached editor layout in it (the cached layout returned, to be
    /// dropped once the subtree is gone), and where focus in it falls.
    fn before_removal(&self, node: NodeId) -> (Option<EditorLayoutEntry>, Option<NodeId>) {
        let inside = |id: NodeId| {
            let scene = self.scene.borrow();
            scene.contains(id) && scene.is_within(id, node)
        };
        let remove_interaction = self
            .storage
            .interaction
            .borrow()
            .as_ref()
            .is_some_and(|pending| inside(pending.job.owner));
        if remove_interaction {
            let old = self.storage.interaction.borrow_mut().take();
            if let Some(pending) = &old {
                pending.job.gesture.cancel();
            }
            drop(old);
        }
        let cached_layout = if self
            .storage
            .editor_layout
            .borrow()
            .as_ref()
            .is_some_and(|entry| inside(entry.node))
        {
            self.storage.editor_layout.borrow_mut().take()
        } else {
            None
        };
        // Focus inside it falls to its nearest focusable ancestor that stays,
        // else to the next Tab stop (as a native toolkit moves it on, rather
        // than to the window), so keys still reach the app and a screen
        // reader says where it went: once the frame settles
        // (`try_prepare_frame`).
        let refocus = self.input.focused().filter(|id| inside(*id)).and_then(|_| {
            let s = self.scene.borrow();
            let mut at = s.parent(node);
            while let Some(id) = at {
                if self.input.options(id).is_some_and(|o| o.focusable) {
                    return Some(id);
                }
                at = s.parent(id);
            }
            // None stays: the next Tab stop takes it, never the window.
            self.input.successor(&s, node)
        });
        (cached_layout, refocus)
    }
    /// Remove `node` and its subtree, disposing their effects, listeners and
    /// semantics. Focus within it moves on to what remains. A large subtree
    /// goes at once (nothing reaches its nodes, its effects never run again
    /// and what its components keep is dropped), and what else its nodes
    /// held is freed over the next frames (`Ui::reclaim`).
    pub fn remove(&mut self, node: NodeId) {
        // Its destructors (a listener's captures, a component's resources)
        // run during it: a write of theirs wakes the subtree's own effects
        // only once they are gone, as `remove_large` stops them all first.
        let _held = self.runtime.hold_effects();
        self.remove_as(node, usize::MAX);
    }
    /// `remove` from inside an effect's run, where writes wait already
    /// (a list's rows going as its effect runs): nothing to hold.
    pub(crate) fn remove_in_effect(&mut self, node: NodeId) {
        debug_assert!(
            self.runtime.deferring(),
            "remove_in_effect outside an effect"
        );
        self.remove_as(node, usize::MAX);
    }
    /// `remove_in_effect` of the child at `index` of its parent.
    pub(crate) fn remove_indexed(&mut self, node: NodeId, index: usize) {
        debug_assert!(self.runtime.deferring(), "remove_indexed outside an effect");
        self.remove_as(node, index);
    }
    #[inline(never)]
    fn remove_as(&mut self, node: NodeId, index: usize) {
        if !self.scene.borrow().contains(node) {
            return;
        }
        let mut ids = vec![node];
        {
            let s = self.scene.borrow();
            let mut i = 0;
            while i < ids.len() {
                if ids.len() > RECLAIM_NOW {
                    drop(s);
                    return self.remove_large(node);
                }
                ids.extend_from_slice(s.children(ids[i]));
                i += 1;
            }
        }
        let (cached_layout, refocus) = self.before_removal(node);
        // Extract all ownership before running user destructors. A destructor may
        // upgrade WeakUi and remove a sibling or the already detached subtree.
        let owned: Vec<_> = {
            let mut storage = self.storage.owned.borrow_mut();
            ids.iter().filter_map(|id| storage.remove(id)).collect()
        };
        let editors: Vec<_> = {
            let mut storage = self.storage.editors.borrow_mut();
            ids.iter().filter_map(|id| storage.remove(id)).collect()
        };
        // A hook can hold a whole script: it goes with its editor.
        let paste_hooks = (!editors.is_empty()).then(|| self.take_paste_hooks(&ids));
        // Detach geometry before listener captures or cached layouts can run
        // user destructors. Reentrant removal of this owner (or an ancestor)
        // then observes an already removed subtree instead of stale node IDs.
        if index != usize::MAX {
            self.scene.borrow_mut().remove_indexed(node, index);
        } else {
            self.scene.borrow_mut().remove(node);
        }
        for id in ids {
            self.storage.content_sizes.borrow_mut().remove(&id);
            self.storage.observed_sizes.borrow_mut().remove(&id);
            self.storage.observed_bounds.borrow_mut().remove(&id);
            self.storage.observed_visibility.borrow_mut().remove(&id);
            if !self.storage.layout_listeners.borrow().is_empty() {
                // Dropped outside the borrow: a listener's captures may reenter.
                let listeners = self.storage.layout_listeners.borrow_mut().remove(&id);
                drop(listeners);
            }
            self.input.unregister(id);
            self.semantics.borrow_mut().remove(id);
        }
        drop(cached_layout);
        self.shrink_storage();
        drop(owned);
        drop(editors);
        drop(paste_hooks);
        if refocus.is_some() && self.input.focused().is_none() {
            self.storage.refocus.set(refocus);
        }
    }
    /// `remove` for a subtree of more than `RECLAIM_NOW` nodes: freeing
    /// each node takes a few hundred nanoseconds, tens of milliseconds for
    /// a page of tens of thousands, so the subtree goes from the scene, the
    /// accessibility tree and input at once, and its nodes are freed over
    /// the next frames. Its effects stop now and its components' resources
    /// (tasks, timers, cleanups) drop now, as a small subtree's do.
    fn remove_large(&mut self, node: NodeId) {
        let (cached_layout, refocus) = self.before_removal(node);
        // No id names its nodes from here: the scene no longer has them.
        let ids = self.scene.borrow_mut().kill(node);
        let resources: Vec<_> = {
            let mut storage = self.storage.owned.borrow_mut();
            let mut kept = Vec::new();
            let mut effects = Vec::with_capacity(ids.len());
            for id in &ids {
                if let Some(owned) = storage.get(id) {
                    effects.extend(owned.effects.iter());
                    if !owned.resources.is_empty() {
                        kept.push(*id);
                    }
                }
            }
            self.runtime.stop_effects(effects);
            let mut resources = Vec::with_capacity(kept.len());
            for id in kept {
                if let Some(owned) = storage.get_mut(&id) {
                    resources.push(std::mem::take(&mut owned.resources));
                }
            }
            resources
        };
        let editors: Vec<_> = {
            let scene = self.scene.borrow();
            let mut storage = self.storage.editors.borrow_mut();
            storage.extract_if(|id, _| !scene.contains(*id)).collect()
        };
        let paste_hooks: Vec<_> = {
            let scene = self.scene.borrow();
            let mut storage = self.storage.paste_hooks.borrow_mut();
            storage.extract_if(|id, _| !scene.contains(*id)).collect()
        };
        self.semantics.borrow_mut().bury(&ids);
        self.input.forget_removed(&self.scene);
        self.storage.dying.borrow_mut().extend(ids);
        self.storage.removed.set(true);
        // User destructors, once nothing names the subtree.
        drop(cached_layout);
        drop(resources);
        drop(editors);
        drop(paste_hooks);
        if refocus.is_some() && self.input.focused().is_none() {
            self.storage.refocus.set(refocus);
        }
        while self.storage.dying.borrow().len() > RECLAIM_BACKLOG {
            self.reclaim(std::time::Duration::ZERO);
        }
    }
}
/// A style `width` by `height` points.
pub fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}

impl Ui {
    /// Editable text with native-host clipboard/IME support. Multiline inputs use
    /// explicit line breaks by default; `EditorHandle::set_wrap` enables soft wrapping.
    pub fn text_input(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<String>,
        width: f32,
        multiline: bool,
    ) -> EditorHandle {
        self.build_text_input(parent, label.into(), value.into(), width, multiline, false)
    }
    /// Undecorated editable control used by the declarative view layer.
    pub(crate) fn text_input_styled(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: EditorValue,
        multiline: bool,
    ) -> EditorHandle {
        self.build_text_input(parent, label.into(), value, 240., multiline, true)
    }
    fn build_text_input(
        &mut self,
        parent: NodeId,
        label: String,
        value: EditorValue,
        width: f32,
        multiline: bool,
        styled: bool,
    ) -> EditorHandle {
        // Canonical model writeback can invoke observers that remove this
        // subtree. Finish attaching owned bindings and listeners first.
        self.runtime
            .clone()
            .batch(|| self.build_text_input_inner(parent, label, value, width, multiline, styled))
    }
    fn build_text_input_inner(
        &mut self,
        parent: NodeId,
        label: String,
        value: EditorValue,
        width: f32,
        multiline: bool,
        styled: bool,
    ) -> EditorHandle {
        let theme = self.theme_scope();
        let height = if multiline {
            156.
        } else {
            theme.number_untracked(tok::CONTROL_HEIGHT)
        };
        let (root, bg, bg_state) = if styled {
            let mut style = fixed(width, height);
            // As a CSS form control in flex: its width is a fixed intrinsic
            // one, never its text's (which scrolls inside it); a stretching
            // parent widens it, and a row too narrow for it shrinks it, to
            // nothing if need be.
            style.intrinsic_width = true;
            style.flex_shrink = 1.;
            style.padding_edges = Some(Insets {
                left: 8.,
                right: 8.,
                top: 6.,
                bottom: 6.,
            });
            (self.container(parent, Layout::Overlay, style), None, None)
        } else {
            let (root, bg, state) = self.control(parent, width, height, tok::INPUT_BG);
            (root, Some(bg), Some(state))
        };
        let mut clip_style = fixed(width - 16., height - 12.);
        clip_style.clip = true;
        let viewport = self.container(root, Layout::Overlay, clip_style);
        self.scene.borrow_mut().set_transform(
            viewport,
            if styled {
                Transform::default()
            } else {
                Transform { x: 8., y: 6. }
            },
        );
        let selection_layer = self.container(viewport, Layout::Overlay, Style::default());
        // Line numbers (`set_line_numbers`): a clipped column beside the
        // viewport, which moves right to make room.
        let gutter = self.container(root, Layout::Overlay, fixed(0., 0.));
        // The text, caret and selection are placed from the text's own
        // layout, which already runs the way the text does (`editor_font`):
        // the scene must not mirror them again.
        {
            let mut scene = self.scene.borrow_mut();
            scene.set_direction(viewport, Some(crate::scene::Direction::Ltr));
            scene.set_direction(gutter, Some(crate::scene::Direction::Ltr));
        }
        let number_nodes: Rc<RefCell<Vec<NodeId>>> = Rc::default();
        let line_numbers = Rc::new(Cell::new(false));
        let draw_numbers = line_numbers.clone();
        let mut initial = value.value_untracked();
        let canonical = match &initial {
            TextValue::Flat(text) => match normalize_text(text, multiline) {
                std::borrow::Cow::Owned(canonical) => Some(canonical),
                std::borrow::Cow::Borrowed(_) => None,
            },
            TextValue::Long(text) if !is_canonical(text, multiline) => {
                Some(normalize_text(&text.to_cow(), multiline).into_owned())
            }
            TextValue::Long(_) => None,
        };
        if let Some(canonical) = canonical {
            initial = canonical.into();
        }
        // A long text's lines show apart (`editor_lines`): its node, never
        // laid out with all of it, starts empty.
        let shown = match &initial {
            TextValue::Flat(text) if !crate::editor_lines::is_long(text) => text.clone(),
            _ => String::new(),
        };
        let text = self.label(viewport, shown, fixed(width - 16., height - 12.));
        self.semantics.borrow_mut().remove(text);
        let preedit_layer = self.container(viewport, Layout::Overlay, Style::default());
        let preedit_nodes = Rc::new(RefCell::new(Vec::new()));
        let spell_nodes: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let caret = self.rect(
            viewport,
            theme.color_untracked(tok::TEXT_PRIMARY),
            1.,
            theme.number_untracked(tok::FONT_SIZE_MD) * 1.4,
            0.,
            0.,
        );
        let editor = Rc::new(RefCell::new(TextEditor::with_value(initial)));
        let focused = Rc::new(RefCell::new(false));
        // Blurred only because its window went to the back: still the focused field.
        let window_blurred = Rc::new(Cell::new(false));
        let draw_window_blurred = window_blurred.clone();
        let scroll = Rc::new(RefCell::new((0f32, 0f32)));
        // Carries the selection's highlight, made when one first shows.
        let selection_node: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
        // A selection showing when the theme (or the system's accent)
        // changes takes the new look, as one drawn after it would.
        {
            let (node, scene, theme) = (selection_node.clone(), self.scene.clone(), theme.clone());
            self.bind_marked(root, true, move || {
                let color = theme.color(tok::SELECTION);
                let mut s = scene.borrow_mut();
                if let Some(id) = node.get()
                    && let Some(shown) = s.text_highlight(id).cloned()
                {
                    let look = crate::scene::TextHighlight {
                        color,
                        radius: theme.number_untracked(tok::SELECTION_RADIUS).max(0.),
                        ..shown
                    };
                    s.set_text_highlight(id, Some(look));
                }
            });
        }
        self.semantics.borrow_mut().set(
            root,
            SemanticNode::new(
                if multiline {
                    Role::MultilineTextInput
                } else {
                    Role::TextInput
                },
                label,
            ),
        );
        // Its text shows in `text`: where assistive technology finds its
        // lines' geometry.
        self.semantics.borrow_mut().set_text_node(root, text);
        let scene = self.scene.clone();
        let sem = self.semantics.clone();
        let edit = editor.clone();
        let focus = focused.clone();
        let draw_caret_width = self.storage.caret_width.clone();
        let draw_blink = self.storage.caret_blink.clone();
        let scroll_for_draw = scroll.clone();
        let reveal_caret = Rc::new(Cell::new(true));
        let draw_reveal_caret = reveal_caret.clone();
        let scroll_limit = Rc::new(Cell::new((0f32, 0f32)));
        let draw_scroll_limit = scroll_limit.clone();
        let typography = Rc::new(RefCell::new((
            theme.color_untracked(tok::TEXT_PRIMARY),
            theme.number_untracked(tok::FONT_SIZE_MD),
            FontStyle::default(),
        )));
        let wrap = Rc::new(Cell::new(false));
        let preferred_x = Rc::new(Cell::new(None));
        let position = Rc::new(Cell::new(TextPosition {
            byte_offset: 0,
            affinity: TextAffinity::After,
        }));
        let draw_wrap = wrap.clone();
        let draw_position = position.clone();
        let geometry = Rc::new(RefCell::new(None));
        let draw_typography = typography.clone();
        let draw_geometry = geometry.clone();
        let draw_storage = Rc::downgrade(&self.storage);
        let read_only = Rc::new(Cell::new(false));
        let draw_read_only = read_only.clone();
        let gesture = Rc::new(EditorDrag::default());
        let draw_gesture = gesture.clone();
        let hint = Rc::new(EditorHint::new(value));
        let wheel_hint = hint.clone();
        hint.composition_layer.set(Some(preedit_layer));
        let draw_hint = hint.clone();
        // Its text's line count and its semantic value, by the revision of
        // the text they are for.
        let draw_line_count: Cell<Option<(u64, usize)>> = Cell::new(None);
        let draw_semantic_revision: Cell<Option<u64>> = Cell::new(None);
        let watched_theme = theme.clone();
        let refresh: Rc<dyn Fn()> = Rc::new(move || {
            let e = edit.borrow();
            let selection = e.selection();
            // What shows: the committed text with the composition in it. A
            // long text's is its chunks, shared (a composition goes into a
            // copy of them, its own chunks made again); a short one's, its
            // string.
            let display_text = e.long_text().map(|text| match e.preedit() {
                Some(preedit) => {
                    let mut text = text.clone();
                    text.replace_range(selection.range(), &preedit.text);
                    text
                }
                None => text.clone(),
            });
            let mut display = if display_text.is_some() {
                std::borrow::Cow::Borrowed("")
            } else {
                e.text()
            };
            let display_cursor = if let Some(preedit) = e.preedit() {
                let r = selection.range();
                if display_text.is_none() {
                    display.to_mut().replace_range(r.clone(), &preedit.text);
                }
                r.start
                    + preedit
                        .cursor
                        .map(|(_, end)| end)
                        .unwrap_or(preedit.text.len())
            } else {
                selection.focus
            };
            let display_len = display_text.as_ref().map_or(display.len(), Text::len);
            let display_cursor = display_cursor.min(display_len);
            // The text the long layout reads, made only if its revision is new.
            let display_source = || display_text.clone().unwrap_or_else(|| Text::new(&display));
            let mut s = scene.borrow_mut();
            if !s.contains(root) {
                return;
            }
            s.prepare_layout();
            let bounds = s.bounds(root);
            let root_style = s.style(root);
            *draw_geometry.borrow_mut() = Some((
                Rect::new(0., 0., bounds.width, bounds.height),
                root_style.clone(),
                s.text_shaper_revision(),
                s.content_insets(root),
            ));
            let padding = if styled {
                s.content_insets(root)
            } else {
                Insets {
                    left: 8.,
                    right: 8.,
                    top: 6.,
                    bottom: 6.,
                }
            };
            let (color, size, font) = draw_typography.borrow().clone();
            let font = crate::text_layout::FontStyle {
                masked: draw_hint.secure.get(),
                ..font
            };
            // The gutter: as wide as the last line's number (two digits at
            // least, so the text does not move at line 10), and a gap.
            let numbered = draw_numbers.get();
            // Long, or coloured a line at a time: laid out by its lines.
            let long = display_text.is_some()
                || crate::editor_lines::is_long(&display)
                || draw_hint.highlighted();
            let (digits, digit) = if numbered {
                // Unwrapped, a long text's layout (its width is not the
                // gutter's to give) has them counted.
                let unwrapped_long = (long && !draw_wrap.get()).then(|| {
                    // In the font its layout below has, so it is the same one.
                    let font = editor_font(&s, root, &font).into_owned();
                    let revision = e.preedit().is_none().then(|| e.revision());
                    long_layout(
                        &draw_hint.line_cache(),
                        &s,
                        display_source,
                        revision,
                        size,
                        None,
                        &font,
                    )
                });
                let lines = match draw_line_count.get() {
                    _ if unwrapped_long.is_some() => unwrapped_long.as_ref().unwrap().line_count(),
                    // Counted for this text (a scroll, a caret move).
                    Some((revision, count))
                        if e.preedit().is_none() && revision == e.revision() =>
                    {
                        count
                    }
                    _ => {
                        let count = if let Some(text) = &display_text {
                            text.line_count()
                        } else if long {
                            display.match_indices('\n').count() + 1
                        } else {
                            memchr_count(b'\n', display.as_bytes()) + 1
                        };
                        if e.preedit().is_none() {
                            draw_line_count.set(Some((e.revision(), count)));
                        }
                        count
                    }
                };
                let digits = lines.max(10).ilog10() as usize + 1;
                (digits, s.measure_text_with_font("0", size, None, &font).0)
            } else {
                (0, 0.)
            };
            let gutter_width = if numbered {
                GUTTER_INSET + digit * digits as f32 + GUTTER_GAP
            } else {
                0.
            };
            let inner_width = (bounds.width - padding.left - padding.right - gutter_width).max(0.);
            let inner_height = (bounds.height - padding.top - padding.bottom).max(0.);
            let mut viewport_style = fixed(inner_width, inner_height);
            viewport_style.clip = true;
            s.set_style(viewport, viewport_style);
            let origin = if styled { (0., 0.) } else { (8., 6.) };
            s.set_transform(
                viewport,
                Transform {
                    x: origin.0 + gutter_width,
                    y: origin.1,
                },
            );
            let mut gutter_style = fixed(gutter_width, if numbered { inner_height } else { 0. });
            gutter_style.clip = true;
            s.set_style(gutter, gutter_style);
            s.set_transform(
                gutter,
                Transform {
                    x: origin.0,
                    y: origin.1,
                },
            );
            if let Some(bg) = bg {
                s.set_style(bg, fixed(bounds.width, bounds.height));
            }
            // Empty, the text node shows the placeholder instead (laid out
            // as the empty text it stands for: it sizes nothing).
            let placeholder = draw_hint.placeholder.borrow();
            let shown = placeholder.as_ref().filter(|_| display_len == 0);
            let (shown_text, shown_color) = match shown {
                Some((hint, hint_color)) => (
                    hint.as_ref(),
                    hint_color.unwrap_or_else(|| {
                        draw_hint
                            .themed
                            .set(draw_hint.themed.get() | THEMED_TERTIARY);
                        theme.color_untracked(tok::TEXT_TERTIARY)
                    }),
                ),
                None => (display.as_ref(), color),
            };
            // A long text's lines show apart (below): this node holds none.
            let shown_text = if long { "" } else { shown_text };
            // Selection and policy changes retain the same immutable paint text.
            let paint_text = match s.kind(text) {
                NodeKind::Text { text, .. } if text.as_ref() == shown_text => text.clone(),
                _ => std::sync::Arc::from(shown_text),
            };
            s.set_kind(
                text,
                NodeKind::Text {
                    text: paint_text,
                    color: shown_color,
                    font_size: size,
                },
            );
            let font = editor_font(&s, root, &font).into_owned();
            s.set_font(text, font.clone());
            let caret_color = draw_hint.caret.get().0.unwrap_or(color);
            s.set_kind(caret, NodeKind::Rect(caret_color));
            let wrap_width = draw_wrap.get().then_some(inner_width.max(1.));
            // The committed text's revision tells its layout it is the same
            // (a scroll), without comparing a long document.
            let revision = e.preedit().is_none().then(|| e.revision());
            let long_lines = long.then(|| {
                long_layout(
                    &draw_hint.line_cache(),
                    &s,
                    display_source,
                    revision,
                    size,
                    wrap_width,
                    &font,
                )
            });
            let layout: Rc<dyn TextLayout> = match &long_lines {
                Some(lines) => lines.clone(),
                None => editor_layout(&draw_storage, &s, root, &display, size, wrap_width, &font),
            };
            let prior_position = draw_position.get();
            let caret_position =
                if e.preedit().is_none() && prior_position.byte_offset == display_cursor {
                    prior_position
                } else {
                    TextPosition {
                        byte_offset: display_cursor,
                        affinity: TextAffinity::After,
                    }
                };
            let caret_rect = layout.caret_position(caret_position);
            let cx = caret_rect.x;
            let cy = caret_rect.y;
            let line_height = caret_rect.height;
            let mut scroll = scroll_for_draw.borrow_mut();
            let reveal = draw_reveal_caret.replace(true) && draw_gesture.mode.get().is_none();
            // A place asked for (find's match), else the caret.
            let (place, align) = draw_hint.reveal_at.take().unzip();
            let (rx, ry, rh, asked) = match place {
                Some(at) => {
                    let r = layout.caret_position(TextPosition {
                        byte_offset: at,
                        affinity: TextAffinity::After,
                    });
                    (r.x, r.y, r.height, true)
                }
                _ => (cx, cy, line_height, draw_hint.reveal.take()),
            };
            if (*focus.borrow() || asked) && reveal {
                if rx - scroll.0 > (inner_width - 8.).max(0.) {
                    scroll.0 = rx - ((inner_width - 8.).max(0.));
                }
                if rx < scroll.0 {
                    scroll.0 = rx;
                }
                if ry - scroll.1 > (inner_height - rh).max(0.) {
                    scroll.1 = ry - ((inner_height - rh).max(0.));
                }
                if ry < scroll.1 {
                    scroll.1 = ry;
                }
                match align {
                    Some(crate::compose::RowAlign::Start) => scroll.1 = ry,
                    Some(crate::compose::RowAlign::Center) => {
                        scroll.1 = ry - (inner_height - rh) / 2.
                    }
                    Some(crate::compose::RowAlign::End) => scroll.1 = ry - (inner_height - rh),
                    _ => {}
                }
            }
            let (tw, th) = layout.size();
            let limit = (
                if draw_wrap.get() {
                    0.
                } else {
                    (tw + 2. - inner_width).max(0.)
                },
                (th - inner_height).max(0.),
            );
            draw_scroll_limit.set(limit);
            // Unwrapped text aligned right (right to left, by default) or
            // centred lies that far into the field while it is narrower:
            // as a scroll before its start, so the caret, selection and
            // pointer, all placed from the layout, follow it.
            let factor = match font.align {
                _ if draw_wrap.get() => 0.,
                crate::text_layout::TextAlign::Right => 1.,
                crate::text_layout::TextAlign::Center => 0.5,
                _ => 0.,
            };
            let lead = (inner_width - 1. - tw).max(0.) * factor;
            scroll.0 = if lead > 0. {
                -lead
            } else {
                scroll.0.clamp(0., limit.0)
            };
            scroll.1 = if multiline {
                scroll.1.clamp(0., limit.1)
            } else {
                // One line, centred in the box (negative: pushed down).
                (line_height - inner_height) / 2.
            };
            if let Some(code) = draw_hint.code()
                && code.top_line_listened.get()
            {
                let top = layout.hit_test(0., scroll.1 + line_height / 2.);
                code.top_line_drawn.set(Some(e.line_of(top) + 1));
            }
            let caret_width = draw_hint
                .caret
                .get()
                .1
                .unwrap_or_else(|| draw_caret_width.get());
            s.set_style(caret, fixed(caret_width, line_height));
            // Aligned, the text's box is its layout's width, which the text
            // aligns in as the layout does; its placeholder, the field's.
            let aligned_placeholder = factor > 0. && shown.is_some();
            let unwrapped = if aligned_placeholder {
                (inner_width - 1.).max(1.)
            } else if factor > 0. {
                tw + 0.01
            } else {
                tw.max(inner_width) + 2.
            };
            let mut text_style = fixed(
                wrap_width.unwrap_or(unwrapped),
                // A wrapped placeholder may take more lines than the text.
                if shown.is_some() {
                    th.max(inner_height)
                } else {
                    th
                },
            );
            text_style.text_wrap = draw_wrap.get();
            s.set_style(text, text_style);
            s.set_transform(
                text,
                Transform {
                    x: if aligned_placeholder { 0. } else { -scroll.0 },
                    y: -scroll.1,
                },
            );
            // A long text shows the lines in view, a text node each (by line
            // number: a scroll gives one node a new line, not each of them).
            let shown_lines = match &long_lines {
                Some(lines) => lines.lines_between(scroll.1, scroll.1 + inner_height),
                None => 0..0,
            };
            // (Only an editor that ever showed a long text has these.)
            let code = if long_lines.is_some() {
                Some(draw_hint.code_made())
            } else {
                draw_hint.code()
            };
            if let Some(code) = code {
                let mut pool = code.line_nodes.borrow_mut();
                if !shown_lines.is_empty()
                    && code.line_layer.get().is_none_or(|layer| !s.contains(layer))
                {
                    let layer = s.append(
                        viewport,
                        NodeKind::Container(Layout::Overlay),
                        Style::default(),
                    );
                    // Where the text node is: over the selection, under the caret.
                    let mut order = s.children(viewport).to_vec();
                    order.retain(|id| *id != layer);
                    let at = order
                        .iter()
                        .position(|id| *id == text)
                        .map_or(order.len(), |i| i + 1);
                    order.insert(at, layer);
                    s.reorder_children(viewport, &order);
                    code.line_layer.set(Some(layer));
                    pool.clear();
                }
                let (kept, free): (Vec<_>, Vec<_>) = pool
                    .drain(..)
                    .partition(|(line, _)| shown_lines.contains(line));
                let mut free: Vec<NodeId> = free.into_iter().map(|(_, id)| id).collect();
                pool.extend(kept);
                if let (Some(lines), Some(layer)) = (&long_lines, code.line_layer.get()) {
                    for i in shown_lines.clone() {
                        if pool.iter().any(|(line, _)| *line == i) {
                            continue;
                        }
                        let id = free.pop().unwrap_or_else(|| {
                            s.append(
                                layer,
                                NodeKind::Text {
                                    text: std::sync::Arc::from(""),
                                    color,
                                    font_size: size,
                                },
                                Style::default(),
                            )
                        });
                        pool.push((i, id));
                    }
                    for (i, id) in pool.iter() {
                        let line = lines.line_text(*i);
                        let colored = draw_hint.line_cache().spans(lines, *i).and_then(|spans| {
                            crate::editor_lines::styled_line(&line, &spans, color, size, &font)
                        });
                        match colored {
                            Some(rich) => {
                                if !matches!(s.kind(*id), NodeKind::RichText { text } if **text == rich)
                                {
                                    s.set_kind(*id, NodeKind::RichText { text: rich.into() });
                                }
                            }
                            None => {
                                let unchanged = matches!(
                                    s.kind(*id),
                                    NodeKind::Text { text, color: c, font_size }
                                        if **text == *line && *c == color && *font_size == size
                                );
                                if !unchanged {
                                    s.set_kind(
                                        *id,
                                        NodeKind::Text {
                                            text: std::sync::Arc::from(&*line),
                                            color,
                                            font_size: size,
                                        },
                                    );
                                }
                            }
                        }
                        s.set_font(*id, font.clone());
                        let width = wrap_width.unwrap_or_else(|| lines.line(*i).size().0 + 2.);
                        let mut style = fixed(width, lines.height(*i));
                        style.text_wrap = draw_wrap.get();
                        s.set_style(*id, style);
                        s.set_transform(
                            *id,
                            Transform {
                                x: -scroll.0,
                                y: lines.top(*i) - scroll.1,
                            },
                        );
                    }
                }
                for id in free {
                    s.remove(id);
                }
                drop(pool);
            }
            s.set_transform(
                caret,
                Transform {
                    x: if draw_wrap.get() {
                        cx.min((inner_width - caret_width).max(0.))
                    } else {
                        cx - scroll.0
                    },
                    y: cy - scroll.1,
                },
            );
            let mut effects = s.effects(caret);
            // Read-only text shows a selection but no caret.
            let caret_shown = *focus.borrow()
                && !draw_read_only.get()
                && e.preedit().is_none_or(|preedit| preedit.cursor.is_some());
            // Shown, it paints unless the host draws it (`Ui::native_caret`).
            effects.opacity = if caret_shown && !draw_blink.drawn_natively() {
                1.
            } else {
                0.
            };
            s.set_effects(caret, effects);
            // Drawn again (typed, selected, moved): solid, then blinking.
            if caret_shown {
                // A bare insertion point: no composition, no selection.
                let bare = e.preedit().is_none() && selection.range().is_empty();
                draw_blink.restart(caret, caret_color, Instant::now(), bare);
            } else {
                draw_blink.release(caret);
            }
            // An editable field shows its selection only while it has the focus (or its
            // window merely went to the back), as AppKit's do: the selection is kept, and
            // shows again when the focus comes back. Read-only text gives its up on blur.
            let shows_selection =
                *focus.borrow() || draw_window_blurred.get() || draw_read_only.get();
            // With a line more each way, so the lines in view know what they
            // meet: a selection running on past the edge is not rounded off.
            let rects: Vec<_> =
                if e.preedit().is_none() && shows_selection {
                    let (top, bottom) = (
                        scroll.1 - line_height,
                        scroll.1 + inner_height + line_height,
                    );
                    layout
                        .selection_within(selection.range(), top, bottom)
                        .into_iter()
                        .filter_map(|r| {
                            let y = r.y - scroll.1;
                            (y + r.height > -line_height && y < inner_height + line_height)
                                .then_some((r.x - scroll.0, y, r.width, r.height))
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            // Behind the selection: the caret's row and the marked ranges
            // (a search's matches), those in view.
            if let Some(code) = draw_hint.code() {
                let mut marks: Vec<(Rect, Color)> = Vec::new();
                if let Some(row) = code.current_line.get() {
                    marks.push((Rect::new(0., cy - scroll.1, inner_width, line_height), row));
                }
                // Each range one shape, as a selection is.
                let mut shaped: Vec<crate::scene::TextMark> = Vec::new();
                let mut ends = Vec::new();
                {
                    let decorations = code.decorations.borrow();
                    // Those in the text in view: from where the first line
                    // shown starts (all of them, if it is short).
                    let shown_from = match &long_lines {
                        Some(lines) => lines.line_range(lines.line_at(scroll.1)).start,
                        None => 0,
                    };
                    let first = decorations.partition_point(|(range, _)| range.end < shown_from);
                    for (range, mark) in &decorations[first..] {
                        let rects = layout.selection_within(
                            range.clone(),
                            scroll.1,
                            scroll.1 + inner_height,
                        );
                        if rects.is_empty()
                            && long_lines.as_ref().is_some_and(|lines| {
                                lines.top(lines.line_of(range.start)) > scroll.1 + inner_height
                            })
                        {
                            break;
                        }
                        shaped.extend(rects.into_iter().map(|r| crate::scene::TextMark {
                            rect: Rect::new(r.x - scroll.0, r.y - scroll.1, r.width, r.height),
                            color: *mark,
                        }));
                        ends.push(shaped.len());
                    }
                }
                if (!marks.is_empty() || !shaped.is_empty())
                    && code
                        .decoration_layer
                        .get()
                        .is_none_or(|layer| !s.contains(layer))
                {
                    let layer = s.append(
                        viewport,
                        NodeKind::Container(Layout::Overlay),
                        Style::default(),
                    );
                    let mut order = s.children(viewport).to_vec();
                    order.retain(|id| *id != layer);
                    let at = order
                        .iter()
                        .position(|id| *id == selection_layer)
                        .unwrap_or(0);
                    order.insert(at, layer);
                    s.reorder_children(viewport, &order);
                    code.decoration_layer.set(Some(layer));
                    code.decoration_nodes.borrow_mut().clear();
                    code.decoration_text.set(None);
                }
                let mut marked = code.decoration_nodes.borrow_mut();
                while marked.len() > marks.len() {
                    s.remove(marked.pop().unwrap());
                }
                if let Some(layer) = code.decoration_layer.get() {
                    while marked.len() < marks.len() {
                        marked.push(s.append(layer, NodeKind::Rect(color), Style::default()));
                    }
                    for (id, (rect, mark)) in marked.iter().zip(marks) {
                        if !matches!(s.kind(*id), NodeKind::Rect(c) if *c == mark) {
                            s.set_kind(*id, NodeKind::Rect(mark));
                        }
                        s.set_style(*id, fixed(rect.width.max(1.), rect.height));
                        s.set_transform(
                            *id,
                            Transform {
                                x: rect.x,
                                y: rect.y,
                            },
                        );
                    }
                }
                drop(marked);
                // The decorations, over the current line: a node of their own.
                if !shaped.is_empty() || code.decoration_text.get().is_some() {
                    let size = (inner_width, inner_height);
                    let radius = theme.number_untracked(tok::MARK_RADIUS);
                    show_decorations(&mut s, code, size, (shaped, ends), radius);
                }
                // Under the text in view, each row of a range a node.
                let underlines = code.underlines.borrow();
                let mut rows: Vec<(Rect, Color, crate::rich_text::LineStyle)> = Vec::new();
                let shown_from = match &long_lines {
                    Some(lines) => lines.line_range(lines.line_at(scroll.1)).start,
                    None => 0,
                };
                let first = underlines.partition_point(|u| u.range.end < shown_from);
                for underline in &underlines[first..] {
                    let found = layout.selection_within(
                        underline.range.clone(),
                        scroll.1,
                        scroll.1 + inner_height,
                    );
                    if found.is_empty()
                        && long_lines.as_ref().is_some_and(|lines| {
                            lines.top(lines.line_of(underline.range.start))
                                > scroll.1 + inner_height
                        })
                    {
                        break;
                    }
                    rows.extend(found.into_iter().map(|r| {
                        let at = Rect::new(r.x - scroll.0, r.y - scroll.1, r.width, r.height);
                        (at, underline.color, underline.line)
                    }));
                }
                let mut drawn = code.underline_nodes.borrow_mut();
                while drawn.len() > rows.len() {
                    s.remove(drawn.pop().unwrap().0);
                }
                for (k, (rect, color, line)) in rows.into_iter().enumerate() {
                    let wanted = ((rect.width * 4.) as u32, color, line);
                    if k == drawn.len() {
                        let id = s.append(viewport, NodeKind::Container(Layout::Overlay), {
                            Style::default()
                        });
                        drawn.push((id, (u32::MAX, color, line)));
                    }
                    let (id, shown) = &mut drawn[k];
                    if *shown != wanted {
                        *shown = wanted;
                        s.set_kind(
                            *id,
                            NodeKind::Canvas(std::sync::Arc::new(underline_canvas(
                                rect.width, color, line,
                            ))),
                        );
                    }
                    s.set_style(*id, fixed(rect.width.max(1.), 3.));
                    s.set_transform(
                        *id,
                        Transform {
                            x: rect.x,
                            y: rect.y + rect.height - 3.,
                        },
                    );
                }
            }
            // The selection, one shape over the view: its node shows no text
            // of its own, a long document's lines being nodes apart.
            if rects.is_empty() {
                if let Some(id) = selection_node.get() {
                    s.set_text_highlight(id, None);
                }
            } else {
                let id = selection_node.get().unwrap_or_else(|| {
                    let id = s.append(
                        selection_layer,
                        NodeKind::Text {
                            text: "".into(),
                            color: Color(0, 0, 0, 0),
                            font_size: 1.,
                        },
                        Style::default(),
                    );
                    selection_node.set(Some(id));
                    id
                });
                // Padded: flush takes its ink from its box, shaping nothing.
                let mut style = fixed(inner_width, inner_height);
                style.padding = 1.;
                s.set_style(id, style);
                let highlight = crate::scene::TextHighlight {
                    color: theme.color_untracked(tok::SELECTION),
                    rects: rects
                        .into_iter()
                        .map(|(x, y, w, h)| Rect::new(x, y, w, h))
                        .collect(),
                    text_color: None,
                    radius: theme.number_untracked(tok::SELECTION_RADIUS).max(0.),
                };
                s.set_text_highlight(id, Some(highlight));
            }
            // The numbers of the lines in view: found by where each line
            // starts (rows are in order, so the first is a binary search),
            // built only for those.
            let mut shown: Vec<(usize, f32)> = Vec::new();
            let mut current = 0;
            if numbered && let Some(lines) = &long_lines {
                // From the line index: no line out of view is shaped.
                for i in lines.line_at(scroll.1)..lines.line_count() {
                    let y = lines.top(i) - scroll.1;
                    if y >= inner_height {
                        break;
                    }
                    if y + lines.height(i) > 0. {
                        shown.push((i + 1, y));
                    }
                }
                current = lines.line_of(display_cursor) + 1;
            } else if numbered {
                let starts: Vec<usize> = std::iter::once(0)
                    .chain(display.match_indices('\n').map(|(at, _)| at + 1))
                    .collect();
                let top_of = |byte_offset: usize| {
                    layout
                        .caret_position(TextPosition {
                            byte_offset,
                            affinity: TextAffinity::After,
                        })
                        .y
                        - scroll.1
                };
                let first = starts
                    .partition_point(|&start| top_of(start) + line_height <= 0.)
                    .saturating_sub(1);
                for (line, &start) in starts.iter().enumerate().skip(first) {
                    let y = top_of(start);
                    if y >= inner_height {
                        break;
                    }
                    if y + line_height > 0. {
                        shown.push((line + 1, y));
                    }
                }
                current = starts.partition_point(|&start| start <= display_cursor);
            }
            let mut numbers = number_nodes.borrow_mut();
            while numbers.len() > shown.len() {
                s.remove(numbers.pop().unwrap());
            }
            while numbers.len() < shown.len() {
                numbers.push(s.append(
                    gutter,
                    NodeKind::Text {
                        text: std::sync::Arc::from(""),
                        color,
                        font_size: size,
                    },
                    Style::default(),
                ));
            }
            if !numbers.is_empty() {
                let themed = draw_hint.themed.get();
                draw_hint
                    .themed
                    .set(themed | THEMED_TERTIARY | THEMED_SECONDARY);
            }
            let (dim, bright) = (
                theme.color_untracked(tok::TEXT_TERTIARY),
                theme.color_untracked(tok::TEXT_SECONDARY),
            );
            // Right-aligned in their column (padding them with figure spaces
            // took a font that has them: one without, a fallback's wider
            // spaces, wrapped them out of sight).
            let number_font = crate::text_layout::FontStyle {
                align: crate::text_layout::TextAlign::End,
                ..font.clone()
            };
            for (id, (line, y)) in numbers.iter().zip(shown) {
                let label = line.to_string();
                let color = if line == current { bright } else { dim };
                let unchanged = matches!(
                    s.kind(*id),
                    NodeKind::Text { text, color: c, font_size }
                        if **text == *label && *c == color && *font_size == size
                );
                if !unchanged {
                    s.set_kind(
                        *id,
                        NodeKind::Text {
                            text: std::sync::Arc::from(label),
                            color,
                            font_size: size,
                        },
                    );
                    s.set_font(*id, number_font.clone());
                }
                s.set_style(*id, fixed(digit * digits as f32 + 1., line_height));
                s.set_transform(*id, Transform { x: GUTTER_INSET, y });
            }
            drop(numbers);
            // Marked text, underlined as a native field underlines it: a thin
            // line under all of it, a thick one under the clause the input
            // method is converting (its cursor range), where its candidates
            // show. Given its clauses (Windows' display attributes), a line
            // under each, apart from the next, thick under the target.
            let mut clause_area = None;
            let lines: Vec<_> = if let Some(preedit) = e.preedit() {
                let clauses = e.preedit_clauses();
                let start = selection.range().start;
                let line = |r: Rect, thick: f32| {
                    let y = mark_y(r, 1. + thick, thick, scroll.1, inner_height);
                    (y >= 0. && y < inner_height).then_some((r.x - scroll.0, y, r.width, thick))
                };
                if !clauses.is_empty() {
                    let mut lines = Vec::new();
                    for (range, style) in &clauses {
                        let rects = layout.selection(start + range.start..start + range.end);
                        let thick = if *style == ClauseStyle::Target {
                            2.
                        } else {
                            1.
                        };
                        if *style == ClauseStyle::Target && clause_area.is_none() {
                            clause_area = rects.first().map(|r| {
                                Rect::new(r.x - scroll.0, r.y - scroll.1, r.width, r.height)
                            });
                        }
                        // A pixel apart from the next clause's.
                        lines.extend(rects.into_iter().filter_map(|r| {
                            line(Rect::new(r.x, r.y, (r.width - 1.).max(1.), r.height), thick)
                        }));
                    }
                    lines
                } else {
                    let mut lines: Vec<_> = layout
                        .selection(start..start + preedit.text.len())
                        .into_iter()
                        .filter_map(|r| line(r, 1.))
                        .collect();
                    if let Some((a, b)) = preedit
                        .cursor
                        .filter(|(a, b)| a < b && *b <= preedit.text.len())
                    {
                        let clause = layout.selection(start + a..start + b);
                        clause_area = clause
                            .first()
                            .map(|r| Rect::new(r.x - scroll.0, r.y - scroll.1, r.width, r.height));
                        lines.extend(clause.into_iter().filter_map(|r| line(r, 2.)));
                    }
                    lines
                }
            } else {
                Vec::new()
            };
            draw_hint.clause.set(clause_area);
            let mut underline = preedit_nodes.borrow_mut();
            while underline.len() > lines.len() {
                s.remove(underline.pop().unwrap());
            }
            if !lines.is_empty() {
                draw_hint.themed.set(draw_hint.themed.get() | THEMED_ACCENT);
                let accent = theme.color_untracked(tok::ACCENT);
                while underline.len() < lines.len() {
                    underline.push(s.append(
                        preedit_layer,
                        surface(&theme, accent),
                        Style::default(),
                    ));
                }
                for (id, (x, y, w, thick)) in underline.iter().zip(lines) {
                    s.set_style(*id, fixed(w, thick));
                    s.set_transform(*id, Transform { x, y });
                    // Drawn before the accent changed: in the new one.
                    if matches!(s.kind(*id), NodeKind::Quad(q) if q.fill != accent) {
                        s.set_kind(*id, surface(&theme, accent));
                    }
                }
            }
            drop(underline);
            // Misspelled words: a dotted line under each, but not under the
            // word the caret of a focused editor is in (it is being typed).
            let caret_at =
                (*focus.borrow() && selection.anchor == selection.focus).then_some(selection.focus);
            let marks: Vec<_> = draw_hint
                .misspelled
                .borrow()
                .iter()
                .filter(|range| range.end <= e.len())
                .filter(|range| caret_at.is_none_or(|at| at < range.start || at > range.end))
                .flat_map(|range| {
                    layout.selection_within(range.clone(), scroll.1, scroll.1 + inner_height)
                })
                .filter_map(|r| {
                    let y = mark_y(r, 2., 2., scroll.1, inner_height);
                    (y >= 0. && y < inner_height).then_some((r.x - scroll.0, y, r.width))
                })
                .collect();
            let mut spelled = spell_nodes.borrow_mut();
            while spelled.len() > marks.len() {
                s.remove(spelled.pop().unwrap());
            }
            if !marks.is_empty() {
                draw_hint.themed.set(draw_hint.themed.get() | THEMED_DANGER);
                let danger = theme.color_untracked(tok::DANGER);
                while spelled.len() < marks.len() {
                    spelled.push(s.append(preedit_layer, spelling_mark(danger), Style::default()));
                }
                for (id, (x, y, w)) in spelled.iter().zip(marks) {
                    s.set_style(*id, fixed(w, 2.));
                    s.set_transform(*id, Transform { x, y });
                    if matches!(s.kind(*id), NodeKind::Quad(q) if q.border_color != danger) {
                        s.set_kind(*id, spelling_mark(danger));
                    }
                }
            }
            if let Some(code) = draw_hint.code()
                && code.caret_listened.get()
            {
                let focus = selection.focus.min(e.len());
                let (line, start) = match &long_lines {
                    Some(lines) if e.preedit().is_none() => {
                        let line = lines.line_of(focus);
                        (line, lines.line_range(line).start)
                    }
                    _ => {
                        let start = e.line_around(focus).start;
                        let before = e.slice(0..start);
                        (before.match_indices('\n').count(), start)
                    }
                };
                code.caret_drawn.set(Some(Caret {
                    line: line + 1,
                    column: e.slice(start..focus).chars().count() + 1,
                    anchor: selection.anchor,
                    focus,
                }));
            }
            let mut semantics = sem.borrow_mut();
            // The text, when it changed (a long document isn't compared on a
            // scroll); the selection always.
            if draw_semantic_revision.get() == Some(e.revision()) {
                semantics.update_text_selection(
                    root,
                    (selection.anchor, selection.focus),
                    draw_read_only.get(),
                );
            } else {
                match e.long_text() {
                    // A long text shared, not copied.
                    Some(text) => semantics.update_long_text_input(
                        root,
                        text,
                        (selection.anchor, selection.focus),
                        draw_read_only.get(),
                    ),
                    None => semantics.update_text_input(
                        root,
                        &e.text(),
                        (selection.anchor, selection.focus),
                        draw_read_only.get(),
                    ),
                }
                draw_semantic_revision.set(Some(e.revision()));
            }
            // What assistive technology reads of a long text: the lines in
            // view and a page each side, or a page around the caret when it
            // is scrolled out of view (`SemanticNode::long_lines`).
            if let (Some(text), Some(lines)) = (e.long_text(), &long_lines) {
                let caret = lines.line_of(selection.focus.min(e.len()));
                let page = shown_lines.len().max(20);
                let around = if shown_lines.contains(&caret) {
                    shown_lines.clone()
                } else {
                    caret..caret + 1
                };
                let window =
                    around.start.saturating_sub(page)..(around.end + page).min(text.line_count());
                semantics.update_long_lines(root, window);
            }
            semantics.set_placeholder(root, placeholder.as_ref().map(|(hint, _)| hint.as_ref()));
        });
        // Geometry refresh may make an existing stationary drag scrollable.
        // The weak callback avoids refresh -> arm -> tick -> refresh ownership cycles.
        let interaction_refresh: InteractionRefresh = Rc::new(RefCell::new(None));
        let refresh: Rc<dyn Fn()> = {
            let hook = interaction_refresh.clone();
            let told = hint.clone();
            let storage = Rc::downgrade(&self.storage);
            Rc::new(move || {
                refresh();
                // Spans that shape their lines found as it drew (the first
                // bold line): what it placed from plain lines is placed again.
                if told
                    .code()
                    .and_then(|code| code.lines.borrow().clone())
                    .is_some_and(|lines| lines.take_restyled())
                {
                    refresh();
                }
                EditorHint::watch_theme(&told, &storage, root, &watched_theme);
                // Where the caret is, told once drawn (nothing borrowed),
                // if it moved. The callback is out while it runs: a refresh
                // it causes finds none and leaves the telling to this one,
                // which tells again where that left the caret (a few times
                // at most, should callbacks keep moving it).
                if let Some(code) = told.code() {
                    for _ in 0..4 {
                        let Some(caret) = code.caret_drawn.get() else {
                            break;
                        };
                        if Some(caret) == code.caret_told.get() {
                            break;
                        }
                        let Some(mut callback) = code.on_caret.borrow_mut().take() else {
                            break;
                        };
                        code.caret_told.set(Some(caret));
                        callback(caret);
                        let mut slot = code.on_caret.borrow_mut();
                        if slot.is_none() {
                            *slot = Some(callback);
                        }
                    }
                    for _ in 0..4 {
                        let line = code.top_line_drawn.get();
                        if line.is_none() || line == code.top_line_told.get() {
                            break;
                        }
                        let Some(mut callback) = code.on_top_line.borrow_mut().take() else {
                            break;
                        };
                        code.top_line_told.set(line);
                        callback(line.unwrap_or(1));
                        let mut slot = code.on_top_line.borrow_mut();
                        if slot.is_none() {
                            *slot = Some(callback);
                        }
                    }
                }
                let arm = hook.borrow().as_ref().and_then(Weak::upgrade);
                if let Some(arm) = arm {
                    arm();
                }
            })
        };
        let handle = EditorHandle {
            multiline,
            read_only: read_only.clone(),
            caret,
            node: root,
            editor: editor.clone(),
            refresh: refresh.clone(),
            typography: typography.clone(),
            wrap: wrap.clone(),
            line_numbers: line_numbers.clone(),
            preferred_x: preferred_x.clone(),
            text,
            scene: self.scene.clone(),
            geometry,
            position: position.clone(),
            hint: hint.clone(),
        };
        self.storage
            .editors
            .borrow_mut()
            .insert(root, handle.clone());
        let edit = editor.clone();
        let model = hint.clone();
        let draw = refresh.clone();
        let model_position = position.clone();
        let model_preferred_x = preferred_x.clone();
        let model_gesture = gesture.clone();
        let model_storage = Rc::downgrade(&self.storage);
        self.bind_input(root, move || {
            // Canonical model text is compared and applied in place, without
            // cloning the full document; the signal's borrow is released
            // before anything else runs.
            let (changed, writeback) = model.value.apply(&edit, multiline);
            if changed {
                model_gesture.cancel();
                if let Some(storage) = model_storage.upgrade() {
                    let remove = storage
                        .interaction
                        .borrow()
                        .as_ref()
                        .is_some_and(|p| p.job.owner == root);
                    if remove {
                        let old = storage.interaction.borrow_mut().take();
                        drop(old);
                    }
                }
                model_preferred_x.set(None);
                model_position.set(TextPosition {
                    byte_offset: edit.borrow().selection().focus,
                    affinity: TextAffinity::After,
                });
            }
            if let Some(canonical) = writeback {
                model.value.set(canonical);
            }
            if !defer_input_draw(root, &draw, &model, &model_storage) {
                draw();
            }
        });
        let scene = self.scene.clone();
        // Which window's editor this is: a node id is each window's own.
        let scene_identity = scene.borrow().identity();
        let input_storage = Rc::downgrade(&self.storage);
        // The byte a drop at `x`, `y` (the window's points) goes in at.
        let drop_point: Rc<dyn Fn(f32, f32) -> usize> = {
            let scene = scene.clone();
            let editor = editor.clone();
            let scroll = scroll.clone();
            let typography = typography.clone();
            let wrap = wrap.clone();
            let input_storage = input_storage.clone();
            Rc::new(move |x, y| {
                let s = scene.borrow();
                let bounds = s.bounds(viewport);
                let offset = *scroll.borrow();
                let (_, size, font) = typography.borrow().clone();
                let edit = editor.borrow();
                let layout = committed_layout(
                    &input_storage,
                    &s,
                    root,
                    &edit,
                    size,
                    wrap.get().then_some(bounds.width.max(1.)),
                    &font,
                );
                let (x, y) = in_viewport(&s, viewport, x, y);
                let x = (x - bounds.x).clamp(0., bounds.width.max(0.)) + offset.0;
                let y = (y - bounds.y).clamp(0., (bounds.height - 0.01).max(0.)) + offset.1;
                layout.hit_position(x, y).byte_offset
            })
        };
        // Kept in the hint, which offset_at and the drop handler both reach.
        let _ = hint.point.set(drop_point);
        let drag_select: Rc<dyn Fn()> = {
            let scene = scene.clone();
            let gesture = gesture.clone();
            let editor = editor.clone();
            let scroll = scroll.clone();
            let typography = typography.clone();
            let wrap = wrap.clone();
            let position = position.clone();
            let input_storage = input_storage.clone();
            Rc::new(move || {
                let Some(mode) = gesture.mode.get() else {
                    return;
                };
                let s = scene.borrow();
                if !s.contains(root) {
                    return;
                }
                let bounds = s.bounds(viewport);
                let offset = *scroll.borrow();
                let (_, size, font) = typography.borrow().clone();
                let mut edit = editor.borrow_mut();
                let layout = committed_layout(
                    &input_storage,
                    &s,
                    root,
                    &edit,
                    size,
                    wrap.get().then_some(bounds.width.max(1.)),
                    &font,
                );
                let (px, py) = gesture.pointer.get();
                let (px, py) = in_viewport(&s, viewport, px, py);
                // Hit the visible edge, not an arbitrarily distant text row.
                let x = (px - bounds.x).clamp(0., bounds.width.max(0.)) + offset.0;
                let y = (py - bounds.y).clamp(0., (bounds.height - 0.01).max(0.)) + offset.1;
                let hit = layout.hit_position(x, y);
                let (anchor, focus) = match mode {
                    EditorPointerSelection::Character(anchor) => (anchor, hit),
                    EditorPointerSelection::Unit { edges, line } => {
                        let current = editor_pointer_unit(
                            edit.around(hit.byte_offset),
                            layout.as_ref(),
                            hit,
                            x,
                            y,
                            line,
                            multiline,
                        );
                        extend_by_units(edges, current)
                    }
                };
                edit.set_selection(anchor, focus.byte_offset);
                position.set(focus);
                if let EditorPointerSelection::Unit { .. } = mode {
                    gesture.units.set(Some((
                        mode,
                        (edit.selection().anchor, edit.selection().focus),
                    )));
                }
            })
        };
        let velocity: Rc<dyn Fn() -> (f32, f32)> = {
            let scene = scene.clone();
            let gesture = gesture.clone();
            let scroll = scroll.clone();
            let limits = scroll_limit.clone();
            let wrap = wrap.clone();
            Rc::new(move || {
                let s = scene.borrow();
                if !s.contains(viewport) || gesture.mode.get().is_none() {
                    return (0., 0.);
                }
                let bounds = s.bounds(viewport);
                let point = gesture.pointer.get();
                let point = in_viewport(&s, viewport, point.0, point.1);
                let offset = *scroll.borrow();
                let limit = limits.get();
                (
                    if wrap.get() {
                        0.
                    } else {
                        editor_drag_velocity(point.0, bounds.x, bounds.width, offset.0, limit.0)
                    },
                    if multiline {
                        editor_drag_velocity(point.1, bounds.y, bounds.height, offset.1, limit.1)
                    } else {
                        0.
                    },
                )
            })
        };
        let can_scroll: Rc<dyn Fn() -> bool> = {
            let velocity = velocity.clone();
            Rc::new(move || velocity() != (0., 0.))
        };
        let tick: Rc<dyn Fn(f64)> = {
            let scroll = scroll.clone();
            let limits = scroll_limit.clone();
            let select = drag_select.clone();
            let draw = refresh.clone();
            let reveal = reveal_caret.clone();
            Rc::new(move |elapsed| {
                let speed = velocity();
                let limit = limits.get();
                {
                    let mut offset = scroll.borrow_mut();
                    offset.0 = (f64::from(offset.0) + f64::from(speed.0) * elapsed)
                        .clamp(0., f64::from(limit.0)) as f32;
                    offset.1 = (f64::from(offset.1) + f64::from(speed.1) * elapsed)
                        .clamp(0., f64::from(limit.1)) as f32;
                }
                select();
                reveal.set(false);
                draw();
            })
        };
        let arm: Rc<dyn Fn()> = {
            let storage = input_storage.clone();
            let gesture = gesture.clone();
            Rc::new(move || {
                let Some(storage) = storage.upgrade() else {
                    return;
                };
                if storage.advancing_interaction.get() {
                    return;
                }
                // A dormant boundary gesture must not steal the one scheduler
                // slot after capture transfers to another control.
                if storage.input.captured().is_some_and(|owner| owner != root) {
                    gesture.cancel();
                    return;
                }
                if gesture.mode.get().is_none() || !can_scroll() {
                    let remove = storage
                        .interaction
                        .borrow()
                        .as_ref()
                        .is_some_and(|p| p.job.owner == root);
                    if remove {
                        let old = storage.interaction.borrow_mut().take();
                        drop(old);
                    }
                    return;
                }
                let generation = gesture.generation.get();
                if storage
                    .interaction
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| p.job.owner == root && p.job.generation == generation)
                {
                    return;
                }
                let now = Instant::now();
                let old = storage
                    .interaction
                    .borrow_mut()
                    .replace(InteractionDeadline {
                        job: Rc::new(EditorInteraction {
                            owner: root,
                            generation,
                            gesture: gesture.clone(),
                            can_scroll: can_scroll.clone(),
                            tick: tick.clone(),
                        }),
                        last: now,
                        next: now + Duration::from_millis(16),
                    });
                drop(old);
            })
        };
        *interaction_refresh.borrow_mut() = Some(Rc::downgrade(&arm));
        let draw = refresh.clone();
        let tab_indent = hint.clone();
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            // Capture/target hooks can cancel editing defaults. Focus and
            // pointer-release cleanup must still run after cancellation.
            if cx.default_prevented()
                && matches!(
                    cx.event,
                    InputEvent::KeyDown { .. }
                        | InputEvent::Text(_)
                        | InputEvent::ImePreedit { .. }
                        | InputEvent::ImeCommit(_)
                        | InputEvent::SetValue(_)
                        | InputEvent::SetTextSelection { .. }
                        | InputEvent::PointerDown { .. }
                        | InputEvent::PointerMove { .. }
                        | InputEvent::Scroll { .. }
                )
            {
                return;
            }
            if read_only.get() {
                let mutation = match &cx.event {
                    InputEvent::Text(_) | InputEvent::ImePreedit { .. } | InputEvent::ImeCommit(_) | InputEvent::SetValue(_) => true,
                    InputEvent::KeyDown { key, modifiers, .. } => matches!(key, Key::Backspace | Key::Delete | Key::Enter)
                        || (modifiers.primary_shortcut() && matches!(key, Key::Character(k) if k.eq_ignore_ascii_case("z") || k.eq_ignore_ascii_case("y")))
                        || (mac_control(*modifiers) && matches!(key, Key::Character(k) if ["d", "h", "k", "y", "t"].iter().any(|c| k.eq_ignore_ascii_case(c)))),
                    _ => false,
                };
                if mutation { cx.prevent_default(); return; }
            }
            let event = cx.event.clone();
            if matches!(
                event,
                InputEvent::PointerDown { .. }
                    | InputEvent::Text(_)
                    | InputEvent::ImeCommit(_)
                    | InputEvent::ImePreedit { .. }
                    | InputEvent::SetValue(_)
                    | InputEvent::SetTextSelection { .. }
                    | InputEvent::Blur
            ) || (gesture.mode.get().is_some() && matches!(event, InputEvent::PointerMove { .. }))
            {
                preferred_x.set(None);
            }
            let mut changed = false;
            match event {
                InputEvent::Focus => {
                    *focused.borrow_mut() = true;
                    if let Some(state) = &bg_state {
                        state.set(HOVERED);
                    }
                    // Focus coming to it, not its window coming back.
                    let dropped = gesture.dropped.replace(false);
                    if hint.select_on_focus.get() && !window_blurred.replace(false) && !dropped {
                        let mut edit = editor.borrow_mut();
                        edit.select_all();
                        position.set(TextPosition {
                            byte_offset: edit.selection().focus,
                            affinity: TextAffinity::After,
                        });
                        // The press that focused it selects no range of its own.
                        gesture.cancel();
                    }
                }
                InputEvent::Blur => {
                    // Still focused: its window went to the back.
                    window_blurred.set(
                        input_storage
                            .upgrade()
                            .is_some_and(|storage| storage.input.focused() == Some(root)),
                    );
                    *focused.borrow_mut() = false;
                    gesture.cancel();
                    editor.borrow_mut().cancel_preedit();
                    // Read-only text gives up its selection with the focus, as a
                    // selectable label does, unless only its window went back.
                    if read_only.get() && !window_blurred.get() {
                        let mut edit = editor.borrow_mut();
                        let end = edit.selection().focus;
                        edit.set_selection(end, end);
                    }
                    if let Some(state) = &bg_state {
                        state.set(RESTING);
                    }
                }
                InputEvent::Scroll {
                    delta_x, delta_y, ..
                } => {
                    if delta_x.is_finite() && delta_y.is_finite() {
                        let mut offset = scroll.borrow_mut();
                        let previous = *offset;
                        let limit = scroll_limit.get();
                        offset.0 = (offset.0 + delta_x).clamp(0., limit.0);
                        if multiline {
                            offset.1 = (offset.1 + delta_y).clamp(0., limit.1);
                        }
                        if previous == *offset {
                            // At its edge: contained, the gesture stays here
                            // (taken, so the scroll views around it don't
                            // move, and the rest of it latches here).
                            if wheel_hint.contain.get() && (delta_y != 0. || delta_x != 0.) {
                                cx.prevent_default();
                                cx.stop_propagation();
                            }
                            return;
                        }
                        drop(offset);
                        reveal_caret.set(false);
                        cx.prevent_default();
                        cx.stop_propagation();
                    }
                }
                InputEvent::Text(t) => {
                    let canonical = normalize_text(&t, multiline);
                    if !canonical
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
                    {
                        changed = editor.borrow_mut().insert(&canonical);
                    }
                }
                InputEvent::ImePreedit { text, cursor } => {
                    editor.borrow_mut().set_preedit(text, cursor)
                }
                InputEvent::SetValue(t) => {
                    let t = normalize_text(&t, multiline);
                    changed = editor.borrow_mut().set_text_ref(&t);
                    cx.prevent_default();
                }
                InputEvent::SetTextSelection { anchor, focus } => {
                    let mut edit = editor.borrow_mut();
                    edit.set_selection(anchor, focus);
                    position.set(TextPosition {
                        byte_offset: edit.selection().focus,
                        affinity: TextAffinity::After,
                    });
                    cx.prevent_default();
                }
                InputEvent::ImeCommit(t) => {
                    changed = editor
                        .borrow_mut()
                        .commit_preedit(&normalize_text(&t, multiline));
                }
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Primary,
                } => {
                    cx.capture_pointer();
                    gesture.pointer.set((x, y));
                    let s = scene.borrow();
                    let bounds = s.bounds(viewport);
                    let scroll = *scroll.borrow();
                    let (_, size, font) = typography.borrow().clone();
                    let mut edit = editor.borrow_mut();
                    let layout = committed_layout(&input_storage, &s, root, &edit, size,
                        wrap.get().then_some(bounds.width.max(1.)), &font,
                    );
                    let (x, y) = in_viewport(&s, viewport, x, y);
                    let x = x - bounds.x + scroll.0;
                    let y = y - bounds.y + scroll.1;
                    let hit = layout.hit_position(x, y);
                    // A press on the selection may drag it (as AppKit's text
                    // views and browsers do): it stays until the pointer
                    // moves or lets go.
                    let selected_range = edit.selection().range();
                    if !cx.pointer_modifiers().shift
                        && cx.click_count() == 1
                        && !hint.secure.get()
                        && !selected_range.is_empty()
                        && selected_range.start < hit.byte_offset
                        && hit.byte_offset < selected_range.end
                    {
                        gesture.cancel();
                        let (px, py) = gesture.pointer.get();
                        gesture.text_press.set(Some((px, py, hit.byte_offset)));
                        return;
                    }
                    let selected = (edit.selection().anchor, edit.selection().focus);
                    let units = gesture
                        .units
                        .get()
                        .filter(|(_, made)| *made == selected)
                        .map(|(units, _)| units);
                    let mode = if cx.pointer_modifiers().shift {
                        units.unwrap_or(EditorPointerSelection::Character(edit.selection().anchor))
                    } else {
                        match cx.click_count() {
                            2 => {
                                let edges = editor_pointer_unit(edit.around(hit.byte_offset), layout.as_ref(), hit, x, y, false, multiline);
                                EditorPointerSelection::Unit { edges, line: false }
                            }
                            3.. => {
                                let edges = editor_pointer_unit(edit.around(hit.byte_offset), layout.as_ref(), hit, x, y, true, multiline);
                                EditorPointerSelection::Unit { edges, line: true }
                            }
                            _ => EditorPointerSelection::Character(hit.byte_offset),
                        }
                    };
                    let (anchor, focus) = match mode {
                        EditorPointerSelection::Character(anchor) => (anchor, hit),
                        EditorPointerSelection::Unit { edges, line } if cx.pointer_modifiers().shift => {
                            let current = editor_pointer_unit(edit.around(hit.byte_offset), layout.as_ref(), hit, x, y, line, multiline);
                            extend_by_units(edges, current)
                        }
                        EditorPointerSelection::Unit { edges, .. } => (edges.0.byte_offset, edges.1),
                    };
                    edit.set_selection(anchor, focus.byte_offset);
                    position.set(focus);
                    gesture.cancel();
                    gesture.mode.set(Some(mode));
                    let made = (edit.selection().anchor, edit.selection().focus);
                    gesture.units.set(match mode {
                        EditorPointerSelection::Unit { .. } => Some((mode, made)),
                        EditorPointerSelection::Character(_) => None,
                    });
                }
                // Middle-click paste, where the desktop has a primary
                // selection: the caret goes under the pointer, and the host
                // inserts the selection there.
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Middle,
                } if cfg!(all(unix, not(target_os = "macos")))
                    && !read_only.get()
                    && input_storage.upgrade().is_some_and(|s| s.primary_paste_on.get()) =>
                {
                    let s = scene.borrow();
                    let bounds = s.bounds(viewport);
                    let scroll = *scroll.borrow();
                    let (_, size, font) = typography.borrow().clone();
                    let mut edit = editor.borrow_mut();
                    let layout = committed_layout(&input_storage, &s, root, &edit, size,
                        wrap.get().then_some(bounds.width.max(1.)), &font,
                    );
                    let (x, y) = in_viewport(&s, viewport, x, y);
                    let hit = layout.hit_position(x - bounds.x + scroll.0, y - bounds.y + scroll.1);
                    edit.set_selection(hit.byte_offset, hit.byte_offset);
                    position.set(hit);
                    gesture.cancel();
                    if let Some(storage) = input_storage.upgrade() {
                        storage.primary_paste.set(Some(root));
                    }
                }
                // A context click outside the selection puts the caret under
                // the pointer first (and on macOS selects the word there, as
                // AppKit's text views do), so the menu acts where it opened.
                // A secondary press while the primary drags a selection is
                // a chord: the drag goes on, and no menu opens.
                InputEvent::PointerDown {
                    button: PointerButton::Secondary,
                    ..
                } if gesture.mode.get().is_some() => {
                    cx.prevent_default();
                }
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Secondary,
                } => {
                    cx.focus();
                    let s = scene.borrow();
                    let bounds = s.bounds(viewport);
                    let scroll = *scroll.borrow();
                    let (_, size, font) = typography.borrow().clone();
                    let mut edit = editor.borrow_mut();
                    let layout = committed_layout(&input_storage, &s, root, &edit, size,
                        wrap.get().then_some(bounds.width.max(1.)), &font,
                    );
                    let (x, y) = in_viewport(&s, viewport, x, y);
                    let (lx, ly) = (x - bounds.x + scroll.0, y - bounds.y + scroll.1);
                    let hit = layout.hit_position(lx, ly);
                    let range = edit.selection().range();
                    if range.is_empty() || !range.contains(&hit.byte_offset) {
                        let (start, end) = if cfg!(target_os = "macos") {
                            let (start, end) = editor_pointer_unit(edit.around(hit.byte_offset), &*layout, hit, lx, ly, false, multiline,
                            );
                            let word = &edit.text()[start.byte_offset..end.byte_offset];
                            if word.chars().any(char::is_alphanumeric) {
                                (start.byte_offset, end.byte_offset)
                            } else {
                                (hit.byte_offset, hit.byte_offset)
                            }
                        } else {
                            (hit.byte_offset, hit.byte_offset)
                        };
                        edit.set_selection(start, end);
                        position.set(hit);
                    }
                    gesture.cancel();
                }
                InputEvent::PointerMove { x, y } if gesture.text_press.get().is_some() => {
                    let (px, py, _) = gesture.text_press.get().unwrap();
                    if (x - px).hypot(y - py) > cx.drag_threshold() {
                        gesture.text_press.set(None);
                        let edit = editor.borrow();
                        let range = edit.selection().range();
                        let text = edit.slice(range.clone()).into_owned();
                        drop(edit);
                        cx.start_drag_from(
                            crate::input::DraggedText::from_editor(
                                text,
                                scene_identity,
                                root,
                                range,
                            ),
                            px,
                            py,
                        );
                    }
                }
                InputEvent::PointerMove { x, y } if gesture.mode.get().is_some() => {
                    gesture.pointer.set((x, y));
                    drag_select();
                    reveal_caret.set(false);
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    // Pressed on the selection and let go: the caret there.
                    if let Some((_, _, at)) = gesture.text_press.take()
                        && matches!(event, InputEvent::PointerUp { .. })
                    {
                        editor.borrow_mut().set_selection(at, at);
                        position.set(TextPosition {
                            byte_offset: at,
                            affinity: TextAffinity::After,
                        });
                        changed = true;
                    }
                    gesture.cancel();
                    cx.release_pointer();
                }
                InputEvent::Drag(drag) => {
                    let dragged = drag.payload.downcast_ref::<crate::input::DraggedText>();
                    match (drag.phase, dragged) {
                        // Out of this editor: other apps take it as text.
                        (crate::input::DragPhase::Start, Some(text))
                            if text.taken_from(scene_identity, root).is_some() =>
                        {
                            cx.set_drag_export(vec![crate::input::DragItem::Text(
                                text.text().to_owned(),
                            )]);
                        }
                        // Over it: the caret where it would drop.
                        (crate::input::DragPhase::Over, Some(_)) if !read_only.get() => {
                            cx.accept_drag();
                            let at = tab_indent.point.get().map_or(0, |point| point(drag.x, drag.y));
                            let mut edit = editor.borrow_mut();
                            if gesture.drop_restore.get().is_none() {
                                gesture.drop_restore.set(Some(edit.selection()));
                            }
                            if edit.selection() != (crate::text_edit::Selection { anchor: at, focus: at }) {
                                edit.set_selection(at, at);
                                position.set(TextPosition {
                                    byte_offset: at,
                                    affinity: TextAffinity::After,
                                });
                                changed = true;
                            }
                        }
                        (crate::input::DragPhase::Leave, Some(_)) => {
                            if let Some(was) = gesture.drop_restore.take()
                                && !drag.accepted
                            {
                                editor.borrow_mut().set_selection(was.anchor, was.focus);
                                changed = true;
                            }
                        }
                        (crate::input::DragPhase::Drop, Some(text))
                            if !read_only.get() && !cx.default_prevented() =>
                        {
                            let at = tab_indent.point.get().map_or(0, |point| point(drag.x, drag.y));
                            let mut edit = editor.borrow_mut();
                            if let Some(was) = gesture.drop_restore.take() {
                                edit.set_selection(was.anchor, was.focus);
                            }
                            let modifiers = cx.pointer_modifiers();
                            let copy = if cfg!(target_os = "macos") {
                                modifiers.alt
                            } else {
                                modifiers.control
                            };
                            // From this very editor (this window's), its text
                            // still there: moved within it.
                            let here = text
                                .taken_from(scene_identity, root)
                                .filter(|range| edit.holds((*range).clone(), text.text()));
                            let moved_here = !copy && here.is_some();
                            let dropped = match here {
                                Some(range) if moved_here => edit.move_text(range.clone(), at),
                                _ => edit.insert_dropped(at, text.text()),
                            };
                            if dropped {
                                // From another editor, moved: it lets it go.
                                if !copy && !moved_here && text.from.is_some() {
                                    text.taken.set(true);
                                }
                                position.set(TextPosition {
                                    byte_offset: edit.selection().focus,
                                    affinity: TextAffinity::After,
                                });
                                changed = true;
                                // Focused, its dropped text selected, as
                                // AppKit's text views and a browser's fields.
                                if !*focused.borrow() {
                                    gesture.dropped.set(true);
                                    cx.focus();
                                }
                            }
                            cx.prevent_default();
                            cx.stop_propagation();
                        }
                        // Dragged out of here and moved elsewhere: let it go.
                        (crate::input::DragPhase::End, Some(text)) if text.taken.get() => {
                            // Only while its text is still where it was: an
                            // app that changed it meanwhile keeps its own.
                            if let Some(range) = text.taken_from(scene_identity, root) {
                                let mut edit = editor.borrow_mut();
                                if edit.holds(range.clone(), text.text()) {
                                    edit.set_selection(range.start, range.end);
                                    changed = edit.backspace();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                InputEvent::KeyUp {
                    key: Key::Space, ..
                } => {
                    // Let keydown deliver native text; suppress the dispatcher's
                    // delayed keyboard activation when the space key is released.
                    cx.prevent_default();
                    return;
                }
                InputEvent::KeyDown { key, modifiers, .. } => {
                    // macOS's Control keys, as its text system binds them: a
                    // paragraph's start and end, a character or a line either
                    // way, deleting either way, killing to the paragraph's end
                    // (Control+K), yanking it back (Control+Y) and swapping
                    // characters (Control+T).
                    let mut mac = None;
                    let (key, modifiers) = match &key {
                        Key::Character(k) if mac_control(modifiers) => {
                            let plain = crate::input::Modifiers {
                                control: false,
                                ..modifiers
                            };
                            let action = |action, mac: &mut Option<MacControl>| {
                                *mac = Some(action);
                                (key.clone(), plain)
                            };
                            match k.to_ascii_lowercase().as_str() {
                                "a" => action(MacControl::ParagraphEdge(false), &mut mac),
                                "e" => action(MacControl::ParagraphEdge(true), &mut mac),
                                "f" => (Key::ArrowRight, plain),
                                "b" => (Key::ArrowLeft, plain),
                                "n" => (Key::ArrowDown, plain),
                                "p" => (Key::ArrowUp, plain),
                                "d" => (Key::Delete, plain),
                                "h" => (Key::Backspace, plain),
                                "k" => action(MacControl::Kill, &mut mac),
                                "y" => action(MacControl::Yank, &mut mac),
                                "t" => action(MacControl::Transpose, &mut mac),
                                _ => (key.clone(), modifiers),
                            }
                        }
                        _ => (key.clone(), modifiers),
                    };
                    let paging = matches!(key, Key::PageUp | Key::PageDown);
                    // Single-line fields and platform shortcuts leave paging to
                    // their ancestors without disturbing editor navigation state.
                    if paging && (!multiline || modifiers.control || modifiers.alt || modifiers.meta) {
                        return;
                    }
                    let mut e = editor.borrow_mut();
                    let command = modifiers.primary_shortcut();
                    let word = modifiers.word_navigation();
                    let visual_edge = match key {
                        Key::Home | Key::End
                            if !command && wrap.get() && !cfg!(target_os = "macos") =>
                        {
                            Some(key == Key::End)
                        }
                        Key::ArrowLeft | Key::ArrowRight
                            if cfg!(target_os = "macos") && modifiers.meta && wrap.get() =>
                        {
                            Some(key == Key::ArrowRight)
                        }
                        _ => None,
                    };
                    let by_paragraph = !cfg!(target_os = "macos")
                        && modifiers.control
                        && !modifiers.alt
                        && !modifiers.meta;
                    let directional = paging || (matches!(key, Key::ArrowUp | Key::ArrowDown)
                        && !(cfg!(target_os = "macos") && (modifiers.meta || modifiers.alt))
                        && !by_paragraph);
                    if !directional {
                        preferred_x.set(None);
                    }
                    if let Some(end) = visual_edge {
                        let s = scene.borrow();
                        let (_, size, font) = typography.borrow().clone();
                        let layout = committed_layout(&input_storage, &s, root, &e,
                            size,
                            Some(s.bounds(viewport).width.max(1.)),
                            &font,
                        );
                        let old = position.get();
                        let current = if old.byte_offset == e.selection().focus {
                            old
                        } else {
                            TextPosition {
                                byte_offset: e.selection().focus,
                                affinity: TextAffinity::After,
                            }
                        };
                        let next = layout.visual_line_edge(current, end);
                        let anchor = if modifiers.shift {
                            e.selection().anchor
                        } else {
                            next.byte_offset
                        };
                        e.set_selection(anchor, next.byte_offset);
                        position.set(next);
                    } else {
                        if !directional {
                            position.set(TextPosition {
                                byte_offset: e.selection().focus,
                                affinity: TextAffinity::After,
                            });
                        }
                        match key {
                            Key::ArrowLeft if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_home(modifiers.shift)
                            }
                            Key::ArrowRight if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_end(modifiers.shift)
                            }
                            Key::ArrowUp if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_document_start(modifiers.shift)
                            }
                            Key::ArrowDown if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_document_end(modifiers.shift)
                            }
                            // By paragraphs, as AppKit's text views go.
                            Key::ArrowUp | Key::ArrowDown
                                if cfg!(target_os = "macos") && modifiers.alt =>
                            {
                                e.move_paragraph(key == Key::ArrowUp, modifiers.shift)
                            }
                            // To paragraphs' starts, as Windows' edit controls and
                            // Chrome there and on Linux go.
                            Key::ArrowUp | Key::ArrowDown if by_paragraph => {
                                e.move_paragraph_start(key == Key::ArrowUp, modifiers.shift)
                            }
                            // Right-to-left text: the arrows go the way it
                            // shows (the macOS way), by its layout.
                            Key::ArrowLeft | Key::ArrowRight
                                if crate::text_layout::has_rtl(&e.around(e.selection().focus).1) =>
                            {
                                let s = scene.borrow();
                                let (_, size, font) = typography.borrow().clone();
                                let layout = committed_layout(&input_storage, &s, root, &e,
                                    size,
                                    Some(s.bounds(viewport).width.max(1.)),
                                    &font,
                                );
                                e.move_visually(
                                    word,
                                    modifiers.shift,
                                    key == Key::ArrowLeft,
                                    |byte| layout.caret(byte),
                                );
                            }
                            Key::ArrowLeft => e.move_left(word, modifiers.shift),
                            Key::ArrowRight => e.move_right(word, modifiers.shift),
                            // macOS: Home and End scroll to the text's start or
                            // end and leave the caret where it is, as NSTextView
                            // (scrollToBeginningOfDocument:) and Chrome there do;
                            // with Shift, the selection extends there. (⌘ with
                            // them still moves the caret to the text's ends.)
                            Key::Home | Key::End if cfg!(target_os = "macos") && !command => {
                                let end = key == Key::End;
                                if modifiers.shift {
                                    if end {
                                        e.move_document_end(true)
                                    } else {
                                        e.move_document_start(true)
                                    }
                                } else {
                                    let limit = scroll_limit.get();
                                    *scroll.borrow_mut() = match (end, multiline) {
                                        (false, _) => (0., 0.),
                                        (true, true) => (0., limit.1),
                                        (true, false) => (limit.0, 0.),
                                    };
                                    reveal_caret.set(false);
                                }
                            }
                            Key::Home => {
                                if command {
                                    e.move_document_start(modifiers.shift)
                                } else {
                                    e.move_home(modifiers.shift)
                                }
                            }
                            Key::End => {
                                if command {
                                    e.move_document_end(modifiers.shift)
                                } else {
                                    e.move_end(modifiers.shift)
                                }
                            }
                            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                                let direction = if matches!(key, Key::ArrowUp | Key::PageUp) { -1. } else { 1. };
                                let (_, size, font) = typography.borrow().clone();
                                let s = scene.borrow();
                                let layout = committed_layout(&input_storage, &s, root, &e,
                                    size,
                                    wrap.get().then_some(s.bounds(viewport).width.max(1.)),
                                    &font,
                                );
                                let old = position.get();
                                let current = if old.byte_offset == e.selection().focus {
                                    old
                                } else {
                                    TextPosition {
                                        byte_offset: e.selection().focus,
                                        affinity: TextAffinity::After,
                                    }
                                };
                                let caret = layout.caret_position(current);
                                let desired_x = if old.byte_offset != e.selection().focus {
                                    caret.x
                                } else {
                                    preferred_x.get().unwrap_or(caret.x)
                                };
                                preferred_x.set(Some(desired_x));
                                let next = if paging {
                                    // One visual line overlaps between pages. Use f64
                                    // arithmetic so tiny positive line pitches cannot
                                    // overflow the line count or displacement.
                                    let pitch = if caret.height.is_finite() && caret.height > 0. {
                                        f64::from(caret.height)
                                    } else { 1. };
                                    let height = s.bounds(viewport).height;
                                    let height = if height.is_finite() { f64::from(height.max(0.)) } else { pitch };
                                    let lines = ((height / pitch).floor() - 1.).max(1.);
                                    let origin = if caret.y.is_finite() { f64::from(caret.y) } else { 0. };
                                    let target = origin + f64::from(direction) * lines * pitch;
                                    let end = TextPosition { byte_offset: e.text().len(), affinity: TextAffinity::After };
                                    let last_y = layout.caret_position(end).y;
                                    if target < 0. {
                                        TextPosition::default()
                                    } else if last_y.is_finite() && target > f64::from(last_y) {
                                        end
                                    } else {
                                        layout.hit_position(desired_x,
                                            (target + pitch * 0.5).clamp(0., f64::from(f32::MAX)) as f32)
                                    }
                                } else {
                                    layout.hit_position(
                                        desired_x,
                                        (caret.y + direction * caret.height).max(0.)
                                            + caret.height * 0.5,
                                    )
                                };
                                if paging {
                                    let next_y = layout.caret_position(next).y;
                                    if next_y.is_finite() && caret.y.is_finite() {
                                        let mut offset = scroll.borrow_mut();
                                        offset.1 = (f64::from(offset.1) + f64::from(next_y) - f64::from(caret.y))
                                            .clamp(0., f64::from(scroll_limit.get().1.max(0.))) as f32;
                                    }
                                }
                                position.set(next);
                                let index = next.byte_offset;
                                let anchor = if modifiers.shift {
                                    e.selection().anchor
                                } else {
                                    index
                                };
                                e.set_selection(anchor, index);
                            }
                            Key::Character(_) if mac.is_some() => match mac {
                                Some(MacControl::ParagraphEdge(end)) => {
                                    if end {
                                        e.move_end(modifiers.shift)
                                    } else {
                                        e.move_home(modifiers.shift)
                                    }
                                }
                                Some(MacControl::Kill) => {
                                    let id = Rc::as_ptr(&editor) as usize;
                                    let before = (id, e.revision(), e.selection().focus);
                                    // A password's is deleted, never kept.
                                    if let Some(killed) = e.kill_to_line_end()
                                        && !hint.secure.get()
                                    {
                                        let after = (id, e.revision(), e.selection().focus);
                                        KILLED.with_borrow_mut(|buffer| {
                                            // A kill straight after another adds to it.
                                            if buffer.after != Some(before) {
                                                buffer.text.clear();
                                            }
                                            buffer.text.push_str(&killed);
                                            buffer.after = Some(after);
                                        });
                                    }
                                    changed = e.revision() != before.1;
                                }
                                Some(MacControl::Yank) => {
                                    let yanked = KILLED.with_borrow(|buffer| buffer.text.clone());
                                    changed = !yanked.is_empty() && e.insert_pasted(&yanked);
                                }
                                Some(MacControl::Transpose) => changed = e.transpose(),
                                None => {}
                            },
                            // ⌘⌫ and ⌘⌦: to the line's start or end.
                            Key::Backspace if cfg!(target_os = "macos") && modifiers.meta => {
                                changed = e.delete_to_line_start();
                            }
                            Key::Delete if cfg!(target_os = "macos") && modifiers.meta => {
                                changed = e.delete_to_line_end();
                            }
                            Key::Backspace => {
                                changed = e.delete_backward(word);
                            }
                            Key::Delete => {
                                changed = e.delete_next(word);
                            }
                            Key::Enter => {
                                if multiline {
                                    changed = e.insert("\n");
                                }
                            }
                            // Control-Tab leaves it, as AppKit's text views.
                            Key::Tab
                                if !command
                                    && !modifiers.control
                                    && !modifiers.alt
                                    && tab_indent.code().is_some_and(|c| c.indent.borrow().is_some()) =>
                            {
                                let indent = tab_indent.code_made().indent.borrow().clone().unwrap();
                                changed = indent_lines(&mut e, &indent, modifiers.shift);
                            }
                            Key::Space => return,
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("a") => {
                                e.select_all()
                            }
                            // With nothing to undo or redo, the key goes on to
                            // the app's commands (its own undo), as AppKit's
                            // responder chain hands it to the window's.
                            Key::Character(ref k)
                                if command
                                    && k.eq_ignore_ascii_case("z")
                                    && (if modifiers.shift { e.can_redo() } else { e.can_undo() }) =>
                            {
                                changed = if modifiers.shift { e.redo() } else { e.undo() };
                            }
                            Key::Character(ref k)
                                if command && k.eq_ignore_ascii_case("y") && e.can_redo() =>
                            {
                                changed = e.redo();
                            }
                            // A composition to cancel takes it; otherwise it goes
                            // on (a dialog around the field closes).
                            Key::Escape if e.preedit().is_some() => e.cancel_preedit(),
                            _ => return,
                        }
                    }
                    cx.prevent_default();
                }
                _ => return,
            }
            if changed {
                position.set(TextPosition {
                    byte_offset: editor.borrow().selection().focus,
                    affinity: TextAffinity::After,
                });
                tab_indent.value.write(&editor);
            }
            draw();
            arm();
        });
        handle
    }
}

impl Ui {
    /// Route an assistive-technology value change through the widget's normal input guards.
    /// Returns whether the request was handled, including a read-only rejection;
    /// it does not indicate that the model changed.
    pub fn set_accessible_value(&mut self, node: NodeId, value: &str) -> bool {
        self.input
            .dispatch_to(&self.scene, node, InputEvent::SetValue(value.to_owned()))
            .default_prevented
    }
    /// Change committed-text selection through normal input and modal guards.
    /// Editor controls clamp offsets to grapheme boundaries and cancel preedit.
    pub fn set_accessible_text_selection(
        &mut self,
        node: NodeId,
        anchor: usize,
        focus: usize,
    ) -> bool {
        self.input
            .dispatch_to(
                &self.scene,
                node,
                InputEvent::SetTextSelection { anchor, focus },
            )
            .default_prevented
    }
    /// Route an assistive technology's request to set `node`'s numeric value
    /// (a slider's) through the widget's normal input guards. Returns whether
    /// the request was handled.
    pub fn set_accessible_numeric_value(&mut self, node: NodeId, value: f64) -> bool {
        self.input
            .dispatch_to(&self.scene, node, InputEvent::SetNumericValue(value))
            .default_prevented
    }
}

/// Tab in an editor with an indent (`EditorHandle::set_indent`): the
/// indent where the caret is, or with lines selected (or Shift) each of
/// them indented, or outdented, as one edit that undo takes back; the
/// lines stay selected. Whether the text changed.
fn indent_lines(e: &mut TextEditor, indent: &str, outdent: bool) -> bool {
    let range = e.selection().range();
    let text = e.text();
    let lines = text[range.clone()].contains('\n');
    if !lines && !outdent {
        return e.insert(indent);
    }
    let start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
    // A selection ending at a line's start leaves that line be.
    let end = if lines && text[..range.end].ends_with('\n') {
        range.end - 1
    } else {
        text[range.end..]
            .find('\n')
            .map_or(text.len(), |at| range.end + at)
    };
    let block = &text[start..end];
    let mut next = String::with_capacity(block.len() + indent.len() * 8);
    for (n, line) in block.split('\n').enumerate() {
        if n > 0 {
            next.push('\n');
        }
        if outdent {
            // The indent, or as much of it as the line starts with (spaces
            // up to its width, or a tab).
            let cut = if line.starts_with(indent) {
                indent.len()
            } else if line.starts_with('\t') {
                1
            } else {
                line.bytes()
                    .take(indent.len().max(1))
                    .take_while(|b| *b == b' ')
                    .count()
            };
            next.push_str(&line[cut..]);
        } else if line.is_empty() {
            // An empty line stays empty.
        } else {
            next.push_str(indent);
            next.push_str(line);
        }
    }
    if next == block {
        return false;
    }
    e.set_selection(start, end);
    let changed = e.insert(&next);
    if lines {
        e.set_selection(start, start + next.len());
    }
    changed
}

/// Control alone on macOS: the text system's Emacs-style keys.
fn mac_control(modifiers: crate::input::Modifiers) -> bool {
    cfg!(target_os = "macos") && modifiers.control && !modifiers.meta && !modifiers.alt
}

/// `(x, y)` in the window, in an editor's viewport's own points: what
/// transforms and zooms its ancestors (and it) have undone, so a press lands
/// on the character that shows under it. As it was with none.
fn in_viewport(s: &Scene, viewport: NodeId, x: f32, y: f32) -> (f32, f32) {
    s.point_in(viewport, x, y).unwrap_or((x, y))
}

/// What a macOS Control key does in an editor that no other key does.
#[derive(Clone, Copy)]
enum MacControl {
    /// Control-A and Control-E: the paragraph's start or end (`true`).
    ParagraphEdge(bool),
    Kill,
    Yank,
    Transpose,
}

/// What Control-K killed, for Control-Y to yank: one buffer every editor
/// shares, as AppKit keeps it.
struct Killed {
    text: String,
    /// Which editor (its model's address), at which revision and caret, the
    /// last kill left: a kill straight after it adds to it.
    after: Option<(usize, u64, usize)>,
}

thread_local! {
    static KILLED: RefCell<Killed> = const {
        RefCell::new(Killed {
            text: String::new(),
            after: None,
        })
    };
}

fn editor_drag_velocity(pointer: f32, start: f32, extent: f32, offset: f32, limit: f32) -> f32 {
    if !pointer.is_finite() || !start.is_finite() || !extent.is_finite() || extent <= 0. {
        return 0.;
    }
    let distance = if pointer < start {
        pointer - start
    } else if pointer > start + extent {
        pointer - start - extent
    } else {
        0.
    };
    if (distance < 0. && offset > 0.) || (distance > 0. && offset < limit) {
        distance.signum() * (distance.abs() * 12.).clamp(24., 1200.)
    } else {
        0.
    }
}

#[derive(Clone, Copy)]
enum EditorPointerSelection {
    Character(usize),
    Unit {
        edges: (TextPosition, TextPosition),
        line: bool,
    },
}

/// A selection by units (`edges`, the first selected) reaching the unit at
/// the pointer (`current`): from the far edge of the first, to the near edge
/// of the one reached, either way.
fn extend_by_units(
    edges: (TextPosition, TextPosition),
    current: (TextPosition, TextPosition),
) -> (usize, TextPosition) {
    if current.0.byte_offset < edges.0.byte_offset {
        (edges.1.byte_offset, current.0)
    } else if current.1.byte_offset > edges.1.byte_offset {
        (edges.0.byte_offset, current.1)
    } else {
        (edges.0.byte_offset, edges.1)
    }
}

pub(crate) fn editor_pointer_unit(
    around: (usize, std::borrow::Cow<'_, str>),
    layout: &dyn TextLayout,
    hit: TextPosition,
    x: f32,
    y: f32,
    line: bool,
    multiline: bool,
) -> (TextPosition, TextPosition) {
    // The text around the hit (its line, in a long text) from where it
    // starts in the whole.
    let (base, text) = (around.0, &*around.1);
    let (start, end) = if line && multiline {
        // Its paragraph, break included, as AppKit's text views and
        // browsers' text areas select one (not the wrapped row).
        let offset = hit.byte_offset.saturating_sub(base).min(text.len());
        let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
        let end = text[offset..]
            .find('\n')
            .map_or(text.len(), |i| offset + i + 1);
        (base + start, base + end)
    } else if line {
        (base, base + text.len())
    } else {
        // A hit is the nearest caret, not necessarily the grapheme underneath
        // the pointer. Check the preceding cell, including RTL geometry, before
        // choosing a Unicode word-bound segment at that caret.
        let offset = hit.byte_offset.saturating_sub(base).min(text.len());
        let previous = text
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < offset)
            .last();
        let index = previous
            .filter(|(start, grapheme)| {
                layout
                    .selection(base + start..base + start + grapheme.len())
                    .iter()
                    .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            })
            .map_or(offset, |(start, _)| start);
        let (start, end) = editor_word_segment(text, index);
        (base + start, base + end)
    };
    (
        TextPosition {
            byte_offset: start,
            affinity: TextAffinity::After,
        },
        TextPosition {
            byte_offset: end,
            affinity: TextAffinity::Before,
        },
    )
}

fn editor_word_segment(text: &str, index: usize) -> (usize, usize) {
    // Chinese, Japanese and Thai by dictionary; else Unicode's boundaries.
    let (start, end) = crate::words::dictionary_word(text, index).unwrap_or_else(|| {
        let (start, segment) = text
            .split_word_bound_indices()
            .find(|(start, segment)| index < start + segment.len())
            .or_else(|| text.split_word_bound_indices().next_back())
            .unwrap_or((0, ""));
        (start, start + segment.len())
    });
    let start = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i <= start)
        .last()
        .unwrap_or(0);
    let end = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .find(|i| *i >= end)
        .unwrap_or(text.len());
    (start, end)
}

fn normalize_text(text: &str, multiline: bool) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if !text.contains('\r') && (multiline || !text.contains('\n')) {
        return Cow::Borrowed(text);
    }
    let mut canonical = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if multiline {
                    canonical.push('\n');
                }
            }
            '\n' if !multiline => {}
            other => canonical.push(other),
        }
    }
    Cow::Owned(canonical)
}

/// A legacy control's background states (`Ui::control`).
const RESTING: u8 = 0;
const HOVERED: u8 = 1;
const ACCENTED: u8 = 2;
impl Ui {
    /// Keep plain rectangles' colours on their tokens as the theme changes.
    fn theme_rects(
        &mut self,
        owner: NodeId,
        theme: Rc<ThemeScope>,
        rects: &[(NodeId, ColorToken)],
    ) {
        let rects = rects.to_vec();
        let scene = self.scene.clone();
        let mut first = true;
        self.bind(owner, move || {
            let colors: Vec<_> = rects.iter().map(|(_, t)| theme.color(*t)).collect();
            if std::mem::take(&mut first) {
                return;
            }
            let mut scene = scene.borrow_mut();
            for ((node, _), color) in rects.iter().zip(colors) {
                if scene.contains(*node) {
                    scene.set_kind(*node, NodeKind::Rect(color));
                }
            }
        });
    }
}
/// A misspelled word's mark: a dotted line, as macOS draws one.
fn spelling_mark(color: Color) -> NodeKind {
    NodeKind::Quad(crate::scene::QuadStyle {
        border_color: color,
        decoration: Some(std::sync::Arc::new(crate::decoration::Decoration {
            border_widths: Some(crate::scene::Insets {
                bottom: 2.,
                ..Default::default()
            }),
            border_style: crate::decoration::BorderStyle::Dashed {
                length: 2.,
                gap: 2.,
            },
            ..Default::default()
        })),
        ..Default::default()
    })
}
/// Where a mark `thick` tall goes under text line `r` (`above` from its
/// bottom) in an editor's viewport, scrolled by `scroll`: a composition's
/// underline, a misspelling's dots. A line taller than the viewport (large
/// type in a short field) keeps its mark inside, at the bottom, where the
/// viewport's clip would cut it off below the line.
fn mark_y(r: Rect, above: f32, thick: f32, scroll: f32, inner_height: f32) -> f32 {
    let y = r.y + r.height - above - scroll;
    if r.height > inner_height && r.y - scroll < inner_height {
        y.min(inner_height - thick)
    } else {
        y
    }
}

fn surface(theme: &ThemeScope, color: Color) -> NodeKind {
    NodeKind::Quad(crate::scene::QuadStyle {
        fill: color,
        radius: theme.number(tok::RADIUS_MD),
        border_color: Color(255, 255, 255, 18),
        border_width: 1.,
        shadow: None,
        decoration: None,
        border_over: false,
        outline: None,
    })
}
impl Ui {
    /// Append `image` under `parent`, which assistive technology names `alt`.
    pub fn image(
        &mut self,
        parent: NodeId,
        image: std::sync::Arc<crate::image::ImageData>,
        alt: impl Into<String>,
        style: Style,
    ) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Image(image), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        self.semantics
            .borrow_mut()
            .set(n, SemanticNode::new(Role::Image, alt));
        n
    }
    /// Append a decorated box (fill, border, corners, shadow) under `parent`.
    pub fn quad(&mut self, parent: NodeId, quad: crate::scene::QuadStyle, style: Style) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Quad(quad), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
}

/// An input waiting to be drawn, with the model effect whose reads its
/// draw's are.
type InputDraw = (NodeId, u64, Rc<dyn Fn()>);
/// A mount's text inputs, drawn once it ends rather than each as its model
/// binds. A draw lays the scene out (to place the text in the editor's box),
/// so drawing inputs one by one laid out again a tree that grew with every
/// row: a mount of n inputs was quadratic, and so was laying out before
/// each callback of the app's that might read geometry. Instead what such
/// an observer reads of geometry is kept (`watch_geometry`), and once the
/// mount has drawn its inputs and laid out, each whose read moved runs
/// again (`finish_input_draws`), as a `ResizeObserver` does. One top-level
/// mount at a time, per thread.
struct InputDraws {
    /// Whether this mount has bound an input.
    bound: bool,
    storage: Weak<UiStorage>,
    waiting: Vec<InputDraw>,
    /// Whether a draw was deferred: observers from then on get a layout.
    deferred: bool,
    /// The observer the scene keeps geometry reads for now
    /// (`Scene::read_geometry_for`): told only when it changes.
    reader: Option<u64>,
}
thread_local! {
    // `InputDrawScope` takes the value on return and unwind: the slot owns
    // none after a mount, so it needs no destructor at thread exit (one
    // registered costs an allocation).
    static INPUT_DRAWS: RefCell<std::mem::ManuallyDrop<Option<InputDraws>>> =
        const { RefCell::new(std::mem::ManuallyDrop::new(None)) };
}
/// Defers the input draws of the mount it was begun for, if `active`.
pub(crate) struct InputDrawScope {
    pub(crate) active: bool,
}
impl InputDrawScope {
    /// For a mount by `ui`: not inside a batch or a flush, where its
    /// effects wait for an outer one, nor inside another mount's scope.
    pub(crate) fn begin(ui: &Ui) -> Self {
        let active = !ui.runtime.deferring()
            && INPUT_DRAWS.with_borrow_mut(|draws| {
                if draws.is_some() {
                    return false;
                }
                **draws = Some(InputDraws {
                    bound: false,
                    storage: Rc::downgrade(&ui.storage),
                    waiting: Vec::new(),
                    deferred: false,
                    reader: None,
                });
                true
            });
        Self { active }
    }
}
impl Drop for InputDrawScope {
    fn drop(&mut self) {
        if self.active {
            let _draws = INPUT_DRAWS.with_borrow_mut(|draws| draws.take());
        }
    }
}
/// Leave the draw of input `root` to its mount, if one is deferring them
/// and it is a plain field (a code editor's caret callbacks are drawn at
/// once). Called from its model binding, whose reads the draw's join.
fn defer_input_draw(
    root: NodeId,
    draw: &Rc<dyn Fn()>,
    hint: &EditorHint,
    storage: &Weak<UiStorage>,
) -> bool {
    INPUT_DRAWS.with_borrow_mut(|draws| {
        let Some(draws) = &mut **draws else {
            return false;
        };
        if storage.as_ptr() != draws.storage.as_ptr() || hint.code().is_some() {
            return false;
        }
        draws.deferred = true;
        let Some(effect) = draws
            .storage
            .upgrade()
            .and_then(|storage| storage.runtime.active_effect())
        else {
            return false;
        };
        draws.waiting.push((root, effect, draw.clone()));
        true
    })
}
/// As observer `effect` starts (`starts`) and ends in a mount deferring its
/// inputs' draws: from the first deferred, what an observer (not one marked
/// `mount_pure`: static styles, inputs' own bindings) reads of
/// `scene`'s geometry is kept (`Scene::read_geometry_for`), to run it again
/// once layout settles (`finish_input_draws`).
pub(crate) fn watch_geometry(effect: u64, starts: bool, scene: &Rc<RefCell<Scene>>) {
    let changed = INPUT_DRAWS.with_borrow_mut(|draws| {
        let draws = draws.as_mut()?;
        let reader = (starts && draws.deferred).then_some(effect);
        (reader != draws.reader).then(|| {
            draws.reader = reader;
            reader
        })
    });
    if let Some(reader) = changed {
        scene.borrow().read_geometry_for(reader);
    }
}
/// Layout passes a mount runs the observers that read its geometry again
/// in, before any left are let wait for the next flush.
pub(crate) const GEOMETRY_PASSES: usize = 4;
/// The end of a mount whose input draws were deferred: the rest drawn and
/// laid out, then each observer that read geometry layout has since moved
/// run again, as a `ResizeObserver` is: until nothing it read moves, or for
/// `GEOMETRY_PASSES` passes.
pub(crate) fn finish_input_draws(scene: &Rc<RefCell<Scene>>) {
    let Some(draws) = INPUT_DRAWS.with_borrow_mut(|draws| draws.take()) else {
        return;
    };
    let Some(storage) = draws.storage.upgrade().filter(|_| draws.deferred) else {
        return;
    };
    let runtime = storage.runtime.clone();
    // Batched: an observer of a signal a shaper writes (unsupported) runs
    // after the draws, not inside an editor's borrow.
    runtime.batch(|| {
        for (_, effect, draw) in draws.waiting {
            runtime.run_as(effect, &*draw);
        }
        scene.borrow_mut().prepare_layout();
    });
    let watch = |effect: u64, starts: bool| {
        scene.borrow().read_geometry_for(starts.then_some(effect));
    };
    for pass in 0..GEOMETRY_PASSES {
        let moved = scene.borrow().changed_geometry_readers();
        if moved.is_empty() {
            return;
        }
        if pass + 1 == GEOMETRY_PASSES {
            #[cfg(debug_assertions)]
            eprintln!(
                "zgx: {} observers' geometry still moved after {GEOMETRY_PASSES} layout passes of a mount; they run again at the next flush",
                moved.len()
            );
            runtime.requeue(&moved);
            return;
        }
        runtime.rerun_watched(&moved, watch);
        scene.borrow_mut().prepare_layout();
    }
}
/// Whether the mount deferring input draws has bound an input: without one
/// its observers run as any batch's.
pub(crate) fn inputs_bound() -> bool {
    INPUT_DRAWS.with_borrow(|draws| draws.as_ref().is_some_and(|draws| draws.bound))
}
/// Whether `node`'s draw is the last a mount on `scene` has waiting.
fn input_draw_pending(node: NodeId, scene: &Rc<RefCell<Scene>>) -> bool {
    INPUT_DRAWS.with_borrow(|draws| {
        draws.as_ref().is_some_and(|draws| {
            draws
                .waiting
                .last()
                .is_some_and(|(owner, _, _)| *owner == node)
                && draws
                    .storage
                    .upgrade()
                    .is_some_and(|storage| Rc::ptr_eq(&storage.scene, scene))
        })
    })
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use std::cell::Cell;

    struct OnDrop(Option<Box<dyn FnOnce()>>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.take().unwrap()();
        }
    }

    #[test]
    fn shared_handles_see_editors_and_preserve_bindings() {
        let mut ui = Ui::new(300., 200.);
        let root = ui.root();
        let value = ui.signal(String::from("initial"));
        let editor = ui.text_input(root, "Name", value.clone(), 200., false);
        let shared = ui.shared();
        assert!(ui.input.focus(&ui.scene, Some(editor.node)));
        assert_eq!(shared.focused_editor().unwrap().node, editor.node);
        drop(ui);
        value.set("still mounted".into());
        assert_eq!(
            shared.focused_editor().unwrap().editor.borrow().text(),
            "still mounted"
        );
        assert!(shared.input.has_listeners(editor.node));
    }

    #[test]
    fn only_last_owner_disposes_resources_and_effects() {
        let mut ui = Ui::new(100., 100.);
        let node = ui.label(ui.root(), "retained", Style::default());
        let runtime = ui.runtime.clone();
        let input = ui.input.clone();
        ui.bind(node, || {});
        ui.on_event(node, true, |_| {});
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        ui.retain(node, OnDrop(Some(Box::new(move || c.set(c.get() + 1)))));
        let weak = ui.downgrade();
        let shared = ui.shared();
        drop(ui);
        assert_eq!(count.get(), 0);
        assert_eq!(runtime.effect_count(), 1);
        assert!(weak.upgrade().is_some());
        drop(shared);
        assert_eq!(count.get(), 1);
        assert_eq!(runtime.effect_count(), 0);
        assert!(!input.has_listeners(node));
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn listener_destructor_can_remove_its_owner_and_ancestor() {
        let mut ui = Ui::new(100., 100.);
        let parent = ui.container(ui.root(), Layout::Column, Style::default());
        let child = ui.container(parent, Layout::Column, Style::default());
        let weak = ui.downgrade();
        let dropped = Rc::new(Cell::new(0));
        let count = dropped.clone();
        let resource = OnDrop(Some(Box::new(move || {
            count.set(count.get() + 1);
            let mut ui = weak.upgrade().unwrap();
            ui.remove(child);
            ui.remove(parent);
        })));
        ui.on_event(child, false, move |_| {
            let _ = &resource;
        });
        ui.remove(child);
        assert_eq!(dropped.get(), 1);
        assert!(!ui.scene.borrow().contains(parent));
        assert!(!ui.scene.borrow().contains(child));
        assert!(!ui.input.has_listeners(child));
        assert!(ui.storage.owned.borrow().is_empty());
    }

    #[test]
    fn resource_cleanup_can_remove_other_owned_subtrees() {
        let mut ui = Ui::new(100., 100.);
        let first = ui.container(ui.root(), Layout::Column, Style::default());
        let second = ui.container(ui.root(), Layout::Column, Style::default());
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        ui.retain(second, OnDrop(Some(Box::new(move || c.set(c.get() + 1)))));
        let weak = ui.downgrade();
        ui.retain(
            first,
            OnDrop(Some(Box::new(move || {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(first);
                ui.remove(second);
            }))),
        );
        ui.remove(first);
        assert_eq!(count.get(), 1);
        assert!(!ui.scene.borrow().contains(first));
        assert!(!ui.scene.borrow().contains(second));
        assert!(ui.storage.owned.borrow().is_empty());
    }
}

/// Whether a size has changed from what was told of it: by a 1024th of a
/// point or more. Less is rounding, not movement. Sums at a column's scale
/// land on neighbouring floats (a list's 29.2342 and 29.234184) depending on
/// what was laid out afresh, so a reader of the size that dirties layout
/// would flip it each pass, for ever (#1027). What creeps is still told,
/// as this compares with what was told last.
#[inline]
fn resized(told: (f32, f32), now: (f32, f32)) -> bool {
    !((told.0 - now.0).abs() < SIZE_NOISE && (told.1 - now.1).abs() < SIZE_NOISE)
}

fn allocated_content_size(scene: &Scene, node: NodeId) -> (f32, f32) {
    // Its own box: a size, whatever moved it.
    let bounds = scene.layout_bounds(node);
    let padding = scene.content_insets(node);
    (
        (bounds.width - padding.left - padding.right).max(0.),
        (bounds.height - padding.top - padding.bottom).max(0.),
    )
}

#[cfg(test)]
mod content_size_observer_tests {
    use super::*;

    #[test]
    fn resize_subscriber_can_remove_itself_and_another_observed_subtree() {
        let mut ui = Ui::new(400., 200.);
        let first = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let second = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let first_size = ui.observe_content_size(first);
        let second_size = ui.observe_content_size(second);
        let weak = ui.downgrade();
        ui.bind(first, move || {
            if first_size.get().0 > 100. {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(first);
                ui.remove(second);
            }
        });
        let second_updates = Rc::new(Cell::new(0));
        let updates = second_updates.clone();
        ui.bind(second, move || {
            second_size.get();
            updates.set(updates.get() + 1);
        });
        ui.scene.borrow_mut().set_style(first, fixed(150., 40.));
        ui.scene.borrow_mut().set_style(second, fixed(150., 40.));
        ui.prepare_frame();
        assert!(!ui.scene.borrow().contains(first));
        assert!(!ui.scene.borrow().contains(second));
        assert!(ui.storage.content_sizes.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
        assert_eq!(second_updates.get(), 1);
        ui.prepare_frame();
    }

    #[test]
    fn resize_subscriber_can_mount_observer_and_reenter_frame_preparation() {
        let mut ui = Ui::new(400., 200.);
        let first = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let first_size = ui.observe_content_size(first);
        let created = Rc::new(RefCell::new(None));
        let created_output = created.clone();
        let weak = ui.downgrade();
        ui.bind(first, move || {
            if first_size.get().0 > 100. && created_output.borrow().is_none() {
                let mut ui = weak.upgrade().unwrap();
                let child = ui.container(first, Layout::Overlay, fixed(30., 20.));
                let size = ui.observe_content_size(child);
                *created_output.borrow_mut() = Some((child, size));
                ui.scene.borrow_mut().set_style(child, fixed(70., 25.));
                ui.prepare_frame();
            }
        });
        ui.scene.borrow_mut().set_style(first, fixed(150., 40.));
        ui.prepare_frame();
        let (child, size) = created.borrow().as_ref().unwrap().clone();
        assert_eq!(size.get(), (70., 25.));
        assert_eq!(ui.storage.content_sizes.borrow().len(), 2);
        ui.remove(first);
        assert!(!ui.scene.borrow().contains(child));
        assert!(ui.storage.content_sizes.borrow().is_empty());
        ui.prepare_frame();
    }
}

#[cfg(test)]
mod layout_feedback_tests {
    use super::*;

    fn feedback_can_be_repaired(next: fn(f32) -> f32) {
        let mut ui = Ui::new(1000., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let enabled = Rc::new(Cell::new(false));
        let active = enabled.clone();
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let scene = ui.scene.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            if active.get() {
                count.set(count.get() + 1);
                assert!(
                    count.get() < 200,
                    "frame preparation failed to bound feedback"
                );
                scene.borrow_mut().set_style(node, fixed(next(width), 40.));
            }
        });
        ui.try_prepare_frame().unwrap();
        enabled.set(true);
        ui.scene.borrow_mut().set_style(node, fixed(101., 40.));
        assert!(ui.try_prepare_frame().is_err());
        assert!(updates.get() > 1);
        enabled.set(false);
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        ui.try_prepare_frame().unwrap();
        let previous = updates.get();
        ui.try_prepare_frame().unwrap();
        assert_eq!(previous, updates.get());
        assert_eq!(ui.scene.borrow().bounds(node).width, 150.);
    }

    #[test]
    fn alternating_allocation_feedback_returns_error_and_recovers() {
        feedback_can_be_repaired(|width| if width == 101. { 102. } else { 101. });
    }

    #[test]
    fn monotonically_growing_allocation_feedback_returns_error_and_recovers() {
        feedback_can_be_repaired(|width| width + 1.);
    }

    #[test]
    fn reentrant_preparation_defers_to_outer_frame_until_allocation_settles() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let weak = ui.downgrade();
        let observed = Rc::new(Cell::new(0.));
        let latest = observed.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            latest.set(width);
            if width == 150. {
                let ui = weak.upgrade().unwrap();
                ui.scene.borrow_mut().set_style(node, fixed(180., 40.));
                // Reactive effects are already flushing. A nested preparation
                // must not spin waiting for this callback's own next delivery.
                ui.try_prepare_frame().unwrap();
            }
        });
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(observed.get(), 180.);
        assert_eq!(ui.scene.borrow().bounds(node).width, 180.);
    }

    #[test]
    fn panicking_resize_callback_does_not_disable_future_preparation() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let fail = Rc::new(Cell::new(true));
        let panic_once = fail.clone();
        let observed = Rc::new(Cell::new(0.));
        let latest = observed.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            if width > 100. && panic_once.replace(false) {
                panic!("resize callback failed");
            }
            latest.set(width);
        });
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.prepare_frame()));
        assert!(result.is_err());
        ui.scene.borrow_mut().set_style(node, fixed(180., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(observed.get(), 180.);
    }
}

fn slider_content_bounds(scene: &Scene, root: NodeId) -> (f32, f32) {
    let bounds = scene.bounds(root);
    let padding = scene.content_insets(root);
    (
        bounds.x + padding.left,
        (bounds.width - padding.left - padding.right).max(0.),
    )
}

#[cfg(test)]
mod bounds_observer_tests {
    use super::*;

    #[test]
    fn world_bounds_follow_ancestor_translation_without_layout_and_idle_notifications() {
        let mut ui = Ui::new(400., 200.);
        let parent = ui.container(ui.root(), Layout::Overlay, fixed(100., 80.));
        let child = ui.container(parent, Layout::Overlay, fixed(30., 20.));
        let bounds = ui.observe_bounds(child);
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let observed = bounds.clone();
        ui.bind(child, move || {
            observed.get();
            count.set(count.get() + 1);
        });
        // Observing does not lay out mid-mount: the first frame publishes the
        // laid-out bounds.
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        assert_eq!(bounds.get(), Rect::new(0., 0., 30., 20.));
        assert_eq!(updates.get(), 2);
        ui.scene
            .borrow_mut()
            .set_transform(parent, Transform { x: 12., y: 34. });
        ui.prepare_frame();
        assert_eq!(bounds.get(), Rect::new(12., 34., 30., 20.));
        assert_eq!(updates.get(), 3);
        assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
        ui.prepare_frame();
        assert_eq!(updates.get(), 3);
        ui.scene.borrow_mut().set_style(child, fixed(45., 25.));
        ui.prepare_frame();
        assert_eq!(bounds.get(), Rect::new(12., 34., 45., 25.));
        assert_eq!(updates.get(), 4);
        ui.remove(parent);
        assert!(ui.storage.observed_bounds.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
    }

    #[test]
    fn bounds_subscriber_can_dispose_itself_and_reenter_preparation() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let bounds = ui.observe_bounds(node);
        let weak = ui.downgrade();
        ui.bind(node, move || {
            if bounds.get().x > 0. {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(node);
                ui.prepare_frame();
            }
        });
        ui.scene
            .borrow_mut()
            .set_transform(node, Transform { x: 1., y: 0. });
        ui.prepare_frame();
        assert!(!ui.scene.borrow().contains(node));
        assert!(ui.storage.observed_bounds.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
    }

    /// A box whose width follows from its own width, 100 ↔ 200 for as long
    /// as `broken` holds, and how often its binding ran.
    fn oscillating(ui: &mut Ui, broken: Rc<Cell<bool>>) -> (NodeId, Rc<Cell<u32>>) {
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_size(node);
        let (scene, runs) = (Rc::downgrade(&ui.scene), Rc::new(Cell::new(0)));
        let told = runs.clone();
        ui.bind(node, move || {
            let seen = size.get().0;
            told.set(told.get() + 1);
            if broken.get() {
                let width = if seen == 200. { 100. } else { 200. };
                let scene = scene.upgrade().unwrap();
                scene.borrow_mut().set_style(node, fixed(width, 40.));
            }
        });
        (node, runs)
    }

    #[test]
    fn layout_that_never_settles_is_held_reported_once_and_recovers() {
        let mut ui = Ui::new(400., 200.);
        ui.contain_layout_feedback();
        let issues = Rc::new(RefCell::new(Vec::<String>::new()));
        let heard = issues.clone();
        // A degradation: the app goes on, and the cause can be the engine's.
        let _listener = crate::issues::on_issue_kind(&ui.runtime, move |kind, issue| {
            assert_eq!(kind, crate::issues::IssueKind::Degraded);
            heard.borrow_mut().push(issue.to_string());
        });
        let delivered = Rc::new(Cell::new(0));
        let read = delivered.clone();
        ui.on_event(ui.root(), false, move |_| read.set(read.get() + 1));
        let broken = Rc::new(Cell::new(true));
        let (node, runs) = oscillating(&mut ui, broken.clone());

        // It would have ended the app: it goes on, told once which node.
        ui.try_prepare_frame().unwrap();
        assert_eq!(issues.borrow().len(), 1);
        let said = issues.borrow()[0].clone();
        assert!(said.contains(&format!("@{}", node.number())), "{said}");
        assert!(said.contains("did not settle after 64 passes"), "{said}");

        // Held: frames do nothing more, so an idle window stays idle.
        let held = runs.get();
        let bounds = ui.scene.borrow().bounds(node);
        for _ in 0..5 {
            ui.try_prepare_frame().unwrap();
        }
        assert_eq!(runs.get(), held, "nothing hears a held layout");
        assert_eq!(ui.scene.borrow().bounds(node), bounds);

        // Input goes on, against what shows.
        ui.try_dispatch(InputEvent::PointerMove { x: 1., y: 1. })
            .unwrap();
        assert!(delivered.get() > 0);

        // Something else changes it: tried again in fewer passes, unreported.
        let other = ui.container(ui.root(), Layout::Overlay, fixed(10., 10.));
        ui.try_prepare_frame().unwrap();
        let retried = runs.get() - held;
        assert!((1..=16).contains(&retried), "{retried} passes");
        assert_eq!(issues.borrow().len(), 1, "one trouble, one report");
        ui.remove(other);

        // Repaired (a reload), it settles and geometry is heard again.
        broken.set(false);
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(ui.scene.borrow().bounds(node).width, 150.);
        let settled = runs.get();
        ui.scene.borrow_mut().set_style(node, fixed(120., 40.));
        ui.try_prepare_frame().unwrap();
        assert!(runs.get() > settled, "heard again once settled");
        assert_eq!(issues.borrow().len(), 1);

        // A new trouble is a new report.
        broken.set(true);
        ui.scene.borrow_mut().set_style(node, fixed(100., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(issues.borrow().len(), 2);
    }

    /// #1027: a size flipping between neighbouring floats, its reader
    /// dirtying layout each time, settles; it is rounding, not movement.
    #[test]
    fn a_size_differing_only_by_rounding_settles_and_real_changes_are_told() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_size(node);
        let (scene, seen) = (Rc::downgrade(&ui.scene), Rc::new(Cell::new((0., 0.))));
        let (told, flips, wanted) = (
            seen.clone(),
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(100.)),
        );
        let (flipped, want) = (flips.clone(), wanted.clone());
        ui.bind(node, move || {
            told.set(size.get());
            // As a list's height does: each reading nudges it a float over.
            let nudge = if flipped.get() % 2 == 0 { 2e-5 } else { 0. };
            flipped.set(flipped.get() + 1);
            let scene = scene.upgrade().unwrap();
            scene
                .borrow_mut()
                .set_style(node, fixed(want.get() + nudge, 40.));
        });
        ui.try_prepare_frame().unwrap();
        assert!((seen.get().0 - 100.).abs() < 0.001, "{:?}", seen.get());
        assert!(flips.get() < 8, "{} readings", flips.get());
        // A change anyone could see is told, at once.
        wanted.set(120.);
        ui.scene.borrow_mut().set_style(node, fixed(120., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(seen.get().0.round(), 120.);
        // So is a creep too slow for any one step to tell, once it adds up.
        let base = seen.get().0;
        for step in 1..=4 {
            let width = base + step as f32 * 0.0004;
            wanted.set(width);
            ui.scene.borrow_mut().set_style(node, fixed(width, 40.));
            ui.try_prepare_frame().unwrap();
        }
        assert!(seen.get().0 > base + 0.0009, "{:?}", seen.get());
    }

    /// A hook can hold a script's whole VM: it goes with its editor.
    #[test]
    fn a_paste_hook_goes_with_its_editor() {
        let mut ui = Ui::new(400., 200.);
        let held = Rc::new(());
        let editor_under = |ui: &mut Ui, parent: NodeId, held: &Rc<()>| {
            let value = ui.signal(String::new());
            let node = ui.text_input(parent, "Field", value, 100., false).node;
            let kept = held.clone();
            ui.set_paste_hook(
                node,
                Rc::new(move |_| {
                    let _ = &kept;
                    true
                }),
            );
            node
        };
        let root = ui.root();
        let node = editor_under(&mut ui, root, &held);
        assert!(ui.takes_paste(node));
        assert_eq!(Rc::strong_count(&held), 2);
        ui.remove(node);
        ui.prepare_frame();
        assert_eq!(Rc::strong_count(&held), 1);
        assert!(!ui.takes_paste(node));
        // Nor does a large removal, which frees its nodes over later frames.
        let parent = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        editor_under(&mut ui, parent, &held);
        for _ in 0..=RECLAIM_NOW {
            ui.container(parent, Layout::Overlay, fixed(1., 1.));
        }
        assert_eq!(Rc::strong_count(&held), 2);
        ui.remove(parent);
        assert_eq!(Rc::strong_count(&held), 1);
    }

    #[test]
    fn without_containment_layout_that_never_settles_is_an_error() {
        let mut ui = Ui::new(400., 200.);
        oscillating(&mut ui, Rc::new(Cell::new(true)));
        assert_eq!(ui.try_prepare_frame().unwrap_err().passes, 64);
    }

    #[test]
    fn transform_feedback_is_bounded_and_recovers_after_disposal() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let bounds = ui.observe_bounds(node);
        let scene = Rc::downgrade(&ui.scene);
        ui.bind(node, move || {
            let x = bounds.get().x;
            scene
                .upgrade()
                .unwrap()
                .borrow_mut()
                .set_transform(node, Transform { x: x + 1., y: 0. });
        });
        assert_eq!(ui.try_prepare_frame().unwrap_err().passes, 64);
        ui.remove(node);
        ui.try_prepare_frame().unwrap();
    }
}

#[cfg(test)]
mod editor_layout_cache_tests {
    use super::*;
    use crate::text_layout::FontFamily;

    #[test]
    fn cache_charges_named_font_key_and_bounds_display_text() {
        let mut ui = Ui::new(300., 200.);
        let editor = ui.text_input(
            ui.root(),
            "Editor",
            ui.signal(String::from("short")),
            100.,
            true,
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(editor.node));
        let storage = Rc::downgrade(&ui.storage);
        let scene = ui.scene.borrow();
        let font = FontStyle::default();
        let first = editor_layout(&storage, &scene, editor.node, "short", 12., None, &font);
        let second = editor_layout(&storage, &scene, editor.node, "short", 12., None, &font);
        assert!(Rc::ptr_eq(&first, &second));
        let huge_font = FontStyle {
            family: FontFamily::Named("f".repeat(EDITOR_CACHE_WEIGHT_LIMIT).into()),
            ..font.clone()
        };
        editor_layout(
            &storage,
            &scene,
            editor.node,
            "short",
            12.,
            None,
            &huge_font,
        );
        assert!(ui.storage.editor_layout.borrow().is_none());
        editor_layout(
            &storage,
            &scene,
            editor.node,
            &"x".repeat(EDITOR_CACHE_TEXT_LIMIT + 1),
            12.,
            None,
            &font,
        );
        assert!(ui.storage.editor_layout.borrow().is_none());
    }

    #[test]
    fn external_editor_handle_does_not_retain_ui_layout_snapshot() {
        let mut ui = Ui::new(300., 200.);
        let editor = ui.text_input(
            ui.root(),
            "Editor",
            ui.signal(String::from("short")),
            100.,
            true,
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(editor.node));
        let layout = Rc::downgrade(&ui.storage.editor_layout.borrow().as_ref().unwrap().layout);
        drop(ui);
        assert!(layout.upgrade().is_none());
        // An externally retained handle may refresh its surviving scene, but
        // cannot resurrect ownership of the destroyed document's cache.
        editor.refresh();
        assert!(layout.upgrade().is_none());
    }
}

#[cfg(test)]
mod disabled_reentry_tests {
    use super::*;
    #[test]
    fn disabling_can_remove_the_target_during_cleanup_without_resurrecting_metadata() {
        for pointer in [false, true] {
            let mut ui = Ui::new(100., 100.);
            let node = ui.container(
                ui.root(),
                Layout::Overlay,
                Style {
                    width: Some(40.),
                    height: Some(40.),
                    ..Default::default()
                },
            );
            let weak = ui.downgrade();
            ui.on_event(node, true, move |cx| {
                if matches!(cx.event, InputEvent::PointerDown { .. }) {
                    cx.capture_pointer();
                }
                if (pointer && matches!(cx.event, InputEvent::PointerCancel))
                    || (!pointer && matches!(cx.event, InputEvent::Blur))
                {
                    weak.upgrade().unwrap().remove(node);
                }
            });
            ui.prepare_frame();
            ui.input.focus(&ui.scene, Some(node));
            if pointer {
                ui.dispatch(InputEvent::PointerDown {
                    x: 10.,
                    y: 10.,
                    button: PointerButton::Primary,
                });
            }
            ui.set_disabled(node, true);
            assert!(!ui.scene.borrow().contains(node));
            assert!(ui.input.options(node).is_none());
            assert!(ui.semantics.borrow().get(node).is_none());
            assert_eq!(ui.input.focused(), None);
            assert_eq!(ui.input.captured(), None);
        }
    }
}

#[cfg(test)]
mod mount_initialization_tests {
    use super::*;
    #[test]
    fn successful_binding_does_not_retain_initialization_token() {
        let mut ui = Ui::new(100., 100.);
        let owner = ui.label(ui.root(), "owner", Style::default());
        let token = ui.begin_mount_initialization();
        token.set_root(owner);
        let weak = Rc::downgrade(&token);
        {
            let _scope = token.enter();
            ui.bind(owner, || {});
        }
        drop(token);
        assert!(weak.upgrade().is_none());
        assert_eq!(ui.runtime.effect_count(), 1);
    }
}
