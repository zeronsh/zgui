//! Checks the zgx patch to cosmic-text (see ../ZGX_PATCH.md) against
//! upstream 0.19.0. `cargo run --release` lays out thousands of random lines
//! with both at several widths and wrap modes and requires identical glyphs,
//! upstream shaping every run afresh (no word cache: the patched one, kept
//! however long, must not change what is drawn);
//! `cargo run --release -- bench` times shaping a streaming transcript, as
//! zgx's lab workload does, with each.
//!
//!     cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

/// Every laid-out glyph of `text`, as comparable lines.
macro_rules! layout {
    ($ct:ident, $fonts:expr, $text:expr, $width:expr, $wrap:expr) => {{
        use $ct::*;
        let wrap = [Wrap::None, Wrap::Word, Wrap::Glyph, Wrap::WordOrGlyph][$wrap];
        let mut buffer = Buffer::new($fonts, Metrics::new(14., 18.));
        buffer.set_wrap(wrap);
        buffer.set_size(Some($width), None);
        let attrs = Attrs::new().family(Family::SansSerif);
        buffer.set_text($text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll($fonts, false);
        let mut out = Vec::new();
        for run in buffer.layout_runs() {
            out.push(format!("line {} rtl {} y {} w {}", run.line_i, run.rtl, run.line_y, run.line_w));
            for g in run.glyphs {
                out.push(format!(
                    "{} {}..{} x {} y {} w {} level {} font {:?}",
                    g.glyph_id, g.start, g.end, g.x, g.y, g.w, g.level.number(), g.font_id
                ));
            }
        }
        out
    }};
}

/// Shaping time for a transcript growing by a token a tick, its last seven
/// 105-byte lines reshaped every tick.
macro_rules! bench {
    ($ct:ident, $ticks:expr $(, $cache:ident)?) => {{
        use $ct::*;
        const TOKEN: &str = "The quick brown fox streams a token. ";
        let mut fonts = FontSystem::new();
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut text = String::new();
        let mut shaping = Duration::ZERO;
        for tick in 0..$ticks {
            text.push_str(&format!("{tick:06} {TOKEN}"));
            let bytes = text.as_bytes();
            let window = &bytes[bytes.len().saturating_sub(735)..];
            let lines: Vec<&str> = window
                .chunks(105)
                .take(7)
                .map(|c| std::str::from_utf8(c).unwrap())
                .collect();
            let started = Instant::now();
            for line in lines {
                let attrs = AttrsList::new(&attrs);
                let mut line = BufferLine::new(line, LineEnding::None, attrs, Shaping::Advanced);
                line.shape(&mut fonts, 8);
            }
            shaping += started.elapsed();
            $(fonts.$cache.trim(2);)?
        }
        shaping.as_secs_f64() * 1e6 / $ticks as f64
    }};
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("bench") {
        // `bench patched`: only the patch, long enough to profile.
        if std::env::args().nth(2).as_deref() == Some("patched") {
            println!(
                "shaping per tick: patched {:.2} µs",
                bench!(patched, 400_000, shape_run_cache)
            );
            return;
        }
        for _ in 0..3 {
            let upstream = bench!(upstream, 50_000);
            let patched = bench!(patched, 50_000, shape_run_cache);
            println!("shaping per tick: upstream (no word cache) {upstream:.2} µs, patched {patched:.2} µs");
        }
        return;
    }
    // ASCII including controls and paragraph separators, and lines mixing in
    // accents, CJK, right-to-left scripts, emoji and bidi controls.
    let ascii: Vec<char> = (0u8..128).map(char::from).filter(|c| *c != '\n').collect();
    let other = ["é", "ü", "日本", "שלום", "مرحبا", "🙂", "\u{200f}", "\u{202e}", "\u{2029}", "ﬁ", "\u{85}"];
    let mut seed = 0x9e37_79b9_7f4a_7c15;
    let mut texts: Vec<String> = ["", " ", "\t", "\n", "a\n", "\r\n", "12 34", "  lead", "trail  ", "x\u{1c}y", "fi ffl"]
        .map(String::from)
        .into();
    for _ in 0..4000 {
        let mixed = next(&mut seed) % 4 == 0;
        let text = (0..next(&mut seed) % 200)
            .map(|_| match mixed && next(&mut seed) % 10 == 0 {
                true => other[(next(&mut seed) % other.len() as u64) as usize].to_string(),
                false => ascii[(next(&mut seed) % ascii.len() as u64) as usize].to_string(),
            })
            .collect();
        texts.push(text);
    }
    let mut up = upstream::FontSystem::new();
    let mut pa = patched::FontSystem::new();
    std::panic::set_hook(Box::new(|_| {}));
    let (mut identical, mut both_panicked) = (0, 0);
    for (i, text) in texts.iter().enumerate() {
        for (width, wrap) in [(1e6, 0), (120., 1), (60., 2), (240., 3)] {
            let a = catch_unwind(AssertUnwindSafe(|| layout!(upstream, &mut up, text, width, wrap)));
            let b = catch_unwind(AssertUnwindSafe(|| layout!(patched, &mut pa, text, width, wrap)));
            match (a, b) {
                (Ok(a), Ok(b)) if a == b => identical += 1,
                // Upstream panics on paragraphs of mixed direction in one line.
                (Err(_), Err(_)) => {
                    both_panicked += 1;
                    up = upstream::FontSystem::new();
                    pa = patched::FontSystem::new();
                }
                (a, b) => {
                    let _ = std::panic::take_hook();
                    let first = match (&a, &b) {
                        (Ok(a), Ok(b)) => a.iter().zip(b).find(|(x, y)| x != y).map(|(x, y)| format!("{x}\n  {y}")),
                        _ => None,
                    };
                    panic!("text {i} {text:?} (wrap {wrap}) differs: {:?}", first.unwrap_or_else(|| format!("upstream panicked {}, patched panicked {}", a.is_err(), b.is_err())));
                }
            }
        }
        if i % 4 == 3 {
            pa.shape_run_cache.trim(1);
        }
    }
    println!("{identical} identical layouts; both panicked on {both_panicked}");
}
