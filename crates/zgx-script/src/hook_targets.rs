//! What plugins can target in an app (`ui.contribute { target = … }`): the
//! `id`s and `hook` names its scripts declare, read from their tokens as
//! the rest of the lint reads them.
//!
//! `zgx types` names them (`HookName`, `HookTarget`) so a plugin's checker
//! completes a target and reports a typo, and the `unknown-target` lint
//! reports a `#id` or `hook:name` target nothing declares: for an app past
//! luau-lsp's union limit, and for scripts nobody type-checks. Only literal
//! declarations count (`id = "save"`, `hook = "toolbar.save"`); an id built
//! at run time is not known here, as it is not to the type checker.
use crate::check::{Diagnostic, Severity};
use crate::lint::{Kind, Token};
use crate::token_lint::{File, called, distance, literal};
use std::collections::BTreeSet;

/// The ids and hook names an app declares.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Targets {
    pub(crate) ids: BTreeSet<String>,
    pub(crate) hooks: BTreeSet<String>,
}
impl Targets {
    /// As a plugin writes them: `#id` and `hook:name`, sorted.
    pub(crate) fn written(&self) -> Vec<String> {
        let ids = self.ids.iter().map(|id| format!("#{id}"));
        let hooks = self.hooks.iter().map(|hook| format!("hook:{hook}"));
        ids.chain(hooks).collect()
    }
    /// How many there are.
    pub(crate) fn len(&self) -> usize {
        self.ids.len() + self.hooks.len()
    }
}

/// Whether `t[i]` is a field of a table constructor (after `{`, `,` or `;`).
fn in_table(t: &[Token], i: usize) -> bool {
    i >= 1 && t[i - 1].kind == Kind::Punct && matches!(t[i - 1].text.as_str(), "{" | "," | ";")
}

/// The literal `id = "…"` and `hook = "…"` fields of `files`' tables.
pub(crate) fn declared(files: &[File]) -> Targets {
    let mut found = Targets::default();
    for file in files {
        let t = file.tokens;
        for i in 0..t.len().saturating_sub(2) {
            if t[i].kind != Kind::Name
                || !matches!(t[i].text.as_str(), "id" | "hook")
                || !(t[i + 1].kind == Kind::Punct && t[i + 1].text == "=")
                || !in_table(t, i)
            {
                continue;
            }
            let Some(value) = literal(&t[i + 2]).filter(|value| !value.is_empty()) else {
                continue;
            };
            let set = if t[i].text == "id" {
                &mut found.ids
            } else {
                &mut found.hooks
            };
            set.insert(value.to_owned());
        }
    }
    found
}

/// Each `ui.contribute { … }` and `ui.filter { … }` call's literal
/// `target`, with its token.
fn targets(t: &[Token]) -> Vec<(&Token, &str)> {
    let mut out = Vec::new();
    for i in 0..t.len() {
        if !matches!(called(t, i).as_deref(), Some("ui.contribute" | "ui.filter")) {
            continue;
        }
        // Its table: after a `(`, or given without one.
        let open = if t[i].text == "(" { i + 1 } else { i };
        if !t
            .get(open)
            .is_some_and(|t| t.kind == Kind::Punct && t.text == "{")
        {
            continue;
        }
        let mut depth = 0;
        for j in open..t.len() {
            if t[j].kind == Kind::Punct {
                match t[j].text.as_str() {
                    "{" | "(" | "[" => depth += 1,
                    "}" | ")" | "]" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if depth == 1
                && t[j].kind == Kind::Name
                && t[j].text == "target"
                && t.get(j + 1).is_some_and(|t| t.text == "=")
                && in_table(t, j)
                && let Some(value) = t.get(j + 2).and_then(literal)
            {
                out.push((&t[j + 2], value));
            }
        }
    }
    out
}

/// `unknown-target`: a `#id` or `hook:name` target no script declares.
/// Selectors by role or name (`ui.selector(…)`) are not checked.
pub(crate) fn unknown_targets(files: &[File]) -> Vec<Diagnostic> {
    let known = declared(files);
    let mut found = Vec::new();
    for file in files {
        for (token, target) in targets(file.tokens) {
            let (names, name, prefix, what) = if let Some(id) = target.strip_prefix('#') {
                (&known.ids, id, "#", "id")
            } else if let Some(hook) = target.strip_prefix("hook:") {
                (&known.hooks, hook, "hook:", "hook")
            } else {
                continue;
            };
            if names.contains(name) {
                continue;
            }
            let near = names
                .iter()
                .map(|known| (distance(name, known), known))
                .filter(|(d, _)| *d <= 2)
                .min();
            let message = match near {
                Some((_, known)) => format!(
                    "`{target}`: no element declares {what} = \"{name}\"; did you mean `{prefix}{known}`?"
                ),
                None => format!("`{target}`: no element declares {what} = \"{name}\""),
            };
            found.push(Diagnostic {
                file: file.path.to_path_buf(),
                line: token.line,
                column: token.column,
                severity: Severity::Warning,
                code: "unknown-target".into(),
                message,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn files(sources: &[&str]) -> (Vec<PathBuf>, Vec<Vec<Token>>) {
        let paths = (0..sources.len())
            .map(|i| PathBuf::from(format!("{i}.luau")))
            .collect();
        let tokens = sources.iter().map(|s| crate::lint::tokenize(s).0).collect();
        (paths, tokens)
    }
    fn with<R>(sources: &[&str], f: impl FnOnce(&[File]) -> R) -> R {
        let (paths, tokens) = files(sources);
        let files: Vec<_> = paths
            .iter()
            .zip(&tokens)
            .map(|(path, tokens)| File {
                path: Path::new(path),
                tokens,
            })
            .collect();
        f(&files)
    }

    #[test]
    fn literal_ids_and_hooks_are_declared_and_others_are_not() {
        let found = with(
            &[r#"
            local id = "not-a-field"
            return ui.row { id = "toolbar", hook = "toolbar.main",
                ui.button { id = "save", hook = "toolbar.save", label = "Save" },
                ui.text { id = name, hook = false, "x" } }"#],
            declared,
        );
        assert_eq!(
            found.written(),
            [
                "#save",
                "#toolbar",
                "hook:toolbar.main",
                "hook:toolbar.save"
            ]
        );
    }

    #[test]
    fn a_target_nothing_declares_is_reported_with_the_nearest() {
        let diagnostics = with(
            &[
                r#"return ui.button { id = "save", hook = "toolbar.save", label = "Save" }"#,
                r##"ui.contribute { target = "#sav", before = ui.text { "x" } }
                ui.contribute { target = "hook:toolbar.save", after = ui.text { "y" } }
                ui.contribute({ target = "hook:toolbar.svae", after = ui.text { "z" } })
                ui.contribute { target = ui.selector('role:button "Save"'), after = ui.text { "w" } }
                ui.contribute("slot-name", ui.text { "v" })
                ui.filter { target = "#saev", text = function(t) return t end }"##,
            ],
            unknown_targets,
        );
        let messages: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "`#sav`: no element declares id = \"sav\"; did you mean `#save`?",
                "`hook:toolbar.svae`: no element declares hook = \"toolbar.svae\"; did you mean `hook:toolbar.save`?",
                "`#saev`: no element declares id = \"saev\"; did you mean `#save`?",
            ]
        );
        assert_eq!(diagnostics[0].line, 1);
        assert_eq!(diagnostics[1].line, 3);
    }
}
