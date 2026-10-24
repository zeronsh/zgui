//! JSON a script receives (`ui.fetch`'s replies, web views' messages): read,
//! and written back.
#![no_main]
use libfuzzer_sys::fuzz_target;
use zgui_script::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(value) = Value::from_json(text) {
        let json = value.to_json();
        assert!(Value::from_json(&json).is_some(), "{json}");
    }
});
