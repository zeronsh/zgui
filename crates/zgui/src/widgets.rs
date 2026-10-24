//! Retained widgets with reactive values, keyboard input and accessibility semantics.
use crate::{
    input::{EventPhase, InputBinding, InputDispatcher, InputEvent, Key, PointerButton},
    reactive::{Effect, Runtime, Signal},
    scene::{Color, Insets, Layout, NodeId, NodeKind, Rect, Scene, Style, Transform},
    semantics::{Role, SemanticNode, Semantics},
    text::{Text, TextValue},
    text_edit::TextEditor,
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

pub use crate::editor_lines::{Highlighter, LineColors};

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
    select_on_focus: Cell<bool>,
    /// A password field: its text shows masked and never leaves it.
    secure: Cell<bool>,
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
    /// What a code editor adds (`CodeHint`), made when first used: an
    /// editor without it carries a pointer's room.
    code: std::cell::OnceCell<Box<CodeHint>>,
}
/// What a code editor adds to an editor's hint: its long text's lines,
/// marks behind its selection, its current line, Tab's indent, and who is
/// told where its caret is.
#[derive(Default)]
struct CodeHint {
    /// Ranges of the text marked with a colour behind them (a search's
    /// matches), by where they start.
    decorations: RefCell<Vec<(std::ops::Range<usize>, Color)>>,
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
    /// Told where the caret is when it moves (`EditorHandle::on_caret`).
    on_caret: RefCell<Option<CaretListener>>,
    /// Where it was last drawn, and last told.
    caret_drawn: Cell<Option<Caret>>,
    caret_told: Cell<Option<Caret>>,
}
impl EditorHint {
    fn new(value: EditorValue) -> Self {
        Self {
            value,
            placeholder: RefCell::default(),
            select_on_focus: Cell::default(),
            secure: Cell::default(),
            misspelled: RefCell::default(),
            clause: Cell::default(),
            composition_layer: Cell::default(),
            reveal: Cell::default(),
            code: std::cell::OnceCell::default(),
        }
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
    /// Where an input method's candidate window goes: under the clause it
    /// is converting (Japanese, Chinese), as a native field puts it, in the
    /// window's points; `None` while there is none (the caret, then).
    pub fn composition_area(&self) -> Option<Rect> {
        let clause = self.hint.clause.get()?;
        let layer = self.hint.composition_layer.get()?;
        let scene = self.scene.borrow();
        if !scene.contains(layer) {
            return None;
        }
        let at = scene.bounds(layer);
        Some(Rect::new(
            at.x + clause.x,
            at.y + clause.y,
            clause.width,
            clause.height,
        ))
    }
    /// The selection as UTF-8 byte offsets into the committed text,
    /// `(anchor, focus)`: equal for a caret, `focus` where the caret is.
    pub fn selection(&self) -> (usize, usize) {
        let selection = self.editor.borrow().selection();
        (selection.anchor, selection.focus)
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
    /// A password field: its text shows as a bullet per character (each in
    /// that character's place, so carets and selections work), and copying
    /// or cutting it gives nothing.
    pub fn set_secure(&self, secure: bool) {
        self.hint.secure.set(secure);
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
        if !self.multiline {
            return;
        }
        self.hint.line_cache().set_highlighter(highlighter);
        self.refresh();
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
    /// Call `told` with where the caret is whenever it moves or the
    /// selection changes (a status bar's line and column), after the
    /// editor is drawn: it may change what it likes.
    pub fn on_caret(&self, told: impl FnMut(Caret) + 'static) {
        let code = self.hint.code_made();
        *code.on_caret.borrow_mut() = Some(Box::new(told));
        code.caret_told.set(None);
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
            self.refresh();
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
    /// Replace the selection with `text`, as the person pasting it would. A
    /// single-line editor joins its lines; a read-only one ignores it.
    pub fn paste(&self, text: &str) {
        if self.is_read_only() {
            return;
        }
        self.preferred_x.set(None);
        self.editor
            .borrow_mut()
            .insert(&normalize_text(text, self.multiline));
        self.hint.value.write(&self.editor);
        self.refresh();
    }
}
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
    storage: Rc<UiStorage>,
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
/// macOS over the first); mimalloc doesn't (zgui-desktop's `mimalloc`).
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

/// `line` in runs of the colours `colors` give it, the rest `color`: a
/// highlighted line (`None` if it has none to give).
fn line_runs(
    line: &str,
    colors: &LineColors,
    color: Color,
    size: f32,
    font: &FontStyle,
) -> Option<crate::rich_text::RichText> {
    if line.is_empty() {
        return None;
    }
    let run = |range: std::ops::Range<usize>, color| crate::rich_text::TextRun {
        range,
        font: font.clone(),
        font_size: size,
        color,
        ..Default::default()
    };
    let mut runs = Vec::with_capacity(colors.len() * 2 + 1);
    let mut at = 0;
    for (range, shade) in colors {
        let start = range.start.max(at);
        if start >= range.end {
            continue;
        }
        if start > at {
            runs.push(run(at..start, color));
        }
        runs.push(run(start..range.end, *shade));
        at = range.end;
    }
    if at < line.len() {
        runs.push(run(at..line.len(), color));
    }
    crate::rich_text::RichText::new(line, runs).ok()
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
        || scene.line_shaper(size, font),
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
}
impl EditorDrag {
    fn cancel(&self) {
        self.mode.set(None);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}
struct UiStorage {
    runtime: Runtime,
    themes: Rc<Themes>,
    /// The theme scope of the element being mounted (`Ui::enter_theme`).
    mount_theme: RefCell<Option<Rc<ThemeScope>>>,
    text_geometry_revision: Signal<u64>,
    presented: Signal<bool>,
    /// Where popups may show (`Ui::set_popup_screens`).
    popup_screens: Signal<Vec<Rect>>,
    /// Open signals of anchored popups, by their popup roots
    /// (`Ui::dismiss_popups`).
    popups: RefCell<Vec<(NodeId, Signal<bool>)>>,
    /// The host's hook for a popup about to open (`Ui::on_popup_soon`), and
    /// whether it was called and not yet heard (`popup_soon_heard`).
    popup_soon: RefCell<Option<Rc<dyn Fn()>>>,
    popup_soon_sent: Cell<bool>,
    /// The host shows drag previews in windows of their own (`set_popup_previews`).
    popup_previews: Cell<bool>,
    scene: Rc<RefCell<Scene>>,
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
    editor_layout: RefCell<Option<EditorLayoutEntry>>,
    editor_layout_revision: Cell<Option<u64>>,
    bounds_geometry_revision: Cell<Option<u64>>,
    preparing_frame: Cell<bool>,
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
    /// Display text's selection (`View::selectable`).
    selection: RefCell<crate::selection::State>,
    /// What the last primary press went to: the keyboard scrolls the scroll
    /// view around it (`scroll_keys`).
    last_pressed: Cell<Option<NodeId>>,
    /// Elastic scrolling (`Ui::set_elastic_scrolling`): off, never read.
    elastic: RefCell<crate::elastic::State>,
    /// Scrollbars hide until their view scrolls (`Ui::set_overlay_scrollbars`).
    overlay_scrollbars: Signal<bool>,
    /// Overlay scrollbars shown, to fade once their view stops scrolling.
    scrollbar_fades: RefCell<Vec<std::rc::Weak<crate::compose_scrollbar::Fade>>>,
    /// Input is being dispatched: what scrolls now, the user scrolled.
    dispatching: Cell<bool>,
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
        let storage = Rc::new(UiStorage {
            themes: Themes::of(&runtime),
            mount_theme: RefCell::new(None),
            text_geometry_revision: runtime.signal(0),
            presented: runtime.signal(true),
            overlay_scrollbars: runtime.signal(false),
            popup_screens: runtime.signal(Vec::new()),
            popups: RefCell::new(Vec::new()),
            popup_soon: RefCell::new(None),
            popup_soon_sent: Cell::new(false),
            popup_previews: Cell::new(false),
            runtime,
            scene: Rc::new(RefCell::new(Scene::new(width, height))),
            input: InputDispatcher::new(),
            semantics: Rc::new(RefCell::new(Semantics::new())),
            owned: RefCell::default(),
            dying: RefCell::default(),
            removed: Cell::new(false),
            editors: RefCell::default(),
            editor_layout: RefCell::new(None),
            editor_layout_revision: Cell::new(None),
            bounds_geometry_revision: Cell::new(None),
            preparing_frame: Cell::new(false),
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
            selection: Default::default(),
            last_pressed: Cell::new(None),
            elastic: Default::default(),
            scrollbar_fades: RefCell::new(Vec::new()),
            dispatching: Cell::new(false),
            document_listeners: RefCell::new(Vec::new()),
        });
        Self::from_storage(storage)
    }
    fn from_storage(storage: Rc<UiStorage>) -> Self {
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
    pub fn bind(&mut self, owner: NodeId, mut f: impl FnMut() + 'static) {
        assert!(
            self.scene.borrow().contains(owner),
            "cannot bind a removed UI node"
        );
        let mut initial = self.storage.mount_initialization.borrow().clone();
        self.runtime.clone().batch(|| {
            let effect = self.runtime.effect(move || {
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
            });
            self.storage
                .owned
                .borrow_mut()
                .entry(owner)
                .or_default()
                .effects
                .push(effect);
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
    pub(crate) fn elastic_state(&self) -> &RefCell<crate::elastic::State> {
        &self.storage.elastic
    }
    pub(crate) fn observe_presentation(&self) -> Signal<bool> {
        self.storage.presented.clone()
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
    /// and call again. Native window hosts propagate this error to `Application::run`.
    pub fn try_prepare_frame(&self) -> Result<(), LayoutFeedbackError> {
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
        for _ in 0..64 {
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
                                let b = scene.layout_bounds(*node);
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
                        .map(|(node, signal)| (*node, signal.clone(), scene.bounds(*node)))
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
                    if self.scene.borrow().contains(node) {
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
        Err(LayoutFeedbackError { passes: 64 })
    }
    /// Recompute editor geometry after changing the font backend or display scale.
    pub(crate) fn observe_text_geometry_revision(&self) -> Signal<u64> {
        self.storage.text_geometry_revision.clone()
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
            event @ InputEvent::Scroll { .. } if elastic => {
                match crate::elastic::before_scroll(self, event) {
                    Some(event) => event,
                    None => return Default::default(),
                }
            }
            event => event,
        };
        let scroll = (elastic && matches!(event, InputEvent::Scroll { .. })).then(|| event.clone());
        let mut result = dispatch(self, event);
        self.after_dispatch(after, &mut result);
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
    /// Next deadline for active editor drag autoscroll. Idle editors schedule nothing.
    pub fn next_interaction_deadline(&self) -> Option<Instant> {
        [
            self.input.next_key_deadline(),
            self.next_editor_interaction_deadline(),
            self.next_selection_deadline(),
            crate::compose_scrollbar::next_fade(self),
            crate::elastic::next_step(self),
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
        // Overlay scrollbars fading out.
        let keys = if crate::compose_scrollbar::advance_fades(self, now) {
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
        // Detach geometry before listener captures or cached layouts can run
        // user destructors. Reentrant removal of this owner (or an ancestor)
        // then observes an already removed subtree instead of stale node IDs.
        self.scene.borrow_mut().remove(node);
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
        self.semantics.borrow_mut().bury(&ids);
        self.input.forget_removed(&self.scene.borrow());
        self.storage.dying.borrow_mut().extend(ids);
        self.storage.removed.set(true);
        // User destructors, once nothing names the subtree.
        drop(cached_layout);
        drop(resources);
        drop(editors);
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
        let selection_nodes = Rc::new(RefCell::new(Vec::new()));
        // A selection showing when the theme (or the system's accent)
        // changes takes the new colour, as one drawn after it would.
        {
            let (nodes, scene, theme) =
                (selection_nodes.clone(), self.scene.clone(), theme.clone());
            self.bind(root, move || {
                let color = theme.color(tok::SELECTION);
                let mut s = scene.borrow_mut();
                for id in nodes.borrow().iter() {
                    if s.contains(*id) {
                        s.set_kind(*id, NodeKind::Rect(color));
                    }
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
        let scene = self.scene.clone();
        let sem = self.semantics.clone();
        let edit = editor.clone();
        let focus = focused.clone();
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
        hint.composition_layer.set(Some(preedit_layer));
        let draw_hint = hint.clone();
        // Its text's line count and its semantic value, by the revision of
        // the text they are for.
        let draw_line_count: Cell<Option<(u64, usize)>> = Cell::new(None);
        let draw_semantic_revision: Cell<Option<u64>> = Cell::new(None);
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
                    hint_color.unwrap_or(theme.color_untracked(tok::TEXT_TERTIARY)),
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
            s.set_kind(caret, NodeKind::Rect(color));
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
            let asked = draw_hint.reveal.take();
            if (*focus.borrow() || asked) && reveal {
                if cx - scroll.0 > (inner_width - 8.).max(0.) {
                    scroll.0 = cx - ((inner_width - 8.).max(0.));
                }
                if cx < scroll.0 {
                    scroll.0 = cx;
                }
                if cy - scroll.1 > (inner_height - line_height).max(0.) {
                    scroll.1 = cy - ((inner_height - line_height).max(0.));
                }
                if cy < scroll.1 {
                    scroll.1 = cy;
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
            s.set_style(caret, fixed(1., line_height));
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
                        let colored = draw_hint
                            .line_cache()
                            .colors(lines, *i)
                            .and_then(|colors| line_runs(&line, &colors, color, size, &font));
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
                        cx.min((inner_width - 1.).max(0.))
                    } else {
                        cx - scroll.0
                    },
                    y: cy - scroll.1,
                },
            );
            let mut effects = s.effects(caret);
            // Read-only text shows a selection but no caret.
            effects.opacity = if *focus.borrow()
                && !draw_read_only.get()
                && e.preedit().is_none_or(|preedit| preedit.cursor.is_some())
            {
                1.
            } else {
                0.
            };
            s.set_effects(caret, effects);
            // An editable field shows its selection only while it has the focus (or its
            // window merely went to the back), as AppKit's do: the selection is kept, and
            // shows again when the focus comes back. Read-only text gives its up on blur.
            let shows_selection =
                *focus.borrow() || draw_window_blurred.get() || draw_read_only.get();
            let rects: Vec<_> = if e.preedit().is_none() && shows_selection {
                layout
                    .selection_within(selection.range(), scroll.1, scroll.1 + inner_height)
                    .into_iter()
                    .filter_map(|r| {
                        let y = r.y - scroll.1;
                        (y + r.height > 0. && y < inner_height).then_some((
                            r.x - scroll.0,
                            y,
                            r.width,
                            r.height,
                        ))
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
                        marks.extend(rects.into_iter().map(|r| {
                            (
                                Rect::new(r.x - scroll.0, r.y - scroll.1, r.width, r.height),
                                *mark,
                            )
                        }));
                    }
                }
                if !marks.is_empty()
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
            }
            let mut nodes = selection_nodes.borrow_mut();
            while nodes.len() > rects.len() {
                s.remove(nodes.pop().unwrap());
            }
            while nodes.len() < rects.len() {
                nodes.push(s.append(
                    selection_layer,
                    NodeKind::Rect(theme.color_untracked(tok::SELECTION)),
                    Style::default(),
                ));
            }
            for (id, (x, y, w, h)) in nodes.iter().zip(rects) {
                s.set_style(*id, fixed(w, h));
                s.set_transform(*id, Transform { x, y });
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
            // show.
            let mut clause_area = None;
            let lines: Vec<_> = if let Some(preedit) = e.preedit() {
                let start = selection.range().start;
                let line = |r: Rect, thick: f32| {
                    let y = r.y + r.height - 1. - thick - scroll.1;
                    (y >= 0. && y < inner_height).then_some((r.x - scroll.0, y, r.width, thick))
                };
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
            } else {
                Vec::new()
            };
            draw_hint.clause.set(clause_area);
            let mut underline = preedit_nodes.borrow_mut();
            while underline.len() > lines.len() {
                s.remove(underline.pop().unwrap());
            }
            while underline.len() < lines.len() {
                underline.push(s.append(
                    preedit_layer,
                    surface(&theme, theme.color_untracked(tok::ACCENT)),
                    Style::default(),
                ));
            }
            for (id, (x, y, w, thick)) in underline.iter().zip(lines) {
                s.set_style(*id, fixed(w, thick));
                s.set_transform(*id, Transform { x, y });
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
                    let y = r.y + r.height - 2. - scroll.1;
                    (y >= 0. && y < inner_height).then_some((r.x - scroll.0, y, r.width))
                })
                .collect();
            let mut spelled = spell_nodes.borrow_mut();
            while spelled.len() > marks.len() {
                s.remove(spelled.pop().unwrap());
            }
            while spelled.len() < marks.len() {
                spelled.push(s.append(
                    preedit_layer,
                    spelling_mark(theme.color_untracked(tok::DANGER)),
                    Style::default(),
                ));
            }
            for (id, (x, y, w)) in spelled.iter().zip(marks) {
                s.set_style(*id, fixed(w, 2.));
                s.set_transform(*id, Transform { x, y });
            }
            if let Some(code) = draw_hint.code()
                && code.on_caret.borrow().is_some()
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
            Rc::new(move || {
                refresh();
                // Where the caret is, told once drawn (nothing borrowed),
                // if it moved; the callback is out while it runs.
                if let Some(code) = told.code() {
                    let caret = code.caret_drawn.get();
                    if caret.is_some() && caret != code.caret_told.get() {
                        code.caret_told.set(caret);
                        let callback = code.on_caret.borrow_mut().take();
                        if let (Some(mut callback), Some(caret)) = (callback, caret) {
                            callback(caret);
                            let mut slot = code.on_caret.borrow_mut();
                            if slot.is_none() {
                                *slot = Some(callback);
                            }
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
        self.bind(root, move || {
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
            draw();
        });
        let scene = self.scene.clone();
        let input_storage = Rc::downgrade(&self.storage);
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
                        if current.0.byte_offset < edges.0.byte_offset {
                            (edges.1.byte_offset, current.0)
                        } else if current.1.byte_offset > edges.1.byte_offset {
                            (edges.0.byte_offset, current.1)
                        } else {
                            (edges.0.byte_offset, edges.1)
                        }
                    }
                };
                edit.set_selection(anchor, focus.byte_offset);
                position.set(focus);
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
                        || (mac_control(*modifiers) && matches!(key, Key::Character(k) if ["d", "h", "k"].iter().any(|c| k.eq_ignore_ascii_case(c)))),
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
                    if hint.select_on_focus.get() && !window_blurred.replace(false) {
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
                    let x = x - bounds.x + scroll.0;
                    let y = y - bounds.y + scroll.1;
                    let hit = layout.hit_position(x, y);
                    let mode = if cx.pointer_modifiers().shift {
                        EditorPointerSelection::Character(edit.selection().anchor)
                    } else {
                        match cx.click_count() {
                            2 => {
                                let edges = editor_pointer_unit(edit.around(hit.byte_offset), layout.as_ref(), hit, x, y, false, multiline);
                                EditorPointerSelection::Unit { edges, line: false }
                            }
                            3 => {
                                let edges = editor_pointer_unit(edit.around(hit.byte_offset), layout.as_ref(), hit, x, y, true, multiline);
                                EditorPointerSelection::Unit { edges, line: true }
                            }
                            _ => EditorPointerSelection::Character(hit.byte_offset),
                        }
                    };
                    let (anchor, focus) = match mode {
                        EditorPointerSelection::Character(anchor) => (anchor, hit),
                        EditorPointerSelection::Unit { edges, .. } => (edges.0.byte_offset, edges.1),
                    };
                    edit.set_selection(anchor, focus.byte_offset);
                    position.set(focus);
                    gesture.cancel();
                    gesture.mode.set(Some(mode));
                }
                // Middle-click paste, where the desktop has a primary
                // selection: the caret goes under the pointer, and the host
                // inserts the selection there.
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Middle,
                } if cfg!(all(unix, not(target_os = "macos"))) && !read_only.get() => {
                    let s = scene.borrow();
                    let bounds = s.bounds(viewport);
                    let scroll = *scroll.borrow();
                    let (_, size, font) = typography.borrow().clone();
                    let mut edit = editor.borrow_mut();
                    let layout = committed_layout(&input_storage, &s, root, &edit, size,
                        wrap.get().then_some(bounds.width.max(1.)), &font,
                    );
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
                    gesture.cancel();
                    cx.release_pointer();
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
                    // line's start and end, a character or a line either way,
                    // deleting either way, and to the line's end (Control+K).
                    let mut kill = false;
                    let (key, modifiers) = match &key {
                        Key::Character(k) if mac_control(modifiers) => {
                            let plain = crate::input::Modifiers {
                                control: false,
                                ..modifiers
                            };
                            match k.to_ascii_lowercase().as_str() {
                                "a" => (Key::Home, plain),
                                "e" => (Key::End, plain),
                                "f" => (Key::ArrowRight, plain),
                                "b" => (Key::ArrowLeft, plain),
                                "n" => (Key::ArrowDown, plain),
                                "p" => (Key::ArrowUp, plain),
                                "d" => (Key::Delete, plain),
                                "h" => (Key::Backspace, plain),
                                "k" => {
                                    kill = true;
                                    (key.clone(), plain)
                                }
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
                        Key::Home | Key::End if !command && wrap.get() => Some(key == Key::End),
                        Key::ArrowLeft | Key::ArrowRight
                            if cfg!(target_os = "macos") && modifiers.meta && wrap.get() =>
                        {
                            Some(key == Key::ArrowRight)
                        }
                        _ => None,
                    };
                    let directional = paging || (matches!(key, Key::ArrowUp | Key::ArrowDown)
                        && !(cfg!(target_os = "macos") && modifiers.meta));
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
                            Key::Character(_) if kill => {
                                changed = e.delete_to_line_end();
                            }
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
                            Key::Tab
                                if !command
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
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("z") => {
                                changed = if modifiers.shift { e.redo() } else { e.undo() };
                            }
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("y") => {
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

pub(crate) fn editor_pointer_unit(
    around: (usize, std::borrow::Cow<'_, str>),
    layout: &dyn TextLayout,
    hit: TextPosition,
    x: f32,
    y: f32,
    line: bool,
    multiline: bool,
) -> (TextPosition, TextPosition) {
    if line && multiline {
        return layout.visual_line_range(hit);
    }
    // The text around the hit (its line, in a long text) from where it
    // starts in the whole.
    let (base, text) = (around.0, &*around.1);
    let (start, end) = if line {
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
    let (start, segment) = text
        .split_word_bound_indices()
        .find(|(start, segment)| index < start + segment.len())
        .or_else(|| text.split_word_bound_indices().next_back())
        .unwrap_or((0, ""));
    let end = start + segment.len();
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
