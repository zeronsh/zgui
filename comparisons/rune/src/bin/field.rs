//! A focused text field left alone (the suite's field scene) in Rune: its
//! own text input, focused from the start, its caret drawn as Rune draws it
//! (it does not blink).
//!
//!     ZGX_SECONDS=30 bin/field
use std::time::Instant;

use rune::tok::TextStyle;
use rune::{AppSpec, Color, Cx, El, TextEdit, Tx, el, text_input};
use zgx_compare_rune::rgb;
use zgx_workload::{Workload, blink::*};

struct Scene {
    edit: TextEdit,
    seconds: f64,
    start: Option<Instant>,
}

fn view(s: &Scene, _: &mut Cx<Scene>) -> El {
    el().w_full().h_full().fill(rgb(BACKGROUND)).pl(40.0).pt(40.0).child(
        text_input(&s.edit, TextStyle::new(14.0, 400.0))
            .single_line()
            .key("field")
            .w(400.0)
            .color(rgb(CARET_COLOR))
            .caret(rgb(CARET_COLOR), Color::WHITE),
    )
}

fn poll(s: &mut Scene, _clock: f64, tx: &mut Tx) -> bool {
    let start = *s.start.get_or_insert_with(Instant::now);
    if s.seconds > 0.0 && start.elapsed().as_secs_f64() >= s.seconds {
        eprintln!("workload_ticks=0");
        tx.quit();
        return false;
    }
    tx.wake_after(std::time::Duration::from_secs_f64((s.seconds - start.elapsed().as_secs_f64()).max(0.05)));
    false
}

fn main() {
    let spec = AppSpec::<Scene> {
        poll: Some(poll),
        ..AppSpec::new(
            "Rune field",
            (960.0, 720.0),
            || {
                let mut edit = TextEdit::new("Reading, not typing");
                edit.focused = true;
                Scene {
                    edit,
                    seconds: Workload::from_env().seconds,
                    start: None,
                }
            },
            view,
        )
    };
    zgx_compare_rune::run(spec, None);
}
