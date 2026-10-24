//! A theme file (JSON with comments and trailing commas).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = zgui::theme::Theme::from_json(text);
    }
});
