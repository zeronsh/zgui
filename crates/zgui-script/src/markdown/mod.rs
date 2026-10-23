//! Markdown as scripts draw it: blocks of styled runs, parsed in Rust (from
//! Rust with [`parse`] and [`Incremental`], from scripts with
//! `ui.markdown`), so a script only lays out what it is given.
//!
//! A block is a table with `t`:
//! - `p` (`s`: runs), `h` (`l`: level, `s`), `code` (`lang`, `text`, and
//!   what [`Options::code`] adds), `ul` / `ol` (`start`, `items`: a list of
//!   block lists), `quote` (`b`), `hr`, `table` (`head`: cells, `rows`:
//!   rows of cells; a cell is runs), `img` (`src`, `alt`).
//!
//! A list item's own text (a tight list's, never a paragraph) is a `p`
//! with `tight`, which has no space under it.
//!
//! A run is `{ x = text, b?, i?, c? (code), s? (strike), a? (link),
//! f? (a repository file, with [`Options::paths`]) }`. A link whose address
//! is still streaming has `a` = [`PENDING_LINK_URL`]: style it as a link,
//! but it goes nowhere yet. Raw HTML is dropped.
//!
//! Streaming text parses incrementally ([`Incremental`]): an append parses
//! again only from the last top-level blocks, and the blocks before them
//! are the same values as before, so what reads them (a script's
//! `ui.each` over them, a reconciling [`crate::Shared`]) skips them at a
//! pointer's glance.
mod mend;

pub use mend::PENDING_LINK_URL;

use crate::Value;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use std::rc::Rc;

/// How text is read.
#[derive(Clone, Default)]
pub struct Options {
    /// Single newlines break lines (as people's messages show them), rather
    /// than joining them as CommonMark does.
    pub breaks: bool,
    /// Repository paths in text (`src/main.rs:12`), in code spans that are a
    /// path whole, and relative links that read as one, are files (`f`).
    pub paths: bool,
    /// More fields for a code block, from its language and text (coloured
    /// lines, say): `(name, value)` pairs.
    pub code: Option<CodeFields>,
}

/// A code block's added fields, from its language and text.
pub type CodeFields = Rc<dyn Fn(&str, &str) -> Vec<(&'static str, Value)>>;

/// Markdown parsed into blocks.
pub fn parse(text: &str, options: &Options) -> Vec<Value> {
    parse_at(text, 0, options)
        .into_iter()
        .flat_map(|top| top.blocks)
        .collect()
}

/// Streaming markdown: [`set_text`](Self::set_text) with the text so far
/// parses only what an append can have changed (the last two top-level
/// blocks; a link reference definition anywhere means the whole text), and
/// [`blocks`](Self::blocks) shares the blocks before that with the last
/// answer. While a marker hangs at the end (`**bold`, a link's address half
/// there), the blocks show it closed, so the text does not restyle and
/// reflow when the closer arrives.
#[derive(Default)]
pub struct Incremental {
    options: Options,
    source: String,
    tops: Vec<Top>,
    /// The last top-level block's blocks as shown, mended; `None` when
    /// nothing hangs.
    display_tail: Option<Vec<Value>>,
    /// Link reference definitions act at a distance: whole parses only.
    full_only: bool,
    /// Bytes parsed by the last update: an append's work is its tail's.
    last_parse_bytes: usize,
}

/// A top-level element: where it starts, and the blocks it made (one, or
/// none for raw HTML).
#[derive(Clone)]
struct Top {
    start: usize,
    blocks: Vec<Value>,
}

impl Incremental {
    /// A parser for markdown rendered with `options`, with no text yet.
    pub fn new(options: Options) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }
    /// The text parsed so far.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Bytes parsed by the last [`set_text`](Self::set_text) (0: the same
    /// text).
    pub fn last_parse_bytes(&self) -> usize {
        self.last_parse_bytes
    }
    /// The text so far: an append takes the incremental path, anything else
    /// parses afresh.
    pub fn set_text(&mut self, text: &str) {
        if text.len() >= self.source.len() && text.starts_with(self.source.as_str()) {
            let delta = &text[self.source.len()..];
            if delta.is_empty() {
                self.last_parse_bytes = 0;
                return;
            }
            self.append(delta);
        } else {
            self.reset(text);
        }
    }
    fn reset(&mut self, text: &str) {
        self.source = text.to_string();
        self.full_only = has_link_defs(text);
        self.tops = parse_at(text, 0, &self.options);
        self.last_parse_bytes = text.len();
        self.remend();
    }
    fn append(&mut self, delta: &str) {
        // The delta may finish a line begun earlier: look for definitions
        // from that line's start.
        let scan_from = self.source.rfind('\n').map_or(0, |i| i + 1);
        self.source.push_str(delta);
        if !self.full_only && has_link_defs(&self.source[scan_from..]) {
            self.full_only = true;
        }
        if self.full_only {
            self.tops = parse_at(&self.source, 0, &self.options);
            self.last_parse_bytes = self.source.len();
            self.remend();
            return;
        }
        // From the start of the second-to-last top-level block, at its
        // line's start (indentation keeps its meaning): a trailing paragraph
        // like `3` can become `3.` and join the list before it, but a block's
        // separation from the one before is settled by its own first bytes,
        // so two blocks back is always enough.
        let boundary = match self.tops.len() {
            0 | 1 => 0,
            n => self.tops[n - 2].start,
        };
        let boundary = self.source[..boundary].rfind('\n').map_or(0, |i| i + 1);
        let mut tail = parse_at(&self.source[boundary..], boundary, &self.options);
        self.last_parse_bytes = self.source.len() - boundary;
        let at = self.tops.partition_point(|t| t.start < boundary);
        // A block parsed again the same keeps its value.
        for (new, old) in tail.iter_mut().zip(&self.tops[at..]) {
            if new.start == old.start && new.blocks == old.blocks {
                new.blocks = old.blocks.clone();
            }
        }
        self.tops.truncate(at);
        self.tops.extend(tail);
        self.remend();
    }
    /// Mend hanging inline markers in the last top-level block, the only
    /// place they can hang (a blank line settles a block).
    fn remend(&mut self) {
        self.display_tail = None;
        let Some(last) = self.tops.last() else { return };
        // Code shows an open fence as it is; rules and tables have no
        // inline tail to mend.
        let kind = last.blocks.first().and_then(|b| b.field("t")).cloned();
        if matches!(&kind, Some(Value::Str(t)) if matches!(&**t, "code" | "hr" | "table")) {
            return;
        }
        let start = last.start;
        let Some(mended) = mend::close_hanging(&self.source[start..]) else {
            return;
        };
        self.last_parse_bytes += mended.len();
        let tail = parse_at(&mended, start, &self.options);
        self.display_tail = Some(tail.into_iter().flat_map(|t| t.blocks).collect());
    }
    /// The blocks to show: those before the last top-level block are the
    /// same values as the last time they were asked for, while they last.
    pub fn blocks(&self) -> Vec<Value> {
        let stable = self.tops.len().saturating_sub(1);
        let mut out: Vec<Value> = self.tops[..stable]
            .iter()
            .flat_map(|t| t.blocks.iter().cloned())
            .collect();
        match &self.display_tail {
            Some(tail) => out.extend(tail.iter().cloned()),
            None => out.extend(
                self.tops[stable..]
                    .iter()
                    .flat_map(|t| t.blocks.iter().cloned()),
            ),
        }
        out
    }
}

/// `ui.markdown(text, { key?, breaks?, paths? })`: the blocks. With `key`
/// (a message's id), text that grows under the same key parses
/// incrementally, sharing its earlier blocks with the last answer; without,
/// the same text answers with the same blocks.
pub(crate) fn script(
    lua: &mlua::Lua,
    (text, opts): (mlua::LuaString, Option<mlua::Table>),
) -> mlua::Result<Value> {
    let text = text.to_str()?;
    let flag = |name: &str| -> mlua::Result<bool> {
        Ok(match &opts {
            Some(t) => t.get::<Option<bool>>(name)?.unwrap_or(false),
            None => false,
        })
    };
    let options = Options {
        breaks: flag("breaks")?,
        paths: flag("paths")?,
        code: None,
    };
    let key: Option<String> = match &opts {
        Some(t) => t.get("key")?,
        None => None,
    };
    // (The VM registers its cache when it starts: `install`.)
    let Some(parsed) = lua.app_data_ref::<Parsed>() else {
        return Ok(Value::List(parse(&text, &options).into()));
    };
    let flags = (options.breaks, options.paths);
    Ok(match key {
        Some(key) => {
            let mut streams = parsed.streams.borrow_mut();
            let fresh = !matches!(streams.get(&key), Some((f, _)) if *f == flags);
            if fresh {
                streams.insert(key.clone(), (flags, Incremental::new(options)));
            }
            let (_, stream) = streams.get_mut(&key).expect("inserted above");
            stream.set_text(&text);
            let blocks = Value::List(stream.blocks().into());
            touch(&mut parsed.stream_order.borrow_mut(), key, &mut streams);
            blocks
        }
        None => {
            let mut texts = parsed.texts.borrow_mut();
            let at = (text.to_string(), flags);
            if let Some(blocks) = texts.get(&at) {
                return Ok(blocks.clone());
            }
            let blocks = Value::List(parse(&text, &options).into());
            if texts.len() >= KEPT {
                texts.clear();
            }
            texts.insert(at, blocks.clone());
            blocks
        }
    })
}

/// Give a new VM its parsed markdown.
pub(crate) fn install(lua: &mlua::Lua) {
    lua.set_app_data(Parsed::default());
}

/// How many texts and streams a script's `ui.markdown` keeps.
const KEPT: usize = 256;

/// `breaks` and `paths`, as a script asked for them.
type Flags = (bool, bool);
type Texts = std::collections::HashMap<(String, Flags), Value>;
type Streams = std::collections::HashMap<String, (Flags, Incremental)>;

/// A VM's parsed markdown.
#[derive(Default)]
struct Parsed {
    texts: std::cell::RefCell<Texts>,
    streams: std::cell::RefCell<Streams>,
    /// Streams by last use, oldest first.
    stream_order: std::cell::RefCell<std::collections::VecDeque<String>>,
}
/// `key` was just used: last in the order, and the oldest streams go past
/// `KEPT`.
fn touch(order: &mut std::collections::VecDeque<String>, key: String, streams: &mut Streams) {
    if order.back() != Some(&key) {
        order.retain(|k| *k != key);
        order.push_back(key);
    }
    while order.len() > KEPT {
        if let Some(old) = order.pop_front() {
            streams.remove(&old);
        }
    }
}

/// Link reference definition lines (`[label]: destination`, up to three
/// spaces in), conservatively.
fn has_link_defs(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        line.len() - trimmed.len() <= 3 && trimmed.starts_with('[') && trimmed.contains("]:")
    })
}

/// `text` (which starts at `offset` in the whole) parsed into its top-level
/// elements.
fn parse_at(text: &str, offset: usize, options: &Options) -> Vec<Top> {
    let mut flags = pulldown_cmark::Options::empty();
    flags.insert(pulldown_cmark::Options::ENABLE_TABLES);
    flags.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    flags.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let mut builder = Builder {
        frames: vec![Frame::Blocks(Vec::new())],
        runs: Vec::new(),
        styles: Vec::new(),
        links: Vec::new(),
        code: None,
        heading: None,
        image: None,
        options,
    };
    let mut tops = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (event, range) in Parser::new_ext(text, flags).into_offset_iter() {
        let opens = matches!(event, Event::Start(_));
        let closes = matches!(event, Event::End(_));
        if depth == 0 {
            start = range.start;
        }
        builder.event(event);
        if opens {
            depth += 1;
        } else if closes {
            depth = depth.saturating_sub(1);
        }
        // A top-level element is done: what it made is its blocks.
        if depth == 0 {
            let blocks = match builder.frames.first_mut() {
                Some(Frame::Blocks(blocks)) => std::mem::take(blocks),
                _ => Vec::new(),
            };
            tops.push(Top {
                start: offset + start,
                blocks,
            });
        }
    }
    tops
}

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

#[derive(Default)]
struct Run {
    text: String,
    style: Style,
    code: bool,
    link: Option<String>,
    file: Option<String>,
}

/// Where a link goes: a page, or (with `paths`) a file in the repository.
#[derive(Clone, PartialEq)]
enum Link {
    Url(String),
    File(String),
}

fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A path segment's characters (`[\w@.-]`).
fn segment_char(c: char) -> bool {
    word(c) || c == '@' || c == '.' || c == '-'
}

/// `s` without a trailing `:line` or `:line:column`.
fn strip_position(s: &str) -> &str {
    let mut out = s;
    for _ in 0..2 {
        if let Some(i) = out.rfind(':') {
            let digits = &out[i + 1..];
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                out = &out[..i];
                continue;
            }
        }
        break;
    }
    out
}

/// Whether `s` is, whole, a repository path:
/// `(?:[\w@.-]+/)+[\w@.-]+\.[a-z\d]{1,8}(?::\d+(?::\d+)?)?`.
fn is_path(s: &str) -> bool {
    let path = strip_position(s);
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() < 2
        || segments
            .iter()
            .any(|seg| seg.is_empty() || !seg.chars().all(segment_char))
    {
        return false;
    }
    let last = segments[segments.len() - 1];
    match last.rfind('.') {
        Some(dot) if dot > 0 => {
            let ext = &last[dot + 1..];
            (1..=8).contains(&ext.len())
                && ext
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        }
        _ => false,
    }
}

/// `text` split into plain pieces and repository paths (the paths standing
/// alone: after a space, `(` or `[`, before a space or `,.;)]`).
fn split_paths(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut plain = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        let after_boundary = i == 0 || matches!(chars[i - 1].1, ' ' | '\t' | '\n' | '(' | '[');
        if !after_boundary || !(segment_char(c)) {
            i += 1;
            continue;
        }
        // The longest stretch that may be a path, then back off to the
        // longest prefix that is one and ends where a path may end.
        let mut j = i;
        while j < chars.len()
            && (segment_char(chars[j].1) || chars[j].1 == '/' || chars[j].1 == ':')
        {
            j += 1;
        }
        let mut found = None;
        let mut k = j;
        while k > i {
            let end = if k < chars.len() {
                chars[k].0
            } else {
                text.len()
            };
            let ends_well = k == chars.len()
                || matches!(chars[k].1, ' ' | '\t' | '\n' | ',' | '.' | ';' | ')' | ']');
            if ends_well && is_path(&text[start..end]) {
                found = Some((k, end));
                break;
            }
            k -= 1;
        }
        match found {
            Some((k, end)) => {
                if plain < start {
                    out.push((&text[plain..start], false));
                }
                out.push((&text[start..end], true));
                plain = end;
                i = k;
            }
            None => i = j.max(i + 1),
        }
    }
    if plain < text.len() {
        out.push((&text[plain..], false));
    }
    out
}

/// A link's destination: with `paths`, a relative href without a query
/// that reads as a path (`src/a.rs`, `README.md:12`) is a file.
fn link_of(href: &str, paths: bool) -> Link {
    if !paths {
        return Link::Url(href.to_string());
    }
    let lower = href.to_ascii_lowercase();
    let scheme = href.find("://").is_some_and(|i| {
        i > 0
            && href[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    if scheme
        || href == PENDING_LINK_URL
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
        || lower.starts_with("javascript:")
        || href.starts_with('/')
        || href.contains('?')
    {
        return Link::Url(href.to_string());
    }
    let path = href.split('#').next().unwrap_or("");
    let first_ok = path
        .chars()
        .next()
        .is_some_and(|c| !matches!(c, '/' | '`' | ':' | '\\') && !c.is_whitespace());
    let rest_ok = path
        .chars()
        .all(|c| !matches!(c, '`' | '\\') && !c.is_whitespace());
    let stripped = strip_position(path);
    if first_ok && rest_ok && !stripped.contains(':') {
        Link::File(stripped.to_string())
    } else {
        Link::Url(href.to_string())
    }
}

fn map(fields: Vec<(&str, Value)>) -> Value {
    let mut fields: Vec<(Rc<str>, Value)> =
        fields.into_iter().map(|(k, v)| (Rc::from(k), v)).collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(fields.into())
}

fn runs_value(runs: &[Run]) -> Value {
    Value::List(
        runs.iter()
            .filter(|r| !r.text.is_empty())
            .map(|r| {
                let mut f = vec![("x", Value::Str(r.text.as_str().into()))];
                if r.style.bold {
                    f.push(("b", Value::Bool(true)));
                }
                if r.style.italic {
                    f.push(("i", Value::Bool(true)));
                }
                if r.style.strike {
                    f.push(("s", Value::Bool(true)));
                }
                if r.code {
                    f.push(("c", Value::Bool(true)));
                }
                if let Some(link) = &r.link {
                    f.push(("a", Value::Str(link.as_str().into())));
                }
                if let Some(file) = &r.file {
                    f.push(("f", Value::Str(file.as_str().into())));
                }
                map(f)
            })
            .collect(),
    )
}

/// What is being built at each level of nesting.
enum Frame {
    /// Blocks of a container (the document, a list item, a quote).
    Blocks(Vec<Value>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<Value>,
    },
    Table {
        head: Vec<Value>,
        rows: Vec<Value>,
        row: Vec<Value>,
        in_head: bool,
    },
}

struct Builder<'a> {
    frames: Vec<Frame>,
    /// Inline runs of the paragraph, heading or cell being read.
    runs: Vec<Run>,
    styles: Vec<Style>,
    links: Vec<Link>,
    code: Option<(String, String)>,
    heading: Option<u8>,
    image: Option<(String, String)>,
    options: &'a Options,
}

impl Builder<'_> {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_text(&mut self, text: &str, code: bool) {
        if let Some(image) = &mut self.image {
            image.1.push_str(text);
            return;
        }
        let paths = self.options.paths;
        match self.links.last().cloned() {
            Some(Link::Url(url)) => self.push_run(text, code, Some(url), None),
            Some(Link::File(path)) => self.push_run(text, false, None, Some(path)),
            // Text outside links and code: paths in it are files.
            None if !code && paths => {
                for (piece, path) in split_paths(text) {
                    let file = path.then(|| strip_position(piece).to_string());
                    self.push_run(piece, false, None, file);
                }
            }
            // Code that is a path, whole, is a file too.
            None if code && paths && is_path(text) => {
                self.push_run(text, false, None, Some(strip_position(text).to_string()))
            }
            None => self.push_run(text, code, None, None),
        }
    }

    fn push_run(&mut self, text: &str, code: bool, link: Option<String>, file: Option<String>) {
        let style = self.style();
        if let Some(last) = self.runs.last_mut() {
            let same = !code
                && !last.code
                && file.is_none()
                && last.file.is_none()
                && last.link == link
                && {
                    let s = last.style;
                    s.bold == style.bold && s.italic == style.italic && s.strike == style.strike
                };
            if same {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(Run {
            text: text.to_string(),
            style,
            code,
            link,
            file,
        });
    }

    fn take_runs(&mut self) -> Value {
        let mut runs = std::mem::take(&mut self.runs);
        // Trailing line breaks inside a block are not shown.
        if let Some(last) = runs.last_mut() {
            while last.text.ends_with('\n') {
                last.text.pop();
            }
        }
        runs_value(&runs)
    }

    /// A tight item's text so far, before a block inside the item (a
    /// nested list): it is the item's first paragraph.
    fn flush_tight(&mut self) {
        if self.runs.is_empty() || self.code.is_some() || self.heading.is_some() {
            return;
        }
        let runs = self.take_runs();
        if let Some(Frame::Blocks(blocks)) = self.frames.last_mut() {
            blocks.push(map(vec![
                ("t", "p".into()),
                ("s", runs),
                ("tight", Value::Bool(true)),
            ]));
        }
    }

    fn block(&mut self, block: Value) {
        // Loose content directly in a list (tight lists have no
        // paragraphs) belongs to the item being read: dropped here.
        if let Some(Frame::Blocks(blocks)) = self.frames.last_mut() {
            blocks.push(block);
        }
    }

    fn event(&mut self, event: Event) {
        if let Event::Start(Tag::List(_) | Tag::CodeBlock(_) | Tag::BlockQuote(_) | Tag::Table(_))
        | Event::Rule = &event
        {
            self.flush_tight();
        }
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    self.heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    })
                }
                Tag::BlockQuote(_) => self.frames.push(Frame::Blocks(Vec::new())),
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(lang) => {
                            lang.split_whitespace().next().unwrap_or("").to_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    self.code = Some((lang, String::new()));
                }
                Tag::List(start) => self.frames.push(Frame::List {
                    ordered: start.is_some(),
                    start: start.unwrap_or(1),
                    items: Vec::new(),
                }),
                Tag::Item => self.frames.push(Frame::Blocks(Vec::new())),
                Tag::Table(_) => self.frames.push(Frame::Table {
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                    in_head: false,
                }),
                Tag::TableHead => {
                    if let Some(Frame::Table { in_head, .. }) = self.frames.last_mut() {
                        *in_head = true;
                    }
                }
                Tag::Emphasis => {
                    let mut s = self.style();
                    s.italic = true;
                    self.styles.push(s);
                }
                Tag::Strong => {
                    let mut s = self.style();
                    s.bold = true;
                    self.styles.push(s);
                }
                Tag::Strikethrough => {
                    let mut s = self.style();
                    s.strike = true;
                    self.styles.push(s);
                }
                Tag::Link { dest_url, .. } => {
                    let link = link_of(&dest_url, self.options.paths);
                    self.links.push(link)
                }
                Tag::Image { dest_url, .. } => {
                    self.image = Some((dest_url.to_string(), String::new()))
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    let runs = self.take_runs();
                    self.block(map(vec![("t", "p".into()), ("s", runs)]));
                }
                TagEnd::Heading(_) => {
                    let runs = self.take_runs();
                    let level = self.heading.take().unwrap_or(1);
                    self.block(map(vec![
                        ("t", "h".into()),
                        ("l", Value::Num(level as f64)),
                        ("s", runs),
                    ]));
                }
                TagEnd::BlockQuote(_) => {
                    if let Some(Frame::Blocks(blocks)) = self.frames.pop() {
                        self.block(map(vec![
                            ("t", "quote".into()),
                            ("b", Value::List(blocks.into())),
                        ]));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, mut text)) = self.code.take() {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        let mut fields = vec![
                            ("t", "code".into()),
                            ("lang", lang.as_str().into()),
                            ("text", text.as_str().into()),
                        ];
                        if let Some(code) = &self.options.code {
                            fields.extend(code(&lang, &text));
                        }
                        self.block(map(fields));
                    }
                }
                TagEnd::Item => {
                    // A tight item's text never became a paragraph.
                    self.flush_tight();
                    if let Some(Frame::Blocks(blocks)) = self.frames.pop()
                        && let Some(Frame::List { items, .. }) = self.frames.last_mut()
                    {
                        items.push(Value::List(blocks.into()));
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List {
                        ordered,
                        start,
                        items,
                    }) = self.frames.pop()
                    {
                        self.block(map(vec![
                            ("t", if ordered { "ol" } else { "ul" }.into()),
                            ("start", Value::Num(start as f64)),
                            ("items", Value::List(items.into())),
                        ]));
                    }
                }
                TagEnd::TableCell => {
                    let runs = self.take_runs();
                    if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                        row.push(runs);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(Frame::Table {
                        head, row, in_head, ..
                    }) = self.frames.last_mut()
                    {
                        *head = std::mem::take(row);
                        *in_head = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(Frame::Table { rows, row, .. }) = self.frames.last_mut() {
                        rows.push(Value::List(std::mem::take(row).into()));
                    }
                }
                TagEnd::Table => {
                    if let Some(Frame::Table { head, rows, .. }) = self.frames.pop() {
                        self.block(map(vec![
                            ("t", "table".into()),
                            ("head", Value::List(head.into())),
                            ("rows", Value::List(rows.into())),
                        ]));
                    }
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    self.styles.pop();
                }
                TagEnd::Link => {
                    self.links.pop();
                }
                TagEnd::Image => {
                    if let Some((src, alt)) = self.image.take() {
                        // An image alone in its paragraph is a block; inline,
                        // its alt text stands in.
                        if self.runs.is_empty() {
                            self.block(map(vec![
                                ("t", "img".into()),
                                ("src", src.as_str().into()),
                                ("alt", alt.as_str().into()),
                            ]));
                        } else {
                            self.push_text(&alt, false);
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => match &mut self.code {
                Some((_, code)) => code.push_str(&text),
                None => self.push_text(&text, false),
            },
            Event::Code(text) => self.push_text(&text, true),
            Event::InlineMath(text) | Event::DisplayMath(text) => self.push_text(&text, true),
            Event::SoftBreak => {
                let breaks = self.options.breaks;
                self.push_text(if breaks { "\n" } else { " " }, false)
            }
            Event::HardBreak => self.push_text("\n", false),
            Event::Rule => self.block(map(vec![("t", "hr".into())])),
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::TaskListMarker(done) => self.push_text(if done { "☑ " } else { "☐ " }, false),
            Event::FootnoteReference(name) => self.push_text(&format!("[{name}]"), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(blocks: &[Value]) -> Vec<String> {
        blocks
            .iter()
            .map(|b| match b.field("t") {
                Some(Value::Str(t)) => t.to_string(),
                _ => "?".into(),
            })
            .collect()
    }
    fn plain() -> Options {
        Options::default()
    }
    fn paths() -> Options {
        Options {
            paths: true,
            ..Options::default()
        }
    }
    fn same(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    #[test]
    fn reads_blocks_and_runs() {
        let blocks = parse(
            "Comet is a **multi-device** tool.\n\n- one\n- two `x`\n\n```rust\nfn a() {}\n```",
            &plain(),
        );
        assert_eq!(kinds(&blocks), ["p", "ul", "code"]);
        let runs = blocks[0].field("s").unwrap().list();
        assert_eq!(runs.len(), 3);
        assert!(runs[1].field("b").is_some());
        let items = blocks[1].field("items").unwrap().list();
        assert_eq!(items.len(), 2);
        let second = items[1].list()[0].field("s").unwrap().list();
        assert!(second[1].field("c").is_some(), "inline code");
        assert_eq!(
            blocks[2].field("lang").map(|v| v.to_json()),
            Some("\"rust\"".into())
        );
    }

    #[test]
    fn breaks_keep_line_breaks() {
        let blocks = parse(
            "one\ntwo",
            &Options {
                breaks: true,
                ..plain()
            },
        );
        let runs = blocks[0].field("s").unwrap().list();
        assert_eq!(
            runs[0].field("x").map(|v| v.to_json()),
            Some("\"one\\ntwo\"".into())
        );
    }

    #[test]
    fn raw_html_is_dropped() {
        let blocks = parse(
            "before <b>bold</b> after\n\n<div>\nhidden\n</div>\n\nlast",
            &plain(),
        );
        let text: Vec<String> = blocks
            .iter()
            .flat_map(|b| b.field("s").map(|s| s.list().to_vec()).unwrap_or_default())
            .filter_map(|r| r.field("x").map(|x| x.to_json()))
            .collect();
        assert_eq!(text, ["\"before bold after\"", "\"last\""]);
    }

    #[test]
    fn repository_paths_are_files_with_paths() {
        let text = "See src/main.rs:12, `apps/notes/ui/theme.luau` and [the guide](docs/guide.md) or [site](https://a.b/c.md).";
        let blocks = parse(text, &paths());
        let runs = blocks[0].field("s").unwrap().list();
        let files: Vec<String> = runs
            .iter()
            .filter_map(|r| r.field("f").map(|f| f.to_json()))
            .collect();
        assert_eq!(
            files,
            [
                "\"src/main.rs\"",
                "\"apps/notes/ui/theme.luau\"",
                "\"docs/guide.md\""
            ]
        );
        assert!(
            runs.iter().all(|r| r.field("c").is_none()),
            "a path in code is a file, not code"
        );
        assert!(
            runs.iter().any(|r| r.field("a").is_some()),
            "a web link stays a link"
        );
        assert!(
            split_paths("v1.2 and e.g. or a/b")
                .iter()
                .all(|(_, path)| !path),
            "no extension, no path"
        );
        // Without `paths`, nothing is a file.
        let blocks = parse(text, &plain());
        let runs = blocks[0].field("s").unwrap().list();
        assert!(runs.iter().all(|r| r.field("f").is_none()));
    }

    #[test]
    fn tight_items_keep_their_text_before_a_nested_list() {
        let blocks = parse("- one\n  - two\n- three", &plain());
        let items = blocks[0].field("items").unwrap().list();
        let first = items[0].list();
        assert_eq!(kinds(first), ["p", "ul"]);
        assert!(first[0].field("tight").is_some());
    }

    #[test]
    fn code_blocks_take_the_options_fields() {
        let options = Options {
            code: Some(Rc::new(|lang: &str, _text: &str| {
                if lang == "rust" {
                    vec![("lines", Value::from("coloured"))]
                } else {
                    Vec::new()
                }
            })),
            ..plain()
        };
        let blocks = parse("```rust\nfn main() {}\n```\n\n```\nplain\n```", &options);
        assert!(blocks[0].field("lines").is_some());
        assert!(blocks[1].field("lines").is_none());
    }

    /// Streams `text` in chunks of `step` bytes (on char boundaries).
    fn stream(text: &str, step: usize, options: &Options) -> Incremental {
        let mut parser = Incremental::new(options.clone());
        let mut end = 0;
        while end < text.len() {
            end = (end + step).min(text.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            parser.set_text(&text[..end]);
        }
        parser
    }

    const CORPUS: &str = "# Title\n\nIntro with **bold** and `code` and a [link](https://x.dev).\n\n- one\n- two\n  - nested\n- three\n\n1. first\n2. second\n\n3\n\n> quoted\n> more\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nLast paragraph, src/main.rs:3.\n";

    #[test]
    fn streaming_ends_where_a_whole_parse_does() {
        for step in [1, 2, 3, 7, 16, 64] {
            for options in [plain(), paths()] {
                let parser = stream(CORPUS, step, &options);
                assert_eq!(
                    parser.blocks(),
                    parse(CORPUS, &options),
                    "streamed {step} bytes at a time"
                );
            }
        }
    }

    #[test]
    fn appends_parse_only_the_tail_and_share_the_rest() {
        let mut parser = Incremental::new(plain());
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("Paragraph {i} with some words.\n\n"));
        }
        parser.set_text(&text);
        let before = parser.blocks();
        text.push_str("A new one, streaming");
        parser.set_text(&text);
        assert!(
            parser.last_parse_bytes() < 200,
            "only the tail: {} bytes",
            parser.last_parse_bytes()
        );
        let after = parser.blocks();
        assert_eq!(after.len(), before.len() + 1);
        assert!(
            before[..before.len() - 1]
                .iter()
                .zip(&after)
                .all(|(a, b)| same(a, b)),
            "the blocks before the tail are the same values"
        );
    }

    #[test]
    fn hanging_markers_show_closed_while_streaming() {
        let mut parser = Incremental::new(plain());
        parser.set_text("Some **bold");
        let blocks = parser.blocks();
        let runs = blocks[0].field("s").unwrap().list();
        assert!(
            runs.iter()
                .any(|r| r.field("b").is_some() && r.field("x") == Some(&Value::from("bold"))),
            "bold before its closer"
        );
        parser.set_text("See [docs](https://x.dev/lo");
        let blocks = parser.blocks();
        let runs = blocks[0].field("s").unwrap().list();
        assert_eq!(
            runs.last().and_then(|r| r.field("a")),
            Some(&Value::from(PENDING_LINK_URL)),
            "a link going nowhere yet"
        );
    }
}
