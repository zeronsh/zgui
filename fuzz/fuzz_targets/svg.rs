//! An SVG (or a raster served where one was asked for), drawn at a size.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&size, svg)) = data.split_first() else {
        return;
    };
    // Small sizes keep runs fast; 1 to 64 pixels a side.
    let side = u32::from(size % 64) + 1;
    let _ = zgui_gpu::assets::decode_svg(svg, side, side.max(u32::from(size >> 2)));
});
