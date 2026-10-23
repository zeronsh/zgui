//! `zgui-run <app.luau-or-dir>`: a zgui Luau script as a desktop app, with
//! hot reload (`zgui_run::run`).
use std::process::ExitCode;

const USAGE: &str = "usage: zgui-run <app.luau-or-dir>

Opens the script (a file, or a directory's app.luau) in a window and
reloads it as its files change. A zgui.json beside it sets the window
(title, width, height, min_width, min_height, titlebar: standard, inset or
hidden, glass) and app_id, where ui.persist keeps its data. Its ui.command
entries are the menu bar.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if path == "-h" || path == "--help" {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match zgui_run::run(std::path::Path::new(path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zgui-run: {error}");
            ExitCode::FAILURE
        }
    }
}
