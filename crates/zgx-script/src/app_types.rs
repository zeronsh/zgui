//! `zgx types <dir>`: zgx's definitions with an app's own theme tokens in
//! them, for editors to complete and check them.
//!
//! It writes `<dir>/.zgx/types.d.luau`: `zgx.d.luau` with `Tokens` (what
//! `ui.tokens` holds) grown by the names the app declares with literal
//! `ui.theme.tokens { … }` tables, and those names in `ui.theme.get`'s.
//! It is all of zgx's definitions, not a second file beside them:
//! luau-lsp reads a second file's types too late or too early for
//! `zgx.d.luau` to name them. An editor lists it in place of
//! `zgx.d.luau`; `zgx check` writes it and checks with it.
//!
//! `ui.tokens.notes.wash` is a field path, so a typo is a type error and
//! the editor completes the next name; a value is a plain `string`, so
//! props stay `string` (a union of 100 token names in `Color` made the
//! solver report false errors and take twice as long). `"$notes.wash"`
//! strings stay valid, and the `unknown-token` lint checks them.
use crate::check::{CheckError, DEFINITIONS, scripts};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the types go in an app's directory.
pub fn types_file(dir: &Path) -> PathBuf {
    dir.join(".zgx").join("types.d.luau")
}

/// A group of `ui.tokens`, as its type is written.
#[derive(Default)]
struct Group {
    /// Its tokens and groups, by the part of the name.
    members: BTreeMap<String, Member>,
    /// Holds names the lint can't read: any field is allowed.
    open: bool,
}
enum Member {
    Token,
    Group(Group),
}
impl Group {
    fn insert(&mut self, name: &str) {
        let (part, rest) = match name.split_once('.') {
            Some((part, rest)) => (part, Some(rest)),
            None => (name, None),
        };
        match rest {
            None => {
                match self.members.get_mut(part) {
                    // A group that is a token too: its own as `value`.
                    Some(Member::Group(group)) => group.insert("value"),
                    _ => {
                        self.members.insert(part.to_owned(), Member::Token);
                    }
                }
            }
            Some(rest) => {
                let member = self
                    .members
                    .entry(part.to_owned())
                    .or_insert_with(|| Member::Group(Group::default()));
                if matches!(member, Member::Token) {
                    let mut group = Group::default();
                    group.insert("value");
                    *member = Member::Group(group);
                }
                if let Member::Group(group) = member {
                    group.insert(rest);
                }
            }
        }
    }
    fn open(&mut self, name: &str) {
        if name.is_empty() {
            self.open = true;
            return;
        }
        let (part, rest) = name.split_once('.').unwrap_or((name, ""));
        let member = self
            .members
            .entry(part.to_owned())
            .or_insert_with(|| Member::Group(Group::default()));
        if matches!(member, Member::Token) {
            let mut group = Group::default();
            group.insert("value");
            *member = Member::Group(group);
        }
        if let Member::Group(group) = member {
            group.open(rest);
        }
    }
    /// Its type: fields by name; numbers as an array's (`space[3]`).
    fn write(&self, depth: usize, out: &mut String) {
        let numbered = self.members.keys().filter(|k| is_index(k)).count();
        if numbered > 0 && numbered == self.members.len() && !self.open {
            out.push_str("{ string }");
            return;
        }
        let indent = "\t".repeat(depth + 1);
        out.push_str("{\n");
        for (key, member) in self.members.iter().filter(|(k, _)| !is_index(k)) {
            out.push_str(&indent);
            // A keyword (`end`) is a field only quoted.
            match KEYWORDS.contains(&key.as_str()) {
                true => out.push_str(&format!("[\"{key}\"]")),
                false => out.push_str(key),
            }
            out.push_str(": ");
            match member {
                Member::Token => out.push_str("string"),
                Member::Group(group) => group.write(depth + 1, out),
            }
            out.push_str(",\n");
        }
        if self.open {
            out.push_str(&format!("{indent}[string]: any,\n"));
        } else if numbered > 0 {
            out.push_str(&format!("{indent}[number]: string,\n"));
        }
        out.push_str(&"\t".repeat(depth));
        out.push('}');
    }
}
/// Luau's reserved words: no field is named one bare.
const KEYWORDS: [&str; 21] = [
    "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until",
];
fn is_index(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())
}
/// Whether a name can be a Luau field (`notes.wash`); others (`"my-card"`)
/// stay reachable as `"$…"` strings only.
fn is_field_path(name: &str) -> bool {
    name.split('.').all(|part| {
        is_index(part)
            || (part.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
    })
}

/// `Tokens`: the type of `ui.tokens` for zgx's standard tokens and
/// `extra` names, `open` groups taking any field.
fn tokens_type(extra: &[String], open: &[String], all_open: bool) -> String {
    let mut root = Group::default();
    for (_, name, _) in zgx::theme::standard() {
        root.insert(name);
    }
    for name in extra.iter().filter(|name| is_field_path(name)) {
        root.insert(name);
    }
    for group in open {
        root.open(group);
    }
    root.open |= all_open;
    let mut out = String::from("export type Tokens = ");
    root.write(0, &mut out);
    out.push('\n');
    out
}
/// `Tokens` for zgx's standard tokens alone, as `zgx.d.luau` has it.
pub fn standard_tokens_type() -> String {
    tokens_type(&[], &[], false)
}

/// zgx's definitions with the tokens an app's scripts (`sources`) declare.
pub fn definitions(sources: &[(PathBuf, String)]) -> String {
    let read: Vec<_> = sources
        .iter()
        .map(|(path, source)| (path, crate::lint::tokenize(source).0))
        .collect();
    let files: Vec<_> = read
        .iter()
        .map(|(path, tokens)| crate::token_lint::File { path, tokens })
        .collect();
    let declared = crate::token_lint::declared(&files);
    let names: Vec<String> = declared
        .names
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let standard = standard_tokens_type();
    let ours = tokens_type(&names, &declared.open, declared.all_open);
    assert!(
        DEFINITIONS.contains(&standard),
        "zgx.d.luau's `Tokens` follows the schema"
    );
    let mut text = DEFINITIONS.replacen(&standard, &ours, 1);
    if !names.is_empty() {
        let app: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
        text = text.replacen(
            "get: (name: TokenName | string) -> any,",
            &format!(
                "get: (name: TokenName | {} | string) -> any,",
                app.join(" | ")
            ),
            1,
        );
    }
    format!(
        "-- zgx's definitions with this app's own theme tokens (`ui.theme.tokens`).\n\
         -- Written by `zgx types` (and `zgx check`, and the debug host after a\n\
         -- reload that changes the names): don't edit it, and leave it out of\n\
         -- version control. Editors list it in place of zgx.d.luau.\n{text}"
    )
}

/// Write the definitions of the app in `dir` to `types_file(dir)`, only
/// when they changed (editors watching it don't reload for nothing);
/// whether it did.
pub fn write_types(dir: &Path) -> Result<bool, CheckError> {
    let mut sources = Vec::new();
    for file in scripts(dir) {
        let source = std::fs::read_to_string(&file).map_err(CheckError::Io)?;
        sources.push((file, source));
    }
    write_types_of(dir, &sources)
}

/// As `write_types`, from sources already read.
pub fn write_types_of(dir: &Path, sources: &[(PathBuf, String)]) -> Result<bool, CheckError> {
    let text = definitions(sources);
    let path = types_file(dir);
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    std::fs::create_dir_all(path.parent().expect("a directory")).map_err(CheckError::Io)?;
    std::fs::write(&path, text).map_err(CheckError::Io)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(source: &str) -> String {
        definitions(&[(PathBuf::from("app.luau"), source.into())])
    }

    #[test]
    fn zgxs_definitions_have_the_standard_tokens_as_fields() {
        assert!(
            DEFINITIONS.contains(&standard_tokens_type()),
            "zgx.d.luau's `Tokens` is not the schema's: write it from `standard_tokens_type`"
        );
    }

    #[test]
    fn a_token_named_by_a_keyword_is_a_quoted_field() {
        let types = app(r##"ui.theme.tokens { fade = { ["end"] = "#000", start = "#fff" } }"##);
        assert!(
            types.contains("\tfade: {\n\t\t[\"end\"]: string,\n\t\tstart: string,\n\t},\n"),
            "{types}"
        );
    }

    #[test]
    fn an_apps_tokens_join_zgxs_as_fields() {
        let types = app(
            r##"ui.theme.tokens { notes = { wash = "#20242c", card = { value = "#fff" } }, ["app.gap"] = 12 }"##,
        );
        assert!(
            types.contains(
                "\tnotes: {\n\t\tcard: {\n\t\t\tvalue: string,\n\t\t},\n\t\twash: string,\n\t},\n"
            ),
            "{types}"
        );
        assert!(
            types.contains("\tapp: {\n\t\tgap: string,\n\t},\n"),
            "{types}"
        );
        assert!(types.contains("\tsurface: {\n"), "zgx's stay: {types}");
        assert!(types.contains("get: (name: TokenName | \"app.gap\" | \"notes.card.value\" | \"notes.wash\" | string)"), "{types}");
    }

    #[test]
    fn a_group_built_at_run_time_takes_any_field() {
        let types = app(r##"ui.theme.tokens { chart = palette() }"##);
        assert!(
            types.contains("\tchart: {\n\t\tvalue: string,\n\t\t[string]: any,\n\t},\n"),
            "{types}"
        );
    }

    #[test]
    fn the_file_is_written_only_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("zgx-types-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("app.luau"),
            r##"ui.theme.tokens { app = { card = "#fff" } }"##,
        )
        .unwrap();
        assert!(write_types(&dir).unwrap());
        assert!(
            !write_types(&dir).unwrap(),
            "the same names: left as it was"
        );
        std::fs::write(
            dir.join("app.luau"),
            r##"ui.theme.tokens { app = { card = "#fff", pane = "#000" } }"##,
        )
        .unwrap();
        assert!(write_types(&dir).unwrap());
        assert!(
            std::fs::read_to_string(types_file(&dir))
                .unwrap()
                .contains("pane: string")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
