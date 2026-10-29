//! What a localized text costs against its template twin, headless: ROWS
//! texts (200) showing one count, changed TICKS times (300).
//! - `MODE=message` shows `ui.t("inbox.count", { n = count })`;
//! - `MODE=template` shows `"{count} messages"`: the same text in English;
//! - `MODE=switch` shows the message and switches the locale (English,
//!   French) every tick instead.
//!
//! Count it under cachegrind once per mode: a message should cost what its
//! template does per update (formatted in Rust when its count changes, never
//! per frame), and a switch only what the texts that changed cost.
//! Run: MODE=message cargo run --release -p zgx-script --example i18n_bench
use zgx_script::{ScriptHost, Value, testing::TestApp};

const EN: &str = r#"{ "inbox": { "count": "{n, plural, one {# message} other {# messages}}" } }"#;
const FR: &str = r#"{ "inbox": { "count": "{n, plural, one {# courriel} other {# courriels}}" } }"#;

fn script(mode: &str, rows: usize) -> String {
    let text = match mode {
        "template" => "\"{count} messages\"".to_owned(),
        _ => "ui.t(\"inbox.count\", { n = count })".to_owned(),
    };
    let setup = match mode {
        "template" => "",
        _ => {
            "ui.i18n { dir = \"locales\" }\n\
              ui.command { id = \"en\", run = function() ui.locale:set(\"en\") end }\n\
              ui.command { id = \"fr\", run = function() ui.locale:set(\"fr\") end }\n"
        }
    };
    format!(
        "{setup}local count = ui.state(\"count\", 1)\n\
         local rows = {{}}\n\
         for i = 1, {rows} do\n    rows[i] = ui.text {{ {text} }}\n\
         end\n\
         return ui.column(rows)\n"
    )
}

fn main() {
    let number = |name: &str, default: usize| {
        zgx::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (rows, ticks) = (number("ROWS", 200), number("TICKS", 300));
    let mode = zgx::env::var("MODE").unwrap_or_else(|_| "message".into());
    let dir = std::env::temp_dir().join(format!("zgx-i18n-bench-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("locales")).unwrap();
    std::fs::write(dir.join("locales/en.json"), EN).unwrap();
    std::fs::write(dir.join("locales/fr.json"), FR).unwrap();
    std::fs::write(dir.join("app.luau"), script(&mode, rows)).unwrap();
    let path = dir.join("app.luau");
    let app = TestApp::new(|runtime| ScriptHost::new(runtime, path));
    app.settle();
    for tick in 0..ticks {
        match mode.as_str() {
            "switch" => {
                let locale = if tick % 2 == 0 { "fr" } else { "en" };
                assert!(app.host.run_command(locale));
            }
            _ => app
                .host
                .set_state("count", Value::Num(tick as f64 + 2.))
                .unwrap(),
        }
        app.frame();
    }
    assert_eq!(app.host.error().get(), None);
    let _ = std::fs::remove_dir_all(&dir);
    println!("{mode}: {rows} rows, {ticks} ticks");
}
