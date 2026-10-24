//! An animated GIF from a file or the network.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = zgui_gpu::assets::decode_gif(data);
});
