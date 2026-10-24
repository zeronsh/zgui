//! zgui's own best-practice lint for scripts (`zgui lint app.luau`): patterns
//! that type check and run, but cost far more than they look or do not do
//! what they look like. Each finding names the cheaper way to say the same
//! thing. It reads the source only (no luau-lsp, no running the script), so
//! it is fast enough to run on every save and over every example in CI.
//!
//! `-- zgui-lint: allow(rule)` on the line of a finding, or the line above
//! it, accepts a deliberate exception (`allow(rule-a, rule-b)` for several).
//!
//! Rules (each is also in [`RULES`]):
//! - `untracked-get`: `state:get()` while an element is built. The interface
//!   is built once, untracked, so the element shows the value it had then.
//! - `state-in-builder`: `ui.state` in a component, `each` row or `list` row.
//!   Its key is the script's, so every instance shares one state.
//! - `timer-interval`: `ui.every` faster than a display refresh.
//! - `tick-tween`: an `anime.animate` in a `ui.every` callback that runs for
//!   half the interval or more with no `frameRate`: the node lays out and
//!   paints on most frames, forever.
//! - `key-reads-state`: an `each` key function reading state. Keys are kept
//!   per item value, so a key that also depends on state goes stale.
//! - `per-frame-state`: an animation callback (`onUpdate`, `onRender`,
//!   `onBeforeUpdate`) that writes state with no `frameRate` cap, which runs
//!   Luau and restyles every reader on every display frame.
//! - `update-copy`: `state:update(fn)` doing what `push`, `insert`, `remove`
//!   or `set_key` do without copying the whole value through Luau.

use crate::check::{CheckError, Diagnostic, Report, Severity, scripts};
use std::path::{Path, PathBuf};

/// Every rule's name and a one-line summary.
pub const RULES: &[(&str, &str)] = &[
    (
        "untracked-get",
        "state:get() while an element is built is read once",
    ),
    (
        "state-in-builder",
        "ui.state in a component or row is shared by every instance",
    ),
    (
        "timer-interval",
        "ui.every faster than the display refreshes",
    ),
    (
        "tick-tween",
        "a tween re-fired by ui.every that runs most of each tick",
    ),
    (
        "key-reads-state",
        "an each key function reading state goes stale",
    ),
    (
        "per-frame-state",
        "an uncapped animation callback that writes state every frame",
    ),
    (
        "update-copy",
        "state:update(fn) copying the whole value for one change",
    ),
    (
        "computed-sort",
        "a computed that sorts a whole list again each time it runs",
    ),
];

/// `ui.*` calls that do not build interface (`ui.state`, `ui.set`,
/// `ui.effect`, a file read in a handler…); the rest do, and values given to
/// them are read at build time. Every function of `ui` in `zgui.d.luau` is
/// one or the other (`every_ui_function_is_classified`).
const NOT_ELEMENTS: &[&str] = &[
    "on_open",
    "state",
    "shared",
    "persist",
    "use_state",
    "computed",
    "sorted",
    "effect",
    "batch",
    "every",
    "after",
    "shortcut_text",
    "set",
    "toggle",
    "add",
    "action",
    "invoke",
    "call",
    "ref",
    "path",
    "linear",
    "sort",
    "number",
    "toast",
    "dismiss_toast",
    "shader",
    "bitmap",
    "raw",
    "tween",
    "spring",
    "loop",
    "web",
    "term",
    "command",
    "on_mount",
    "null",
    "fetch",
    "variants",
    "component",
    "on_cleanup",
    "now",
    "object",
    "call_sync",
    "task",
    "open_files",
    "save_file",
    "read_file",
    "write_file",
];

/// Lint every `.luau` script at `paths` (files, or directories searched).
pub fn lint(paths: &[PathBuf]) -> Result<Report, CheckError> {
    let mut report = Report::default();
    for path in paths {
        let files = scripts(path);
        if files.is_empty() {
            return Err(CheckError::NoScripts(path.clone()));
        }
        for file in files {
            let source = std::fs::read_to_string(&file).map_err(CheckError::Io)?;
            report.diagnostics.extend(lint_source(&file, &source));
            report.files += 1;
        }
    }
    Ok(report)
}

/// Lint one script's source; `file` names it in the findings.
pub fn lint_source(file: &Path, source: &str) -> Vec<Diagnostic> {
    let (tokens, comments) = tokenize(source);
    let mut linter = Linter {
        file,
        tokens: &tokens,
        found: Vec::new(),
    };
    linter.run();
    let allowed = |line: u32, rule: &str| {
        comments.iter().any(|(at, text)| {
            (*at == line || *at + 1 == line)
                && text
                    .split_once("zgui-lint:")
                    .and_then(|(_, rest)| rest.trim().strip_prefix("allow("))
                    .and_then(|rest| rest.split_once(')'))
                    .is_some_and(|(names, _)| names.split(',').any(|name| name.trim() == rule))
        })
    };
    let mut found = linter.found;
    found.retain(|d| !allowed(d.line, &d.code));
    found.sort_by_key(|d| (d.line, d.column));
    found.dedup();
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Name,
    Number,
    Str,
    Punct,
}
#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    text: String,
    line: u32,
    column: u32,
}
impl Token {
    fn is(&self, text: &str) -> bool {
        self.kind != Kind::Str && self.text == text
    }
}

/// Luau source as tokens, and its comments by line. Strings (long and
/// interpolated ones too) are single tokens; this lint does not look inside.
fn tokenize(source: &str) -> (Vec<Token>, Vec<(u32, String)>) {
    let chars: Vec<char> = source.chars().collect();
    let (mut tokens, mut comments) = (Vec::new(), Vec::new());
    let (mut i, mut line, mut column) = (0, 1u32, 1u32);
    // Advance past `n` characters, keeping the position.
    let advance = |i: &mut usize, line: &mut u32, column: &mut u32, n: usize| {
        for _ in 0..n {
            if chars.get(*i) == Some(&'\n') {
                *line += 1;
                *column = 1;
            } else {
                *column += 1;
            }
            *i += 1;
        }
    };
    // The level of a long bracket opening at `at` (`[[` 0, `[==[` 2).
    let long_open = |at: usize| -> Option<usize> {
        if chars.get(at) != Some(&'[') {
            return None;
        }
        let level = chars[at + 1..].iter().take_while(|c| **c == '=').count();
        (chars.get(at + 1 + level) == Some(&'[')).then_some(level)
    };
    let long_len = |at: usize, level: usize| -> usize {
        let close: Vec<char> = std::iter::once(']')
            .chain(std::iter::repeat_n('=', level))
            .chain(std::iter::once(']'))
            .collect();
        let body = at + level + 2;
        (body..chars.len())
            .find(|&j| chars[j..].starts_with(&close))
            .map_or(chars.len() - at, |j| j + close.len() - at)
    };
    while i < chars.len() {
        let c = chars[i];
        let (start_line, start_column) = (line, column);
        if c.is_whitespace() {
            advance(&mut i, &mut line, &mut column, 1);
            continue;
        }
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            let len = match long_open(i + 2) {
                Some(level) => 2 + long_len(i + 2, level),
                None => chars[i..].iter().take_while(|c| **c != '\n').count(),
            };
            comments.push((start_line, chars[i..i + len].iter().collect()));
            advance(&mut i, &mut line, &mut column, len);
            continue;
        }
        let (kind, len) = if let Some(level) = long_open(i) {
            (Kind::Str, long_len(i, level))
        } else if c == '"' || c == '\'' || c == '`' {
            let mut j = i + 1;
            let mut depth = 0;
            while j < chars.len() {
                match chars[j] {
                    '\\' => j += 1,
                    '{' if c == '`' => depth += 1,
                    '}' if c == '`' && depth > 0 => depth -= 1,
                    '\n' if c != '`' => break,
                    q if q == c && depth == 0 => break,
                    _ => {}
                }
                j += 1;
            }
            (Kind::Str, (j + 1).min(chars.len()) - i)
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let hex = chars.get(i + 1).is_some_and(|c| matches!(c, 'x' | 'X'));
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric()
                    || chars[j] == '_'
                    || chars[j] == '.'
                    || (matches!(chars[j], '+' | '-') && !hex && matches!(chars[j - 1], 'e' | 'E')))
            {
                j += 1;
            }
            (Kind::Number, j - i)
        } else if c.is_alphabetic() || c == '_' {
            let len = chars[i..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || **c == '_')
                .count();
            (Kind::Name, len)
        } else {
            const LONG: &[&str] = &[
                "...", "..=", "//=", "==", "~=", "<=", ">=", "..", "::", "->", "+=", "-=", "*=",
                "/=", "%=", "^=", "//",
            ];
            let len = LONG
                .iter()
                .find(|op| {
                    op.chars()
                        .enumerate()
                        .all(|(k, o)| chars.get(i + k) == Some(&o))
                })
                .map_or(1, |op| op.len());
            (Kind::Punct, len)
        };
        tokens.push(Token {
            kind,
            text: chars[i..i + len].iter().collect(),
            line: start_line,
            column: start_column,
        });
        advance(&mut i, &mut line, &mut column, len);
    }
    (tokens, comments)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Paren,
    Brace,
    Bracket,
    Function,
    Block,
}
/// An open bracket or block, as the linter walks into it.
#[derive(Debug)]
struct Frame {
    open: Open,
    /// The token that opened it.
    at: usize,
    /// For a call's `(` or `{`: the name called (`ui.each`, `count:update`).
    callee: Option<String>,
    /// For a function or table given as a table field: the field's name.
    key: Option<String>,
    /// For a function: its parameters.
    params: Vec<String>,
}

const KEYWORDS: &[&str] = &[
    "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
/// State methods that write.
const WRITES: &[&str] = &[
    "set", "set_key", "patch", "update", "insert", "push", "remove", "toggle", "add",
];

struct Linter<'a> {
    file: &'a Path,
    tokens: &'a [Token],
    found: Vec<Diagnostic>,
}
impl Linter<'_> {
    fn report(&mut self, at: usize, rule: &str, message: String) {
        let token = &self.tokens[at];
        self.found.push(Diagnostic {
            file: self.file.to_path_buf(),
            line: token.line,
            column: token.column,
            severity: Severity::Warning,
            code: rule.into(),
            message,
        });
    }
    fn is(&self, at: usize, text: &str) -> bool {
        self.tokens.get(at).is_some_and(|t| t.is(text))
    }
    fn name(&self, at: usize) -> Option<&str> {
        self.tokens
            .get(at)
            .filter(|t| t.kind == Kind::Name && !KEYWORDS.contains(&t.text.as_str()))
            .map(|t| t.text.as_str())
    }
    /// The dotted name ending just before `at` (`ui.each`, `count:update`).
    fn callee(&self, at: usize) -> Option<String> {
        let mut parts = vec![self.name(at.checked_sub(1)?)?.to_string()];
        let mut j = at - 1;
        while j >= 2
            && (self.is(j - 1, ".") || self.is(j - 1, ":"))
            && let Some(name) = self.name(j - 2)
        {
            parts.insert(0, format!("{name}{}", self.tokens[j - 1].text));
            j -= 2;
        }
        Some(parts.concat())
    }
    /// A number literal at `at` (`1_000` too).
    fn number(&self, at: usize) -> Option<f64> {
        self.tokens
            .get(at)
            .filter(|t| t.kind == Kind::Number)
            .and_then(|t| t.text.replace('_', "").parse().ok())
    }
    /// The table field a value starting at `at` is given to (`key = value`).
    fn field(&self, at: usize) -> Option<String> {
        (at >= 2 && self.is(at - 1, "=") && !self.is(at - 2, "]"))
            .then(|| self.name(at - 2))
            .flatten()
            .filter(|_| at < 3 || !(self.is(at - 3, ".") || self.is(at - 3, "local")))
            .map(str::to_string)
    }
    /// Whether an `if` at `at` starts an expression (`x = if a then b else
    /// c`), which has no `end`, rather than a statement.
    fn expression_if(&self, at: usize) -> bool {
        let Some(previous) = at.checked_sub(1).map(|j| &self.tokens[j]) else {
            return false;
        };
        match previous.kind {
            Kind::Punct => !matches!(previous.text.as_str(), ")" | "]" | "}" | ";"),
            Kind::Name => matches!(
                previous.text.as_str(),
                "return" | "and" | "or" | "not" | "in"
            ),
            _ => false,
        }
    }
    /// The index of the bracket that closes the one opened at `at`.
    fn closing(&self, at: usize) -> usize {
        let mut depth = 0;
        for (j, token) in self.tokens.iter().enumerate().skip(at) {
            if token.kind != Kind::Punct {
                continue;
            }
            match token.text.as_str() {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" => {
                    depth -= 1;
                    if depth == 0 {
                        return j;
                    }
                }
                _ => {}
            }
        }
        self.tokens.len()
    }

    fn run(&mut self) {
        let mut stack: Vec<Frame> = Vec::new();
        for at in 0..self.tokens.len() {
            let token = &self.tokens[at];
            if token.kind == Kind::Str || token.kind == Kind::Number {
                continue;
            }
            match token.text.as_str() {
                "(" | "{" | "[" => {
                    let open = match token.text.as_str() {
                        "(" => Open::Paren,
                        "{" => Open::Brace,
                        _ => Open::Bracket,
                    };
                    let callee = (open != Open::Bracket)
                        .then(|| self.callee(at))
                        .flatten()
                        .filter(|_| !self.is(at.wrapping_sub(2), "function"));
                    stack.push(Frame {
                        open,
                        at,
                        callee,
                        key: self.field(at),
                        params: Vec::new(),
                    });
                }
                ")" | "}" | "]" => {
                    let want = match token.text.as_str() {
                        ")" => Open::Paren,
                        "}" => Open::Brace,
                        _ => Open::Bracket,
                    };
                    if let Some(index) = stack.iter().rposition(|f| f.open == want) {
                        stack.truncate(index);
                    }
                }
                "function" if token.kind == Kind::Name => {
                    let open = (at..self.tokens.len())
                        .find(|&j| self.is(j, "("))
                        .unwrap_or(self.tokens.len());
                    let close = self.closing(open);
                    let params = (open + 1..close)
                        .filter_map(|j| self.name(j).filter(|_| !self.is(j - 1, ":")))
                        .map(str::to_string)
                        .collect();
                    stack.push(Frame {
                        open: Open::Function,
                        at,
                        callee: None,
                        key: self.field(at),
                        params,
                    });
                }
                "do" | "repeat" if token.kind == Kind::Name => stack.push(Frame {
                    open: Open::Block,
                    at,
                    callee: None,
                    key: None,
                    params: Vec::new(),
                }),
                "if" if token.kind == Kind::Name && !self.expression_if(at) => stack.push(Frame {
                    open: Open::Block,
                    at,
                    callee: None,
                    key: None,
                    params: Vec::new(),
                }),
                "end" | "until" if token.kind == Kind::Name => {
                    if let Some(index) = stack
                        .iter()
                        .rposition(|f| matches!(f.open, Open::Function | Open::Block))
                    {
                        let frame = stack.remove(index);
                        stack.truncate(index);
                        if frame.open == Open::Function {
                            self.function_rules(&frame, stack.last(), at);
                        }
                    }
                }
                _ => self.token_rules(at, &stack),
            }
        }
    }

    /// Rules about a call or field at `at`, given the frames around it.
    fn token_rules(&mut self, at: usize, stack: &[Frame]) {
        let function = stack.iter().rposition(|f| f.open == Open::Function);
        let inside = |from: Option<usize>| &stack[from.map_or(0, |f| f + 1)..];
        // `state:get()` built into an element, with no function between.
        if self.is(at, ":") && self.is(at + 1, "get") && self.is(at + 2, "(") {
            for frame in inside(function).iter().rev() {
                let Some(callee) = &frame.callee else {
                    continue;
                };
                if is_initial_value(callee) {
                    break;
                }
                if is_element(callee) {
                    let state = self.name(at.wrapping_sub(1)).unwrap_or("state").to_string();
                    self.report(
                        at - 1,
                        "untracked-get",
                        format!(
                            "`{state}:get()` is read once, while `{callee}` is built, so it \
                             shows the value of that moment and never updates. Give the state \
                             itself (`{state}`), a text template (\"{{{state}}}\" for a \
                             `ui.state` key) or a function (`function() return {state}:get() \
                             end`), which reruns when it changes."
                        ),
                    );
                    break;
                }
            }
        }
        // `ui.state(...)` inside a component, `each` row or `list` row.
        // A key built from the instance (`\`file:{pane}\``) is deliberate.
        let literal_key = self.tokens.get(at + 4).is_some_and(|t| {
            t.kind == Kind::Str && !t.text.starts_with('`') && !self.is(at + 5, "..")
        });
        if self.is(at, "ui")
            && self.is(at + 1, ".")
            && (self.is(at + 2, "state") || self.is(at + 2, "persist"))
            && self.is(at + 3, "(")
            && literal_key
        {
            let builder = stack.iter().enumerate().rev().find_map(|(index, frame)| {
                (frame.open == Open::Function).then_some(())?;
                let parent = index.checked_sub(1).map(|p| &stack[p])?;
                match parent.callee.as_deref() {
                    Some(c @ ("ui.component" | "ui.each" | "ui.presence")) => Some(c.to_string()),
                    _ if frame.key.as_deref() == Some("row") => Some("ui.list".to_string()),
                    _ => None,
                }
            });
            if let Some(builder) = builder {
                let what = &self.tokens[at + 2].text;
                self.report(
                    at,
                    "state-in-builder",
                    format!(
                        "`ui.{what}` in a `{builder}` builder is keyed to the script, so every \
                         instance shares one state (and the key collides with any other use). \
                         Use `ui.use_state(initial)` for state of each instance."
                    ),
                );
            }
        }
        // A `key` function of `ui.each` reading state.
        if self.is(at, ":")
            && (self.is(at + 1, "get") || self.is(at + 1, "peek"))
            && self.is(at + 2, "(")
            && let Some(f) = function
            && f >= 2
            && stack[f].key.as_deref() == Some("key")
            && stack[f - 1].open == Open::Brace
            && stack[f - 2].callee.as_deref() == Some("ui.each")
        {
            let state = self.name(at.wrapping_sub(1)).unwrap_or("state").to_string();
            self.report(
                at - 1,
                "key-reads-state",
                format!(
                    "`each` keeps each item's key while the item stays the same value, so a \
                     key reading `{state}` does not follow it: rows keep stale keys. Key on \
                     the item alone (`item.id`), and read `{state}` in the row instead."
                ),
            );
        }
        // `anime.animate` in a `ui.every` callback, running most of the tick.
        if self.is(at, "anime")
            && self.is(at + 1, ".")
            && self.is(at + 2, "animate")
            && self.is(at + 3, "(")
            && let Some(f) = function
            && f > 0
            && stack[f - 1].callee.as_deref() == Some("ui.every")
            && let Some(interval) = self.number(stack[f - 1].at + 1)
        {
            let close = self.closing(at + 3);
            // A field of the parameters table, at the call's own depth.
            let field = |name: &str| {
                let mut depth = 0;
                (at + 4..close).find_map(|j| {
                    if self.tokens[j].kind == Kind::Punct {
                        match self.tokens[j].text.as_str() {
                            "(" | "{" | "[" => depth += 1,
                            ")" | "}" | "]" => depth -= 1,
                            _ => {}
                        }
                    }
                    (depth == 1 && self.is(j, name) && self.is(j + 1, "=")).then_some(j + 2)
                })
            };
            let capped = field("frameRate").or_else(|| field("frame_rate")).is_some();
            let duration = field("duration").map_or(Some(1000.), |j| self.number(j));
            if let Some(duration) = duration
                && !capped
                && duration * 2. >= interval
            {
                self.report(
                    at,
                    "tick-tween",
                    format!(
                        "a {duration} ms tween fired every {interval} ms keeps its node \
                         animating {}% of the time: it lays out and paints on most frames, \
                         for as long as the timer runs. Cap it with `frameRate` (the rate the \
                         eye needs, 30 or less), or make it much shorter than the tick.",
                        (duration.min(interval) / interval * 100.).round()
                    ),
                );
            }
        }
        // `ui.every(ms)` faster than a display refresh.
        if self.is(at, "ui")
            && self.is(at + 1, ".")
            && self.is(at + 2, "every")
            && self.is(at + 3, "(")
            && let Some(ms) = self
                .tokens
                .get(at + 4)
                .filter(|t| t.kind == Kind::Number && self.is(at + 5, ","))
                .and_then(|t| t.text.replace('_', "").parse::<f64>().ok())
            && ms < 16.
        {
            self.report(
                at,
                "timer-interval",
                format!(
                    "`ui.every({ms})` calls the script up to {} times a second, faster than a \
                     display refreshes, so most calls change nothing anyone sees. Animate with \
                     `anime` or `ui.tween` (run in Rust, paced by the display), or tick at the \
                     rate the value really changes.",
                    (1000. / ms.max(1.)).round()
                ),
            );
        }
    }

    /// Rules about a function's body, once its `end` (at `end`) is reached.
    fn function_rules(&mut self, frame: &Frame, parent: Option<&Frame>, end: usize) {
        let body = frame.at..end;
        // Calls of a state writer in the body: (index of the method name).
        let writes: Vec<usize> = body
            .clone()
            .filter(|&j| {
                self.is(j, ":")
                    && self
                        .tokens
                        .get(j + 1)
                        .is_some_and(|t| WRITES.contains(&t.text.as_str()))
                    && self.is(j + 2, "(")
            })
            .map(|j| j + 1)
            .collect();
        let capped = parent.is_some_and(|table| {
            table.open == Open::Brace
                && (table.at..self.closing(table.at)).any(|j| {
                    (self.is(j, "frameRate") || self.is(j, "frame_rate")) && self.is(j + 1, "=")
                })
        });
        if let Some(key @ ("onUpdate" | "onRender" | "onBeforeUpdate")) = frame.key.as_deref()
            && !writes.is_empty()
            && !capped
        {
            self.report(
                frame.at,
                "per-frame-state",
                format!(
                    "`{key}` writes state on every display frame of the animation: each frame \
                     runs the script, then restyles everything that reads the state. Animate \
                     the property itself (`anime.animate(\"#id\", {{ x = … }})`), give the \
                     state as the target (`anime.animate(state, {{ value = … }})`), which the \
                     engine writes without calling the script, or cap it with `frameRate` at \
                     the rate the value is read (a clock: 1)."
                ),
            );
        }
        // `state:update(function(v) table.insert(v, x) end)` and the like.
        let updates = parent
            .and_then(|p| p.callee.as_deref())
            .is_some_and(|c| c.ends_with(":update"));
        if updates
            && let Some(value) = frame.params.first()
            && !body.clone().any(|j| {
                ["for", "while", "repeat"]
                    .iter()
                    .any(|loop_| self.tokens[j].is(loop_))
            })
        {
            let method = body.clone().find_map(|j| {
                let table = self.is(j, "table") && self.is(j + 1, ".") && self.is(j + 3, "(");
                if table && self.name(j + 4) == Some(value) {
                    match self.tokens[j + 2].text.as_str() {
                        "insert" => Some(":push(v) or :insert(i, v)"),
                        "remove" => Some(":remove(i)"),
                        _ => None,
                    }
                } else if self.name(j) == Some(value)
                    && (self.is(j + 1, ".") || self.is(j + 1, "["))
                    && !self.is(j.wrapping_sub(1), ".")
                {
                    // Past the whole path (`v.a[3].b`) to an `=`.
                    let mut k = j + 1;
                    loop {
                        if self.is(k, ".") && self.name(k + 1).is_some() {
                            k += 2;
                        } else if self.is(k, "[") {
                            k = self.closing(k) + 1;
                        } else {
                            break;
                        }
                    }
                    self.is(k, "=")
                        .then_some(":set_key(path, v) or :patch({ … })")
                } else {
                    None
                }
            });
            if let Some(method) = method {
                self.report(
                    frame.at,
                    "update-copy",
                    format!(
                        "`update` copies the whole value into Luau and back for this change; \
                         `{method}` copies only what it changes."
                    ),
                );
            }
        }
        // `ui.computed(function() … ui.sort(…) … end)`: every change to
        // what it reads sorts the whole list again.
        let computes = parent
            .and_then(|p| p.callee.as_deref())
            .is_some_and(|c| c == "ui.computed");
        if computes
            && let Some(at) = body.clone().find(|&j| {
                self.is(j, "ui")
                    && self.is(j + 1, ".")
                    && self.is(j + 2, "sort")
                    && self.is(j + 3, "(")
            })
        {
            self.report(
                at,
                "computed-sort",
                "this computed sorts its whole list each time what it reads changes (a row \
                 written, a tick): `ui.sorted(list, { by = …, filter = … })` keeps a list \
                 state's order as it changes, re-placing only the rows written."
                    .into(),
            );
        }
    }
}

/// A call whose argument is a starting value, read once by design.
fn is_initial_value(callee: &str) -> bool {
    matches!(
        callee,
        "ui.state" | "ui.use_state" | "ui.shared" | "ui.persist" | "ui.set" | "ui.add" | "ui.ref"
    )
}
/// A call that builds interface: `ui.text`, `ui.each`, or a component
/// (a capitalized name, `Card { … }`).
fn is_element(callee: &str) -> bool {
    match callee.strip_prefix("ui.") {
        Some(name) => !name.contains(['.', ':']) && !NOT_ELEMENTS.contains(&name),
        None => {
            !callee.contains(['.', ':']) && callee.chars().next().is_some_and(char::is_uppercase)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Functions of `ui` that return no `Node` yet whose arguments are read
    /// while the interface is built: a window's options, a contribution, a
    /// document parsed into blocks.
    const BUILDS: &[&str] = &["window", "contribute", "markdown"];

    /// A function added to `ui` is either an element or listed in
    /// `NOT_ELEMENTS`, so `untracked-get` neither flags a file written in a
    /// click handler nor misses a new element.
    #[test]
    fn every_ui_function_is_classified() {
        let types = include_str!("../types/zgui.d.luau");
        let start = types.find("\ndeclare ui: {\n").expect("ui's declaration");
        let body = &types[start..];
        let body = &body[..body.find("\n}").unwrap()];
        let mut unclassified = Vec::new();
        for line in body.lines() {
            let Some((name, ty)) = line.strip_prefix('\t').and_then(|l| l.split_once(": ")) else {
                continue;
            };
            if !name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                || !(ty.starts_with('(') || ty.starts_with('<'))
            {
                continue; // A table (`ui.clipboard`) or a value (`ui.null`).
            }
            // Not one returning a function that returns one (`ui.component`).
            let element =
                (ty.ends_with("-> Node,") || ty.ends_with("-> Span,")) && !ty.contains(") -> (");
            let listed = NOT_ELEMENTS.contains(&name);
            assert!(
                !(element && listed),
                "ui.{name} returns a Node but is listed in NOT_ELEMENTS"
            );
            if !element && !listed && !BUILDS.contains(&name) {
                unclassified.push(name);
            }
        }
        assert!(
            unclassified.is_empty(),
            "add these to NOT_ELEMENTS (or to BUILDS if their arguments are read while \
             the interface is built): {unclassified:?}"
        );
    }

    #[test]
    fn file_and_task_calls_in_handlers_are_not_elements() {
        let found = rules(
            "local body = ui.state(\"body\", \"\")\n\
             ui.on_cleanup(function() ui.write_file(\"notes.txt\", body:get()) end)\n\
             return ui.button {\n\
             \ton_click = function() ui.task(save, body:get()) end,\n\
             \t\"Save\",\n\
             }\n",
        );
        assert_eq!(found, Vec::new());
    }

    fn rules(source: &str) -> Vec<(u32, String)> {
        lint_source(Path::new("app.luau"), source)
            .into_iter()
            .map(|d| (d.line, d.code))
            .collect()
    }
    fn one(line: u32, rule: &str) -> Vec<(u32, String)> {
        vec![(line, rule.to_string())]
    }

    #[test]
    fn a_computed_that_sorts_a_whole_list_is_reported() {
        let source = r##"
local rows = ui.state("rows", {})
local view = ui.computed(function()
    return ui.sort(table.clone(rows:get()), "price")
end)
local sorted = ui.sort({ 3, 1, 2 }, function(n) return n end) -- fine: once
local live = ui.sorted(rows, { by = "price" })
"##;
        assert_eq!(rules(source), one(4, "computed-sort"));
    }

    #[test]
    fn a_get_built_into_an_element_is_reported() {
        let source = r##"
local count = ui.state("count", 0)
local open = ui.state("open", false)
local initial = count:get() -- fine: not built into anything
return ui.column {
    ui.text { "Count: " .. count:get() },
    ui.show(open:get(), ui.text "open"),
    ui.text { function() return "Count: " .. count:get() end },
    ui.button { on_click = function() count:set(count:get() + 1) end, "+" },
    ui.text { "Count: {count}", color = string.format("#%06x", count:get()) },
    ui.div { hover = { bg = open:get() and "#fff" or "#000" } },
    ui.text { ui.use_state(count:get()) },
}
"##;
        assert_eq!(
            rules(source),
            [
                (6, "untracked-get".to_string()),
                (7, "untracked-get".to_string()),
                (10, "untracked-get".to_string()),
                (11, "untracked-get".to_string()),
            ]
        );
    }

    #[test]
    fn rows_and_components_build_untracked_too() {
        let source = r##"
local selected = ui.state("selected", 0)
local Card = ui.component(function(props)
    local open = ui.use_state(false)
    return ui.column { open:get() and ui.text "open" or nil }
end)
return ui.each(items, function(item)
    return Card { title = selected:get() == item.id and "selected" or item.title }
end, { key = function(item) return item.id end })
"##;
        assert_eq!(
            rules(source),
            [
                (5, "untracked-get".to_string()),
                (8, "untracked-get".to_string())
            ]
        );
    }

    #[test]
    fn literal_state_keys_in_builders_are_shared() {
        let source = r##"
local Row = ui.component(function(props)
    local hovered = ui.state("hovered", false)
    local mine = ui.use_state(false)
    local per_pane = ui.state(`file:{props.pane}`, nil)
    local joined = ui.state("file:" .. props.pane, nil)
    return ui.div {}
end)
local rows = ui.each(list, function(item)
    local expanded = ui.persist("expanded", false)
    return ui.div {}
end)
local virtual = ui.list { count = 10, row = function(i)
    local s = ui.state("row", i)
    return ui.div {}
end }
local top = ui.state("top", 0)
"##;
        assert_eq!(
            rules(source),
            [
                (3, "state-in-builder".to_string()),
                (10, "state-in-builder".to_string()),
                (14, "state-in-builder".to_string()),
            ]
        );
    }

    #[test]
    fn timers_faster_than_a_refresh() {
        assert_eq!(
            rules("ui.every(0, function() end)"),
            one(1, "timer-interval")
        );
        assert_eq!(rules("ui.every(5, tick)"), one(1, "timer-interval"));
        assert!(rules("ui.every(16, tick)\nui.every(1_000, tick)\nui.every(ms, tick)").is_empty());
    }

    #[test]
    fn uncapped_callbacks_that_write_state() {
        let source = r##"
anime.animate("#a", { x = 100, onUpdate = function(a) progress:set(a.progress) end })
anime.createTimer { frameRate = 8, onUpdate = function(t) clock:set(t.currentTime) end }
anime.animate("#b", { x = 100, onUpdate = function(a) print(a.progress) end })
anime.animate("#c", { x = 100, onComplete = function() done:set(true) end })
"##;
        assert_eq!(rules(source), one(2, "per-frame-state"));
    }

    #[test]
    fn updates_that_one_method_does() {
        let source = r##"
items:update(function(list) table.insert(list, item) end)
items:update(function(list) list[3].title = "x" end)
config:update(function(c) c.theme = "dark" end)
items:update(function(list) for i, v in list do v.done = true end end)
items:update(function(list) return sort(list) end)
"##;
        assert_eq!(
            rules(source),
            [
                (2, "update-copy".to_string()),
                (3, "update-copy".to_string()),
                (4, "update-copy".to_string()),
            ]
        );
    }

    #[test]
    fn tweens_refired_by_a_timer_for_most_of_each_tick() {
        let source = r##"
ui.every(500, function() anime.animate("#bar", { width = sample(), duration = 420 }) end)
ui.every(500, function() anime.animate("#bar", { width = sample() }) end)
ui.every(500, function() anime.animate("#bar", { width = sample(), duration = 420, frameRate = 20 }) end)
ui.every(1000, function() anime.animate("#bar", { width = sample(), duration = 200 }) end)
ui.every(ms, function() anime.animate("#bar", { width = sample(), duration = 420 }) end)
"##;
        assert_eq!(
            rules(source),
            [(2, "tick-tween".to_string()), (3, "tick-tween".to_string())]
        );
    }

    #[test]
    fn each_keys_reading_state() {
        let source = r##"
local sort = ui.state("sort", "name")
ui.each(rows, build, { key = function(row) return row.id .. sort:get() end })
ui.each(rows, build, { key = function(row) return row.id end })
ui.each(rows, function(row) return ui.text { function() return sort:get() end } end, {
    key = function(row) return row.id end,
})
"##;
        assert_eq!(rules(source), one(3, "key-reads-state"));
    }

    #[test]
    fn allow_comments_and_tricky_source() {
        let source = r##"
-- zgui-lint: allow(timer-interval)
ui.every(1, poll)
ui.every(1, poll) -- zgui-lint: allow(untracked-get, timer-interval)
local s = "ui.every(1, x) count:get()" --[[ ui.every(1, y) ]]
local long = [==[ ui.text { count:get() } ]==]
local v = if open:get() then 1 else 2
if ready then ui.every(2, f) end
return ui.text { `{count:get()}` }
"##;
        assert_eq!(rules(source), one(8, "timer-interval"));
    }
}
