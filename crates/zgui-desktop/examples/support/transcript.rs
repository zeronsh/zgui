//! A Zeron-style agent transcript: a scripted session that streams assistant
//! text and dozens of tool calls. Layout, colours and motion follow Zeron's
//! `crates/ui/src/transcript.rs`: tool groups with a connector tree, rows that
//! grow in with a staggered branch, a shimmering group title while tools run,
//! a fading veil on streamed text, and groups that fold when the turn ends.
//! Icons are from the Solar icon set (CC BY 4.0), as bundled with Zeron.
//!
//! Animations read a clock that only ticks while something moves, so the
//! page costs nothing once a replay settles.

use std::{
    cell::Cell,
    collections::HashMap,
    f32::consts::FRAC_PI_2,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use zgui::{
    affine::Affine,
    compose::{Tasks, prelude::*},
    cursor::Cursor,
    frame::FrameClock,
    reactive::{Runtime, Signal},
    scene::Color,
    svg::SvgData,
    text_layout::FontFamily,
    timer::sleep,
};

use super::{Tween, ease_out_expo, eyebrow, header, pill_button, smoothstep};

// White at Zeron's text/muted/faint contrast, as alpha so glass shows through.
const TEXT: Color = Color(255, 255, 255, 230);
const MUTED: Color = Color(255, 255, 255, 158);
const FAINT: Color = Color(255, 255, 255, 112);
/// Output bodies open and close over the same curve as group folds.
const OPEN: f32 = 0.2;
/// Thoughts wrap at this many columns, like Zeron's 96 at a wider width,
/// so a body's height is known before layout and can animate.
const THOUGHT_COLUMNS: usize = 76;
const DANGER: Color = Color(255, 100, 103, 255);
const ADDED: Color = Color(0, 212, 146, 255);
const ACCENT: Color = Color(124, 134, 255, 255);

fn ink(alpha: f32) -> Color {
    Color(255, 255, 255, (alpha * 255.).round() as u8)
}

fn mix(from: Color, to: Color, t: f32) -> Color {
    let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color(
        lerp(from.0, to.0),
        lerp(from.1, to.1),
        lerp(from.2, to.2),
        lerp(from.3, to.3),
    )
}

fn ease_out_quint(x: f32) -> f32 {
    1. - (1. - x.clamp(0., 1.)).powi(5)
}

/// Progress through `[start, end]` of a normalised timeline.
fn window(p: f32, start: f32, end: f32) -> f32 {
    ((p - start) / (end - start)).clamp(0., 1.)
}

const ICONS: [(&str, &[u8]); 12] = [
    ("chevron", include_bytes!("icons/alt-arrow-down.svg")),
    ("bot", include_bytes!("icons/bot.svg")),
    ("thought", include_bytes!("icons/chat-round-line.svg")),
    ("checklist", include_bytes!("icons/checklist.svg")),
    ("document-add", include_bytes!("icons/document-add.svg")),
    ("document", include_bytes!("icons/document.svg")),
    ("folder", include_bytes!("icons/folder-with-files.svg")),
    ("global", include_bytes!("icons/global.svg")),
    ("search", include_bytes!("icons/magnifer.svg")),
    ("pen", include_bytes!("icons/pen.svg")),
    ("terminal", include_bytes!("icons/terminal.svg")),
    ("widget", include_bytes!("icons/widget.svg")),
];

thread_local! {
    static ICON_DATA: HashMap<&'static str, SvgData> = ICONS
        .iter()
        .map(|(name, bytes)| (*name, SvgData::new(*bytes).expect("bundled icon")))
        .collect();
}

fn icon_data(name: &str) -> SvgData {
    ICON_DATA.with(|icons| icons[name].clone())
}

fn icon(name: &str, size: f32, color: Color) -> View {
    svg(name, Arc::new(icon_data(name).tinted(color))).size(size, size)
}

/// Ambient loops (title shimmer, working dots, elapsed time) look the same
/// at this rate; entrances, fades and folds run at the display's rate.
const AMBIENT_HZ: f64 = 30.;

/// A page clock that ticks on display frames only while an animation needs
/// it: at full rate until `until`, at `AMBIENT_HZ` while `ambient` loops run,
/// and not at all otherwise. Views read it through `at`, which stops
/// tracking once they settle.
#[derive(Clone)]
struct Anim {
    clock: Signal<f32>,
    origin: Instant,
    until: Rc<Cell<f32>>,
    ambient: Rc<Cell<bool>>,
    running: Rc<Cell<bool>>,
    tasks: Tasks,
    frames: FrameClock,
}

impl Anim {
    fn now(&self) -> f32 {
        self.origin.elapsed().as_secs_f32()
    }
    /// Keep full-rate frames coming for at least `seconds` more.
    fn keep(&self, seconds: f32) {
        self.until.set(self.until.get().max(self.now() + seconds));
        self.run();
    }
    /// Tick at `AMBIENT_HZ` between full-rate animations while `on`.
    fn set_ambient(&self, on: bool) {
        self.ambient.set(on);
        if on {
            self.run();
        }
    }
    fn run(&self) {
        if self.running.replace(true) {
            return;
        }
        let anim = self.clone();
        self.tasks.spawn(async move {
            loop {
                let full = anim.now() < anim.until.get();
                if !full && !anim.ambient.get() {
                    break;
                }
                if full {
                    anim.frames.next().await;
                } else {
                    anim.frames.next().max_rate(AMBIENT_HZ).await;
                }
                anim.clock.set(anim.now());
            }
            anim.running.set(false);
        });
    }
    /// The time, tracked until `end` and a constant afterwards.
    fn at(&self, end: f32) -> f32 {
        let now = self.clock.with_untracked(|now| *now);
        if now >= end { now } else { self.clock.get() }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Run,
    Read,
    Edit,
    Write,
    Search,
    Glob,
    Fetch,
    Web,
    Todo,
    Think,
}

impl Kind {
    fn verb(self) -> &'static str {
        match self {
            Kind::Run => "Run",
            Kind::Read => "Read",
            Kind::Edit => "Edit",
            Kind::Write => "Write",
            Kind::Search => "Search",
            Kind::Glob => "Glob",
            Kind::Fetch => "Fetch",
            Kind::Web => "Web",
            Kind::Todo => "Todo",
            Kind::Think => "Thought process",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Kind::Run => "terminal",
            Kind::Read => "document",
            Kind::Edit => "pen",
            Kind::Write => "document-add",
            Kind::Search => "search",
            Kind::Glob => "folder",
            Kind::Fetch | Kind::Web => "global",
            Kind::Todo => "checklist",
            Kind::Think => "thought",
        }
    }
    /// File tools show the file name in a badge instead of plain detail.
    fn file(self) -> bool {
        matches!(self, Kind::Read | Kind::Edit | Kind::Write)
    }
}

/// One step of the scripted session.
enum Step {
    User(&'static str),
    /// Assistant markdown: `code` and **bold** spans, "\n" line breaks.
    Say(&'static str),
    Think(&'static str),
    Tool {
        kind: Kind,
        detail: &'static str,
        output: &'static [&'static str],
        ms: u64,
        failed: bool,
    },
    /// The turn is over: its groups fold into their summaries.
    EndTurn,
}

const fn tool(kind: Kind, detail: &'static str, output: &'static [&'static str], ms: u64) -> Step {
    Step::Tool {
        kind,
        detail,
        output,
        ms,
        failed: false,
    }
}

const SCRIPT: &[Step] = &[
    Step::User(
        "Long transcripts stutter while a response streams in. Can you find out why and fix it?",
    ),
    Step::Say(
        "I'll look at how the transcript measures rows and where the scroll position comes from before changing anything.",
    ),
    Step::Think(
        "The stutter only shows up in long sessions, so the cost is probably proportional to the transcript length. Two likely suspects: every streamed token re-measures all rows, or the scroll anchor is recomputed from the top on each frame. Start with measurement, since that runs on every chunk.",
    ),
    tool(
        Kind::Search,
        "fn measure_row in crates/ui",
        &[
            "crates/ui/src/list/measure.rs:48: pub fn measure_row(",
            "crates/ui/src/transcript.rs:1871: fn measure_row_height(",
        ],
        420,
    ),
    tool(
        Kind::Read,
        "crates/ui/src/transcript.rs",
        &[
            "1860  fn top_gap_for(prev: &Row, row: &Row) -> Pixels {",
            "1861      match (prev.kind, row.kind) {",
            "1862          (_, RowKind::User) => px(16.),",
            "1863          (RowKind::Markdown, RowKind::Markdown) => px(12.),",
            "1864          _ => px(8.),",
        ],
        380,
    ),
    tool(
        Kind::Read,
        "crates/ui/src/list/virtual_list.rs",
        &[
            "212  pub fn on_content_changed(&mut self, row: usize) {",
            "213      self.invalidate_all();",
            "214      self.schedule_layout();",
            "215  }",
        ],
        340,
    ),
    tool(
        Kind::Glob,
        "crates/ui/src/list/*.rs",
        &["anchor.rs", "measure.rs", "mod.rs", "virtual_list.rs"],
        240,
    ),
    tool(
        Kind::Read,
        "crates/ui/src/list/measure.rs",
        &[
            "48  pub fn measure_row(&mut self, row: usize, width: Pixels) -> Pixels {",
            "49      if let Some(height) = self.heights.get(&row) {",
            "50          return *height;",
            "51      }",
        ],
        360,
    ),
    tool(
        Kind::Search,
        "invalidate_all in crates/ui/src/list",
        &[
            "virtual_list.rs:213:     self.invalidate_all();",
            "virtual_list.rs:388: fn invalidate_all(&mut self) {",
        ],
        300,
    ),
    tool(
        Kind::Read,
        "crates/ui/src/markdown/veil.rs",
        &[
            "1  //! Fades newly streamed text in place.",
            "2  pub struct Veil {",
            "3      chunks: VecDeque<Chunk>,",
        ],
        280,
    ),
    tool(
        Kind::Run,
        "rg -n \"heights.clear()\" crates/ui",
        &[
            "crates/ui/src/list/virtual_list.rs:389:        self.heights.clear();",
            "crates/ui/src/list/anchor.rs:77:        list.heights.clear();",
        ],
        520,
    ),
    Step::Say(
        "Found it. Every streamed chunk calls `invalidate_all()`, which throws away the cached height of **every** row, so the list re-measures the whole transcript on each frame even though only the streaming row changed.",
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/list/virtual_list.rs",
        &[
            "@@ -210,7 +210,7 @@",
            "   pub fn on_content_changed(&mut self, row: usize) {",
            "-      self.invalidate_all();",
            "+      self.invalidate(row);",
            "       self.schedule_layout();",
            "   }",
        ],
        420,
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/list/anchor.rs",
        &[
            "@@ -74,6 +74,7 @@",
            "   fn restore(&mut self, list: &mut VirtualList) {",
            "-      list.heights.clear();",
            "+      let offset = list.offset_of(self.row);",
            "+      list.scroll_to(offset + self.within);",
            "   }",
        ],
        440,
    ),
    tool(
        Kind::Write,
        "crates/ui/src/list/tests/streaming.rs",
        &[
            "+ #[test]",
            "+ fn streaming_measures_only_the_changed_row() {",
            "+     let mut list = VirtualList::with_rows(5_000);",
            "+     for _ in 0..2_000 { list.on_content_changed(4_999); list.layout(); }",
            "+     assert_eq!(list.measured_rows(), 5_000 + 2_000);",
            "+ }",
        ],
        380,
    ),
    tool(
        Kind::Run,
        "cargo check -p ui",
        &[
            "    Checking ui v0.1.0 (crates/ui)",
            "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.21s",
        ],
        900,
    ),
    Step::Tool {
        kind: Kind::Run,
        detail: "cargo test -p ui list::",
        output: &[
            "running 18 tests",
            "test list::tests::anchor::keeps_offset_on_insert ... ok",
            "test list::tests::anchor::keeps_offset_on_resize ... FAILED",
            "test list::tests::measure::caches_heights ... ok",
            "test list::tests::streaming::streaming_measures_only_the_changed_row ... ok",
            "failures:",
            "---- list::tests::anchor::keeps_offset_on_resize stdout ----",
            "assertion `left == right` failed: left: 1204.0, right: 1180.0",
            "test result: FAILED. 17 passed; 1 failed",
        ],
        ms: 1400,
        failed: true,
    },
    tool(
        Kind::Read,
        "crates/ui/src/list/tests/anchor.rs",
        &[
            "61  #[test]",
            "62  fn keeps_offset_on_resize() {",
            "63      // The anchor row grew by 24 px above the viewport.",
            "64      assert_eq!(list.offset(), 1180.);",
        ],
        320,
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/list/tests/anchor.rs",
        &[
            "@@ -61,6 +61,7 @@",
            "   fn keeps_offset_on_resize() {",
            "-      // The anchor row grew by 24 px above the viewport.",
            "-      assert_eq!(list.offset(), 1180.);",
            "+      // Growth above the anchor shifts the offset; the anchor stays put.",
            "+      assert_eq!(list.offset(), 1204.);",
            "+      assert_eq!(list.anchor_screen_y(), 96.);",
        ],
        400,
    ),
    tool(
        Kind::Run,
        "cargo test -p ui list::",
        &[
            "running 18 tests",
            "test list::tests::anchor::keeps_offset_on_insert ... ok",
            "test list::tests::anchor::keeps_offset_on_resize ... ok",
            "test list::tests::measure::caches_heights ... ok",
            "test list::tests::measure::remeasures_on_width_change ... ok",
            "test list::tests::streaming::streaming_measures_only_the_changed_row ... ok",
            "test result: ok. 18 passed; 0 failed; finished in 0.41s",
        ],
        1200,
    ),
    tool(
        Kind::Todo,
        "3/3 done",
        &[
            "✓ Find what re-measures rows while streaming",
            "✓ Invalidate only the changed row",
            "✓ Keep the scroll anchor stable",
        ],
        200,
    ),
    Step::Say(
        "The list now re-measures only the row that changed:\n•  `invalidate(row)` replaces `invalidate_all()` for streamed chunks\n•  the scroll anchor keeps its screen position instead of being recomputed from the top\n•  a new test streams 2,000 chunks into a 5,000-row list and checks that each one measures a single row",
    ),
    Step::EndTurn,
    Step::User("Nice. Do the other lists in the app go through the same path?"),
    Step::Think(
        "Search the whole workspace for invalidate_all and heights.clear, then read each caller to see whether it runs per update or only on resize.",
    ),
    tool(
        Kind::Search,
        "invalidate_all",
        &[
            "crates/ui/src/sidebar/sessions.rs:140",
            "crates/ui/src/picker/file_picker.rs:233",
            "crates/ui/src/changes/pane.rs:512",
            "crates/ui/src/list/virtual_list.rs:388",
        ],
        260,
    ),
    tool(Kind::Read, "crates/ui/src/sidebar/sessions.rs", &[], 180),
    tool(Kind::Read, "crates/ui/src/picker/file_picker.rs", &[], 160),
    tool(Kind::Read, "crates/ui/src/changes/pane.rs", &[], 170),
    tool(Kind::Read, "crates/ui/src/changes/hunk.rs", &[], 150),
    tool(Kind::Read, "crates/ui/src/picker/fuzzy.rs", &[], 140),
    tool(
        Kind::Glob,
        "crates/ui/src/**/list*.rs",
        &[
            "crates/ui/src/list/mod.rs",
            "crates/ui/src/sidebar/list_row.rs",
        ],
        150,
    ),
    tool(Kind::Read, "crates/ui/src/sidebar/list_row.rs", &[], 150),
    tool(
        Kind::Run,
        "rg -c invalidate_all crates",
        &[
            "crates/ui/src/sidebar/sessions.rs:2",
            "crates/ui/src/picker/file_picker.rs:3",
            "crates/ui/src/changes/pane.rs:1",
        ],
        380,
    ),
    tool(
        Kind::Web,
        "virtual list measure cache invalidation",
        &[
            "Result: “Virtualizing variable-height lists” — measure lazily, cache by key, invalidate per item.",
        ],
        520,
    ),
    tool(
        Kind::Fetch,
        "https://docs.rs/gpui/latest/gpui/struct.ListState.html",
        &["ListState::splice(old_range, count): invalidates only the spliced items."],
        480,
    ),
    Step::Say(
        "Three other lists call `invalidate_all()` on every update: the sidebar session list, the file picker and the changes pane. I'll switch all three to per-row invalidation; the file picker can also skip measuring when only the selection moves.",
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/sidebar/sessions.rs",
        &[
            "@@ -138,7 +138,7 @@",
            "-      self.list.invalidate_all();",
            "+      self.list.invalidate(index);",
        ],
        300,
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/picker/file_picker.rs",
        &[
            "@@ -230,9 +230,8 @@",
            "   fn select(&mut self, index: usize) {",
            "-      self.list.invalidate_all();",
            "       self.selected = index;",
            "+      self.list.repaint(index);",
        ],
        320,
    ),
    tool(
        Kind::Edit,
        "crates/ui/src/changes/pane.rs",
        &[
            "@@ -510,7 +510,7 @@",
            "-      self.rows.invalidate_all();",
            "+      self.rows.invalidate_range(hunk.rows());",
        ],
        300,
    ),
    tool(
        Kind::Run,
        "cargo test -p ui",
        &[
            "running 214 tests",
            "test changes::tests::hunk_rows_follow_edits ... ok",
            "test changes::tests::pane_scroll_restores ... ok",
            "test list::tests::anchor::keeps_offset_on_insert ... ok",
            "test list::tests::anchor::keeps_offset_on_resize ... ok",
            "test list::tests::streaming::streaming_measures_only_the_changed_row ... ok",
            "test picker::tests::fuzzy_ranks_prefix_first ... ok",
            "test picker::tests::selection_does_not_remeasure ... ok",
            "test sidebar::tests::sessions_update_one_row ... ok",
            "test transcript::tests::veil_fades_new_chunks ... ok",
            "test transcript::tests::groups_fold_after_turn ... ok",
            "test result: ok. 214 passed; 0 failed; finished in 2.87s",
        ],
        2200,
    ),
    Step::Say(
        "All 214 `ui` tests pass. Streaming into a 5,000-message session now measures one row per frame instead of 5,000, and moving the selection in the file picker no longer measures anything.",
    ),
    Step::EndTurn,
];

/// A list entry compared by identity, so appending to a signal's list only
/// compares pointers.
struct Shared<T>(Rc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SpanStyle {
    Plain,
    Code,
    Bold,
}

#[derive(Clone, PartialEq)]
struct Chunk {
    text: String,
    style: SpanStyle,
    born: f32,
}

struct ToolRow {
    kind: Kind,
    detail: String,
    index: usize,
    born: f32,
    output: Signal<Vec<String>>,
    failed: Signal<bool>,
    open: Signal<Tween>,
}

struct Group {
    rows: Signal<Vec<Shared<ToolRow>>>,
    /// Tools are running: the title shimmers.
    live: Signal<bool>,
    folded: Signal<Tween>,
}

enum Block {
    User { text: &'static str, born: f32 },
    Text { chunks: Signal<Vec<Chunk>> },
    Group(Rc<Group>),
}

/// `page` is the kitchen sink's page scroll, which follows the stream.
pub fn section(cx: &mut Context, page: &Signal<f32>) -> View {
    let generation = cx.state(0_u32);
    let replay = {
        let generation = generation.clone();
        move || {
            generation.update(|g| *g += 1);
        }
    };
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Transcript",
            &[
                "Replay streams a scripted agent session styled after Zeron's transcript.",
                "Tool rows grow in one after another; the group title shimmers while tools run.",
                "New text fades in word by word; groups fold into a summary when a turn ends.",
                "Click a group title to unfold it, and a tool row to see its output.",
            ],
        ))
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(eyebrow("SESSION"))
                .child(pill_button("Replay").on_click(replay)),
        )
        .child({
            let page = page.clone();
            switch(move || generation.get(), move |_, cx| transcript(cx, &page)).w_full()
        })
}

fn transcript(cx: &mut Context, page: &Signal<f32>) -> View {
    let anim = Anim {
        clock: cx.state(0_f32),
        origin: Instant::now(),
        until: Rc::new(Cell::new(0.)),
        ambient: Rc::new(Cell::new(false)),
        running: Rc::new(Cell::new(false)),
        tasks: cx.tasks(),
        frames: cx.frames(),
    };
    let blocks: Signal<Vec<Shared<Block>>> = cx.state(Vec::new());
    let live = cx.state(true);
    let offset = page.clone();
    let started = anim.now();
    cx.tasks().spawn(play(
        anim.clone(),
        cx.runtime(),
        blocks.clone(),
        live.clone(),
        offset.clone(),
    ));
    let body = {
        let (anim, blocks) = (anim.clone(), blocks.clone());
        let keys = blocks.clone();
        keyed(
            move || (0..keys.with(Vec::len)).collect::<Vec<_>>(),
            move |index, _| {
                let block = blocks.with_untracked(|blocks| blocks[index].clone());
                let previous = index
                    .checked_sub(1)
                    .map(|i| blocks.with_untracked(|blocks| matches!(*blocks[i], Block::Group(_))));
                let gap = match (&*block, previous) {
                    (_, None) => 0.,
                    (Block::User { .. }, _) => 16.,
                    (Block::Group(_), _) | (_, Some(true)) => 12.,
                    _ => 8.,
                };
                block_view(&anim, &block).mt(gap)
            },
        )
    };
    column()
        .w_full()
        .max_w(736.)
        .pb(24.)
        .selectable(true)
        .child(body.w_full())
        .child(trailer(&anim, &live, started))
}

fn block_view(anim: &Anim, block: &Block) -> View {
    match block {
        Block::User { text, born } => user_bubble(anim, text, *born),
        Block::Text { chunks } => markdown(anim, chunks),
        Block::Group(group) => tool_group(anim, group),
    }
}

/// 500 ms expo: opacity 0 → 1 while rising 4 px.
fn fade_in(anim: &Anim, born: f32) -> impl Fn() -> Styles + 'static {
    let anim = anim.clone();
    move || {
        let p = ease_out_expo((anim.at(born + 0.5) - born) / 0.5);
        Styles::new().opacity(p).translate(0., 4. * (1. - p))
    }
}

fn user_bubble(anim: &Anim, text_: &'static str, born: f32) -> View {
    column()
        .w_full()
        .items_end()
        .reactive_style(fade_in(anim, born))
        .child(
            text(text_)
                .text_wrap(true)
                .max_w_percent(80.)
                .rounded(16.)
                .px(16.)
                .py(10.)
                .bg(Color(235, 235, 235, 20))
                .text_size(14.)
                .line_height(22.)
                .text_color(TEXT),
        )
}

/// Streamed text under a veil: each new chunk fades in place. One span per
/// chunk keeps run ranges fixed while chunks fade, so a frame between chunk
/// arrivals only recolours the paragraph: no layout, no shaping.
fn markdown(anim: &Anim, chunks: &Signal<Vec<Chunk>>) -> View {
    const FADE: f32 = 0.24;
    let (anim, chunks) = (anim.clone(), chunks.clone());
    rich_text_signal(move || {
        chunks.with(|chunks| {
            let newest = chunks.last().map_or(0., |chunk| chunk.born);
            let now = anim.at(newest + FADE);
            chunks
                .iter()
                .map(|chunk| {
                    let p = ((now - chunk.born) / FADE).clamp(0., 1.);
                    span(chunk.text.clone(), chunk.style, 1. - (1. - p).powf(1.6))
                })
                .collect()
        })
    })
    .w_full()
    .text_wrap(true)
    .text_size(14.)
    .line_height(22.)
    .text_color(TEXT)
}

fn span(text: String, style: SpanStyle, alpha: f32) -> TextSpan {
    let fade = |c: Color| Color(c.0, c.1, c.2, (f32::from(c.3) * alpha).round() as u8);
    let span = text_span(text);
    match style {
        SpanStyle::Plain => span.text_color(fade(TEXT)),
        SpanStyle::Bold => span.text_color(fade(TEXT)).font_weight(600),
        SpanStyle::Code => span
            .text_color(fade(ACCENT))
            .background(fade(Color(124, 134, 255, 31)))
            .font_family(FontFamily::Monospace)
            .text_size(13.),
    }
}

fn summary(rows: &[Shared<ToolRow>]) -> String {
    let count = |kinds: &[Kind]| rows.iter().filter(|row| kinds.contains(&row.kind)).count();
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            one.to_owned()
        } else {
            many.replace('N', &n.to_string())
        }
    };
    let mut parts = Vec::new();
    let mut add = |n: usize, one: &str, many: &str| {
        if n > 0 {
            parts.push(plural(n, one, many));
        }
    };
    add(count(&[Kind::Run]), "ran 1 command", "ran N commands");
    add(
        count(&[Kind::Edit, Kind::Write]),
        "edited 1 file",
        "edited N files",
    );
    add(count(&[Kind::Read]), "read 1 file", "read N files");
    add(
        count(&[Kind::Search, Kind::Glob]),
        "searched once",
        "searched N times",
    );
    add(
        count(&[Kind::Fetch, Kind::Web]),
        "fetched 1 page",
        "fetched N pages",
    );
    add(count(&[Kind::Todo]), "updated todos", "updated todos");
    add(count(&[Kind::Think]), "thought process", "thought N times");
    let failed = rows.iter().filter(|row| row.failed.get()).count();
    add(failed, "1 failed", "N failed");
    let mut text = parts.join(" · ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn tool_group(anim: &Anim, group: &Rc<Group>) -> View {
    let toggle = {
        let (anim, group) = (anim.clone(), group.clone());
        move || {
            let now = anim.now();
            group.folded.update(|fold| {
                let target = if fold.target > 0.5 { 0. } else { 1. };
                *fold = fold.toward(target, now, 0.18, ease_out_expo);
            });
            anim.keep(0.3);
        }
    };
    let chevron = {
        let (anim, group) = (anim.clone(), group.clone());
        svg_signal("toggle", move || {
            let fold = group.folded.get();
            let folded = fold.value(anim.at(fold.at + 0.18), 0.18, ease_out_expo);
            Arc::new(
                icon_data("chevron")
                    .tinted(MUTED)
                    .transformed(Affine::rotation(-FRAC_PI_2 * folded)),
            )
        })
        .size(14., 14.)
    };
    let title = {
        let (anim, group) = (anim.clone(), group.clone());
        rich_text_signal(move || {
            let text = group.rows.with(|rows| summary(rows));
            if !group.live.get() {
                return vec![text_span(text)];
            }
            // Zeron's 3.4 s sweep: a soft peak of `TEXT` crossing `MUTED`.
            let phase = (anim.clock.get() / 3.4).rem_euclid(1.);
            let centre = -0.4 + 1.8 * phase;
            let last = (text.chars().count().max(2) - 1) as f32;
            text.chars()
                .enumerate()
                .map(|(i, ch)| {
                    let glow = smoothstep(1. - (i as f32 / last - centre).abs() / 0.36);
                    text_span(ch.to_string()).text_color(mix(MUTED, TEXT, glow))
                })
                .collect()
        })
    };
    let rows = {
        let (anim, group) = (anim.clone(), group.clone());
        let keys = group.rows.clone();
        keyed(
            move || (0..keys.with(Vec::len)).collect::<Vec<_>>(),
            move |index, _| {
                let row = group.rows.with_untracked(|rows| rows[index].clone());
                tool_row(&anim, &group, row)
            },
        )
    };
    column()
        .w_full()
        .child(
            row()
                .h(26.)
                .gap(6.)
                .pr(4.)
                .items_center()
                .text_size(12.)
                .line_height(18.)
                .text_color(MUTED)
                .hover(|s| s.text_color(TEXT))
                .cursor(Cursor::Pointer)
                .on_click(toggle)
                .child(
                    row()
                        .size(22., 18.)
                        .items_center()
                        .justify_center()
                        .child(chevron),
                )
                .child(title),
        )
        .child(column().w_full().pt(2.).child(rows))
}

fn tool_row(anim: &Anim, group: &Rc<Group>, row_: Shared<ToolRow>) -> View {
    // Zeron's row entrance: height over 360 ms expo; the connector over
    // 480 ms quint, then the branch; content fades and rises with the branch.
    let grown = {
        let (anim, group, row_) = (anim.clone(), group.clone(), row_.clone());
        move || {
            let now = anim.at(row_.born + 0.48);
            let grow = ease_out_expo((now - row_.born) / 0.36);
            let fold = group.folded.get();
            let open = 1. - fold.value(anim.at(fold.at + 0.18), 0.18, ease_out_expo);
            Styles::new().h(32. * grow * open).opacity(open)
        }
    };
    let timeline = {
        let (anim, row_) = (anim.clone(), row_.clone());
        move || ease_out_quint((anim.at(row_.born + 0.48) - row_.born) / 0.48)
    };
    let (line_span, branch_span) = if row_.index == 0 {
        ((0., 0.62), (0.58, 1.))
    } else {
        ((0.45, 0.72), (0.68, 1.))
    };
    let line = {
        let (group, timeline) = (group.clone(), timeline.clone());
        let index = row_.index;
        div().w(1.).bg(ink(0.12)).reactive_style(move || {
            let last = group.rows.with(|rows| rows.len() - 1 == index);
            let p = window(timeline(), line_span.0, line_span.1);
            Styles::new().h(if last { 16. } else { 32. } * p)
        })
    };
    let branch = {
        let timeline = timeline.clone();
        div().h(1.).bg(ink(0.12)).reactive_style(move || {
            Styles::new().w(15. * window(timeline(), branch_span.0, branch_span.1))
        })
    };
    let content_style = {
        let timeline = timeline.clone();
        move || {
            let p = window(timeline(), branch_span.0, branch_span.1);
            Styles::new().opacity(p).translate(0., 4. * (1. - p))
        }
    };
    let tint = {
        let failed = row_.failed.clone();
        move || if failed.get() { DANGER } else { MUTED }
    };
    let icon_view = {
        let (tint, name) = (tint.clone(), row_.kind.icon());
        svg_signal(name, move || Arc::new(icon_data(name).tinted(tint())))
            .size(16., 16.)
            .ml(4.)
            .mt(8.)
    };
    let detail: View = if row_.kind.file() {
        let name = row_
            .detail
            .rsplit('/')
            .next()
            .unwrap_or(&row_.detail)
            .to_owned();
        row()
            .h(22.)
            .rounded(5.)
            .bg(ink(0.06))
            .pl(1.)
            .pr(6.)
            .gap(6.)
            .items_center()
            .child(
                row()
                    .size(20., 20.)
                    .rounded(4.)
                    .items_center()
                    .justify_center()
                    .child(icon("document", 14., MUTED)),
            )
            .child(text(name).truncate())
    } else if row_.kind == Kind::Think {
        div()
    } else {
        text(row_.detail.clone()).truncate().min_w(0.).shrink_0()
    };
    let has_output = row_.kind == Kind::Think || !row_.output.with_untracked(Vec::is_empty);
    let toggle = {
        let (anim, row_) = (anim.clone(), row_.clone());
        move || {
            let now = anim.now();
            row_.open.update(|open| {
                let target = if open.target > 0.5 { 0. } else { 1. };
                *open = open.toward(target, now, OPEN, ease_out_expo);
            });
            anim.keep(0.2);
        }
    };
    let header_style = {
        let tint = tint.clone();
        move || {
            let styles = grown();
            styles.text_color(if tint() == DANGER { DANGER } else { MUTED })
        }
    };
    let header_row = row()
        .w_full()
        .overflow_hidden()
        .items_start()
        .text_size(12.)
        .line_height(18.)
        .hover(|s| s.text_color(TEXT))
        .reactive_style(header_style)
        .when(has_output, |r| r.cursor(Cursor::Pointer))
        .on_click(toggle)
        .child(
            row()
                .w(48.)
                .h(32.)
                .shrink_0()
                .items_start()
                .child(div().w(12.))
                .child(line)
                .child(column().w(15.).pt(16.).child(branch))
                .child(icon_view),
        )
        .child(
            row()
                .grow()
                .min_w(0.)
                .h(32.)
                .ml(8.)
                .gap(8.)
                .items_center()
                .overflow_hidden()
                .reactive_style(content_style)
                .child(text(row_.kind.verb()).shrink_0())
                .child(detail),
        );
    column()
        .w_full()
        .child(header_row)
        .child(output_body(anim, group, &row_))
}

/// A row's output under its fold: 12 px mono (thoughts in the body font),
/// capped at 24 lines, diff lines tinted.
/// The lines a row's output body shows: thoughts pre-wrapped with their
/// newest lines in view, other output capped with a "more" line.
fn shown_lines(think: bool, output: &[String]) -> Vec<(String, Color)> {
    const CAP: usize = 24;
    if think {
        let mut lines = Vec::new();
        for paragraph in output {
            let mut line = String::new();
            for word in paragraph.split(' ') {
                if !line.is_empty() && line.len() + 1 + word.len() > THOUGHT_COLUMNS {
                    lines.push((std::mem::take(&mut line), FAINT));
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            if !line.is_empty() {
                lines.push((line, FAINT));
            }
        }
        return lines.split_off(lines.len().saturating_sub(CAP));
    }
    let mut lines: Vec<_> = output
        .iter()
        .take(CAP)
        .map(|line| {
            let color = match line.as_bytes().first() {
                Some(b'+') => ADDED,
                Some(b'-') => DANGER,
                _ if line.contains("FAILED") => DANGER,
                _ => FAINT,
            };
            (line.clone(), color)
        })
        .collect();
    if output.len() > CAP {
        lines.push((format!("… {} more lines", output.len() - CAP), FAINT));
    }
    lines
}

/// A row's output under its fold: 12 px mono (thoughts in the body font),
/// one line per row so its height is known and opening tweens it smoothly.
fn output_body(anim: &Anim, group: &Rc<Group>, row_: &ToolRow) -> View {
    const LINE: f32 = 18.;
    const PAD: f32 = 6.;
    let think = row_.kind == Kind::Think;
    let lines = {
        let output = row_.output.clone();
        rich_text_signal(move || {
            let lines = output.with(|output| shown_lines(think, output));
            let count = lines.len();
            lines
                .into_iter()
                .enumerate()
                .map(|(i, (line, color))| {
                    let text = if i + 1 < count { line + "\n" } else { line };
                    text_span(text).text_color(color)
                })
                .collect()
        })
        .w_full()
        .text_wrap(false)
        .text_size(12.)
        .line_height(LINE)
        .when(!think, |t| t.font_family(FontFamily::Monospace))
    };
    let (anim, group, open, output) = (
        anim.clone(),
        group.clone(),
        row_.open.clone(),
        row_.output.clone(),
    );
    column()
        .w_full()
        .pl(56.)
        .pr(8.)
        .overflow_hidden()
        .reactive_style(move || {
            let open = open.get();
            let fold = group.folded.get();
            let p = open.value(anim.at(open.at + OPEN), OPEN, ease_out_expo)
                * (1. - fold.value(anim.at(fold.at + 0.18), 0.18, ease_out_expo));
            if p <= 0. {
                return Styles::new().hidden();
            }
            let count = output.with(|output| shown_lines(think, output).len());
            let full = count as f32 * LINE + 2. * PAD;
            Styles::new().flex().h(full * p).opacity(p.powf(0.6))
        })
        .child(column().w_full().py(PAD).child(lines))
}

/// "Thinking…" with a pulse and the elapsed time, under the live turn.
fn trailer(anim: &Anim, live: &Signal<bool>, started: f32) -> View {
    const WORDS: [&str; 6] = [
        "Thinking",
        "Reading",
        "Tracing",
        "Untangling",
        "Measuring",
        "Checking",
    ];
    let dots = (0..3).map(|i| {
        let anim = anim.clone();
        div()
            .size(4., 4.)
            .rounded(2.)
            .bg(TEXT)
            .reactive_style(move || {
                let phase = (anim.clock.get() / 0.9 - i as f32 / 6.).rem_euclid(1.);
                Styles::new().opacity(super::blink(phase))
            })
    });
    let word = {
        let anim = anim.clone();
        text_signal(move || {
            let index = ((anim.clock.get() - started) / 7.).max(0.) as usize % WORDS.len();
            format!("{}…", WORDS[index])
        })
        .text_color(MUTED)
    };
    let elapsed = {
        let anim = anim.clone();
        text_signal(move || {
            let seconds = (anim.clock.get() - started).max(0.) as u32;
            if seconds >= 60 {
                format!("{}m {}s", seconds / 60, seconds % 60)
            } else {
                format!("{seconds}s")
            }
        })
        .text_color(FAINT)
    };
    let live = live.clone();
    row()
        .pt(16.)
        .gap(8.)
        .items_center()
        .text_size(12.)
        .reactive_style(move || {
            if live.get() {
                Styles::new().flex()
            } else {
                Styles::new().hidden()
            }
        })
        .child(row().gap(3.).items_center().children(dots))
        .child(word)
        .child(elapsed)
}

/// Split markdown into word-sized chunks: `code` and **bold** keep their style.
fn tokenize(source: &str) -> Vec<(String, SpanStyle)> {
    let mut pieces = Vec::new();
    let mut style = SpanStyle::Plain;
    let mut rest = source;
    while !rest.is_empty() {
        let (marker, next) = match style {
            SpanStyle::Plain => {
                let code = rest.find('`');
                let bold = rest.find("**");
                match (code, bold) {
                    (Some(c), Some(b)) if b < c => (b, Some((SpanStyle::Bold, 2))),
                    (Some(c), _) => (c, Some((SpanStyle::Code, 1))),
                    (None, Some(b)) => (b, Some((SpanStyle::Bold, 2))),
                    (None, None) => (rest.len(), None),
                }
            }
            SpanStyle::Code => (
                rest.find('`').unwrap_or(rest.len()),
                Some((SpanStyle::Plain, 1)),
            ),
            SpanStyle::Bold => (
                rest.find("**").unwrap_or(rest.len()),
                Some((SpanStyle::Plain, 2)),
            ),
        };
        let (run, after) = rest.split_at(marker);
        let mut word = String::new();
        for ch in run.chars() {
            word.push(ch);
            if ch == ' ' || ch == '\n' {
                pieces.push((std::mem::take(&mut word), style));
            }
        }
        if !word.is_empty() {
            pieces.push((word, style));
        }
        match next {
            Some((to, skip)) if !after.is_empty() => {
                style = to;
                rest = &after[skip..];
            }
            _ => rest = "",
        }
    }
    pieces
}

/// Keeps the newest content in view unless the reader scrolled up.
struct Follow {
    offset: Signal<f32>,
    bottom: Cell<f32>,
}

impl Follow {
    /// Content shrank on purpose (a fold): its lower offset is not the reader.
    fn repin(&self) {
        self.bottom.set(0.);
        self.offset.set(1e9);
    }
    fn update(&self) {
        // The offset is clamped after layout, so the last jump reads back as
        // the bottom. Scrolling well above it pauses following.
        let current = self.offset.with_untracked(|offset| *offset);
        if current + 48. < self.bottom.get() {
            return;
        }
        self.bottom.set(self.bottom.get().max(current));
        self.offset.set(1e9);
    }
}

async fn play(
    anim: Anim,
    runtime: Runtime,
    blocks: Signal<Vec<Shared<Block>>>,
    live: Signal<bool>,
    offset: Signal<f32>,
) {
    let follow = Follow {
        offset,
        bottom: Cell::new(0.),
    };
    let push = |block: Block| {
        blocks.update(|blocks| blocks.push(Shared(Rc::new(block))));
    };
    // Waiting costs no frames; each event keeps full rate for its own
    // animation, and a live turn keeps the ambient loops at `AMBIENT_HZ`.
    let wait = |ms: u64| sleep(Duration::from_millis(ms));
    let mut group: Option<Rc<Group>> = None;
    let mut turn: Vec<Rc<Group>> = Vec::new();
    wait(350).await;
    for step in SCRIPT {
        match step {
            Step::User(text) => {
                live.set(true);
                anim.set_ambient(true);
                push(Block::User {
                    text,
                    born: anim.now(),
                });
                anim.keep(0.55);
                follow.update();
                wait(700).await;
            }
            Step::Say(source) => {
                if let Some(group) = group.take() {
                    group.live.set(false);
                }
                let chunks = runtime.signal(Vec::<Chunk>::new());
                push(Block::Text {
                    chunks: chunks.clone(),
                });
                for (text, style) in tokenize(source) {
                    chunks.update(|chunks| {
                        chunks.push(Chunk {
                            text,
                            style,
                            born: anim.now(),
                        })
                    });
                    anim.keep(0.3);
                    follow.update();
                    wait(28).await;
                }
                wait(420).await;
            }
            Step::Think(_) | Step::Tool { .. } => {
                let current = group.get_or_insert_with(|| {
                    let created = Rc::new(Group {
                        rows: runtime.signal(Vec::new()),
                        live: runtime.signal(true),
                        folded: runtime.signal(Tween::settled(0.)),
                    });
                    push(Block::Group(created.clone()));
                    turn.push(created.clone());
                    created
                });
                current.live.set(true);
                let (kind, detail, output, ms, failed) = match step {
                    Step::Think(thought) => (Kind::Think, *thought, &[][..], 0, false),
                    Step::Tool {
                        kind,
                        detail,
                        output,
                        ms,
                        failed,
                    } => (*kind, *detail, *output, *ms, *failed),
                    _ => unreachable!(),
                };
                let index = current.rows.with_untracked(Vec::len);
                let row = Shared(Rc::new(ToolRow {
                    kind,
                    detail: if kind == Kind::Think {
                        String::new()
                    } else {
                        detail.to_owned()
                    },
                    index,
                    born: anim.now() + if index == 0 { 0.09 } else { 0. },
                    output: runtime.signal(Vec::new()),
                    failed: runtime.signal(false),
                    open: runtime.signal(Tween::settled(0.)),
                }));
                current.rows.update(|rows| rows.push(row.clone()));
                anim.keep(0.6);
                follow.update();
                let now = anim.now();
                let open = |row: &ToolRow, open: bool| {
                    row.open.update(|tween| {
                        *tween = tween.toward(if open { 1. } else { 0. }, now, OPEN, ease_out_expo)
                    });
                };
                match kind {
                    Kind::Think => {
                        // Thoughts stream open, then fold away when done.
                        open(&row, true);
                        let mut thought = String::new();
                        for (word, _) in tokenize(detail) {
                            thought.push_str(&word);
                            row.output.set(vec![thought.clone()]);
                            follow.update();
                            wait(22).await;
                        }
                        wait(500).await;
                        let now = anim.now();
                        row.open
                            .update(|tween| *tween = tween.toward(0., now, OPEN, ease_out_expo));
                        anim.keep(0.3);
                        follow.repin();
                    }
                    Kind::Run => {
                        // Commands stream their output while they run.
                        open(&row, true);
                        wait(ms / 3).await;
                        let pace = (ms * 2 / 3) / output.len().max(1) as u64;
                        for line in output.iter() {
                            row.output.update(|lines| lines.push((*line).to_owned()));
                            follow.update();
                            wait(pace).await;
                        }
                        row.failed.set(failed);
                        wait(250).await;
                        let now = anim.now();
                        row.open
                            .update(|tween| *tween = tween.toward(0., now, OPEN, ease_out_expo));
                        anim.keep(0.3);
                        follow.repin();
                    }
                    _ => {
                        wait(ms).await;
                        row.output
                            .set(output.iter().map(|line| (*line).to_owned()).collect());
                        row.failed.set(failed);
                    }
                }
            }
            Step::EndTurn => {
                if let Some(group) = group.take() {
                    group.live.set(false);
                }
                live.set(false);
                anim.set_ambient(false);
                wait(900).await;
                let now = anim.now();
                for group in turn.drain(..) {
                    group
                        .folded
                        .update(|fold| *fold = fold.toward(1., now, 0.18, ease_out_expo));
                }
                anim.keep(0.4);
                follow.repin();
                wait(900).await;
            }
        }
    }
}
