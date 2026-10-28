//! What a script passes to Rust never panics or aborts it: a value no
//! duration, count or size can be (NaN, infinite, absurdly large) is a
//! script error naming the script's line, or is clamped where a large value
//! has a meaning (a timer a year away), and the app keeps running.
mod common;

use common::Harness;
use std::sync::atomic::{AtomicUsize, Ordering};
use zgx::reactive::Runtime;
use zgx_script::ScriptHost;

/// `code` as a script file of its own (`anime` is there only for files).
fn script(code: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "zgx-crash-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.luau");
    std::fs::write(&path, code).unwrap();
    path
}

/// The error `code` gives as it loads, or in its first frames (an
/// animation builds its tracks as it starts), if any.
fn error_of(code: &str) -> Option<String> {
    if let Some(error) = ScriptHost::new(&Runtime::new(), script(code))
        .reload()
        .err()
    {
        return Some(error);
    }
    let path = script(code);
    let h = Harness::new(|runtime| ScriptHost::new(runtime, path));
    h.frames(10);
    h.host.error().get()
}

/// `code` loads, and runs a few frames, without an error.
fn runs(code: &str) {
    let path = script(code);
    let h = Harness::new(|runtime| ScriptHost::new(runtime, path));
    h.frames(10);
    assert_eq!(h.host.error().get(), None, "{code}");
}

/// `code` fails as a script error on its line 1, naming `what`.
fn fails_at_line(code: &str, what: &str) {
    let error = error_of(code).unwrap_or_else(|| panic!("{code} runs"));
    assert!(error.contains("app.luau:1"), "{code}: {error}");
    assert!(error.contains(what), "{code}: {error}");
}

#[test]
fn durations_no_clock_can_hold_are_errors_and_long_ones_wait_a_year() {
    fails_at_line(
        "ui.toast { title = 'x', duration = math.huge } return ui.text 'x'",
        "ui.toast's duration",
    );
    fails_at_line(
        "return ui.div { tooltip = 'x', tooltip_delay = math.huge }",
        "tooltip_delay",
    );
    fails_at_line(
        "return ui.div { tooltip = 'x', tooltip_delay = 0/0 }",
        "tooltip_delay",
    );
    // Finite, but past what `Duration` or the clock holds: a year.
    runs("ui.every(1e300, function() end) return ui.text 'x'");
    runs("ui.after(1e300, function() end) return ui.text 'x'");
    runs("ui.after(1e22, function() end) return ui.text 'x'");
    runs("ui.toast { title = 'x', duration = 1e300 } return ui.text 'x'");
    runs("return ui.div { tooltip = 'x', tooltip_delay = 1e300 }");
}

#[test]
fn anime_utils_take_nan_bounds_and_refuse_absurd_sizes() {
    runs(
        "assert(anime.utils.clamp(1, 0/0, 0/0) == 1) \
         assert(anime.utils.clamp(5, 3, 0) == 3) return ui.text 'x'",
    );
    fails_at_line(
        "local s = anime.utils.roundPad(1, 1e18) return ui.text 'x'",
        "roundPad",
    );
    runs("assert(anime.utils.roundPad(1.5, 2) == '1.50') return ui.text 'x'");
    fails_at_line(
        "local s = anime.utils.padStart('x', 1e18) return ui.text 'x'",
        "padStart",
    );
    fails_at_line(
        "local s = anime.utils.padEnd('x', 1e18, 'ab') return ui.text 'x'",
        "padEnd",
    );
    runs("assert(anime.utils.padStart('7', 3, '0') == '007') return ui.text 'x'");
}

#[test]
fn anime_timing_that_is_no_number_is_an_error() {
    for (key, value) in [
        ("loopDelay", "math.huge"),
        ("loopDelay", "0/0"),
        ("loopDelay", "-math.huge"),
        ("duration", "math.huge"),
        ("duration", "0/0"),
        ("delay", "math.huge"),
        ("playbackRate", "0/0"),
        ("frameRate", "math.huge"),
        ("loop", "0/0"),
    ] {
        fails_at_line(
            &format!(
                "anime.animate(ui.state('x', 0), {{ value = 1, {key} = {value} }}) return ui.text 'x'"
            ),
            "",
        );
    }
    // `loop = true` loops for ever, with or without delays between.
    runs(
        "anime.animate(ui.state('x', 0), { value = 1, loop = true, loopDelay = 100 }) return ui.text 'x'",
    );
    runs(
        "anime.animate(ui.state('x', 0), { value = 1, duration = 0, loop = true }) return ui.text 'x'",
    );
    runs("anime.animate(ui.state('x', 0), { value = 1, loopDelay = 1e300 }) return ui.text 'x'");
}

/// Every duration and number entry point, given the values a fuzzer finds:
/// each loads as a script error or runs, and nothing panics.
#[test]
fn every_duration_and_number_entry_point_errors_or_runs() {
    const VALUES: &[&str] = &[
        "0/0",
        "math.huge",
        "-math.huge",
        "1e300",
        "1e22",
        "-1",
        "0",
        "1e-300",
        "2^53",
    ];
    const ENTRIES: &[&str] = &[
        "ui.every(V, function() end) return ui.text 'x'",
        "ui.after(V, function() end) return ui.text 'x'",
        "ui.toast { title = 'x', duration = V } return ui.text 'x'",
        "return ui.div { tooltip = 'x', tooltip_delay = V }",
        "anime.animate(ui.state('x', 0), { value = 1, duration = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, delay = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, loop = 2, loopDelay = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, loopDelay = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, loop = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, playbackRate = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = 1, frameRate = V }) return ui.text 'x'",
        "anime.animate(ui.state('x', 0), { value = { { to = 1, duration = V } } }) return ui.text 'x'",
        "local t = anime.createTimer({ duration = V }) return ui.text 'x'",
        "local s = anime.utils.clamp(V, V, V) return ui.text 'x'",
        "local s = anime.utils.roundPad(1, math.min(math.max(V, 0), 1e18)) return ui.text 'x'",
        "local s = anime.utils.padStart('x', math.min(math.max(V, 0), 1e18)) return ui.text 'x'",
        "local s = anime.utils.padEnd('x', math.min(math.max(V, 0), 1e18)) return ui.text 'x'",
        "local s = anime.utils.snap(V, V) .. anime.utils.wrap(V, 0, V) return ui.text 'x'",
        "local s = anime.utils.mapRange(V, 0, V, 0, 1) + anime.utils.lerp(0, V, V) return ui.text 'x'",
    ];
    for entry in ENTRIES {
        for value in VALUES {
            let code = entry.replace('V', value);
            if let Some(error) = error_of(&code) {
                assert!(error.contains("app.luau:1"), "{code}: {error}");
            }
        }
    }
}
