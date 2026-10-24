//! Text measurement benchmark: many rich paragraphs measured cold, again at
//! the same width, at a new width (a resize) and while one paragraph streams.
//! Run: cargo run --release -p zgx-gpu --example text_bench
use std::time::Instant;
use zgx::{
    rich_text::{RichText, TextRun},
    text_layout::{FontFamily, FontStyle},
};

const WORDS: &[&str] = &[
    "the",
    "renderer",
    "keeps",
    "every",
    "glyph",
    "retained",
    "while",
    "streaming",
    "tokens",
    "arrive",
    "layout",
    "measures",
    "paragraphs",
    "at",
    "two",
    "widths",
    "per",
    "pass",
    "and",
    "virtualized",
    "transcripts",
    "need",
    "exact",
    "heights",
    "before",
    "mounting",
    "views",
    "fonts",
    "shape",
    "once",
    "wrapping",
    "is",
    "arithmetic",
    "over",
    "cached",
    "advances",
];

fn paragraph(seed: usize, chars: usize) -> RichText {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut i = seed;
    let body = FontStyle::default();
    let bold = FontStyle {
        weight: 700,
        ..FontStyle::default()
    };
    let code = FontStyle {
        family: FontFamily::Monospace,
        ..FontStyle::default()
    };
    while text.len() < chars {
        let font = match i % 11 {
            3 => bold.clone(),
            7 => code.clone(),
            _ => body.clone(),
        };
        let start = text.len();
        for _ in 0..(1 + i % 4) {
            text.push_str(WORDS[(i * 7 + seed) % WORDS.len()]);
            text.push(' ');
            i += 1;
        }
        runs.push(TextRun {
            range: start..text.len(),
            font,
            font_size: 14.,
            ..Default::default()
        });
    }
    RichText::new(text, runs).unwrap()
}

fn plain(text: &str) -> RichText {
    let runs = if text.is_empty() {
        vec![]
    } else {
        vec![TextRun {
            range: 0..text.len(),
            font_size: 14.,
            ..Default::default()
        }]
    };
    RichText::new(text, runs).unwrap()
}

fn main() {
    let context = zgx_gpu::GpuContext::new().expect("GPU");
    let fonts = context.text_system();
    let cache = context.text_cache();
    let prepared = std::env::var_os("PREPARED").is_some();
    let measure = |rich: &RichText, width: f32| {
        if prepared {
            cache
                .borrow_mut()
                .measure(&mut fonts.borrow_mut(), rich, Some(width))
        } else {
            zgx_gpu::text::ShapedText::with_runs(&mut fonts.borrow_mut(), rich, Some(width)).size()
        }
    };
    let paragraphs: Vec<RichText> = (0..1000)
        .map(|i| paragraph(i, 200 + (i * 37) % 400))
        .collect();
    // Both paths must agree on every size, measured from shaped lines (the
    // first width) and from compact measurements (the rest).
    let mut odd = vec![
        plain(""),
        plain("\n\nsupercalifragilisticexpialidocious  wrapped\n"),
        plain("a   b    c\tdone\r\nnext  "),
    ];
    let mixed = "Heading then body text in a smaller size with code";
    odd.push(
        RichText::new(
            mixed,
            vec![
                TextRun {
                    range: 0..8,
                    font: FontStyle {
                        weight: 700,
                        ..FontStyle::default()
                    },
                    font_size: 22.,
                    ..Default::default()
                },
                TextRun {
                    range: 8..mixed.len(),
                    font: FontStyle::default(),
                    font_size: 13.,
                    ..Default::default()
                },
            ],
        )
        .unwrap(),
    );
    let widths = [
        None,
        Some(736.),
        Some(700.),
        Some(320.),
        Some(211.3),
        Some(90.),
        Some(41.7),
        Some(12.),
        Some(1.),
    ];
    let parity = zgx_gpu::prepared::TextCache::default();
    let parity = std::cell::RefCell::new(parity);
    for p in paragraphs[..200].iter().chain(&odd) {
        for width in widths {
            let old =
                zgx_gpu::text::ShapedText::with_runs(&mut fonts.borrow_mut(), p, width).size();
            let new = parity
                .borrow_mut()
                .measure(&mut fonts.borrow_mut(), p, width);
            assert_eq!(old, new, "width {width:?}: {:?}", p.text());
        }
    }
    // Warm fonts and fallback lookups once, as a running app would have.
    measure(&paragraphs[0], 736.);

    let time = |label: &str, mut run: Box<dyn FnMut()>| {
        let start = Instant::now();
        run();
        let elapsed = start.elapsed();
        println!("{label:32} {:8.2} ms", elapsed.as_secs_f64() * 1e3);
    };
    time(
        "1000 paragraphs, first measure",
        Box::new(|| {
            for p in &paragraphs {
                measure(p, 736.);
            }
        }),
    );
    time(
        "same paragraphs, same width",
        Box::new(|| {
            for p in &paragraphs {
                measure(p, 736.);
            }
        }),
    );
    time(
        "same paragraphs, new width",
        Box::new(|| {
            for p in &paragraphs {
                measure(p, 700.);
            }
        }),
    );
    let full = paragraph(99_999, 3000);
    let text = full.text().to_owned();
    time(
        "stream 3000 chars, 6 per step",
        Box::new(|| {
            let mut end = 6;
            while end < text.len() {
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                let runs: Vec<TextRun> = full
                    .runs()
                    .iter()
                    .filter(|r| r.range.start < end)
                    .map(|r| TextRun {
                        range: r.range.start..r.range.end.min(end),
                        ..r.clone()
                    })
                    .collect();
                let partial = RichText::new(&text[..end], runs).unwrap();
                measure(&partial, 736.);
                measure(&partial, 968.);
                end += 6;
            }
        }),
    );
    let (shaped, compact) = cache.borrow().bytes();
    println!(
        "prepared: {:.1} MB shaped, {:.1} MB compact",
        shaped as f64 / 1048576.,
        compact as f64 / 1048576.
    );
}
