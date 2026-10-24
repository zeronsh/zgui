//! Converts a VS Code colour theme (following its `include`) to a zgx
//! theme file, its workbench colours mapped onto zgx's tokens.
//! Run: cargo run -p zgx --example vscode_theme -- path/to/theme.json > theme.json
use zgx::{reactive::Runtime, theme::Themes};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: vscode_theme <vscode-theme.json>");
        std::process::exit(2);
    };
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    match themes.load(&path) {
        Ok(name) => print!("{}", themes.theme(&name).unwrap().to_json()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
