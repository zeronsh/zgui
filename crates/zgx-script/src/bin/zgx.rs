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
  snapshot <target> [--width PX] [--out file.png]
  record               record stop [--out gesture.tape]
  replay <target> <gesture.tape>         its pacing, through the window's input
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
            "-h" | "--help" => {
                println!("{INSPECT_USAGE}");
                return ExitCode::SUCCESS;
            }
            "--all" | "--full" | "--gone" => flags.push((arg[2..].to_string(), None)),
            "--out" => match args.next() {
                Some(path) => out = Some(path),
                None => {
                    eprintln!("zgx inspect: --out takes a file");
                    return ExitCode::from(2);
                }
            },
            "--depth" | "--budget" | "--text" | "--timeout" | "--width" => {
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
            ("dy" | "ms" | "n" | "width", Value::Str(s)) => {
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
        &[("n", "number", false)],
        "The last frames that did anything: each phase's time (update, layout, text, paint, present…), the states written, the runs they caused and their costs.",
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
        &[("target", "string", true), ("tape_file", "string", true)],
        "Play a recorded wheel tape (a file) back at a node, through the window's own input path: answers the frames' pacing and work.",
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
