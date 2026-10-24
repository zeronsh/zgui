//! Every example in `docs/reference`, `docs/getting-started.md` and the
//! README runs as the page says. Each ```lua
//! block is a script: it loads headless with no error, runs a few frames,
//! and passes `zgx check` in strict mode when luau-lsp is installed.
//! Blocks that start with `-- file: <path>` (and ```json file=.luaurc
//! blocks) are one project per page, its entry `app.luau` (with `plugins/`
//! loaded as plugins).
mod common;

use common::Harness;
use std::path::{Path, PathBuf};
use zgx_script::ScriptHost;

const DOCS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/reference");

/// A 1×1 PNG and an SVG, for the examples' `photo.png` and `icon.svg`.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0x60, 0x00, 0x00,
    0x00, 0x06, 0x00, 0x02, 0x30, 0x81, 0xd0, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'>\
    <circle cx='12' cy='12' r='9' fill='#fff'/></svg>";

struct Example {
    /// `page.md:line`, for messages.
    at: String,
    files: Vec<(String, String)>,
}

fn examples() -> Vec<Example> {
    let mut pages: Vec<PathBuf> = std::fs::read_dir(DOCS)
        .expect("docs/reference")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    pages.sort();
    // The guide and the README run as the reference does.
    for page in ["../getting-started.md", "../../README.md"] {
        pages.push(Path::new(DOCS).join(page));
    }
    // `ZGX_REFERENCE_PAGE=text.md` runs one page's.
    if let Some(only) = zgx::env::var_os("ZGX_REFERENCE_PAGE") {
        pages.retain(|p| p.file_name() == Some(only.as_os_str()));
    }
    let mut out = Vec::new();
    for page in pages {
        let name = page.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&page).unwrap();
        let mut project: Option<Example> = None;
        let mut lines = text.lines().enumerate();
        while let Some((n, line)) = lines.next() {
            // A project's `.luaurc`: ```json file=.luaurc
            let config = line.trim().strip_prefix("```json file=").map(str::to_owned);
            if line.trim() != "```lua" && config.is_none() {
                continue;
            }
            let mut code = String::new();
            for (_, line) in lines.by_ref() {
                if line.trim() == "```" {
                    break;
                }
                code.push_str(line);
                code.push('\n');
            }
            let at = format!("{name}:{}", n + 1);
            if let Some(path) = config {
                project
                    .get_or_insert_with(|| Example {
                        at: at.clone(),
                        files: Vec::new(),
                    })
                    .files
                    .push((path, code));
                continue;
            }
            match code.strip_prefix("-- file: ") {
                Some(rest) => {
                    let (path, body) = rest.split_once('\n').unwrap();
                    project
                        .get_or_insert_with(|| Example {
                            at: at.clone(),
                            files: Vec::new(),
                        })
                        .files
                        .push((path.trim().to_owned(), body.to_owned()));
                }
                None => out.push(Example {
                    at,
                    files: vec![("app.luau".into(), code)],
                }),
            }
        }
        out.extend(project);
    }
    out
}

fn write(example: &Example, root: &Path) -> PathBuf {
    let dir = root.join(example.at.replace([':', '.'], "_"));
    for (path, code) in &example.files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, code).unwrap();
    }
    std::fs::write(dir.join("photo.png"), PNG).unwrap();
    std::fs::write(dir.join("icon.svg"), SVG).unwrap();
    dir
}

#[test]
fn reference_examples_run_and_type_check() {
    let examples = examples();
    let one_page = zgx::env::var_os("ZGX_REFERENCE_PAGE").is_some();
    assert!(
        one_page || examples.len() >= 15,
        "found {} examples",
        examples.len()
    );
    let root = std::env::temp_dir().join(format!("zgx-reference-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut dirs = Vec::new();
    for example in &examples {
        eprintln!("{}: {} file(s)", example.at, example.files.len());
        let dir = write(example, &root);
        let (app, plugins) = (dir.join("app.luau"), dir.join("plugins"));
        assert!(app.is_file(), "{}: no app.luau", example.at);
        let h = Harness::new(move |runtime| {
            let host = ScriptHost::new(runtime, app);
            // What the examples ask of their host.
            host.action("save", |_| {});
            host.shared("user", "Ada");
            host.expose_shared("user", None);
            if plugins.is_dir() {
                host.plugins(plugins)
            } else {
                host
            }
        });
        for _ in 0..10 {
            h.frame();
        }
        assert_eq!(h.host.error().get(), None, "{}", example.at);
        dirs.push(dir);
    }
    if zgx_script::check::find_luau_lsp().is_none() {
        eprintln!("not type checked: luau-lsp is not installed");
    } else {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_zgx"))
            .arg("check")
            .args(&dirs)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.trim().is_empty(),
            "the examples type check in strict mode:\n{stdout}"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_guide_patches_what_the_workspace_patches() {
    // A git dependency doesn't bring zgx's `[patch]` with it: an app
    // copies it from the guide, so the guide's list is the workspace's.
    let names = |text: &str, start: &str| -> Vec<String> {
        let mut names: Vec<String> = text
            .split(start)
            .nth(1)
            .expect("a [patch.crates-io] section")
            .lines()
            .skip(1)
            .take_while(|line| !line.starts_with('[') && !line.starts_with("```"))
            .filter_map(|line| {
                let name = line.split('=').next()?.trim();
                (!name.is_empty() && !name.starts_with('#')).then(|| name.to_owned())
            })
            .collect();
        names.sort();
        names
    };
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let workspace = std::fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    let guide = std::fs::read_to_string(format!("{root}/docs/getting-started.md")).unwrap();
    assert_eq!(
        names(&guide, "[patch.crates-io]"),
        names(&workspace, "[patch.crates-io]"),
        "docs/getting-started.md's [patch.crates-io] is the workspace's"
    );
}
