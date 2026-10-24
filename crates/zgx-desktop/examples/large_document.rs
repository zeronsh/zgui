//! A long document in a native text area, for `scripts/large_document_smoke.py`:
//! a file of 100,000 lines (the path given, or one made up) opened into a
//! `Signal<TextValue>`, then typed into, composed in with the input method
//! and scrolled. `--smoke-test` reports the editor ten times a second.
use std::time::{Duration, Instant};
use zgx::{compose::prelude::*, scene::NodeKind, text::TextValue, text_layout::FontFamily};
use zgx_desktop::{Application, WindowOptions};

/// What the app holds, in kilobytes (Linux; 0 elsewhere).
fn resident_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
            line.trim().trim_end_matches("kB").trim().parse().ok()
        })
        .unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let path = std::env::args().skip(1).find(|arg| !arg.starts_with("--"));
    Application::new()
        .window(WindowOptions {
            title: "zgx large document".into(),
            width: 900.,
            height: 640.,
            ..Default::default()
        })
        .run(move |cx| {
            let started = Instant::now();
            let document = match &path {
                Some(path) => std::fs::read_to_string(path).expect("the document"),
                None => (0..100_000)
                    .map(|i| format!("fn line_{i}() -> usize {{ {i} }} // a line of the document"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            let value = cx.ui.signal(TextValue::from(document));
            let viewport = cx.viewport.clone();
            let title = path.clone().unwrap_or_else(|| "A sample of 100,000 lines".into());
            let mounted = cx.render(
                column()
                    .p(16.)
                    .gap(8.)
                    .text_size(14.)
                    .line_height(20.)
                    .child(text(title).h(20.))
                    .child(
                        text_area_value("Document", value.clone())
                            .id("editor")
                            .p(8.)
                            .font_family(FontFamily::Monospace)
                            .reactive_style(move || {
                                Styles::new()
                                    .w((viewport.get().0 - 32.).max(100.))
                                    .h((viewport.get().1 - 60.).max(80.))
                            }),
                    ),
            );
            let open_ms = started.elapsed().as_secs_f64() * 1000.;
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            let editor = cx.ui.focused_editor().unwrap();
            // A file opened shows its start (an editor's caret starts at
            // its text's end).
            editor.set_selection(0, 0);
            if !smoke {
                return;
            }
            let scene = cx.ui.scene.clone();
            let window = cx.window.clone();
            let report = move |tick: usize| {
                {
                    let e = editor.editor.borrow();
                    let Some(long) = e.long_text() else {
                        println!("DOCUMENT tick={tick} short");
                        return;
                    };
                    let selection = e.selection();
                    let ours = e.value();
                    let shared = value.with_untracked(|v| v.ptr_eq(&ours));
                    let (lines, len) = (long.line_count(), long.len());
                    let caret_line = long.line_of(selection.focus);
                    let first_line = long.line(0).into_owned();
                    let last_line = long.line(lines - 1).into_owned();
                    let preedit = e.preedit().map(|p| p.text.clone()).unwrap_or_default();
                    drop(e);
                    // The lines in view: the text nodes in the viewport, top
                    // to bottom.
                    let (top, bottom) = {
                        let scene = scene.borrow();
                        let view = scene.bounds(scene.parent(editor.caret).unwrap());
                        let mut shown = Vec::new();
                        let mut pending = vec![editor.node];
                        while let Some(id) = pending.pop() {
                            if let NodeKind::Text { text, .. } = scene.kind(id) {
                                let at = scene.bounds(id);
                                if !text.is_empty()
                                    && at.y + at.height > view.y + 1.
                                    && at.y < view.y + view.height - 1.
                                {
                                    shown.push((at.y, text.to_string()));
                                }
                            }
                            pending.extend(scene.children(id));
                        }
                        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
                        (
                            shown.first().map(|s| s.1.clone()).unwrap_or_default(),
                            shown.last().map(|s| s.1.clone()).unwrap_or_default(),
                        )
                    };
                    println!(
                        "DOCUMENT tick={tick} lines={lines} len={len} anchor={} focus={} caret_line={caret_line} shared={shared} open_ms={open_ms:.1} resident_kb={} first_line={first_line:?} last_line={last_line:?} preedit={preedit:?} top={top:?} bottom={bottom:?}",
                        selection.anchor,
                        selection.focus,
                        resident_kb(),
                    );
                }
            };
            cx.tasks.spawn(async move {
                for tick in 0..900 {
                    report(tick);
                    zgx::timer::sleep(Duration::from_millis(100)).await;
                }
                window.close();
            });
        })
}
