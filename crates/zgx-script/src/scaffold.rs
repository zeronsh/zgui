//! `zgx new <name>`: a Luau app that runs at once (`zgx run <name>`), its
//! checks clean (`zgx check`, `zgx lint`, `zgx audit`, `zgx test`) and a
//! CI workflow running them. Each template is a trimmed copy of an example,
//! built into the binary (`templates/`); `{{name}}`, `{{title}}`,
//! `{{template}}` and `{{version}}` are filled in as the files are written.
//! Nothing else runs: no cargo, no network, no git.
use std::path::{Path, PathBuf};

/// The templates, the first the default, with what each shows.
pub const TEMPLATES: &[(&str, &str)] = &[
    (
        "basic",
        "a task list: state, a component, the theme and a test",
    ),
    (
        "router",
        "notes with routes: a list, a note, settings as a modal route",
    ),
    (
        "list",
        "a long list paged as it scrolls, searched as you type, reorderable favourites",
    ),
    (
        "form",
        "a profile form: validation, dirty tracking, save, revert and a toast",
    ),
    (
        "plugins",
        "tasks made to be extended: a plugin adds, filters and calls the app's action",
    ),
];

/// A file: where it goes in the app, and its text.
type Files = &'static [(&'static str, &'static str)];

/// What every template has.
const COMMON: Files = &[
    ("zgx.json", include_str!("../templates/common/zgx.json")),
    ("theme.luau", include_str!("../templates/common/theme.luau")),
    (".luaurc", include_str!("../templates/common/luaurc")),
    (".gitignore", include_str!("../templates/common/gitignore")),
    ("README.md", include_str!("../templates/common/README.md")),
    (
        ".github/workflows/zgx.yml",
        include_str!("../templates/common/workflow.yml"),
    ),
];

/// A template's own files, and those `--i18n` puts in their place (or
/// adds): its text as `ui.t` messages, and their English source.
fn files(template: &str) -> Option<(Files, Files)> {
    Some(match template {
        "basic" => (
            &[
                ("app.luau", include_str!("../templates/basic/app.luau")),
                (
                    "components/task_row.luau",
                    include_str!("../templates/basic/components/task_row.luau"),
                ),
                (
                    "tests/app.test.luau",
                    include_str!("../templates/basic/tests/app.test.luau"),
                ),
            ],
            &[
                ("app.luau", include_str!("../templates/basic/i18n/app.luau")),
                (
                    "locales/en.json",
                    include_str!("../templates/basic/i18n/en.json"),
                ),
            ],
        ),
        "router" => (
            &[
                ("app.luau", include_str!("../templates/router/app.luau")),
                (
                    "components/toolbar.luau",
                    include_str!("../templates/router/components/toolbar.luau"),
                ),
                (
                    "tests/app.test.luau",
                    include_str!("../templates/router/tests/app.test.luau"),
                ),
            ],
            &[
                (
                    "app.luau",
                    include_str!("../templates/router/i18n/app.luau"),
                ),
                (
                    "components/toolbar.luau",
                    include_str!("../templates/router/i18n/toolbar.luau"),
                ),
                (
                    "locales/en.json",
                    include_str!("../templates/router/i18n/en.json"),
                ),
            ],
        ),
        "list" => (
            &[
                ("app.luau", include_str!("../templates/list/app.luau")),
                ("people.luau", include_str!("../templates/list/people.luau")),
                (
                    "components/person_row.luau",
                    include_str!("../templates/list/components/person_row.luau"),
                ),
                (
                    "tests/app.test.luau",
                    include_str!("../templates/list/tests/app.test.luau"),
                ),
            ],
            &[
                ("app.luau", include_str!("../templates/list/i18n/app.luau")),
                (
                    "locales/en.json",
                    include_str!("../templates/list/i18n/en.json"),
                ),
            ],
        ),
        "form" => (
            &[
                ("app.luau", include_str!("../templates/form/app.luau")),
                (
                    "components/field.luau",
                    include_str!("../templates/form/components/field.luau"),
                ),
                (
                    "tests/app.test.luau",
                    include_str!("../templates/form/tests/app.test.luau"),
                ),
            ],
            &[
                ("app.luau", include_str!("../templates/form/i18n/app.luau")),
                (
                    "locales/en.json",
                    include_str!("../templates/form/i18n/en.json"),
                ),
            ],
        ),
        "plugins" => (
            &[
                ("zgx.json", include_str!("../templates/plugins/zgx.json")),
                ("app.luau", include_str!("../templates/plugins/app.luau")),
                (
                    "components/task_row.luau",
                    include_str!("../templates/plugins/components/task_row.luau"),
                ),
                (
                    "plugins/focus/init.luau",
                    include_str!("../templates/plugins/plugins/focus/init.luau"),
                ),
                (
                    "tests/app.test.luau",
                    include_str!("../templates/plugins/tests/app.test.luau"),
                ),
            ],
            &[
                (
                    "app.luau",
                    include_str!("../templates/plugins/i18n/app.luau"),
                ),
                (
                    "locales/en.json",
                    include_str!("../templates/plugins/i18n/en.json"),
                ),
            ],
        ),
        _ => return None,
    })
}

/// Why an app wasn't made.
#[derive(Debug)]
pub enum NewError {
    /// The name isn't letters, digits, `-` and `_`.
    Name(String),
    /// No such template.
    Template(String),
    /// The directory holds something already.
    NotEmpty(PathBuf),
    /// A file couldn't be written.
    Io(PathBuf, std::io::Error),
}
impl std::fmt::Display for NewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewError::Name(name) => write!(
                f,
                "`{name}` can't name an app: use letters, digits, `-` and `_`, starting with a letter"
            ),
            NewError::Template(template) => {
                let names: Vec<&str> = TEMPLATES.iter().map(|(name, _)| *name).collect();
                write!(
                    f,
                    "no template `{template}` (there are {})",
                    names.join(", ")
                )
            }
            NewError::NotEmpty(dir) => write!(f, "{} isn't empty", dir.display()),
            NewError::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}
impl std::error::Error for NewError {}

/// The title a name reads as: `my-notes` is "My Notes".
pub fn title(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Writes the app `template` (with its text as `ui.t` messages if `i18n`)
/// into `dir`, which must be empty or not exist, named by its last
/// component; then its editor types (`.zgx/types.d.luau`). The files
/// written, in order.
pub fn create(dir: &Path, template: &str, i18n: bool) -> Result<Vec<PathBuf>, NewError> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(NewError::Name(name));
    }
    let (own, translated) = files(template).ok_or_else(|| NewError::Template(template.into()))?;
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |error: std::io::Error| NewError::Io(path, error)
    };
    if dir.exists() {
        let mut entries = std::fs::read_dir(dir).map_err(io(dir))?;
        if entries.next().is_some() {
            return Err(NewError::NotEmpty(dir.to_path_buf()));
        }
    }
    let title = title(&name);
    let fill = |text: &str| {
        text.replace("{{name}}", &name)
            .replace("{{title}}", &title)
            .replace("{{template}}", template)
            .replace("{{version}}", env!("CARGO_PKG_VERSION"))
    };
    // A template's own file takes a common one's place (the plugins
    // template's zgx.json), as a translated one takes its own's.
    let mut chosen: Vec<(&str, &str)> = COMMON.to_vec();
    let layers: &[Files] = if i18n { &[own, translated] } else { &[own] };
    for &(path, text) in layers.iter().copied().flatten() {
        match chosen.iter_mut().find(|(at, _)| *at == path) {
            Some(file) => file.1 = text,
            None => chosen.push((path, text)),
        }
    }
    let mut written = Vec::new();
    for (path, text) in chosen {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        std::fs::write(&path, fill(text)).map_err(io(&path))?;
        written.push(path);
    }
    crate::app_types::write_types(dir).map_err(|error| {
        NewError::Io(
            crate::app_types::types_file(dir),
            std::io::Error::other(error.to_string()),
        )
    })?;
    written.push(crate::app_types::types_file(dir));
    Ok(written)
}
