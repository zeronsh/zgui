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
this user can reach.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
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
