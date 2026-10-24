//! Markdown as messages and documents show it (`ui.markdown`).
#![no_main]
use libfuzzer_sys::fuzz_target;
use zgx_script::markdown::{Options, parse};

fuzz_target!(|data: &[u8]| {
    let Some((&flags, text)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    let options = Options {
        breaks: flags & 1 != 0,
        paths: flags & 2 != 0,
        code: None,
    };
    parse(text, &options);
});
