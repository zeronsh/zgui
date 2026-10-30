//! `zgx`: developer tools for zgx-script.
//!
//! `zgx new <name> [--template T] [--i18n]` writes a Luau app that runs at
//! once, with its tests and a CI workflow (`zgx_script::scaffold`).
//!
//! `zgx run [--inspect] <app> [args…]` runs the app by zgx-run (beside this
//! binary, or on PATH), as `zgx audit` audits it.
//!
//! `zgx check [--nonstrict] <file-or-dir>...` type checks Luau scripts
//! with luau-lsp and the bundled `ui`/`anime` definitions, printing
//! `file:line:col` findings; it exits 1 when there are errors.
//!
//! `zgx lint <file-or-dir>...` reports patterns that cost far more than
//! they look or do not do what they look like (`zgx_script::lint`); it
//! exits 1 when it finds any.
//!
//! `zgx types <dir>…` writes each app's own theme tokens as Luau types
//! (`<dir>/.zgx/types.d.luau`, `zgx_script::app_types`), for editors to
//! complete `"$app.…"`.
//!
//! `zgx test [<app-dir>] [<filter>]` runs the app's Luau tests
//! (`*.test.luau`) against it mounted headless (`zgx_script::test_runner`).
//!
//! `zgx inspect <command>…` asks a running app serving devtools (`zgx-run
//! --inspect`): its tree, nodes, states and errors, and clicks, typing and
//! keys (`zgx_script::inspect`; `docs/agents/inspect.md`).
use std::{path::PathBuf, process::ExitCode};
use zgx_script::check::{CheckError, Options, Report, check};

const USAGE: &str = "usage: zgx new <name> [--template basic|router|list|form|plugins] [--i18n]
       zgx run [--inspect] <app.luau-or-dir> [args...]
       zgx check [--nonstrict] <file-or-dir>...
       zgx lint [--format text|json] <file-or-dir>...
       zgx lint --rules
       zgx types <dir>...
       zgx test [<app-dir>] [<name filter>]
       zgx inspect [--app PID] [--json] <command> [args]   (zgx inspect help)
       zgx audit [--human] <app.luau-or-dir>

new: writes a Luau app into the directory <name> (empty or new): app.luau,
components/, a theme, tests/, zgx.json, a README and a CI workflow running
the checks below. `zgx run <name>` runs it, and check, lint, audit and test
are clean. --i18n writes its text as ui.t messages with locales/en.json.

run: opens the app in a window and reloads it as its files change
(zgx-run, beside zgx or on PATH; `zgx-run --help` says what zgx.json sets).
--inspect serves devtools (`zgx inspect help`).

check: type checks zgx Luau scripts with luau-lsp, the bundled ui/anime
types, the standard platform and the new type solver. Strict mode applies
unless a .luaurc (languageMode) or a --!nonstrict comment says otherwise.

lint: zgx's best-practice rules (no luau-lsp needed). A finding names the
cheaper way to write it; `-- zgx-lint: allow(rule)` on its line or the
line above accepts a deliberate exception. It exits 1 when it finds any;
`--format json` prints them as one JSON object for tools.

types: writes each app's own theme tokens (`ui.theme.tokens`) as Luau
types to <dir>/.zgx/types.d.luau, for editors to complete \"$app.…\" (list
it in luau-lsp's definition files). `zgx check` writes it too.

audit: what assistive technology gets of an app, headless (zgx-run
--audit): its first screen, then each command, each of its buttons
pressed and each route, from a fresh start. One line of JSON (each finding's
rule, target, role, name, detail, source line and state), or a table with
--human. It exits 1 when it finds any (docs/accessibility-audit.md).

test: runs the app's tests (files named *.test.luau under <app-dir>, `.` if
not given), each against the app mounted headless: `test(name, body)`,
`mount()` and `expect(v)` (docs/scripting.md, Testing an app). A filter runs the
tests whose names hold it. It exits 1 when any fails.";

/// `zgx types <dir>...`
fn types(args: Vec<String>) -> ExitCode {
    if args.is_empty() || args.iter().any(|a| a.starts_with('-')) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    for dir in args.iter().map(PathBuf::from) {
        if !dir.is_dir() {
            eprintln!("zgx types: {} is not an app's directory", dir.display());
            return ExitCode::from(2);
        }
        match zgx_script::app_types::write_types(&dir) {
            Ok(changed) => {
                let file = zgx_script::app_types::types_file(&dir);
                println!(
                    "{} {}\n  (editors: add it to luau-lsp.types.definitionFiles)",
                    if changed { "wrote" } else { "unchanged:" },
                    file.display()
                );
            }
            Err(error) => {
                eprintln!("zgx types: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// `zgx audit [--human] <app.luau-or-dir>`: the app's accessibility audit,
/// run headless by `zgx-run --audit` (beside this binary, or on PATH), which
/// has the desktop's accessibility projection this tool doesn't link.
fn audit(args: Vec<String>) -> ExitCode {
    if args.iter().filter(|a| !a.starts_with('-')).count() != 1
        || args.iter().any(|a| a.starts_with('-') && a != "--human")
    {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let run = zgx_run();
    match std::process::Command::new(&run)
        .arg("--audit")
        .args(&args)
        .status()
    {
        Ok(status) => ExitCode::from(status.code().unwrap_or(2).clamp(0, 255) as u8),
        Err(error) => {
            eprintln!(
                "zgx audit: couldn't run {} ({error}); install zgx-run beside zgx or on PATH",
                run.display()
            );
            ExitCode::from(2)
        }
    }
}

/// `zgx new <name> [--template T] [--i18n]`
fn new(args: Vec<String>) -> ExitCode {
    let mut dir: Option<PathBuf> = None;
    let mut template = zgx_script::scaffold::TEMPLATES[0].0.to_string();
    let mut i18n = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--template" | "-t" => match args.next() {
                Some(name) => template = name,
                None => {
                    eprintln!("zgx new: --template takes a name\n");
                    for (name, about) in zgx_script::scaffold::TEMPLATES {
                        eprintln!("  {name:8} {about}");
                    }
                    return ExitCode::from(2);
                }
            },
            "--i18n" => i18n = true,
            "-h" | "--help" => {
                println!("{USAGE}\n\ntemplates:");
                for (name, about) in zgx_script::scaffold::TEMPLATES {
                    println!("  {name:8} {about}");
                }
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("zgx new: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            _ if dir.is_some() => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            path => dir = Some(PathBuf::from(path)),
        }
    }
    let Some(dir) = dir else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match zgx_script::scaffold::create(&dir, &template, i18n) {
        Ok(files) => {
            for file in &files {
                println!("  {}", file.strip_prefix(&dir).unwrap_or(file).display());
            }
            println!(
                "\nMade {}. Next:\n  cd {}\n  zgx run .\n  zgx test",
                dir.display(),
                dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("zgx new: {error}");
            ExitCode::from(2)
        }
    }
}

/// `zgx test [<app-dir>] [<name filter>]`
fn test(args: Vec<String>) -> ExitCode {
    if args.len() > 2 || args.iter().any(|a| a.starts_with('-')) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let dir = PathBuf::from(args.first().map_or(".", String::as_str));
    let filter = args.get(1).map(String::as_str);
    let report = zgx_script::test_runner::run(&dir, filter, |outcome| {
        let millis = outcome.duration.as_secs_f64() * 1e3;
        match &outcome.failure {
            None => println!("ok   {} ({millis:.0} ms)", outcome.name),
            Some(failure) => {
                println!("FAIL {} ({})", outcome.name, outcome.file.display());
                for line in failure.lines() {
                    println!("     {line}");
                }
            }
        }
    });
    match report {
        Ok(report) => {
            let failed = report.failed();
            eprintln!("{} passed, {failed} failed", report.outcomes.len() - failed);
            if failed > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("zgx test: {error}");
            ExitCode::from(2)
        }
    }
}

/// The zgx-run beside this binary, else the one on PATH: what `zgx run`
/// and `zgx audit` hand an app to (it links the desktop, which this tool
/// doesn't).
fn zgx_run() -> PathBuf {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name(format!("zgx-run{}", std::env::consts::EXE_SUFFIX)))
        .filter(|run| run.is_file())
        .unwrap_or_else(|| PathBuf::from("zgx-run"))
}

/// `zgx run [--inspect] <app.luau-or-dir> [args…]`: the app in a window,
/// reloading as it changes, by zgx-run. On Unix zgx-run takes this
/// process's place, so signals and the exit status are its own.
fn run(args: Vec<String>) -> ExitCode {
    if args.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let run = zgx_run();
    let mut command = std::process::Command::new(&run);
    command.args(&args);
    #[cfg(unix)]
    let error = std::os::unix::process::CommandExt::exec(&mut command);
    #[cfg(not(unix))]
    let error = match command.status() {
        Ok(status) => return ExitCode::from(status.code().unwrap_or(2).clamp(0, 255) as u8),
        Err(error) => error,
    };
    eprintln!(
        "zgx run: couldn't run {} ({error}); install zgx-run beside zgx or on PATH \
         (cargo install zgx-run)",
        run.display()
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => {}
        Some("test") => return test(args.collect()),
        Some("new") => return new(args.collect()),
        Some("lint") => return lint(args.collect()),
        Some("types") => return types(args.collect()),
        Some("inspect") => return inspect(args.collect()),
        Some("audit") => return audit(args.collect()),
        Some("run") => return run(args.collect()),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let mut options = Options::default();
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--nonstrict" | "--non-strict" => options.strict = false,
            "--strict" => options.strict = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("zgx check: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match check(&paths, &options) {
        Ok(report) => {
            print_report(&report);
            if report.errors() > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error @ CheckError::NotInstalled) => {
            eprintln!("zgx check: {error}");
            ExitCode::from(3)
        }
        Err(error) => {
            eprintln!("zgx check: {error}");
            ExitCode::from(2)
        }
    }
}

fn print_report(report: &Report) {
    for d in &report.diagnostics {
        println!("{d}");
    }
    let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    eprintln!(
        "{}, {} in {}",
        plural(report.errors(), "error"),
        plural(report.warnings(), "warning"),
        plural(report.files, "file")
    );
}

fn lint(args: Vec<String>) -> ExitCode {
    let mut paths = Vec::new();
    let mut json = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => match args.next().as_deref() {
                Some("json") => json = true,
                Some("text") => json = false,
                other => {
                    eprintln!("zgx lint: --format takes text or json, not {other:?}\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--json" => json = true,
            "--rules" => {
                for (rule, summary) in zgx_script::lint::RULES {
                    println!("{rule:18} {summary}");
                }
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("zgx lint: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match zgx_script::lint::lint(&paths) {
        Ok(report) if json => {
            let findings: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "file": d.file.display().to_string(),
                        "line": d.line,
                        "column": d.column,
                        "rule": d.code,
                        "message": d.message,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({ "files": report.files, "findings": findings })
            );
            if report.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Ok(report) => {
            print_report(&report);
            if report.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("zgx lint: {error}");
            ExitCode::from(2)
        }
    }
}

const INSPECT_USAGE: &str = "usage: zgx inspect [--app PID] [--json] <command> [args]

Asks a running app serving devtools (`zgx-run --inspect`, or
ZGX_INSPECT_SOCKET=1): with one running, --app is not needed.

  list                                   apps serving devtools
  tree [target] [--depth N] [--budget N] [--all]
  node <target>        find <target>
  state [name] [--full]                  set <name> <json>
  click <target>       type <target> <text>      key <keys>
  scroll <target> <dy> command <id>
  errors               lint
  frames [n] [--runs N] [--all] [--since MARK|FRAME] [--keep N]
  mark [name]          switch [name] [on|off]
  wait <target> [--text T] [--gone] [--timeout MS]
  snapshot <target> [--width PX] [--out file.png]
  record               record stop [--out gesture.tape]
  replay <target> <gesture.tape> [--repeat N] [--every MS]
                       its pacing, through the window's input
  search [query] [--next] [--prev] [--all] [--close]
                       find in page: count, current, the first matches
  zoom [in|out|reset|150%]               the window's zoom and its viewport
  memory [--human]                       what the app holds, a line each, as JSON
  sleep <ms>           raw <json>
  help                 what the app answers
  mcp                  serve these as MCP tools on stdio, for agents
                       (claude mcp add zgx -- zgx inspect mcp)

Targets: #id, \"label or text\", role:button, @12, row 3, [2], a >> b.
Text by default, compact; --json prints the answer as JSON.";

fn inspect(args: Vec<String>) -> ExitCode {
    use zgx_script::{Value, inspect_client};
    let mut app: Option<u32> = None;
    let mut json = false;
    let mut human = false;
    let mut words: Vec<String> = Vec::new();
    let mut flags: Vec<(String, Option<String>)> = Vec::new();
    // Where `snapshot` copies its picture, or `record stop` writes its tape.
    let mut out: Option<String> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app" => match args.next().and_then(|p| p.parse().ok()) {
                Some(pid) => app = Some(pid),
                None => {
                    eprintln!("zgx inspect: --app takes a process id");
                    return ExitCode::from(2);
                }
            },
            "--json" => json = true,
            "--human" => human = true,
            "-h" | "--help" => {
                println!("{INSPECT_USAGE}");
                return ExitCode::SUCCESS;
            }
            "--all" | "--full" | "--gone" | "--next" | "--prev" | "--close" => {
                flags.push((arg[2..].to_string(), None))
            }
            "--out" => match args.next() {
                Some(path) => out = Some(path),
                None => {
                    eprintln!("zgx inspect: --out takes a file");
                    return ExitCode::from(2);
                }
            },
            "--depth" | "--budget" | "--text" | "--timeout" | "--width" | "--runs" | "--since"
            | "--keep" | "--repeat" | "--every" => {
                flags.push((arg[2..].to_string(), args.next()));
            }
            _ => words.push(arg),
        }
    }
    let Some(cmd) = words.first().cloned() else {
        eprintln!("{INSPECT_USAGE}");
        return ExitCode::from(2);
    };
    if cmd == "mcp" {
        return mcp();
    }
    if cmd == "list" {
        let apps = inspect_client::apps();
        if apps.is_empty() {
            println!("none");
        }
        for a in apps {
            println!("{} {}", a.pid, a.app);
        }
        return ExitCode::SUCCESS;
    }
    let arg = |i: usize| words.get(i).cloned();
    let mut fields: Vec<(String, Value)> = vec![("cmd".into(), Value::from(cmd.as_str()))];
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            fields.push((k.into(), Value::from(v.as_str())));
        }
    };
    match cmd.as_str() {
        "tree" | "node" | "find" | "click" | "wait" | "snapshot" => put("target", arg(1)),
        "record" => {
            if arg(1).as_deref() == Some("stop") {
                fields.push(("stop".into(), Value::Bool(true)));
            }
        }
        "replay" => {
            put("target", arg(1));
            match arg(2)
                .map(|path| std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}")))
            {
                Some(Ok(tape)) => put("tape", Some(tape)),
                Some(Err(error)) => {
                    eprintln!("zgx inspect: {error}");
                    return ExitCode::from(2);
                }
                None => {
                    eprintln!("zgx inspect: replay takes a target and a tape file");
                    return ExitCode::from(2);
                }
            }
        }
        "type" => {
            put("target", arg(1));
            put("text", arg(2));
        }
        "scroll" => {
            put("target", arg(1));
            put("dy", arg(2));
        }
        "key" => put("keys", arg(1)),
        "command" => put("id", arg(1)),
        "state" => put("name", arg(1)),
        "sleep" => put("ms", arg(1)),
        "frames" => put("n", arg(1)),
        "search" => put("query", arg(1)),
        "zoom" => put("to", arg(1)),
        "mark" => put("name", arg(1)),
        "switch" => {
            put("name", arg(1));
            put("on", arg(2));
        }
        "set" => {
            put("name", arg(1));
            match arg(2).map(|v| Value::from_json(&v).unwrap_or_else(|| Value::from(v.as_str()))) {
                Some(value) => fields.push(("value".into(), value)),
                None => {
                    eprintln!("zgx inspect: set takes a name and a value (JSON)");
                    return ExitCode::from(2);
                }
            }
        }
        _ => {}
    }
    for (name, value) in flags {
        let key = if name == "timeout" {
            "timeout_ms".to_string()
        } else {
            name
        };
        let value = match value {
            None => Value::Bool(true),
            Some(v) => v
                .parse::<f64>()
                .map(Value::Num)
                .unwrap_or_else(|_| Value::from(v.as_str())),
        };
        fields.push((key, value));
    }
    // Numbers given as words are numbers.
    let fields: Vec<(String, Value)> = fields
        .into_iter()
        .map(|(k, v)| match (&*k, &v) {
            ("dy" | "ms" | "n" | "width" | "runs" | "keep" | "repeat" | "every", Value::Str(s)) => {
                (k, s.parse::<f64>().map(Value::Num).unwrap_or(v))
            }
            _ => (k, v),
        })
        .collect();
    let request = if cmd == "raw" {
        match arg(1).and_then(|r| Value::from_json(&r)) {
            Some(r) => r,
            None => {
                eprintln!("zgx inspect: raw takes a JSON object");
                return ExitCode::from(2);
            }
        }
    } else {
        Value::record(fields)
    };
    let pid = match inspect_client::pick(app) {
        Ok(pid) => pid,
        Err(error) => {
            eprintln!("zgx inspect: {error}");
            return ExitCode::from(3);
        }
    };
    let answer = match inspect_client::request(pid, &request) {
        Ok(answer) => answer,
        Err(error) => {
            eprintln!("zgx inspect: {error}");
            return ExitCode::from(3);
        }
    };
    if json {
        println!("{answer}");
    }
    let parsed = Value::from_json(&answer).unwrap_or_default();
    if parsed.field("ok").is_some_and(Value::truthy) {
        // A snapshot, or a recorded tape, where it was asked for.
        let data = parsed.field("data");
        let picture = data.and_then(|d| d.field("path")).map(Value::to_string);
        let tape = data.and_then(|d| d.field("tape")).map(Value::to_string);
        if let Some(out) = &out
            && let Err(error) = match (picture, tape) {
                (Some(from), _) => std::fs::copy(&from, out).map(|_| ()),
                (None, Some(tape)) => std::fs::write(out, tape),
                (None, None) => Ok(()),
            }
        {
            eprintln!("zgx inspect: {out}: {error}");
            return ExitCode::FAILURE;
        }
        if cmd == "memory" && !json && !human {
            // For agents first: its lines, as JSON. `--human` for the table.
            println!("{}", data.cloned().unwrap_or_default().to_json());
        } else if !json {
            let text = parsed
                .field("text")
                .map(Value::to_string)
                .unwrap_or_default();
            println!("{}", text.trim_end());
        }
        ExitCode::SUCCESS
    } else {
        if !json {
            let error = parsed
                .field("error")
                .map(Value::to_string)
                .unwrap_or(answer);
            eprintln!("{error}");
        }
        ExitCode::FAILURE
    }
}

/// The MCP tools: each a devtools command, its arguments (all strings
/// unless said), and what it does.
/// A tool's argument: its name, its JSON type, and whether it is required.
type Arg = (&'static str, &'static str, bool);
/// A tool: its name, the command it runs, its arguments and what it does.
type Tool = (&'static str, &'static str, &'static [Arg], &'static str);
const TOOLS: &[Tool] = &[
    (
        "zgx_apps",
        "list",
        &[],
        "The running zgx apps serving devtools: pid and script.",
    ),
    (
        "zgx_tree",
        "tree",
        &[
            ("target", "string", false),
            ("depth", "number", false),
            ("budget", "number", false),
            ("all", "boolean", false),
        ],
        "The interface, a node a line in document order: `@n #id role \"name\" =\"value\" x,y wxh`. Give a target to see a part, depth to see less.",
    ),
    (
        "zgx_node",
        "node",
        &[("target", "string", true)],
        "One node: kind, the script line that made it, role, name, value, state, box, text.",
    ),
    (
        "zgx_find",
        "find",
        &[("target", "string", true)],
        "Every node a target matches.",
    ),
    (
        "zgx_search",
        "search",
        &[
            ("query", "string", false),
            ("next", "boolean", false),
            ("prev", "boolean", false),
            ("all", "boolean", false),
            ("n", "number", false),
            ("close", "boolean", false),
        ],
        "Find in page, as Cmd/Ctrl+F: case and accents ignored, all text including fields and virtual lists' unmounted rows. Answers the count, the current match and the first n (5; all with all): where each is (@node, or a list's row), its range, the text around it, its box if shown. next/prev step the current match in the window.",
    ),
    (
        "zgx_memory",
        "memory",
        &[],
        "What the app holds, by what holds it, one line each: the process's footprint and its allocator, each Luau VM's heap, decoded pictures shown and cached, text caches, the glyph atlas, GPU textures, layers and buffers, scene nodes and the frame log. Compare two answers to see which line grew.",
    ),
    (
        "zgx_zoom",
        "zoom",
        &[("to", "string", false)],
        "The window's zoom, as a browser's page zoom (Cmd/Ctrl +, -, 0): `to` is in, out, reset, a factor (1.5) or a percent (150%). Answers the zoom and the viewport in points.",
    ),
    (
        "zgx_state",
        "state",
        &[("name", "string", false), ("full", "boolean", false)],
        "Named states (ui.state, ui.persist, ui.shared) and their values; long values are cut unless full.",
    ),
    (
        "zgx_set_state",
        "set",
        &[("name", "string", true), ("value", "any", true)],
        "Set a named state to a JSON value.",
    ),
    (
        "zgx_click",
        "click",
        &[("target", "string", true)],
        "Click a node.",
    ),
    (
        "zgx_type",
        "type",
        &[("target", "string", true), ("text", "string", true)],
        "Click a node (an input), then type text into it.",
    ),
    (
        "zgx_key",
        "key",
        &[("keys", "string", true)],
        "Press keys where focus is: Enter, Escape, Tab, cmd+k, alt+ArrowDown.",
    ),
    (
        "zgx_scroll",
        "scroll",
        &[("target", "string", true), ("dy", "number", true)],
        "Scroll the viewport under a node by dy points (down is positive).",
    ),
    (
        "zgx_command",
        "command",
        &[("id", "string", true)],
        "Run a ui.command, as its menu item or shortcut does.",
    ),
    (
        "zgx_errors",
        "errors",
        &[],
        "The script error, type errors, lint findings and layout warnings, or none.",
    ),
    (
        "zgx_lint",
        "lint",
        &[],
        "zgx lint over the app's scripts: file:line:col rule message.",
    ),
    (
        "zgx_frames",
        "frames",
        &[
            ("n", "number", false),
            ("runs", "number", false),
            ("all", "boolean", false),
            ("since", "string", false),
            ("keep", "number", false),
        ],
        "The last frames that did anything: each phase's time (update, layout, text, paint, present…), the GPU's own time, the states written, the runs they caused and their costs (the costliest `runs`, default 5, or `all`), and the engine's notes on why (a scroll repainted rather than copied, a wait for a later refresh). `since` a mark (zgx_mark) or a frame number reads what followed; `keep` sets how many frames are kept (default 480). Warns first when the window isn't presenting.",
    ),
    (
        "zgx_mark",
        "mark",
        &[("name", "string", false)],
        "Mark now, so zgx_frames with since=name reads only the frames after.",
    ),
    (
        "zgx_switch",
        "switch",
        &[("name", "string", false), ("on", "boolean", false)],
        "List the engine's switchable fast paths (scroll_copy, refresh_grid, partial_repaint), or switch one on or off: compare it with what it stands in for in one running app.",
    ),
    (
        "zgx_wait",
        "wait",
        &[
            ("target", "string", true),
            ("text", "string", false),
            ("gone", "boolean", false),
            ("timeout_ms", "number", false),
        ],
        "Wait until a node shows (holding text, if given), or is gone; fails with why after the timeout.",
    ),
    (
        "zgx_snapshot",
        "snapshot",
        &[("target", "string", true), ("width", "number", false)],
        "A picture of a node as the window last drew it (a PNG, at most width pixels wide): to see what it looks like.",
    ),
    (
        "zgx_record",
        "record",
        &[("stop", "boolean", false), ("out", "string", false)],
        "Record the window's wheel input (a person scrolls it meanwhile); with stop, end and answer the tape, written to out (a file) if given.",
    ),
    (
        "zgx_replay",
        "replay",
        &[
            ("target", "string", true),
            ("tape_file", "string", true),
            ("repeat", "number", false),
            ("every", "number", false),
        ],
        "Play a recorded wheel tape (a file) back at a node, through the window's own input path: answers the frames' pacing and work. `repeat` plays it several times and gives the spread; `every` (ms) spreads its motion over events that far apart, for a tape recorded while the window fell behind. Refused while the window isn't presenting (hidden or covered).",
    ),
    (
        "zgx_sleep",
        "sleep",
        &[("ms", "number", true)],
        "Let the app run for ms milliseconds.",
    ),
];

/// `bytes` in base64 (RFC 4648, padded): an MCP image's data.
fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

const TARGETS: &str = "Targets: #id, \"label or text\" (in quotes), role:button, @12 (a tree number), #list row 3, \"Copy\"[2], #a >> \"b\" (inside).";

/// `zgx inspect mcp`: devtools as MCP tools, JSON-RPC on stdin and stdout.
fn mcp() -> ExitCode {
    use std::io::{BufRead, Write};
    use zgx_script::{Value, inspect_client};
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let text = |t: String| {
        Value::record([
            ("type", Value::from("text")),
            ("text", Value::from(t.as_str())),
        ])
    };
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Some(message) = Value::from_json(line.trim()) else {
            continue;
        };
        let method = message
            .field("method")
            .map(Value::to_string)
            .unwrap_or_default();
        let Some(id) = message.field("id").cloned() else {
            // A notification (`notifications/initialized`): no answer.
            continue;
        };
        let result = match method.as_str() {
            "initialize" => Ok(Value::record([
                ("protocolVersion", message.field("params").and_then(|p| p.field("protocolVersion")).cloned().unwrap_or(Value::from("2025-06-18"))),
                // (An empty table is written as a list: give each object a field.)
                ("capabilities", Value::record([("tools", Value::record([("listChanged", Value::Bool(false))]))])),
                ("serverInfo", Value::record([("name", Value::from("zgx")), ("version", Value::from(env!("CARGO_PKG_VERSION")))])),
                ("instructions", Value::from(format!("Drive a running zgx Luau app (started with `zgx-run --inspect`). Read with zgx_tree, zgx_node, zgx_state, zgx_errors, zgx_frames; act with zgx_click, zgx_type, zgx_key, zgx_command, zgx_set_state; zgx_wait for what takes time. {TARGETS} Pass `app` (a pid) when several apps run.").as_str())),
            ])),
            "ping" => Ok(Value::record([("ok", Value::Bool(true))])),
            "tools/list" => Ok(Value::record([(
                "tools",
                Value::List(
                    TOOLS
                        .iter()
                        .map(|(name, _, args, about)| {
                            let mut properties: Vec<(&str, Value)> = args
                                .iter()
                                .map(|(arg, ty, _)| {
                                    let schema = if *ty == "any" {
                                        Value::record([("description", Value::from("any JSON value"))])
                                    } else {
                                        Value::record([("type", Value::from(*ty))])
                                    };
                                    (*arg, schema)
                                })
                                .collect();
                            properties.push(("app", Value::record([("type", Value::from("number")), ("description", Value::from("the app's pid, when several run"))])));
                            let required: Vec<Value> = args.iter().filter(|a| a.2).map(|a| Value::from(a.0)).collect();
                            Value::record([
                                ("name", Value::from(*name)),
                                ("description", Value::from(if args.iter().any(|a| a.0 == "target") { format!("{about} {TARGETS}") } else { about.to_string() }.as_str())),
                                ("inputSchema", Value::record([
                                    ("type", Value::from("object")),
                                    ("properties", Value::record(properties)),
                                    ("required", Value::List(required.into())),
                                ])),
                            ])
                        })
                        .collect::<Vec<_>>()
                        .into(),
                ),
            )])),
            "tools/call" => {
                let params = message.field("params").cloned().unwrap_or_default();
                let name = params.field("name").map(Value::to_string).unwrap_or_default();
                let args = params.field("arguments").cloned().unwrap_or_default();
                let mut picture: Option<String> = None;
                let answer = match TOOLS.iter().find(|t| t.0 == name) {
                    None => Err(format!("no tool {name}")),
                    Some((_, "list", _, _)) => Ok(inspect_client::apps().iter().map(|a| format!("{} {}", a.pid, a.app)).collect::<Vec<_>>().join("\n")),
                    Some((_, cmd, params, _)) => {
                        let mut fields: Vec<(String, Value)> = vec![("cmd".into(), Value::from(*cmd))];
                        let mut read: Result<(), String> = Ok(());
                        for (arg, _, _) in params.iter() {
                            match (*arg, args.field(arg)) {
                                // A tape is sent as its text.
                                ("tape_file", Some(path)) => match std::fs::read_to_string(path.to_string()) {
                                    Ok(tape) => fields.push(("tape".into(), Value::from(tape.as_str()))),
                                    Err(e) => read = Err(format!("{path}: {e}")),
                                },
                                ("out", _) => {}
                                (_, Some(v)) => fields.push((arg.to_string(), v.clone())),
                                (_, None) => {}
                            }
                        }
                        let out = args.field("out").map(Value::to_string);
                        let pid = args.field("app").and_then(Value::as_f64).map(|p| p as u32);
                        read.and_then(|()| inspect_client::pick(pid))
                            .and_then(|pid| inspect_client::request(pid, &Value::record(fields)))
                            .and_then(|answer| {
                                let answer = Value::from_json(&answer).unwrap_or_default();
                                if answer.field("ok").is_some_and(Value::truthy) {
                                    // A recorded tape, written where asked.
                                    if let (Some(out), Some(tape)) = (&out, answer.field("data").and_then(|d| d.field("tape")).map(Value::to_string)) {
                                        std::fs::write(out, tape).map_err(|e| format!("{out}: {e}"))?;
                                    }
                                    let text = answer.field("text").map(Value::to_string).unwrap_or_default();
                                    // A snapshot's picture, beside its text.
                                    if let Some(path) = answer.field("data").and_then(|d| d.field("path")).map(Value::to_string) {
                                        let png = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
                                        picture = Some(base64(&png));
                                    }
                                    Ok(text)
                                } else {
                                    Err(answer.field("error").map(Value::to_string).unwrap_or_default())
                                }
                            })
                    }
                };
                Ok(match answer {
                    Ok(t) => {
                        let mut content = vec![text(if t.is_empty() { "ok".into() } else { t })];
                        if let Some(data) = picture {
                            content.push(Value::record([
                                ("type", Value::from("image")),
                                ("data", Value::from(data.as_str())),
                                ("mimeType", Value::from("image/png")),
                            ]));
                        }
                        Value::record([("content", Value::List(content.into())), ("isError", Value::Bool(false))])
                    }
                    Err(e) => Value::record([("content", Value::List(vec![text(e)].into())), ("isError", Value::Bool(true))]),
                })
            }
            other => Err(format!("no method {other}")),
        };
        let reply = match result {
            Ok(result) => Value::record([
                ("jsonrpc", Value::from("2.0")),
                ("id", id),
                ("result", result),
            ]),
            Err(message) => Value::record([
                ("jsonrpc", Value::from("2.0")),
                ("id", id),
                (
                    "error",
                    Value::record([
                        ("code", Value::Num(-32601.)),
                        ("message", Value::from(message.as_str())),
                    ]),
                ),
            ]),
        };
        if writeln!(out, "{}", reply.to_json()).is_err() || out.flush().is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_pads_as_rfc_4648_does() {
        for (bytes, encoded) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foobar", "Zm9vYmFy"),
            (b"\xff\xfe\x00", "//4A"),
        ] {
            assert_eq!(super::base64(bytes), encoded);
        }
    }
}
