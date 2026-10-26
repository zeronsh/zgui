//! `zgx`: developer tools for zgx-script.
//!
//! `zgx check [--nonstrict] <file-or-dir>...` type checks Luau scripts
//! with luau-lsp and the bundled `ui`/`anime` definitions, printing
//! `file:line:col` findings; it exits 1 when there are errors.
//!
//! `zgx lint <file-or-dir>...` reports patterns that cost far more than
//! they look or do not do what they look like (`zgx_script::lint`); it
//! exits 1 when it finds any.
//!
//! `zgx inspect <command>…` asks a running app serving devtools (`zgx-run
//! --inspect`): its tree, nodes, states and errors, and clicks, typing and
//! keys (`zgx_script::inspect`; `docs/agents/inspect.md`).
use std::{path::PathBuf, process::ExitCode};
use zgx_script::check::{CheckError, Options, Report, check};

const USAGE: &str = "usage: zgx check [--nonstrict] <file-or-dir>...
       zgx lint [--format text|json] <file-or-dir>...
       zgx lint --rules
       zgx inspect [--app PID] [--json] <command> [args]   (zgx inspect help)

check: type checks zgx Luau scripts with luau-lsp, the bundled ui/anime
types, the standard platform and the new type solver. Strict mode applies
unless a .luaurc (languageMode) or a --!nonstrict comment says otherwise.

lint: zgx's best-practice rules (no luau-lsp needed). A finding names the
cheaper way to write it; `-- zgx-lint: allow(rule)` on its line or the
line above accepts a deliberate exception. It exits 1 when it finds any;
`--format json` prints them as one JSON object for tools.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => {}
        Some("lint") => return lint(args.collect()),
        Some("inspect") => return inspect(args.collect()),
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
  errors               lint               frames [n]
  wait <target> [--text T] [--gone] [--timeout MS]
  sleep <ms>           raw <json>
  help                 what the app answers

Targets: #id, \"label or text\", role:button, @12, row 3, [2], a >> b.
Text by default, compact; --json prints the answer as JSON.";

fn inspect(args: Vec<String>) -> ExitCode {
    use zgx_script::{Value, inspect_client};
    let mut app: Option<u32> = None;
    let mut json = false;
    let mut words: Vec<String> = Vec::new();
    let mut flags: Vec<(String, Option<String>)> = Vec::new();
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
            "-h" | "--help" => {
                println!("{INSPECT_USAGE}");
                return ExitCode::SUCCESS;
            }
            "--all" | "--full" | "--gone" => flags.push((arg[2..].to_string(), None)),
            "--depth" | "--budget" | "--text" | "--timeout" => {
                flags.push((arg[2..].to_string(), args.next()));
            }
            _ => words.push(arg),
        }
    }
    let Some(cmd) = words.first().cloned() else {
        eprintln!("{INSPECT_USAGE}");
        return ExitCode::from(2);
    };
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
        "tree" | "node" | "find" | "click" | "wait" => put("target", arg(1)),
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
            ("dy" | "ms" | "n", Value::Str(s)) => {
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
        if !json {
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
