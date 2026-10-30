//! zgx's own best-practice lint for scripts (`zgx lint app.luau`): patterns
//! that type check and run, but cost far more than they look or do not do
//! what they look like. Each finding names the cheaper way to say the same
//! thing. It reads the source only (no luau-lsp, no running the script), so
//! it is fast enough to run on every save and over every example in CI.
//!
//! `-- zgx-lint: allow(rule)` on the line of a finding, or the line above
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
//! - `list-key`: a positional `ui.list` over a list the file removes from or
//!   inserts into: every row after the change is written again.
//! - `list-measured`: a `ui.list` of fixed heights whose rows wrap text: a
//!   row's lines depend on its text and the width, which a script can't
//!   measure; `measured = true` gives each row its content's height.
//! - `reserved-system-key`: a `ui.command` with `reserved = true` on an
//!   editing or system key (`cmd+c`, `cmd+v`, `cmd+z`, `cmd+q`…): reserved,
//!   it is the app's even in a web page or terminal, which then can't use it.
//! - `canvas-view`: a `ui.canvas` without `view` whose `draw` reads a zoom:
//!   panning and zooming by drawing again runs the script for every mark;
//!   `view` places what it drew instead.
//! - `draw-item-state`: a `ui.canvas` `draw_item` reading state. Each item's
//!   drawing is kept until that item is written, so what it read goes stale.
//! - `animated-zoom`: a `zoom` given a tween or a spring, or animated by
//!   `anime`: a zoom lays its subtree out again, so every frame of the
//!   animation does; `scale` only composites.
//! - `zoomable-zoom`: an element given both `zoom` and `zoomable`. Its
//!   zoomable state sets its zoom; a `zoom` beside it fights that state.
//! - `list-marker-hidden`: a `role = "listitem"` whose first child is a
//!   text marker ("•", "3.") not `aria_hidden`. A marker hidden from
//!   assistive technology is the list's: selection and plain text leave it
//!   out and copied HTML makes it a `<ul>` or `<ol>`; one that isn't is
//!   copied, and read, as text.
//! - `window-size-layout`: an element's size or place (`w`, `max_w`, `x`…)
//!   a function computes from `ui.window.size`, which runs Luau on every
//!   frame of a resize. Layout says it without the script (a percentage,
//!   `grow`, `max_w`); the window's size is for decisions (a sidebar
//!   collapsing).
//! - `unknown-token`: a `"$…"` token in a style, a `hover` table or a
//!   computed style that neither zgx nor the app's `ui.theme.tokens`
//!   declares (`"$app.crad"`: did you mean `$app.card`?), one of the wrong
//!   kind for its field (`p = "$app.accent"`), or a theme file setting an
//!   undeclared name. Unlike the rest it reads the app's scripts together
//!   (`lint_app`): one may declare what another uses.
//! - `unknown-target`: a `ui.contribute` or `ui.filter` target `"#id"` or `"hook:name"`
//!   that no script declares with a literal `id` or `hook` (`"#sav"`: did
//!   you mean `#save`?). Read across the app's and its plugins' scripts as
//!   `unknown-token` is; for an app past luau-lsp's union limit (where
//!   `zgx types` leaves `target` a string) and for plugins nobody
//!   type-checks. A selector (`ui.selector(…)`) is not checked.
//! - `unknown-route`: a literal route name given to a router's `navigate`,
//!   `replace` or `is`, or a case of its `view`, that no literal
//!   `ui.router { routes = … }` declares (`"thraed"`: did you mean
//!   `thread`?). Types catch the calls; a misspelled `view` case they
//!   don't (Luau takes a table's extra fields), nor an app past their
//!   limit.

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
    (
        "list-key",
        "a positional ui.list over a list state the file removes from or inserts into",
    ),
    (
        "list-measured",
        "a ui.list of fixed heights whose rows wrap text",
    ),
    (
        "reserved-system-key",
        "a reserved command on an editing or system key, taken from every page and terminal",
    ),
    (
        "canvas-view",
        "a ui.canvas that pans or zooms by drawing again rather than by `view`",
    ),
    (
        "draw-item-state",
        "a ui.canvas draw_item reading state, kept until its item changes",
    ),
    (
        "animated-zoom",
        "a zoom animated frame by frame lays its subtree out on every frame",
    ),
    (
        "zoomable-zoom",
        "an element given both `zoom` and `zoomable`: its state sets the zoom",
    ),
    (
        "list-marker-hidden",
        "a list item's marker text not `aria_hidden`: copied and read as text",
    ),
    (
        "window-size-layout",
        "an element's size computed from ui.window.size rather than laid out",
    ),
    (
        "unknown-token",
        "a \"$…\" token nothing declares, or of the wrong kind for its field",
    ),
    ("i18n-parse", "a locale file or message that doesn't parse"),
    (
        "i18n-missing",
        "a key the source locale has and this locale lacks",
    ),
    (
        "i18n-extra",
        "a key a locale has and the source locale lacks",
    ),
    (
        "i18n-args",
        "a message naming other arguments than the source locale's",
    ),
    (
        "i18n-plural",
        "a plural lacking a category the locale's numbers need",
    ),
    ("i18n-key", "ui.t with a key the source locale lacks"),
    (
        "unknown-target",
        "a ui.contribute or ui.filter target `#id` or `hook:name` no element declares",
    ),
    (
        "unknown-route",
        "a route name given to a router's navigate, replace, is or view that no ui.router declares",
    ),
];

/// `ui.*` calls that do not build interface (`ui.state`, `ui.set`,
/// `ui.effect`, a file read in a handler…); the rest do, and values given to
/// them are read at build time. Every function of `ui` in `zgx.d.luau` is
/// one or the other (`every_ui_function_is_classified`).
const NOT_ELEMENTS: &[&str] = &[
    "on_open",
    "state",
    "shared",
    "persist",
    "use_state",
    "computed",
    "sorted",
    "resource",
    "paged",
    "debounced",
    "socket",
    "sleep",
    "form",
    "try",
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
    "layer_shader",
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
    "use_context",
    "now",
    "object",
    "call_sync",
    "task",
    "router",
    "async",
    "optimistic",
    "open_files",
    "save_file",
    "notify",
    "badge",
    "read_file",
    "write_file",
    "i18n",
    "t",
    "selector",
];

/// Lint every `.luau` script at `paths` (files, or directories searched).
pub fn lint(paths: &[PathBuf]) -> Result<Report, CheckError> {
    let mut report = Report::default();
    for path in paths {
        let files = scripts(path);
        if files.is_empty() {
            return Err(CheckError::NoScripts(path.clone()));
        }
        let mut sources = Vec::new();
        for file in files {
            let source = std::fs::read_to_string(&file).map_err(CheckError::Io)?;
            sources.push((file, source));
            report.files += 1;
        }
        report.diagnostics.extend(lint_app(&sources));
    }
    Ok(report)
}

/// Lint an app's scripts together: each on its own, and what one names
/// against what the others declare (`unknown-token`).
pub fn lint_app(sources: &[(PathBuf, String)]) -> Vec<Diagnostic> {
    let read: Vec<_> = sources
        .iter()
        .map(|(file, source)| (file, tokenize(source)))
        .collect();
    let mut found = Vec::new();
    for (file, (tokens, comments)) in &read {
        found.extend(lint_tokens(file, tokens, comments));
    }
    let files: Vec<_> = read
        .iter()
        .map(|(path, (tokens, _))| crate::token_lint::File { path, tokens })
        .collect();
    let mut tokens = crate::token_lint::unknown_tokens(&files);
    tokens.extend(crate::hook_targets::unknown_targets(&files));
    tokens.extend(crate::routes::unknown_routes(&files));
    tokens.retain(|d| {
        read.iter()
            .find(|(file, _)| **file == d.file)
            .is_none_or(|(_, (_, comments))| !allowed(comments, d.line, &d.code))
    });
    tokens.sort_by_key(|d| (d.line, d.column));
    found.extend(tokens);
    let mut locales = lint_locales(&files);
    locales.retain(|d| {
        read.iter()
            .find(|(file, _)| **file == d.file)
            .is_none_or(|(_, (_, comments))| !allowed(comments, d.line, &d.code))
    });
    found.extend(locales);
    found
}

/// Whether a `-- zgx-lint: allow(rule)` comment on `line`, or the line
/// above it, accepts a finding of `rule` there.
fn allowed(comments: &[(u32, String)], line: u32, rule: &str) -> bool {
    comments.iter().any(|(at, text)| {
        (*at == line || *at + 1 == line)
            && text
                .split_once("zgx-lint:")
                .and_then(|(_, rest)| rest.trim().strip_prefix("allow("))
                .and_then(|rest| rest.split_once(')'))
                .is_some_and(|(names, _)| names.split(',').any(|name| name.trim() == rule))
    })
}

/// What the scripts ask of `ui.i18n` and `ui.t`: each catalog (its
/// directory and source locale), and each literal key with where it is.
#[derive(Default)]
struct Asked {
    catalogs: Vec<(PathBuf, String)>,
    keys: Vec<(PathBuf, u32, u32, String)>,
}
fn asked(file: &Path, tokens: &[Token], into: &mut Asked) {
    let base = file.parent().unwrap_or(Path::new("."));
    let call = |at: usize, name: &str| {
        tokens.get(at).is_some_and(|t| t.is("ui"))
            && tokens.get(at + 1).is_some_and(|t| t.is("."))
            && tokens.get(at + 2).is_some_and(|t| t.is(name))
    };
    for at in 0..tokens.len() {
        if call(at, "i18n") {
            // `ui.i18n { dir = "…", source = "…" }`, or `({ … })`.
            let (mut dir, mut locale) = ("locales".to_owned(), "en".to_owned());
            let mut i = at + 3;
            let mut depth = 0;
            while let Some(t) = tokens.get(i) {
                if t.is("{") || t.is("(") {
                    depth += 1;
                } else if t.is("}") || t.is(")") {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                } else if depth == 1
                    && (t.is("dir") || t.is("source"))
                    && tokens.get(i + 1).is_some_and(|t| t.is("="))
                    && let Some(value) = tokens.get(i + 2).filter(|v| v.kind == Kind::Str)
                {
                    let target = if t.is("dir") { &mut dir } else { &mut locale };
                    *target = unquote(&value.text).to_owned();
                }
                i += 1;
            }
            into.catalogs
                .push((crate::normalize(&base.join(dir)), locale));
        } else if call(at, "t")
            && tokens.get(at + 3).is_some_and(|t| t.is("("))
            && let Some(key) = tokens.get(at + 4).filter(|t| t.kind == Kind::Str)
        {
            into.keys.push((
                file.to_path_buf(),
                key.line,
                key.column,
                unquote(&key.text).to_owned(),
            ));
        }
    }
}

/// The `i18n` rules: each catalog's locale files against its source
/// locale, and `ui.t`'s literal keys against the source.
/// The keys of the first catalog `files` load (`ui.i18n`), in its source
/// locale, sorted: what `zgx types` writes as `MessageKey`.
pub(crate) fn source_keys(files: &[crate::token_lint::File]) -> Option<Vec<String>> {
    let mut asked_for = Asked::default();
    for file in files {
        asked(file.path, file.tokens, &mut asked_for);
    }
    let (dir, source) = asked_for.catalogs.first()?;
    let text = std::fs::read_to_string(dir.join(format!("{source}.json"))).ok()?;
    let (catalog, _) = zgx::i18n::Catalog::parse(source, &text);
    let mut keys: Vec<String> = catalog.keys().map(|(key, _)| key.to_owned()).collect();
    keys.sort();
    Some(keys)
}
fn lint_locales(files: &[crate::token_lint::File]) -> Vec<Diagnostic> {
    use zgx::i18n::{ArgumentKind, Catalog, plural_categories};
    let mut asked_for = Asked::default();
    for file in files {
        asked(file.path, file.tokens, &mut asked_for);
    }
    asked_for.catalogs.sort();
    asked_for.catalogs.dedup();
    let mut found = Vec::new();
    let mut report =
        |file: &Path, (line, column): (u32, u32), severity, code: &str, message: String| {
            found.push(Diagnostic {
                file: file.to_path_buf(),
                line,
                column,
                severity,
                code: code.into(),
                message,
            })
        };
    for (dir, source_locale) in &asked_for.catalogs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        let mut locales = Vec::new();
        for path in files {
            let Some(locale) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (catalog, errors) = Catalog::parse(&locale, &text);
            for error in errors {
                let at = match error.key.as_str() {
                    "" => (error.line.max(1) as u32, error.column.max(1) as u32),
                    key => key_position(&text, key),
                };
                report(&path, at, Severity::Error, "i18n-parse", error.to_string());
            }
            let keys: std::collections::BTreeMap<String, Vec<zgx::i18n::Argument>> = catalog
                .keys()
                .map(|(key, args)| (key.to_owned(), args))
                .collect();
            locales.push((locale, path, text, keys));
        }
        let Some(source) = locales.iter().position(|(l, ..)| l == source_locale) else {
            continue;
        };
        let source_keys = locales[source].3.clone();
        let names = |args: &[zgx::i18n::Argument]| {
            let mut names: Vec<String> = args.iter().map(|a| a.name.clone()).collect();
            names.sort();
            names.dedup();
            names
        };
        for (locale, path, text, keys) in &locales {
            for (key, args) in keys {
                let Some(source_args) = source_keys.get(key) else {
                    report(
                        path,
                        key_position(text, key),
                        Severity::Warning,
                        "i18n-extra",
                        format!("{key} isn't in {source_locale}.json, the source locale"),
                    );
                    continue;
                };
                if names(args) != names(source_args) {
                    report(
                        path,
                        key_position(text, key),
                        Severity::Warning,
                        "i18n-args",
                        format!(
                            "{key} names {{{}}}, where {source_locale} names {{{}}}",
                            names(args).join("}, {"),
                            names(source_args).join("}, {")
                        ),
                    );
                }
                for arg in args {
                    let (cases, ordinal) = match &arg.kind {
                        ArgumentKind::Plural(cases) => (cases, false),
                        ArgumentKind::Ordinal(cases) => (cases, true),
                        _ => continue,
                    };
                    let lacking: Vec<&str> = plural_categories(locale, ordinal)
                        .into_iter()
                        .filter(|c| !cases.iter().any(|have| have == c))
                        .collect();
                    if !lacking.is_empty() {
                        report(
                            path,
                            key_position(text, key),
                            Severity::Warning,
                            "i18n-plural",
                            format!(
                                "{key}'s {{{}}} needs {} in {locale}",
                                arg.name,
                                lacking.join(", ")
                            ),
                        );
                    }
                }
            }
            if locale != source_locale {
                for key in source_keys.keys().filter(|k| !keys.contains_key(*k)) {
                    report(
                        path,
                        (1, 1),
                        Severity::Warning,
                        "i18n-missing",
                        format!("{locale} lacks {key} (it shows {source_locale}'s)"),
                    );
                }
            }
        }
        for (file, line, column, key) in &asked_for.keys {
            if file.starts_with(dir.parent().unwrap_or(dir)) && !source_keys.contains_key(key) {
                report(
                    file,
                    (*line, *column),
                    Severity::Warning,
                    "i18n-key",
                    format!("ui.t(\"{key}\"): {source_locale}.json has no such key"),
                );
            }
        }
    }
    found
}
/// A string literal's text without its quotes (`"x"`, `'x'`, `` `x` ``).
fn unquote(literal: &str) -> &str {
    let quoted = literal.len() >= 2
        && ['"', '\'', '`']
            .iter()
            .any(|q| literal.starts_with(*q) && literal.ends_with(*q));
    if quoted {
        &literal[1..literal.len() - 1]
    } else {
        literal
    }
}
/// Where in a locale file `key` (dotted, `inbox.count`) is: its last part's
/// name, quoted, the first time it is a key; else the file's start.
fn key_position(text: &str, key: &str) -> (u32, u32) {
    let last = key.rsplit('.').next().unwrap_or(key);
    let quoted = format!("\"{last}\"");
    for (n, line) in text.lines().enumerate() {
        if let Some(at) = line.find(&quoted)
            && line[at + quoted.len()..].trim_start().starts_with(':')
        {
            return (n as u32 + 1, at as u32 + 1);
        }
    }
    (1, 1)
}

/// Lint one script's source on its own; `file` names it in the findings.
pub fn lint_source(file: &Path, source: &str) -> Vec<Diagnostic> {
    let (tokens, comments) = tokenize(source);
    lint_tokens(file, &tokens, &comments)
}

fn lint_tokens(file: &Path, tokens: &[Token], comments: &[(u32, String)]) -> Vec<Diagnostic> {
    let mut linter = Linter {
        file,
        tokens,
        found: Vec::new(),
    };
    linter.run();
    linter.list_key();
    linter.list_measured();
    linter.reserved_system_key();
    linter.canvas_view();
    linter.draw_item_state();
    let mut found = linter.found;
    found.retain(|d| !allowed(comments, d.line, &d.code));
    found.sort_by_key(|d| (d.line, d.column));
    found.dedup();
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Name,
    Number,
    Str,
    Punct,
}
#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    pub(crate) text: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}
impl Token {
    fn is(&self, text: &str) -> bool {
        self.kind != Kind::Str && self.text == text
    }
}

/// Luau source as tokens, and its comments by line. Strings (long and
/// interpolated ones too) are single tokens; this lint does not look inside.
pub(crate) fn tokenize(source: &str) -> (Vec<Token>, Vec<(u32, String)>) {
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

/// Element fields layout sizes and places by itself.
const LAID_OUT: &[&str] = &[
    "w", "h", "size", "min_w", "max_w", "min_h", "max_h", "basis", "x", "y", "left", "right",
    "top", "bottom",
];

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
    /// In the element table opened at `table`, its first child when that
    /// is a `ui.text` of a list marker ("•", "-", "3.", "b)", `` `{i}.` ``)
    /// without `aria_hidden = true`: the marker's string.
    fn unhidden_marker(&self, table: usize) -> Option<usize> {
        let end = self.closing(table);
        let mut depth = 0;
        let mut j = table + 1;
        // The first `ui.name` call among its fields.
        let call = loop {
            if j >= end {
                return None;
            }
            match self.tokens[j].text.as_str() {
                "(" | "{" | "[" if self.tokens[j].kind == Kind::Punct => depth += 1,
                ")" | "}" | "]" if self.tokens[j].kind == Kind::Punct => depth -= 1,
                _ => {}
            }
            if depth == 0 && self.is(j, "ui") && self.is(j + 1, ".") && self.name(j + 2).is_some() {
                break j;
            }
            j += 1;
        };
        if !self.is(call + 2, "text") || !(self.is(call + 3, "{") || self.is(call + 3, "(")) {
            return None;
        }
        let close = self.closing(call + 3);
        let marker = (call + 4..close).find(|&k| self.tokens[k].kind == Kind::Str)?;
        let text = &self.tokens[marker].text;
        let inner = text.get(1..text.len().saturating_sub(1))?.trim();
        let bullet = [
            "•", "◦", "▪", "‣", "·", "-", "–", "—", "*", "○", "●", "■", "□",
        ]
        .contains(&inner);
        let numbered = inner.len() > 1 && inner.ends_with(['.', ')']) && {
            let head = &inner[..inner.len() - 1];
            head.chars().all(|c| c.is_ascii_digit())
                || (head.chars().count() == 1 && head.chars().all(|c| c.is_ascii_alphabetic()))
                || (text.starts_with('`') && head.starts_with('{') && head.ends_with('}'))
        };
        if !bullet && !numbered {
            return None;
        }
        let mut depth = 0;
        let hidden = (call + 4..close).any(|k| {
            match self.tokens[k].text.as_str() {
                "(" | "{" | "[" if self.tokens[k].kind == Kind::Punct => depth += 1,
                ")" | "}" | "]" if self.tokens[k].kind == Kind::Punct => depth -= 1,
                _ => {}
            }
            depth == 0 && self.is(k, "aria_hidden") && self.is(k + 1, "=") && self.is(k + 2, "true")
        });
        (!hidden).then_some(marker)
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

    /// A `ui.list` without `key` in a file that reads a list state by index
    /// (`rows:get()[i]`, `rows:at(i)`) and removes from it or inserts into
    /// it: rows by position are each written again when one goes or comes.
    fn list_key(&mut self) {
        // Calls `name:method(`, by the state's name.
        let calls = |method: &str| -> Vec<String> {
            (0..self.tokens.len())
                .filter(|&at| self.is(at, "(") && self.is(at.wrapping_sub(1), method))
                .filter_map(|at| self.callee(at))
                .filter_map(|callee| {
                    callee
                        .strip_suffix(&format!(":{method}"))
                        .map(str::to_string)
                })
                .collect()
        };
        let mut changed = calls("remove");
        changed.extend(calls("insert"));
        if changed.is_empty() {
            return;
        }
        let mut indexed = calls("at");
        // `name:get()[` and `name:peek()[`.
        for method in ["get", "peek"] {
            for at in 0..self.tokens.len() {
                if self.is(at, "(")
                    && self.is(at.wrapping_sub(1), method)
                    && self.is(at + 1, ")")
                    && self.is(at + 2, "[")
                    && let Some(name) = self
                        .callee(at)
                        .and_then(|c| c.strip_suffix(&format!(":{method}")).map(str::to_string))
                {
                    indexed.push(name);
                }
            }
        }
        let Some(list) = indexed.iter().find(|name| changed.contains(name)).cloned() else {
            return;
        };
        for at in 0..self.tokens.len() {
            if !(self.is(at, "list")
                && self.is(at.wrapping_sub(1), ".")
                && self.is(at.wrapping_sub(2), "ui"))
            {
                continue;
            }
            let open = if self.is(at + 1, "{") {
                at + 1
            } else if self.is(at + 1, "(") && self.is(at + 2, "{") {
                at + 2
            } else {
                continue;
            };
            let close = self.closing(open);
            let keyed = (open + 1..close).any(|j| {
                self.is(j, "key")
                    && self.is(j + 1, "=")
                    && (self.is(j - 1, ",") || self.is(j - 1, "{"))
            });
            if !keyed {
                self.report(
                    at,
                    "list-key",
                    format!(
                        "this list's rows are by position, and `{list}` has items removed or \
                         inserted: each change writes every row after it again (text, layout, \
                         paint). `key = function(i) return …id end` makes a row follow its item, \
                         the row reading it by key."
                    ),
                );
            }
        }
    }

    /// The `end` (or `until`) closing the block a `function`, `if`, `do` or
    /// `repeat` at `at` opens.
    fn block_end(&self, at: usize) -> usize {
        let mut depth = 0;
        for j in at..self.tokens.len() {
            if self.tokens[j].kind != Kind::Name {
                continue;
            }
            match self.tokens[j].text.as_str() {
                "function" | "do" | "repeat" => depth += 1,
                "if" if !self.expression_if(j) => depth += 1,
                "end" | "until" => {
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

    /// A `ui.list` without `measured` whose row (an inline function, or a
    /// local function by name) wraps text with no cap on its lines: fixed
    /// heights cut such rows off or leave gaps.
    fn list_measured(&mut self) {
        let field = |linter: &Self, open: usize, close: usize, name: &str| {
            (open + 1..close).find(|&j| {
                linter.is(j, name)
                    && linter.is(j + 1, "=")
                    && (linter.is(j - 1, ",") || linter.is(j - 1, "{"))
            })
        };
        for at in 0..self.tokens.len() {
            if !(self.is(at, "list")
                && self.is(at.wrapping_sub(1), ".")
                && self.is(at.wrapping_sub(2), "ui"))
            {
                continue;
            }
            let open = if self.is(at + 1, "{") {
                at + 1
            } else if self.is(at + 1, "(") && self.is(at + 2, "{") {
                at + 2
            } else {
                continue;
            };
            let close = self.closing(open);
            if field(self, open, close, "measured").is_some() {
                continue;
            }
            let Some(row) = field(self, open, close, "row") else {
                continue;
            };
            // The row's function: written here, or a local one by name.
            let body = if self.is(row + 2, "function") {
                Some(row + 2)
            } else {
                self.name(row + 2).and_then(|name| {
                    (0..self.tokens.len()).find(|&j| {
                        self.is(j, "function")
                            && (self.is(j + 1, name) && self.is(j + 2, "(")
                                || self.is(j.wrapping_sub(1), "=")
                                    && self.is(j.wrapping_sub(2), name)
                                    && self.is(j.wrapping_sub(3), "local"))
                    })
                })
            };
            let Some(start) = body else { continue };
            let end = self.block_end(start);
            let wraps = (start..end).any(|j| {
                self.is(j, "wrap") && self.is(j + 1, "=") && self.is(j + 2, "true")
                    || self.is(j, "markdown") && self.is(j.wrapping_sub(1), ".")
            });
            let capped = (start..end).any(|j| self.is(j, "lines") && self.is(j + 1, "="));
            if wraps && !capped {
                self.report(
                    at,
                    "list-measured",
                    "this list's rows wrap text but have fixed heights: how many lines a \
                     row takes depends on its text and the width, which the script can't \
                     measure. `measured = true` gives each row its content's height \
                     (`height` the estimate until a row is measured)."
                        .into(),
                );
            }
        }
    }

    /// A `ui.command` with `reserved = true` on an editing or system key
    /// (`cmd+c`, `cmd+v`, `cmd+z`, `cmd+q`…): reserved, it is the app's
    /// even in a page or terminal, which then can neither copy nor paste.
    fn reserved_system_key(&mut self) {
        let field = |linter: &Self, open: usize, close: usize, name: &str| {
            (open + 1..close).find(|&j| {
                linter.is(j, name)
                    && linter.is(j + 1, "=")
                    && (linter.is(j - 1, ",") || linter.is(j - 1, "{"))
            })
        };
        for at in 0..self.tokens.len() {
            if !(self.is(at, "command")
                && self.is(at.wrapping_sub(1), ".")
                && self.is(at.wrapping_sub(2), "ui"))
            {
                continue;
            }
            let open = if self.is(at + 1, "{") {
                at + 1
            } else if self.is(at + 1, "(") && self.is(at + 2, "{") {
                at + 2
            } else {
                continue;
            };
            let close = self.closing(open);
            if !field(self, open, close, "reserved").is_some_and(|j| self.is(j + 2, "true")) {
                continue;
            }
            let Some(key) = field(self, open, close, "key")
                .and_then(|j| self.tokens.get(j + 2))
                .filter(|t| t.kind == Kind::Str)
                .map(|t| t.text.trim_matches(['"', '\'']).to_ascii_lowercase())
            else {
                continue;
            };
            let mut parts: Vec<&str> = key.split('+').map(str::trim).collect();
            let letter = parts.pop().unwrap_or_default();
            let primary = parts
                .iter()
                .any(|p| matches!(*p, "cmd" | "ctrl" | "control"));
            // Shift only with Z: ⌘⇧Z is redo.
            let other = parts
                .iter()
                .any(|p| matches!(*p, "alt" | "option") || (*p == "shift" && letter != "z"));
            let editing = matches!(letter, "c" | "v" | "x" | "z" | "a" | "y");
            let system = parts.contains(&"cmd") && matches!(letter, "q" | "h" | "m");
            if primary && !other && (editing || system) {
                self.report(
                    at,
                    "reserved-system-key",
                    format!(
                        "`{key}` reserved is the app's even in a web page or a terminal, which \
                         then cannot use it (copy, paste, undo, quit…). Leave it unreserved: \
                         a page or terminal leaves alone what it does not use."
                    ),
                );
            }
        }
    }

    /// A `ui.canvas` without `view` whose `draw` (written there, or a local
    /// function by name) reads a `.zoom`: it pans and zooms by drawing again.
    fn canvas_view(&mut self) {
        for at in 0..self.tokens.len() {
            if !(self.is(at, "canvas")
                && self.is(at.wrapping_sub(1), ".")
                && self.is(at.wrapping_sub(2), "ui")
                && self.is(at + 1, "{"))
            {
                continue;
            }
            let (open, close) = (at + 1, self.closing(at + 1));
            let field = |name: &str| {
                (open + 1..close).find(|&j| {
                    self.is(j, name)
                        && self.is(j + 1, "=")
                        && (self.is(j - 1, ",") || self.is(j - 1, "{"))
                })
            };
            if field("view").is_some() {
                continue;
            }
            let Some(draw) = field("draw") else { continue };
            let Some(start) = self.function_given(draw + 2) else {
                continue;
            };
            let end = self.block_end(start);
            if (start..end).any(|j| self.is(j, "zoom") && self.is(j.wrapping_sub(1), ".")) {
                self.report(
                    at,
                    "canvas-view",
                    "this canvas draws through a zoom: each pan or zoom runs `draw` again, \
                     the script for every mark. Draw in the canvas's own coordinates and \
                     give `view = state` ({ x, y, zoom }): the view places what it drew, \
                     and `bg_grid` draws a grid under it. What keeps its size on screen \
                     (handles, outlines) can be a small canvas of its own."
                        .into(),
                );
            }
        }
    }

    /// A `ui.canvas`'s `draw_item` (written there, or a local function by
    /// name) that reads state with `:get()`: its item's drawing is kept until
    /// the item is written, so the state's later values never show.
    fn draw_item_state(&mut self) {
        for at in 0..self.tokens.len() {
            if !(self.is(at, "draw_item")
                && self.is(at + 1, "=")
                && (self.is(at.wrapping_sub(1), ",") || self.is(at.wrapping_sub(1), "{")))
            {
                continue;
            }
            let Some(start) = self.function_given(at + 2) else {
                continue;
            };
            let end = self.block_end(start);
            if let Some(read) = (start..end)
                .find(|&j| self.is(j, ":") && self.is(j + 1, "get") && self.is(j + 2, "("))
            {
                self.report(
                    read,
                    "draw-item-state",
                    "this `draw_item` reads state: each item's drawing is kept until that \
                     item is written, so it shows the value it read then. Keep what an \
                     item looks like in the item (its colour, whether it is chosen), or \
                     draw what depends on other state in a canvas of its own."
                        .into(),
                );
            }
        }
    }

    /// The function a field's value at `at` names: written there, or a local
    /// function by name.
    fn function_given(&self, at: usize) -> Option<usize> {
        if self.is(at, "function") {
            return Some(at);
        }
        let name = self.name(at)?;
        (0..self.tokens.len()).find(|&j| {
            self.is(j, "function")
                && (self.is(j + 1, name) && self.is(j + 2, "(")
                    || self.is(j.wrapping_sub(1), "=")
                        && self.is(j.wrapping_sub(2), name)
                        && self.is(j.wrapping_sub(3), "local"))
        })
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
        // `zoom = ui.tween(…)` or `ui.spring(…)`, or `zoom` animated by `anime`.
        if self.is(at, "zoom") && self.is(at + 1, "=") && self.field(at + 2).is_some() {
            let tweened = self.is(at + 2, "ui")
                && self.is(at + 3, ".")
                && (self.is(at + 4, "tween") || self.is(at + 4, "spring"))
                && self.is(at + 5, "(");
            let animated = stack.len() >= 2
                && stack[stack.len() - 1].open == Open::Brace
                && matches!(
                    stack[stack.len() - 2].callee.as_deref(),
                    Some("anime.animate" | "anime.createAnimatable")
                );
            if tweened || animated {
                self.report(
                    at,
                    "animated-zoom",
                    "an animated `zoom` lays its subtree out again on every frame of the \
                     animation, as CSS's does: text wraps anew and everything inside moves. \
                     Animate `scale` instead (drawn scaled, without layout), and set `zoom` \
                     once, where it settles."
                        .into(),
                );
            }
        }
        // `w = function() … ui.window.size … end`: a size from the window's.
        if self.is(at, "ui")
            && self.is(at + 1, ".")
            && self.is(at + 2, "window")
            && self.is(at + 3, ".")
            && self.is(at + 4, "size")
            && !self.is(at.wrapping_sub(1), ".")
            && let Some(f) = function
            && let Some(key) = stack[f].key.as_deref()
            && LAID_OUT.contains(&key)
        {
            self.report(
                at,
                "window-size-layout",
                format!(
                    "`{key}` computed from `ui.window.size` runs Luau on every frame of a \
                     resize, for what layout does without the script: a percentage \
                     (`{key} = \"30%\"`), `grow`, `min_w` and `max_w`. Read the window's size \
                     for decisions layout can't make (collapsing a sidebar to icons), and a \
                     ref's `width` for the room one element has."
                ),
            );
        }
        // `zoomable = state` in a table that also gives `zoom`.
        if self.is(at, "zoomable")
            && self.is(at + 1, "=")
            && self.field(at + 2).is_some()
            && let Some(table) = stack.last().filter(|frame| frame.open == Open::Brace)
        {
            let end = self.closing(table.at);
            let mut depth = 0;
            let zoom = (table.at + 1..end).any(|j| {
                match self.tokens[j].text.as_str() {
                    "(" | "{" | "[" => depth += 1,
                    ")" | "}" | "]" => depth -= 1,
                    _ => {}
                }
                depth == 0 && self.is(j, "zoom") && self.is(j + 1, "=") && !self.is(j - 1, ".")
            });
            if zoom {
                self.report(
                    at,
                    "zoomable-zoom",
                    "this element's `zoomable` state sets its zoom, and the `zoom` beside it \
                     sets it again each time its style applies. Give the zoom to the state \
                     alone (`ui.state(\"zoom\", 1.5)`) and drop `zoom`."
                        .into(),
                );
            }
        }
        // `role = "listitem"` whose first child is a marker text not hidden.
        if self.is(at, "role")
            && self.is(at + 1, "=")
            && self.tokens.get(at + 2).is_some_and(|t| {
                t.kind == Kind::Str && t.text.trim_matches(['"', '\'']) == "listitem"
            })
            && let Some(table) = stack.last().filter(|frame| frame.open == Open::Brace)
            && let Some(marker) = self.unhidden_marker(table.at)
        {
            self.report(
                marker,
                "list-marker-hidden",
                "this list item's marker shows as text: selecting the item takes it, its \
                 plain text copies it, and screen readers read it. Give it \
                 `aria_hidden = true`: the list then draws it, as a browser draws an `<li>`'s \
                 marker (copied HTML makes the list a `<ul>`, or an `<ol>` from the number)."
                    .into(),
            );
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
                 state's order as it changes, re-placing only the rows written (a `filter` \
                 of fields' tests, `{ sector = \"Tech\" }`, is read in Rust)."
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
    /// document parsed into blocks, a 3D scene's objects.
    const BUILDS: &[&str] = &[
        "window",
        "contribute",
        "filter",
        "override",
        "markdown",
        "mesh",
        "light",
        "group",
        "model",
    ];

    /// A function added to `ui` is either an element or listed in
    /// `NOT_ELEMENTS`, so `untracked-get` neither flags a file written in a
    /// click handler nor misses a new element.
    #[test]
    fn every_ui_function_is_classified() {
        let types = include_str!("../types/zgx.d.luau");
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
    fn a_list_items_marker_not_hidden_is_reported() {
        let source = r##"
ui.row { role = "listitem", ui.text { "•" }, ui.text { "one" } }
ui.row { role = "listitem", ui.text { `{i}.`, color = "$text.tertiary" }, ui.text { "two" } }
ui.row { role = "listitem", ui.text { "•", aria_hidden = true }, ui.text { "three" } }
ui.row { role = "listitem", ui.text { "Name" }, ui.text { "four" } }
ui.row { role = "listitem", ui.image { src = "dot.png" }, ui.text { "five" } }
ui.row { role = "option", ui.text { "•" } }
"##;
        assert_eq!(
            rules(source),
            vec![
                (2, "list-marker-hidden".to_string()),
                (3, "list-marker-hidden".to_string())
            ]
        );
    }

    #[test]
    fn a_size_from_the_window_is_reported_and_a_decision_is_not() {
        let source = r##"
ui.div { w = function() return ui.window.size:get().width * 0.3 end }
ui.div { max_w = function() return ui.window.size:get().height - 40 end }
local narrow = ui.computed(function() return ui.window.size:get().width < 700 end)
ui.div { visible = function() return ui.window.size:get().width >= 700 end }
ui.div { w = function() return if narrow:get() then 48 else 240 end }
ui.div { w = "30%", grow = 1 }
"##;
        assert_eq!(
            rules(source),
            [
                (2, "window-size-layout".to_string()),
                (3, "window-size-layout".to_string())
            ]
        );
    }

    #[test]
    fn a_zoom_beside_a_zoomable_is_reported() {
        let source = r##"
local z = ui.state("z", 1)
ui.column { zoomable = z, zoom = 2 }
ui.column { zoomable = z, ui.div { zoom = 2 } }
ui.column { zoomable = z }
"##;
        assert_eq!(rules(source), one(3, "zoomable-zoom"));
    }

    #[test]
    fn an_animated_zoom_is_reported_and_a_set_one_is_not() {
        let source = r##"
local big = ui.state("big", false)
ui.div { zoom = ui.tween(function() return big:get() and 2 or 1 end) }
ui.div { zoom = ui.spring(function() return big:get() and 2 or 1 end) }
anime.animate("#panel", { zoom = 2, duration = 300 })
ui.div { zoom = 1.5, zoom_fixed = true }
ui.div { zoom = function() return big:get() and 2 or 1 end }
ui.div { scale = ui.tween(function() return big:get() and 2 or 1 end) }
local zoom = ui.tween(big)
"##;
        assert_eq!(
            rules(source),
            [
                (3, "animated-zoom".to_string()),
                (4, "animated-zoom".to_string()),
                (5, "animated-zoom".to_string()),
            ]
        );
    }

    #[test]
    fn a_positional_list_over_a_list_with_removals_is_reported() {
        let source = r##"
local items = ui.state("items", {})
local function drop() items:remove(1) end
local list = ui.list { count = 10, row = function(i) return ui.text { function() return items:get()[i].name end } end }
local keyed = ui.list { count = 10, key = function(i) return items:at(i).id end, row = function(id) return ui.text("x") end }
"##;
        assert_eq!(rules(source), one(4, "list-key"));
        // No removal or insertion: rows by position are fine.
        let still = r##"
local items = ui.state("items", {})
local list = ui.list { count = 10, row = function(i) return ui.text { function() return items:get()[i].name end } end }
"##;
        assert_eq!(rules(still), vec![]);
    }

    #[test]
    fn a_list_of_fixed_heights_with_wrapped_rows_is_reported() {
        let source = r##"
local function message(i)
    if i > 0 then return ui.text { "x", wrap = true } end
    return ui.text "y"
end
local list = ui.list { count = 10, height = 40, row = message }
local inline = ui.list { count = 10, row = function(i) return ui.markdown("**x**") end }
local measured = ui.list { count = 10, measured = true, row = message }
local capped = ui.list { count = 10, row = function(i) return ui.text { "x", wrap = true, lines = 2 } end }
local plain = ui.list { count = 10, row = function(i) return ui.text "x" end }
"##;
        assert_eq!(
            rules(source),
            vec![(6, "list-measured".into()), (7, "list-measured".into())]
        );
    }

    #[test]
    fn a_reserved_editing_or_system_key_is_reported() {
        let source = r##"
ui.command { id = "copy", key = "cmd+c", reserved = true, run = function() end }
ui.command { id = "quit", key = "Cmd+Q", reserved = true, run = function() end }
ui.command { id = "tab", key = "cmd+t", reserved = true, run = function() end }
ui.command { id = "paste", key = "cmd+v", run = function() end }
ui.command { id = "redo", key = "cmd+shift+z", reserved = true, run = function() end }
ui.command { id = "undo", key = "ctrl+z", reserved = false, run = function() end }
"##;
        assert_eq!(
            rules(source),
            vec![
                (2, "reserved-system-key".into()),
                (3, "reserved-system-key".into()),
                (6, "reserved-system-key".into())
            ]
        );
    }

    #[test]
    fn a_canvas_that_pans_by_drawing_again_is_reported() {
        let source = r##"
local view = ui.state("view", { x = 0, y = 0, zoom = 1 })
local function board(c) local v = view:get() c:rect(v.x, v.y, 10 * v.zoom, 10 * v.zoom, "#fff") end
local drawn = ui.canvas { draw = board }
local inline = ui.canvas { draw = function(c) c:rect(0, 0, view:get().zoom, 1, "#fff") end }
local viewed = ui.canvas { view = view, draw = function(c) c:rect(0, 0, 10, 10, "#fff") end }
local plain = ui.canvas { draw = function(c, w, h) c:rect(0, 0, w, h, "#fff") end }
"##;
        assert_eq!(
            rules(source),
            vec![(4, "canvas-view".into()), (5, "canvas-view".into())]
        );
    }

    #[test]
    fn a_draw_item_reading_state_is_reported() {
        let source = r##"
local shapes = ui.state("shapes", {})
local chosen = ui.state("chosen", 0)
local function shape(c, s) c:rect(s.x, s.y, 4, 4, if chosen:get() == s.id then "#f00" else "#fff") end
local named = ui.canvas { each = shapes, draw_item = shape }
local inline = ui.canvas { each = shapes, draw_item = function(c, s) c:rect(s.x, chosen:get(), 4, 4, "#fff") end }
local plain = ui.canvas { each = shapes, draw_item = function(c, s) c:rect(s.x, s.y, 4, 4, s.fill) end }
"##;
        assert_eq!(
            rules(source),
            vec![(4, "draw-item-state".into()), (6, "draw-item-state".into())]
        );
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
-- zgx-lint: allow(timer-interval)
ui.every(1, poll)
ui.every(1, poll) -- zgx-lint: allow(untracked-get, timer-interval)
local s = "ui.every(1, x) count:get()" --[[ ui.every(1, y) ]]
local long = [==[ ui.text { count:get() } ]==]
local v = if open:get() then 1 else 2
if ready then ui.every(2, f) end
return ui.text { `{count:get()}` }
"##;
        assert_eq!(rules(source), one(8, "timer-interval"));
    }
}
