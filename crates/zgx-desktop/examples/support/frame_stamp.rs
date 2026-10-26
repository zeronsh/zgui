//! DIAGNOSTIC (`ZGX_FRAME_STAMP=1`): a row of 12 cells in the window's top
//! left, black or white by the bits of a frame counter, then a white and a
//! black marker cell, so a capture of the
//! window says which frame it shows (scripts/frameseq.swift `stamp`).
use zgx::{compose::prelude::*, reactive::Signal, scene::Color};

/// Whether frames carry the stamp.
pub fn enabled() -> bool {
    std::env::var_os("ZGX_FRAME_STAMP").is_some()
}

/// `view` with the stamp of `frame` laid over it.
pub fn over(view: View, frame: Signal<u64>, width: f32, height: f32) -> View {
    div()
        .size(width, height)
        .child(view)
        .children((0..14u32).map(move |bit| {
            let frame = frame.clone();
            div()
                .absolute()
                .ml(8. + bit as f32 * 8.)
                .mt(8.)
                .size(8., 8.)
                .reactive_style(move || {
                    let on = match bit {
                        12 => true,
                        13 => false,
                        _ => frame.get() >> bit & 1 == 1,
                    };
                    Styles::new().bg(if on {
                        Color(255, 255, 255, 255)
                    } else {
                        Color(0, 0, 0, 255)
                    })
                })
        }))
}
