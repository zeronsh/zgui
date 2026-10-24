//! A PNG or JPEG from a file or the network.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zgui_gpu::assets::decode_image(data);
});
