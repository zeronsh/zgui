//! The `unknown-token` lint: a `"$…"` token a script names that nothing
//! declares (a misspelled `"$app.crad"` reads as transparent at run time),
//! or a token of one kind where another goes (`p = "$app.accent"`).
//!
//! It reads an app's scripts and theme files only, as the rest of the lint
//! does. The names it knows are zgx's standard tokens and those an app
//! declares with a literal `ui.theme.tokens { … }` table (nested groups as
//! dotted names; a default's kind is the declared kind). A theme file
//! loaded with a literal path (`ui.theme.load("glass.json")`) or a literal
//! `ui.theme.register { … }` table adds none: a theme overrides what is
//! declared, so one setting an undeclared name is itself a finding. A
//! declaration that isn't literal (a computed key, a table built at run
//! time) opens its group: names under it are not reported, and the run
//! time warning covers them.
use crate::check::{Diagnostic, Severity};
use crate::lint::{Kind, Token};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use zgx::theme::{TokenInput, TokenKind};

/// What an app declares and themes, from its scripts.
#[derive(Default)]
struct Names {
    /// Each declared name and its kind, where known.
    declared: HashMap<String, Option<TokenKind>>,
    /// A declared name whose default only names another: that one.
    references: HashMap<String, String>,
    /// Groups declared in a way the lint can't read: names under them pass.
    open: Vec<String>,
    /// Every name is open (a whole table built at run time).
    all_open: bool,
}
impl Names {
    fn is_open(&self, name: &str) -> bool {
        self.all_open
            || self
                .open
                .iter()
                .any(|group| name == group || name.starts_with(&format!("{group}.")))
    }
}

/// The standard tokens' names and kinds.
fn standard() -> HashMap<&'static str, TokenKind> {
    zgx::theme::standard()
        .map(|(_, name, kind)| (name, kind))
        .collect()
}

/// The kind of value each style field takes, from the definitions luau-lsp
/// checks (`types/zgx.d.luau`): a field typed `Color`, `Num`, `Length`,
/// `Shadow` or `Font`, alone or `Reactive<…>`. A field typed differently in
/// different element types is left out.
fn field_kinds() -> &'static HashMap<String, TokenKind> {
    static KINDS: std::sync::OnceLock<HashMap<String, TokenKind>> = std::sync::OnceLock::new();
    KINDS.get_or_init(|| {
        let mut kinds: HashMap<String, Option<TokenKind>> = HashMap::new();
        for line in include_str!("../types/zgx.d.luau").lines() {
            let Some((field, ty)) = line.trim().split_once(": ") else {
                continue;
            };
            if !field.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                continue;
            }
            let ty = ty.trim_end_matches(',').trim_end_matches('?');
            let ty = ty
                .strip_prefix("Reactive<")
                .and_then(|t| t.strip_suffix('>'))
                .unwrap_or(ty);
            let kind = match ty {
                "Color" => TokenKind::Color,
                "Num" | "Length" => TokenKind::Number,
                "Shadow" => TokenKind::Shadow,
                "Font" => TokenKind::Font,
                _ => continue,
            };
            let entry = kinds.entry(field.to_owned()).or_insert(Some(kind));
            if *entry != Some(kind) {
                *entry = None;
            }
        }
        kinds
            .into_iter()
            .filter_map(|(field, kind)| Some((field, kind?)))
            .collect()
    })
}

fn kind_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Color => "colour",
        TokenKind::Number => "number",
        TokenKind::Font => "font",
        TokenKind::Shadow => "shadow",
        TokenKind::Text => "text",
    }
}

/// The token a string literal names, if it is one: `"$app.card"` gives
/// `app.card`. Quotes of any kind; a name of letters, digits, `_`, `-` and
/// dots, starting with a letter.
fn token_name(literal: &str) -> Option<&str> {
    let inner = literal
        .strip_prefix(['"', '\''])
        .and_then(|s| s.strip_suffix(['"', '\'']))?;
    let name = inner.strip_prefix('$')?;
    let valid = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.ends_with('.');
    valid.then_some(name)
}
/// A plain string literal's text, if it is one (no interpolation).
pub(crate) fn literal(token: &Token) -> Option<&str> {
    (token.kind == Kind::Str)
        .then(|| {
            token
                .text
                .strip_prefix(['"', '\''])
                .and_then(|s| s.strip_suffix(['"', '\'']))
        })
        .flatten()
        .filter(|text| !text.contains('\\'))
}

/// The edit distance between two names, for a suggestion.
pub(crate) fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j] + 1)
                .min(row[j + 1] + 1)
                .min(previous + usize::from(ca != *cb));
            previous = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// One file's tokens, as `lint` read them.
pub(crate) struct File<'a> {
    pub(crate) path: &'a Path,
    pub(crate) tokens: &'a [Token],
}

/// What an app's scripts (`files`) declare with literal `ui.theme.tokens`
/// tables: each name, sorted, with its kind where it shows (through the
/// references a default makes, to zgx's tokens or the app's own).
pub(crate) fn declared(files: &[File]) -> Declared {
    let standard = standard();
    let mut names = Names::default();
    for file in files {
        let t = file.tokens;
        for i in 0..t.len() {
            if called(t, i).as_deref() == Some("ui.theme.tokens") {
                let arg = if t[i].kind == Kind::Punct && t[i].text == "(" {
                    i + 1
                } else {
                    i
                };
                declarations(t, arg, &mut names);
            }
        }
    }
    let kind_of = |name: &str| -> Option<TokenKind> {
        let mut name = name.to_owned();
        for _ in 0..8 {
            if let Some(kind) = standard.get(name.as_str()) {
                return Some(*kind);
            }
            if let Some(Some(kind)) = names.declared.get(&name) {
                return Some(*kind);
            }
            name = names.references.get(&name)?.clone();
        }
        None
    };
    let mut out: Vec<_> = names
        .declared
        .keys()
        .filter(|name| !standard.contains_key(name.as_str()))
        .map(|name| (name.clone(), kind_of(name)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    let mut open = names.open.clone();
    open.sort();
    open.dedup();
    Declared {
        names: out,
        open,
        all_open: names.all_open,
    }
}

/// What `declared` finds.
pub(crate) struct Declared {
    /// Each name, sorted, with its kind where it shows.
    pub(crate) names: Vec<(String, Option<TokenKind>)>,
    /// Groups declared in a way the lint can't read.
    pub(crate) open: Vec<String>,
    /// Every name is open.
    pub(crate) all_open: bool,
}

/// The `unknown-token` findings over an app's scripts (`files`, all of one
/// app: what one declares, another may use).
pub(crate) fn unknown_tokens(files: &[File]) -> Vec<Diagnostic> {
    let standard = standard();
    let mut names = Names::default();
    let mut themes: Vec<(usize, usize, PathBuf)> = Vec::new();
    let mut registered: Vec<(usize, usize)> = Vec::new();
    for (f, file) in files.iter().enumerate() {
        let t = file.tokens;
        for i in 0..t.len() {
            // The call's argument: after its `(`, or the table or string
            // given without one.
            let arg = if t[i].kind == Kind::Punct && t[i].text == "(" {
                i + 1
            } else {
                i
            };
            match called(t, i).as_deref() {
                Some("ui.theme.tokens") => declarations(t, arg, &mut names),
                Some("ui.theme.load") => {
                    if let Some(path) = t.get(arg).and_then(literal) {
                        let dir = file.path.parent().unwrap_or(Path::new("."));
                        themes.push((f, arg, dir.join(path)));
                    }
                }
                Some("ui.theme.register")
                    if t.get(arg)
                        .is_some_and(|t| t.kind == Kind::Punct && t.text == "{") =>
                {
                    registered.push((f, arg))
                }
                _ => {}
            }
        }
    }
    let known = |name: &str| -> Option<Option<TokenKind>> {
        standard
            .get(name)
            .map(|kind| Some(*kind))
            .or_else(|| names.declared.get(name).copied())
    };
    // A declared name's kind, through the references it makes.
    let kind_of = |name: &str| -> Option<TokenKind> {
        let mut name = name.to_owned();
        for _ in 0..8 {
            if let Some(kind) = standard.get(name.as_str()) {
                return Some(*kind);
            }
            if let Some(Some(kind)) = names.declared.get(&name) {
                return Some(*kind);
            }
            name = names.references.get(&name)?.clone();
        }
        None
    };
    let suggest = |name: &str| -> String {
        let candidates = standard
            .keys()
            .copied()
            .chain(names.declared.keys().map(String::as_str));
        let best = candidates
            .map(|c| (distance(name, c), c))
            .min_by_key(|(d, c)| (*d, c.len()));
        match best {
            Some((d, c)) if d <= (name.len() / 3).max(2) => format!(": did you mean ${c}?"),
            _ => String::new(),
        }
    };
    let mut found = Vec::new();
    let mut report = |file: &File, at: usize, message: String| {
        let token = &file.tokens[at];
        found.push(Diagnostic {
            file: file.path.to_path_buf(),
            line: token.line,
            column: token.column,
            severity: Severity::Warning,
            code: "unknown-token".into(),
            message,
        });
    };
    for file in files {
        let t = file.tokens;
        for i in 0..t.len() {
            let Some(name) = literal(&t[i]).and_then(|_| token_name(&t[i].text)) else {
                continue;
            };
            if !in_value_position(t, i) {
                continue;
            }
            match known(name) {
                None if names.is_open(name) => {}
                None => report(
                    file,
                    i,
                    format!(
                        "`${name}` is no token: neither zgx's nor declared by `ui.theme.tokens`{}",
                        suggest(name)
                    ),
                ),
                Some(_) => {
                    let wanted = field_of(t, i).and_then(|field| field_kinds().get(field));
                    if let (Some(wanted), Some(kind)) = (wanted, kind_of(name))
                        && *wanted != kind
                    {
                        report(
                            file,
                            i,
                            format!(
                                "`${name}` is a {} token, and `{}` takes a {}",
                                kind_name(kind),
                                field_of(t, i).unwrap_or_default(),
                                kind_name(*wanted)
                            ),
                        );
                    }
                }
            }
        }
    }
    // Themes: what they set must be declared.
    let mut seen = HashSet::new();
    for (f, at, path) in themes {
        let file = &files[f];
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(theme) = zgx::theme::Theme::from_json(&text) else {
            continue;
        };
        let shown = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        for (id, _) in theme.tokens() {
            let name = id.name();
            if known(&name).is_none()
                && !names.is_open(&name)
                && seen.insert((shown.clone(), name.to_string()))
            {
                report(
                    file,
                    at,
                    format!(
                        "{shown} sets `{name}`, which nothing declares: declare it in `ui.theme.tokens` (its default for every theme){}",
                        suggest(&name)
                    ),
                );
            }
        }
    }
    for (f, at) in registered {
        let file = &files[f];
        for (name, token) in registered_tokens(file.tokens, at) {
            if known(&name).is_none() && !names.is_open(&name) {
                report(
                    file,
                    token,
                    format!(
                        "this theme sets `{name}`, which nothing declares: declare it in `ui.theme.tokens`{}",
                        suggest(&name)
                    ),
                );
            }
        }
    }
    found
}

/// The dotted name called by the call whose argument starts at `at` (a
/// `(`, a `{` or a string), as `ui.theme.tokens { … }` and
/// `ui.theme.load("x.json")` are called.
pub(crate) fn called(t: &[Token], at: usize) -> Option<String> {
    let open = t.get(at)?;
    let callee_end = match open.kind {
        Kind::Punct if open.text == "(" || open.text == "{" => at,
        Kind::Str => at,
        _ => return None,
    };
    // A `(` opening the call: the argument is the token after it.
    let mut parts = Vec::new();
    let mut j = callee_end;
    while j >= 1 && t[j - 1].kind == Kind::Name {
        parts.insert(0, t[j - 1].text.clone());
        if j >= 2 && t[j - 2].text == "." && t[j - 2].kind == Kind::Punct {
            j -= 2;
        } else {
            break;
        }
    }
    if parts.is_empty() {
        return None;
    }
    let name = parts.join(".");
    // `ui.theme.tokens(…)`: the argument after the parenthesis.
    Some(name)
}

/// The index of the bracket closing the one opened at `at`.
pub(crate) fn closing(t: &[Token], at: usize) -> usize {
    let mut depth = 0;
    for (j, token) in t.iter().enumerate().skip(at) {
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
    t.len()
}

/// Read the declarations of the `ui.theme.tokens` call whose argument is
/// at `at`: a table, or anything else (every name is then open).
fn declarations(t: &[Token], at: usize, names: &mut Names) {
    match t.get(at) {
        Some(table) if table.kind == Kind::Punct && table.text == "{" => group(t, at, "", names),
        _ => names.all_open = true,
    }
}

/// The fields of the table opened at `open`, declared under `prefix`.
fn group(t: &[Token], open: usize, prefix: &str, names: &mut Names) {
    let end = closing(t, open);
    let mut j = open + 1;
    while j < end {
        // A field: `name = value` or `["a.b"] = value`.
        let key = if t[j].kind == Kind::Name && t.get(j + 1).is_some_and(|t| t.text == "=") {
            let key = t[j].text.clone();
            j += 2;
            Some(key)
        } else if t[j].text == "["
            && let Some(key) = t.get(j + 1).and_then(literal)
            && t.get(j + 2).is_some_and(|t| t.text == "]")
            && t.get(j + 3).is_some_and(|t| t.text == "=")
        {
            j += 4;
            Some(key.to_owned())
        } else {
            None
        };
        let Some(key) = key else {
            // A computed key (`[name] = …`) or a positional value: this
            // group can't be read.
            names.open.push(prefix.to_owned());
            if prefix.is_empty() {
                names.all_open = true;
            }
            j = next_field(t, j, end);
            continue;
        };
        let name = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        let value = &t[j];
        let is_end_of_value =
            |k: usize| t.get(k).is_some_and(|t| t.text == "," || t.text == ";") || k == end;
        if value.text == "{" && value.kind == Kind::Punct {
            group(t, j, &name, names);
            j = closing(t, j) + 1;
        } else if let Some(text) = literal(value).filter(|_| is_end_of_value(j + 1)) {
            match TokenInput::parse(text) {
                Ok(input) => {
                    if let Some(to) = input.refers_to() {
                        names.references.insert(name.clone(), to.name().to_string());
                    }
                    names.declared.insert(name, input.kind());
                }
                Err(_) => {
                    names.declared.insert(name, None);
                }
            }
            j += 1;
        } else if value.kind == Kind::Number && is_end_of_value(j + 1) {
            names.declared.insert(name, Some(TokenKind::Number));
            j += 1;
        } else {
            // A value computed at run time: declared, of a kind not known,
            // and maybe a whole group (`chart = palette()`).
            names.open.push(name.clone());
            names.declared.insert(name, None);
            j = next_field(t, j, end);
            continue;
        }
        j = next_field(t, j, end);
    }
}

/// The start of the field after the one at `at`, in a table ending at `end`.
pub(crate) fn next_field(t: &[Token], at: usize, end: usize) -> usize {
    let mut j = at;
    while j < end {
        match (t[j].kind, t[j].text.as_str()) {
            (Kind::Punct, "," | ";") => return j + 1,
            (Kind::Punct, "(" | "{" | "[") => j = closing(t, j) + 1,
            _ => j += 1,
        }
    }
    end
}

/// The token names a literal `ui.theme.register { tokens = { … } }` sets,
/// with where each is.
fn registered_tokens(t: &[Token], open: usize) -> Vec<(String, usize)> {
    let end = closing(t, open);
    let mut j = open + 1;
    while j + 2 < end {
        if t[j].text == "tokens" && t[j + 1].text == "=" && t[j + 2].text == "{" {
            let mut out = Vec::new();
            collect_set(t, j + 2, "", &mut out);
            return out;
        }
        j = next_field(t, j, end);
    }
    Vec::new()
}
fn collect_set(t: &[Token], open: usize, prefix: &str, out: &mut Vec<(String, usize)>) {
    let end = closing(t, open);
    let mut j = open + 1;
    while j < end {
        let key = if t[j].kind == Kind::Name && t.get(j + 1).is_some_and(|t| t.text == "=") {
            Some((t[j].text.clone(), j, j + 2))
        } else if t[j].text == "["
            && let Some(key) = t.get(j + 1).and_then(literal)
            && t.get(j + 3).is_some_and(|t| t.text == "=")
        {
            Some((key.to_owned(), j + 1, j + 4))
        } else {
            None
        };
        let Some((key, at, value)) = key else {
            j = next_field(t, j, end);
            continue;
        };
        let name = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        if t.get(value)
            .is_some_and(|t| t.text == "{" && t.kind == Kind::Punct)
        {
            collect_set(t, value, &name, out);
            j = closing(t, value) + 1;
            j = next_field(t, j, end);
        } else {
            out.push((name, at));
            j = next_field(t, value, end);
        }
    }
}

/// Whether the string at `at` is given as a value where a token goes: a
/// field's (`bg = "$…"`), a return's (a computed style), a branch of an
/// `if` expression or of `and`/`or`, or `ui.theme.get`'s.
fn in_value_position(t: &[Token], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1).map(|j| &t[j]) else {
        return false;
    };
    matches!(
        (previous.kind, previous.text.as_str()),
        (Kind::Punct, "=") | (Kind::Name, "return" | "then" | "else" | "and" | "or")
    )
}

/// The field the value at `at` is given to (`bg` in `bg = "$…"`), also
/// through an `if`/`and`/`or` expression on the field's line.
fn field_of(t: &[Token], at: usize) -> Option<&str> {
    let mut j = at;
    while j >= 2 {
        let previous = &t[j - 1];
        if previous.kind == Kind::Punct && previous.text == "=" {
            let field = &t[j - 2];
            return (field.kind == Kind::Name).then_some(field.text.as_str());
        }
        if previous.kind == Kind::Punct && matches!(previous.text.as_str(), "," | "{" | "(" | ";")
            || previous.line != t[at].line
        {
            return None;
        }
        j -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(files: &[(&str, &str)]) -> Vec<String> {
        let tokenized: Vec<(PathBuf, Vec<Token>)> = files
            .iter()
            .map(|(path, source)| (PathBuf::from(path), crate::lint::tokenize(source).0))
            .collect();
        let files: Vec<File> = tokenized
            .iter()
            .map(|(path, tokens)| File { path, tokens })
            .collect();
        unknown_tokens(&files)
            .into_iter()
            .map(|d| format!("{}:{}: {}", d.file.display(), d.line, d.message))
            .collect()
    }

    #[test]
    fn a_misspelled_app_token_is_named_with_the_nearest() {
        let found = lint(&[(
            "app.luau",
            r##"ui.theme.tokens { app = { card = "#20242c", gap = 12 } }
return ui.column { bg = "$app.crad", gap = "$app.gap" }"##,
        )]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("$app.crad") && found[0].contains("did you mean $app.card?"),
            "{found:?}"
        );
    }

    #[test]
    fn declared_and_standard_tokens_pass_across_files() {
        let found = lint(&[
            (
                "tokens.luau",
                r##"ui.theme.tokens { app = { card = "#20242c" }, ["app.ring"] = "{text.primary}" }"##,
            ),
            (
                "view.luau",
                r##"return ui.row { bg = "$app.card", border_color = "$app.ring", color = "$text.primary" }"##,
            ),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_token_of_the_wrong_kind_is_named() {
        let found = lint(&[(
            "app.luau",
            r##"ui.theme.tokens { app = { accent = "#3b82f6" } }
return ui.row { p = "$app.accent", bg = "$app.accent" }"##,
        )]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("colour token, and `p` takes a number"),
            "{found:?}"
        );
    }

    #[test]
    fn a_computed_group_is_open() {
        let found = lint(&[(
            "app.luau",
            r##"ui.theme.tokens { app = { card = "#20242c", [series] = "#fff" }, chart = palette() }
return ui.row { bg = "$chart.line", color = "$app.anything", border_color = "$apps.card" }"##,
        )]);
        // `chart` is built at run time and `app` has a computed key: their
        // names pass; `apps` is neither.
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("$apps.card") && found[0].contains("did you mean $app.card?"),
            "{found:?}"
        );
    }

    #[test]
    fn a_table_built_at_run_time_opens_every_name() {
        let found = lint(&[(
            "app.luau",
            r##"ui.theme.tokens(defaults)
return ui.row { bg = "$app.crad" }"##,
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_registered_theme_setting_an_undeclared_name_is_named() {
        let found = lint(&[(
            "app.luau",
            r##"ui.theme.tokens { app = { card = "#20242c" } }
ui.theme.register { name = "Glass", tokens = { app = { card = "#000", crad = "#111" } } }"##,
        )]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("`app.crad`, which nothing declares"),
            "{found:?}"
        );
    }

    #[test]
    fn text_that_only_looks_like_a_token_is_left_alone() {
        let found = lint(&[(
            "app.luau",
            r##"return ui.column { ui.text { "$total" }, label("$5"), title = "Price: $9" }"##,
        )]);
        assert!(found.is_empty(), "{found:?}");
    }
}
