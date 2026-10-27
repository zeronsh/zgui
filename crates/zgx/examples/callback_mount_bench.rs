//! Mounting rows of text inputs that each run a callback of the app's own
//! (a reactive style), headless: how the mount's cost grows with its rows.
//! `READS=1`: each callback reads its input's geometry, as a style derived
//! from a measured width does.
//! Run: cargo run -p zgx --release --example callback_mount_bench [rows]
use std::time::Instant;
use zgx::{
    compose::{column, row, text_input},
    style::{Styled, Styles},
    widgets::Ui,
};

fn main() {
    let rows: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let reads = std::env::var_os("READS").is_some_and(|v| v == "1");
    let mut ui = Ui::new(1200., 800.);
    let values: Vec<_> = (0..rows).map(|_| ui.signal(String::new())).collect();
    let scene = ui.scene.clone();
    let started = Instant::now();
    let view = column().children(values.into_iter().map(|value| {
        let scene = scene.clone();
        row()
            .gap(8.)
            .child(text_input("Name", value).w(160.))
            .reactive_style(move || {
                let width = match reads {
                    true => {
                        let scene = scene.borrow();
                        scene.bounds(scene.root()).width
                    }
                    false => 0.,
                };
                Styles::default().h(28. + width * 0.)
            })
    }));
    let _handle = ui.mount(view);
    ui.scene.borrow_mut().flush();
    // `PICTURE=1`: a hash of what the mount paints, every item's fields.
    if std::env::var_os("PICTURE").is_some_and(|v| v == "1") {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let scene = ui.scene.borrow();
        for item in scene.layer_items(None) {
            format!(
                "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                item.id,
                item.bounds,
                item.kind,
                item.font,
                item.text_options,
                item.effects,
                item.clip,
                item.extras
            )
            .hash(&mut hash);
        }
        println!("picture {:016x}", hash.finish());
    }
    println!(
        "{rows} rows, callbacks {}: mount {:.2} ms",
        if reads {
            "reading geometry"
        } else {
            "not reading geometry"
        },
        started.elapsed().as_secs_f64() * 1e3
    );
}
