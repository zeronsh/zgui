//! A caret blinking in a still window (see zgx_workload::blink) in Rune:
//! the caret's opacity is a `Live` value the poll flips each phase, asking
//! for its next poll a phase later (`Tx::wake_after`).
//!
//!     ZGX_SECONDS=10 bin/blink
use std::time::{Duration, Instant};

use rune::{AppSpec, Cx, El, Live, Tx, el};
use zgx_compare_rune::rgb;
use zgx_workload::{Workload, blink::*};

struct Scene {
    caret: Live<f32>,
    shown: bool,
    seconds: f64,
    start: Option<Instant>,
    frames: u64,
}

fn view(s: &Scene, _: &mut Cx<Scene>) -> El {
    let (x, y, w, h) = CARET;
    el().w_full().h_full().fill(rgb(BACKGROUND)).child(
        el().absolute(x, y)
            .size(w, h)
            .fill(rgb(CARET_COLOR))
            .live_opacity(&s.caret),
    )
}

fn poll(s: &mut Scene, _clock: f64, tx: &mut Tx) -> bool {
    let start = *s.start.get_or_insert_with(Instant::now);
    if s.seconds > 0.0 && start.elapsed().as_secs_f64() >= s.seconds {
        eprintln!("blink_frames={}", s.frames);
        tx.quit();
        return false;
    }
    s.frames += 1;
    s.shown = !s.shown;
    s.caret.set(if s.shown { 1.0 } else { 0.0 });
    tx.wake_after(Duration::from_millis(PHASE_MS));
    false
}

fn main() {
    let spec = AppSpec::<Scene> {
        poll: Some(poll),
        ..AppSpec::new(
            "Rune blink",
            (960.0, 720.0),
            || Scene {
                caret: Live::new(1.0),
                shown: true,
                seconds: Workload::from_env().seconds,
                start: None,
                frames: 0,
            },
            view,
        )
    };
    zgx_compare_rune::run(spec, None);
}
