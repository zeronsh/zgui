//! `zgx-run <app.luau-or-dir>`: a zgx Luau script as a desktop app, with
//! hot reload (`zgx_run::run`).
use std::process::ExitCode;

const USAGE: &str = "usage: zgx-run [--inspect] <app.luau-or-dir> [args…]

Opens the script (a file, or a directory's app.luau) in a window and
reloads it as its files change. A zgx.json beside it sets the window
(title, width, height, min_width, min_height, titlebar: standard, inset or
hidden, glass) and app_id, where ui.persist keeps its data and, on Linux,
the desktop file ID its windows carry (app_id.desktop). Its ui.command
entries are the menu bar. What follows the script is its ui.args, and the
files among them open with it (ui.on_open); with an application_id
(reverse-DNS) in zgx.json, so do those the system opens with it. Its
packages ({ \"kit\": \"../shared/kit\" }) are module folders outside the
script's directory it may require (require(\"@kit/button\")).

--inspect serves devtools for agents: `zgx inspect help` drives the
running app (its tree, states, clicks, typing, keys) over a socket only
this user can reach.

--audit [--human] audits the app for assistive technology, headless (what
`zgx audit` runs): its first screen, its commands, the first screen's
buttons pressed and its routes, each from a fresh start. One line of JSON,
or a table with --human; exit 1 with findings, 2 on an error.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--audit") {
        args.remove(0);
        let human = args.iter().any(|a| a == "--human");
        args.retain(|a| a != "--human");
        let [path] = args.as_slice() else {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        };
        return match zgx_run::audit::audit_app(std::path::Path::new(path)) {
            Ok(audit) => {
                if human {
                    print!("{}", audit.to_human());
                } else {
                    println!("{}", audit.to_json());
                }
                if audit.findings.is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("zgx audit: {error}");
                ExitCode::from(2)
            }
        };
    }
    if args.first().is_some_and(|a| a == "--inspect") {
        args.remove(0);
        zgx_run::set_inspect(true);
    }
    let [path, rest @ ..] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if path == "-h" || path == "--help" {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match zgx_run::run_with(std::path::Path::new(path), rest.to_vec()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zgx-run: {error}");
            ExitCode::FAILURE
        }
    }
}
