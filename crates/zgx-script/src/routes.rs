//! The routes an app's literal `ui.router { routes = { … } }` tables
//! declare: their names and the `:params` their paths name. `zgx types`
//! writes them into `RouteName`, `Navigate` and `RouteCases`, so a
//! misspelled route or param is a type error; the `unknown-route` lint
//! names a literal route no router declares where the types can't (a case
//! of `view`, an app past the union limit, an unchecked script).
use crate::check::{Diagnostic, Severity};
use crate::lint::{Kind, Token};
use crate::token_lint::{File, called, closing, distance, literal, next_field};
use std::collections::BTreeMap;

/// What an app's routers declare.
#[derive(Default)]
pub(crate) struct Routes {
    /// Each route's name and its path's params, by name.
    pub(crate) routes: BTreeMap<String, Vec<String>>,
    /// A router whose routes can't be read (not a literal table, a computed
    /// name or path): its names are unknown, so none is checked.
    pub(crate) open: bool,
}

/// A path's params: its `:name` segments, as the router matches them.
fn params(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| segment.strip_prefix(':'))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The routes every `ui.router` in `files` declares.
pub(crate) fn declared(files: &[File]) -> Routes {
    let mut out = Routes::default();
    for file in files {
        let t = file.tokens;
        for i in 0..t.len() {
            if called(t, i).as_deref() != Some("ui.router") {
                continue;
            }
            let arg = if t[i].kind == Kind::Punct && t[i].text == "(" {
                i + 1
            } else {
                i
            };
            match t.get(arg) {
                Some(open) if open.kind == Kind::Punct && open.text == "{" => {
                    read_router(t, arg, &mut out)
                }
                _ => out.open = true,
            }
        }
    }
    out
}

/// The `routes` of the router spec opened at `open`.
fn read_router(t: &[Token], open: usize, out: &mut Routes) {
    let end = closing(t, open);
    let mut j = open + 1;
    while j + 2 < end {
        if t[j].kind == Kind::Name && t[j].text == "routes" && t[j + 1].text == "=" {
            match t[j + 2].kind == Kind::Punct && t[j + 2].text == "{" {
                true => read_routes(t, j + 2, out),
                false => out.open = true,
            }
            return;
        }
        j = next_field(t, j, end);
    }
    // No `routes` field: nothing to read.
    out.open = true;
}

/// The `name = "path"` fields of the table opened at `open`.
fn read_routes(t: &[Token], open: usize, out: &mut Routes) {
    let end = closing(t, open);
    let mut j = open + 1;
    while j < end {
        let named = if t[j].kind == Kind::Name && t.get(j + 1).is_some_and(|t| t.text == "=") {
            Some((t[j].text.clone(), j + 2))
        } else if t[j].text == "["
            && let Some(key) = t.get(j + 1).and_then(literal)
            && t.get(j + 2).is_some_and(|t| t.text == "]")
            && t.get(j + 3).is_some_and(|t| t.text == "=")
        {
            Some((key.to_owned(), j + 4))
        } else {
            None
        };
        // The path a plain string, and the field ends after it.
        let path = named.as_ref().and_then(|(_, at)| {
            let path = t.get(*at).and_then(literal)?;
            t.get(at + 1)
                .is_some_and(|t| t.text == "," || t.text == ";" || t.text == "}")
                .then_some(path)
        });
        match (named, path) {
            (Some((name, _)), Some(path)) => {
                out.routes.entry(name).or_insert_with(|| params(path));
            }
            _ => out.open = true,
        }
        j = next_field(t, j, end);
    }
}

/// The local names a router is held in (`local router = ui.router { … }`).
fn holders(t: &[Token]) -> Vec<&str> {
    let mut out = Vec::new();
    for i in 2..t.len() {
        if t[i].kind == Kind::Name
            && t[i].text == "ui"
            && t.get(i + 1).is_some_and(|t| t.text == ".")
            && t.get(i + 2).is_some_and(|t| t.text == "router")
            && t[i - 1].kind == Kind::Punct
            && t[i - 1].text == "="
            && t[i - 2].kind == Kind::Name
        {
            out.push(t[i - 2].text.as_str());
        }
    }
    out
}

/// `unknown-route`: a literal route name given to a router's `navigate`,
/// `replace` or `is`, or a case of its `view`, that no `ui.router`
/// declares. Paths ("/thread/7") aren't names and aren't checked; nothing
/// is while a router's routes can't be read.
pub(crate) fn unknown_routes(files: &[File]) -> Vec<Diagnostic> {
    let known = declared(files);
    if known.open || known.routes.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for file in files {
        let t = file.tokens;
        let holders = holders(t);
        let mut named: Vec<&Token> = Vec::new();
        for i in 0..t.len().saturating_sub(3) {
            // `router.navigate("name"` (or `:navigate`), `router.view({`.
            if !(t[i].kind == Kind::Name
                && holders.contains(&t[i].text.as_str())
                && (t[i + 1].text == "." || t[i + 1].text == ":")
                && t[i + 1].kind == Kind::Punct)
            {
                continue;
            }
            let method = t[i + 2].text.as_str();
            let mut arg = i + 3;
            if t.get(arg)
                .is_some_and(|t| t.kind == Kind::Punct && t.text == "(")
            {
                arg += 1;
            }
            let Some(first) = t.get(arg) else { continue };
            match method {
                "navigate" | "replace" | "is" => {
                    if let Some(name) = literal(first)
                        && !name.starts_with('/')
                    {
                        named.push(first);
                    }
                }
                "view" if first.kind == Kind::Punct && first.text == "{" => {
                    let end = closing(t, arg);
                    let mut j = arg + 1;
                    while j < end {
                        if t[j].kind == Kind::Name && t.get(j + 1).is_some_and(|t| t.text == "=") {
                            named.push(&t[j]);
                        } else if t[j].text == "[" && t.get(j + 1).and_then(literal).is_some() {
                            named.push(&t[j + 1]);
                        }
                        j = next_field(t, j, end);
                    }
                }
                _ => {}
            }
        }
        for token in named {
            let name = literal(token).unwrap_or(&token.text);
            if known.routes.contains_key(name) {
                continue;
            }
            let near = known
                .routes
                .keys()
                .map(|known| (distance(name, known), known))
                .filter(|(d, _)| *d <= 2)
                .min();
            let message = match near {
                Some((_, known)) => {
                    format!("no ui.router declares the route `{name}`; did you mean `{known}`?")
                }
                None => format!("no ui.router declares the route `{name}`"),
            };
            found.push(Diagnostic {
                file: file.path.to_path_buf(),
                line: token.line,
                column: token.column,
                severity: Severity::Warning,
                code: "unknown-route".into(),
                message,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn lint(sources: &[&str]) -> Vec<String> {
        let tokens: Vec<_> = sources.iter().map(|s| crate::lint::tokenize(s).0).collect();
        let files: Vec<_> = tokens
            .iter()
            .map(|tokens| File {
                path: Path::new("app.luau"),
                tokens,
            })
            .collect();
        unknown_routes(&files)
            .into_iter()
            .map(|d| format!("{}:{} {}", d.line, d.column, d.message))
            .collect()
    }
    const ROUTER: &str =
        "local router = ui.router { routes = { list = \"/\", thread = \"/thread/:id\" } }\n";

    #[test]
    fn a_route_no_router_declares_is_named_with_the_nearest() {
        let found = lint(&[&format!(
            "{ROUTER}router.navigate(\"thraed\", {{ id = 1 }})\nrouter:replace(\"list\")\n\
             return router.view({{ lsit = ui.text {{ \"x\" }}, thread = ui.text {{ \"y\" }} }})\n"
        )]);
        assert_eq!(
            found,
            [
                "2:17 no ui.router declares the route `thraed`; did you mean `thread`?",
                "4:22 no ui.router declares the route `lsit`; did you mean `list`?",
            ]
        );
    }

    #[test]
    fn paths_other_receivers_and_unread_routers_are_left_alone() {
        // A path is no name; `other` holds no router.
        assert!(
            lint(&[&format!(
                "{ROUTER}router.navigate(\"/thread/7\")\nother.navigate(\"nowhere\")\n"
            )])
            .is_empty()
        );
        // Routes made at run time: no name is known.
        assert!(
            lint(&["local r = ui.router { routes = make() }\nr.navigate(\"anything\")\n"])
                .is_empty()
        );
        // Declared in one script, used in another.
        assert!(lint(&[ROUTER, "router.navigate(\"list\")\n"]).is_empty());
    }

    #[test]
    fn a_paths_params_are_its_colon_segments() {
        assert_eq!(params("/thread/:id/reply/:n"), ["id", "n"]);
        assert!(params("/").is_empty());
    }
}
