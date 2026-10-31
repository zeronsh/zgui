//! Type checks scripts before they run (`zgx check app.luau`): luau-lsp's
//! `analyze` with the bundled `ui`/`anime` definitions, the standard platform
//! (`require` by path), the new type solver and strict mode unless a
//! `.luaurc` or a `--!` comment says otherwise. It catches what otherwise
//! shows only at run time: a misspelled property or event, a call with the
//! wrong arguments, a local function used before its definition (`nil`
//! until called).
use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

/// The `ui` and `anime` globals' types, as shipped with this crate.
pub const DEFINITIONS: &str = include_str!("../types/zgx.d.luau");

/// How to get luau-lsp, shown when it is missing.
pub const INSTALL: &str = "luau-lsp is not installed. Download it from \
https://github.com/JohnnyMorganz/luau-lsp/releases and put `luau-lsp` on your PATH \
(or install it with a toolchain manager: `rokit add --global JohnnyMorganz/luau-lsp`), \
or point LUAU_LSP at the binary.";

/// How bad a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// A type or syntax error.
    Error,
    /// A lint, such as an unused variable.
    Warning,
}

/// One finding, at a 1-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The script it is in.
    pub file: PathBuf,
    /// Its line, from 1.
    pub line: u32,
    /// Its column, from 1.
    pub column: u32,
    /// Whether it is an error or a warning.
    pub severity: Severity,
    /// luau-lsp's name for it: `TypeError`, `SyntaxError`, `LocalUnused`…
    pub code: String,
    /// What it says.
    pub message: String,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(
            f,
            "{}:{}:{}: {severity}[{}]: {}",
            self.file.display(),
            self.line,
            self.column,
            self.code,
            self.message
        )
    }
}

/// What checking scripts found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many scripts were checked.
    pub files: usize,
    /// What was found.
    pub diagnostics: Vec<Diagnostic>,
}
impl Report {
    /// How many errors it found.
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }
    /// How many warnings it found.
    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }
    fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }
}

/// Why scripts could not be checked.
#[derive(Debug)]
pub enum CheckError {
    /// luau-lsp is not on the PATH (see [`INSTALL`]).
    NotInstalled,
    /// A path that does not exist, or holds no `.luau` files.
    NoScripts(PathBuf),
    /// Running luau-lsp failed.
    Io(std::io::Error),
}
impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::NotInstalled => f.write_str(INSTALL),
            CheckError::NoScripts(path) => write!(f, "{}: no .luau scripts here", path.display()),
            CheckError::Io(e) => write!(f, "running luau-lsp: {e}"),
        }
    }
}
impl std::error::Error for CheckError {}

/// How scripts are checked.
#[derive(Clone, Debug)]
pub struct Options {
    /// Strict mode where no `.luaurc` (or `--!` comment) picks a mode.
    pub strict: bool,
    /// luau-lsp itself; found on the PATH (or `LUAU_LSP`) when `None`.
    pub luau_lsp: Option<PathBuf>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            strict: true,
            luau_lsp: None,
        }
    }
}

/// `LUAU_LSP`, or `luau-lsp` on the PATH.
pub fn find_luau_lsp() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LUAU_LSP").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let name = if cfg!(windows) {
        "luau-lsp.exe"
    } else {
        "luau-lsp"
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The `.luau` scripts at `path`: the file itself, or those under the
/// directory (skipping hidden directories), sorted.
pub fn scripts(path: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if path.is_dir() {
                if !hidden {
                    walk(&path, out);
                }
            } else if path
                .extension()
                .is_some_and(|ext| ext == "luau" || ext == "lua")
                && !path.to_string_lossy().ends_with(".d.luau")
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    if path.is_dir() {
        walk(path, &mut out);
    } else if path.is_file() {
        out.push(path.to_path_buf());
    }
    out.sort();
    out
}

/// Type checks the scripts at `paths` (files or directories).
pub fn check(paths: &[PathBuf], options: &Options) -> Result<Report, CheckError> {
    let lsp = options
        .luau_lsp
        .clone()
        .or_else(find_luau_lsp)
        .ok_or(CheckError::NotInstalled)?;
    let mut files = Vec::new();
    let mut sources = Vec::new();
    for path in paths {
        let found = scripts(path);
        if found.is_empty() {
            return Err(CheckError::NoScripts(path.clone()));
        }
        let mut own = Vec::new();
        for file in &found {
            own.push((
                file.clone(),
                std::fs::read_to_string(file).map_err(CheckError::Io)?,
            ));
        }
        // An app's directory: its tokens' types, for its editor too.
        if path.is_dir() {
            crate::app_types::write_types_of(path, &own)?;
        }
        sources.extend(own);
        files.extend(found);
    }
    // The definitions and the default configuration, written where
    // luau-lsp can read them; one directory per run, so checks in parallel
    // (a hot-reloading host's, a terminal's) do not race.
    let dir = std::env::temp_dir().join(format!(
        "zgx-check-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(CheckError::Io)?;
    let _cleanup = Cleanup(dir.clone());
    let definitions = dir.join("zgx.d.luau");
    // zgx's, with the apps' own tokens in them (`app_types`).
    std::fs::write(&definitions, crate::app_types::definitions(&sources))
        .map_err(CheckError::Io)?;
    // An app whose scripts require `@app/…` (its own modules, which a
    // plugin may replace) or `@app-original/…` (a replacement's original)
    // is checked with both aliases naming its directory: the nearest
    // holding an `app.luau`. All its scripts are, since luau-lsp checks a
    // module again with each script requiring it (a component using
    // `@app/theme`, required by an app.luau that doesn't). A `.luaurc`
    // cannot name them (zgx resolves them).
    // Test files (`*.test.luau`) are checked apart, with `zgx test`'s
    // globals (`test`, `mount`, `expect`) beside zgx's: the app's own
    // scripts don't see them.
    let aliased: std::collections::HashSet<PathBuf> = files
        .iter()
        .zip(&sources)
        .filter(|(_, (_, code))| code.contains("\"@app/") || code.contains("\"@app-original/"))
        .filter_map(|(file, _)| app_root(file))
        .collect();
    let mut groups: Vec<(Option<PathBuf>, bool, Vec<PathBuf>)> = Vec::new();
    for file in &files {
        let root = app_root(file).filter(|root| aliased.contains(root));
        let test = crate::test_runner::is_test(file);
        match groups
            .iter_mut()
            .find(|(at, tests, _)| *at == root && *tests == test)
        {
            Some((_, _, group)) => group.push(file.clone()),
            None => groups.push((root, test, vec![file.clone()])),
        }
    }
    let test_definitions = dir.join("zgx-test.d.luau");
    if groups.iter().any(|(_, test, _)| *test) {
        std::fs::write(&test_definitions, crate::test_runner::DEFINITIONS)
            .map_err(CheckError::Io)?;
    }
    let analyze = |files: &[PathBuf],
                   app: Option<&Path>,
                   tests: bool,
                   at: usize|
     -> Result<String, CheckError> {
        let mut command = Command::new(&lsp);
        command
            .arg("analyze")
            .arg("--platform=standard")
            .arg("--flag:LuauSolverV2=true")
            .arg(format!("--definitions={}", definitions.display()))
            .arg("--formatter=gnu");
        if tests {
            command.arg(format!("--definitions={}", test_definitions.display()));
        }
        let mut base = serde_json::Map::new();
        if options.strict {
            base.insert("languageMode".into(), "strict".into());
        }
        if let Some(app) = app {
            let app = app.canonicalize().unwrap_or_else(|_| app.to_path_buf());
            let app = serde_json::Value::from(app.to_string_lossy().into_owned());
            base.insert(
                "aliases".into(),
                serde_json::json!({ "app": app, "app-original": app }),
            );
        }
        if !base.is_empty() {
            let file = dir.join(format!("base-{at}.luaurc"));
            std::fs::write(&file, serde_json::Value::Object(base).to_string())
                .map_err(CheckError::Io)?;
            command.arg(format!("--base-luaurc={}", file.display()));
        }
        let output = command.args(files).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CheckError::NotInstalled
            } else {
                CheckError::Io(e)
            }
        })?;
        Ok(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    };
    let mut text = String::new();
    for (at, (root, tests, group)) in groups.iter().enumerate() {
        text.push_str(&analyze(group, root.as_deref(), *tests, at)?);
    }
    let mut diagnostics = parse(&text);
    // luau-lsp names files relative to the working directory; show them
    // as they were given.
    let given: std::collections::HashMap<PathBuf, &PathBuf> = files
        .iter()
        .filter_map(|f| Some((f.canonicalize().ok()?, f)))
        .collect();
    for d in &mut diagnostics {
        if let Ok(full) = d.file.canonicalize()
            && let Some(file) = given.get(&full)
        {
            d.file = (*file).clone();
        }
    }
    diagnostics.retain(|d| !templated(d));
    // A module's findings come again with each file that requires it.
    let mut seen = std::collections::HashSet::new();
    diagnostics.retain(|d| seen.insert(d.to_string()));
    Ok(Report {
        files: files.len(),
        diagnostics,
    })
}
/// The app `file` belongs to: the nearest directory holding an `app.luau`.
fn app_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join("app.luau").is_file())
        .map(Path::to_path_buf)
}
static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// luau-lsp's `--formatter=gnu` output: `file:line.col-line.col: Code:
/// message`, a message's further lines following it. Its own log lines
/// (`[INFO] …`) are dropped.
pub fn parse(text: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut continuing = false;
    for line in text.lines() {
        if line.starts_with('[') && line.contains(']') {
            continuing = false;
            continue;
        }
        match parse_line(line) {
            Some(d) => {
                out.push(d);
                continuing = true;
            }
            None if continuing && !line.trim().is_empty() => {
                let last = out.last_mut().expect("continuing a diagnostic");
                last.message.push('\n');
                last.message.push_str(line.trim_end());
            }
            None => continuing = false,
        }
    }
    out
}
fn parse_line(line: &str) -> Option<Diagnostic> {
    // `path:12.5-12.9: TypeError: message`; the path may hold colons.
    let (head, message) = line.split_once(": ").and_then(|(head, rest)| {
        // The head ends with `:L.C-L.C`; find the range's colon from the end.
        let colon = head.rfind(':')?;
        let range = &head[colon + 1..];
        let (start, _) = range.split_once('-').unwrap_or((range, ""));
        let (l, c) = start.split_once('.')?;
        let line: u32 = l.parse().ok()?;
        let column: u32 = c.parse().ok()?;
        Some(((&head[..colon], line, column), rest))
    })?;
    let (code, message) = message.split_once(": ")?;
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let severity = if code.ends_with("Error") {
        Severity::Error
    } else {
        Severity::Warning
    };
    Some(Diagnostic {
        file: PathBuf::from(head.0),
        line: head.1,
        column: head.2,
        severity,
        code: code.to_string(),
        message: message.to_string(),
    })
}
/// An "unused" local that a text template uses: `local count =
/// ui.state("count", 0)` with `"{count}"` in a string. Luau sees no use.
fn templated(d: &Diagnostic) -> bool {
    if d.code != "LocalUnused" {
        return false;
    }
    let Some(name) = d
        .message
        .strip_prefix("Variable '")
        .and_then(|rest| rest.split_once('\''))
        .map(|(name, _)| name)
    else {
        return false;
    };
    std::fs::read_to_string(&d.file).is_ok_and(|code| code.contains(&format!("{{{name}}}")))
}

/// A check running on its own thread (a hot-reloading host's): its report
/// is taken once it finishes.
pub struct Background {
    result: Arc<Mutex<Option<Result<Report, CheckError>>>>,
}
impl Background {
    /// Check `paths` with `options` on a thread of its own.
    pub fn start(paths: Vec<PathBuf>, options: Options) -> Self {
        let result = Arc::new(Mutex::new(None));
        let slot = result.clone();
        std::thread::Builder::new()
            .name("zgx-check".into())
            .spawn(move || {
                let report = check(&paths, &options);
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
            })
            .expect("spawn the type check thread");
        Self { result }
    }
    /// The result, once the check finished.
    pub fn take(&self) -> Option<Result<Report, CheckError>> {
        self.result.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_gnu_output_with_continuations_and_logs() {
        let text = "[INFO] Loading definitions file: @roblox - zgx.d.luau\n\
app.luau:3.21-3.26: TypeError: Unknown global 'helper'; consider assigning to it first\n\
dir/b.luau:12.9-12.17: TypeError: No valid instantiation. It was expected to be at least:\n\
\tstring | nil\n\
but these types are not compatible with one another.\n\
app.luau:1.7-1.11: LocalUnused: Variable 'count' is never used; prefix with '_' to silence\n";
        let d = parse(text);
        assert_eq!(d.len(), 3, "{d:#?}");
        assert_eq!(
            d[0].to_string(),
            "app.luau:3:21: error[TypeError]: Unknown global 'helper'; consider assigning to it first"
        );
        assert_eq!(d[1].file, PathBuf::from("dir/b.luau"));
        assert!(d[1].message.ends_with("not compatible with one another."));
        assert_eq!(d[2].severity, Severity::Warning);
    }
}
