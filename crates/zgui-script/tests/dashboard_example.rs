//! The dashboard example (examples/dashboard), run headless: a live feed
//! over 100,000 rows, sorting, filtering, number formats, resizable panes,
//! themes, and what a tick costs.
mod common;

use common::Harness;
use std::time::Instant;
use zgui::input::{InputEvent, Key, PointerButton};
use zgui_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/dashboard/app.luau");

fn dashboard() -> Harness {
    // A debug build runs Luau with its asserts, some 5 times slower: sorting
    // 100,000 rows takes it past the release budgets.
    let limits = zgui_script::Limits {
        load: std::time::Duration::from_secs(if cfg!(debug_assertions) { 10 } else { 1 }),
        call: std::time::Duration::from_millis(if cfg!(debug_assertions) { 2000 } else { 50 }),
        ..Default::default()
    };
    let h = Harness::new(|runtime| ScriptHost::new(runtime, APP).limits(limits));
    h.ui.scene.borrow_mut().resize(1280., 800.);
    h.frames(2);
    h
}

/// The first `n` rows' cells in column `key`.
fn column(h: &mut Harness, key: &str, n: usize) -> Vec<String> {
    (1..=n).map(|i| h.text(&format!("{key}-{i}"))).collect()
}
/// A shown price ("1,234.56") as a number.
fn price(text: &str) -> f64 {
    text.replace(',', "")
        .parse()
        .unwrap_or_else(|_| panic!("a price: {text}"))
}

#[test]
fn the_table_sorts_and_follows_the_feed() {
    let mut h = dashboard();
    assert_eq!(h.error(), None);
    let symbols = column(&mut h, "symbol", 10);
    assert!(
        symbols.windows(2).all(|w| w[0] <= w[1]),
        "by symbol: {symbols:?}"
    );
    assert_eq!(h.text("sort-symbol"), "Symbol ↑");

    h.click("sort-price");
    let prices: Vec<f64> = column(&mut h, "price", 10)
        .iter()
        .map(|p| price(p))
        .collect();
    assert!(
        prices.windows(2).all(|w| w[0] <= w[1]),
        "by price: {prices:?}"
    );
    assert!(prices[0] < 6., "the cheapest of 100,000 first: {prices:?}");
    h.click("sort-price");
    assert_eq!(h.text("sort-price"), "Price ↓");
    let prices: Vec<f64> = column(&mut h, "price", 10)
        .iter()
        .map(|p| price(p))
        .collect();
    assert!(
        prices.windows(2).all(|w| w[0] >= w[1]),
        "descending: {prices:?}"
    );

    // The feed moves prices; sorted by price, the order holds through it.
    for _ in 0..20 {
        assert!(h.host.run_command("refresh"));
        h.frame();
    }
    let after: Vec<f64> = column(&mut h, "price", 10)
        .iter()
        .map(|p| price(p))
        .collect();
    assert!(
        after.windows(2).all(|w| w[0] >= w[1]),
        "still sorted: {after:?}"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn search_and_sector_filter_the_rows() {
    let mut h = dashboard();
    let unfiltered = column(&mut h, "symbol", 3);
    h.type_text("search", "zq");
    let symbols = column(&mut h, "symbol", 5);
    assert!(
        symbols.iter().all(|s| s.contains("ZQ")),
        "the search matches symbols, whatever its case: {symbols:?}"
    );
    h.click("sector-Energy");
    let sectors = column(&mut h, "sector", 3);
    assert!(sectors.iter().all(|s| s == "Energy"), "{sectors:?}");
    let symbols = column(&mut h, "symbol", 3);
    assert!(
        symbols.iter().all(|s| s.contains("ZQ")),
        "both filters: {symbols:?}"
    );
    h.click("sector-All");
    h.click("search");
    for _ in 0..2 {
        h.key(Key::Backspace);
    }
    assert_eq!(
        column(&mut h, "symbol", 3),
        unfiltered,
        "cleared, every row again"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn numbers_show_formatted() {
    let mut h = dashboard();
    let price = h.text("price-1");
    assert!(
        price.split_once('.').is_some_and(|(_, d)| d.len() == 2),
        "two decimals: {price}"
    );
    let change = h.text("change-1");
    assert!(
        change.ends_with('%') && (change.starts_with('+') || change.starts_with('-')),
        "a signed percentage: {change}"
    );
    let volume = h.text("volume-1");
    assert!(
        volume.ends_with(['K', 'M']) || volume.parse::<f64>().is_ok(),
        "compact: {volume}"
    );
    let summary = h
        .texts()
        .into_iter()
        .find(|t| t.starts_with("Market value"))
        .expect("the summary");
    assert!(
        summary.starts_with("Market value $") && summary.ends_with(['B', 'T']),
        "{summary}"
    );
}

#[test]
fn the_divider_resizes_the_sidebar_within_its_limits() {
    let mut h = dashboard();
    let width = |h: &mut Harness| h.bounds("sidebar").width;
    assert_eq!(width(&mut h), 360.);
    let b = h.bounds("sidebar");
    let (x, y) = (b.x + b.width + 2.5, b.y + 200.);
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(InputEvent::PointerMove { x: x + 100., y });
    h.frame();
    assert_eq!(width(&mut h), 460., "dragged 100 points wider");
    // Past the second pane's minimum, it stops.
    h.ui.dispatch(InputEvent::PointerMove { x: x + 2000., y });
    h.frame();
    assert_eq!(width(&mut h), 640., "its max");
    h.ui.dispatch(InputEvent::PointerMove { x: 0., y });
    h.ui.dispatch(InputEvent::PointerUp { x: 0., y, button });
    h.frame();
    assert_eq!(width(&mut h), 240., "its min");
    // Focused, the arrows move it.
    h.key(Key::ArrowRight);
    assert_eq!(width(&mut h), 250.);
    h.key(Key::End);
    assert_eq!(width(&mut h), 640.);
    assert_eq!(h.error(), None);
}

#[test]
fn the_theme_switches_and_charts_follow_it() {
    let mut h = dashboard();
    assert_eq!(&*h.ui.theme().current(), "zgui-dark");
    assert_eq!(h.text("appearance"), "Light");
    h.click("appearance");
    h.frames(2);
    assert_eq!(&*h.ui.theme().current(), "zgui-light");
    assert_eq!(h.text("appearance"), "Dark");
    assert_eq!(h.error(), None, "the charts draw in theme colours");
}

#[test]
fn a_feed_tick_costs_what_it_shows() {
    let mut h = dashboard();
    // The live feed paused: only the ticks counted here run, however long
    // the frames take (the feed's timer fires every 250 ms of the clock).
    h.click("live");
    let tick = |h: &mut Harness| {
        let (calls, converted) = (h.calls(), zgui_script::values_converted());
        for _ in 0..10 {
            assert!(h.host.run_command("refresh"));
            h.frame();
        }
        (
            (h.calls() - calls) / 10,
            (zgui_script::values_converted() - converted) / 10,
        )
    };
    // Sorted by symbol, a tick changes what the visible rows show and the
    // charts: no filter or sort runs, and nothing of the 100,000 rows is
    // converted.
    let (calls, converted) = tick(&mut h);
    assert!(calls < 300, "{calls} script calls a tick");
    assert!(converted < 100, "{converted} values converted a tick");
    // Sorted by price, each tick sorts again: one list of indices.
    h.click("sort-price");
    let (calls, converted) = tick(&mut h);
    assert!(calls < 300, "{calls} script calls a tick");
    assert!(converted < 250_000, "{converted} values converted a tick");
}

#[test]
#[ignore = "a timing probe: cargo test --release -- --ignored --nocapture"]
fn how_long_the_dashboard_takes() {
    let start = Instant::now();
    let mut h = dashboard();
    eprintln!("load: {:?}", start.elapsed());
    let time = |h: &mut Harness, what: &str, f: &mut dyn FnMut(&mut Harness)| {
        let calls = h.host.script_calls();
        let start = Instant::now();
        for _ in 0..10 {
            f(h);
            h.frames(1);
        }
        eprintln!(
            "{what}: {:?} each, {} script calls",
            start.elapsed() / 10,
            (h.host.script_calls() - calls) / 10
        );
    };
    time(&mut h, "feed tick, sorted by symbol", &mut |h| {
        assert!(h.host.run_command("refresh"));
    });
    time(&mut h, "sort click", &mut |h| h.click("sort-volume"));
    h.click("sort-price");
    h.frames(1);
    time(&mut h, "feed tick, sorted by price", &mut |h| {
        assert!(h.host.run_command("refresh"));
    });
    time(&mut h, "search keystroke and backspace", &mut |h| {
        h.type_text("search", "B");
        h.frames(1);
        h.key(Key::Backspace);
    });
    time(&mut h, "sector chip", &mut |h| h.click("sector-Tech"));
    time(&mut h, "theme switch", &mut |h| h.click("appearance"));
    let b = h.bounds("sidebar");
    let (x, y) = (b.x + b.width + 2.5, b.y + 200.);
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown { x, y, button });
    let mut at = x;
    time(&mut h, "divider drag step", &mut |h| {
        at += 3.;
        h.ui.dispatch(InputEvent::PointerMove { x: at, y });
    });
    h.ui.dispatch(InputEvent::PointerUp { x: at, y, button });
    eprintln!("error: {:?}", h.error());
}
