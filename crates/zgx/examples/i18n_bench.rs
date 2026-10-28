//! What localized text costs against its template twin, headless: a list
//! of plural messages (`{n, plural, one {# message} other {# messages}}`)
//! whose counts change each frame, then a locale switch, against the same
//! list as `format!` text (`template`).
//! Run: cargo run -p zgx --release --example i18n_bench [i18n|template]
//! (`FRAMES` sets how many frames; 600 by default. `ROWS`, 1000.)
use std::time::Instant;
use zgx::{
    compose::{column, text_message, text_signal},
    i18n::I18n,
    widgets::Ui,
};

const EN: &str = r#"{ "inbox": { "count": "{n, plural, one {# message} other {# messages}}" } }"#;
const FR: &str =
    r#"{ "inbox": { "count": "{n, plural, one {# message} other {# messages reçus}}" } }"#;

fn env(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let template = std::env::args().nth(1).as_deref() == Some("template");
    let (rows, frames) = (env("ROWS", 1000), env("FRAMES", 600));
    let mut ui = Ui::new(1200., 900.);
    let i18n = I18n::new(&ui.runtime, "en");
    i18n.insert("en", EN).unwrap();
    i18n.insert("fr", FR).unwrap();
    i18n.set_locale(Some("en"));
    let counts: Vec<_> = (0..rows).map(|i| ui.signal(i)).collect();
    let view = column().children(counts.iter().map(|count| {
        let count = count.clone();
        if template {
            text_signal(move || {
                let n = count.get();
                if n == 1 {
                    format!("{n} message")
                } else {
                    format!("{n} messages")
                }
            })
        } else {
            text_message(i18n.t("inbox.count").arg("n", count))
        }
    }));
    ui.mount(view);
    ui.scene.borrow_mut().flush();
    let started = Instant::now();
    // A tenth of the rows change a frame.
    for frame in 1..=frames {
        for count in counts.iter().skip(frame as usize % 10).step_by(10) {
            count.set(count.with_untracked(|n| *n) + 1);
        }
        ui.prepare_frame();
        std::hint::black_box(ui.scene.borrow_mut().flush());
    }
    let updates = started.elapsed();
    // A locale switch, there and back (the template has none to make).
    let started = Instant::now();
    for locale in ["fr", "en"] {
        if !template {
            i18n.set_locale(Some(locale));
        }
        ui.prepare_frame();
        std::hint::black_box(ui.scene.borrow_mut().flush());
    }
    println!(
        "{}: {:.1} us per frame, {:.1} us a locale switch",
        if template { "template" } else { "i18n" },
        updates.as_secs_f64() * 1e6 / f64::from(frames),
        started.elapsed().as_secs_f64() * 1e6 / 2.,
    );
}
