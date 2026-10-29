//! The widget gallery's interface (`examples/gallery.rs`), shared with the
//! accessibility audit (`tests/accessibility_audit.rs`), which mounts it
//! headless. `async_update` runs the Async update button's work.
use zgx::{
    reactive::Signal,
    scene::{Layout, Style},
    widgets::{Ui, fixed},
};
pub fn build(ui: &mut Ui, async_update: impl Fn(Signal<String>) + 'static) {
    let root = ui.root();
    let mut style = fixed(900., 760.);
    style.padding = 24.;
    style.gap = 16.;
    ui.scene.borrow_mut().set_style(root, style);
    ui.label(root, "zgx · Linux, macOS & Windows", fixed(820., 28.));
    ui.label(
        root,
        "Retained widgets · GPU shaping · keyboard navigation · native accessibility",
        fixed(840., 26.),
    );
    let count = ui.signal(0i32);
    let status = ui.signal(String::from("Ready"));
    let row = ui.container(
        root,
        Layout::Row,
        Style {
            gap: 12.,
            ..Default::default()
        },
    );
    let n = count.clone();
    let text = status.clone();
    ui.button(row, "Increment", 130., move || {
        n.update(|n| *n += 1);
        text.set(format!("Count: {}", n.get()));
    });
    let text = status.clone();
    ui.button(row, "Async update", 160., move || {
        async_update(text.clone())
    });
    let disabled = ui.button(row, "Disabled", 130., || {});
    ui.set_disabled(disabled, true);
    ui.label_signal(root, status, fixed(800., 26.));
    let checked = ui.signal(true);
    ui.checkbox(root, "Enable notifications", checked, 320.);
    let amount = ui.signal(0.4);
    ui.label(root, "Volume — drag or use arrow keys", fixed(800., 24.));
    ui.slider(root, "Volume", amount.clone(), 0.0..=1.0, 420.);
    ui.progress(root, "Volume level", amount, 420.);
    ui.label(
        root,
        "Name — selection, clipboard, undo, IME",
        fixed(800., 24.),
    );
    let name = ui.signal(String::from("Hello, 世界 👋"));
    ui.text_input(root, "Name", name, 520., false);
    ui.label(root, "Notes — multiline editing", fixed(800., 24.));
    let notes=ui.signal(String::from("Tab moves focus. Shift+Tab moves back.\nTry selecting Unicode text and composing with your IME.\nUse Ctrl/Cmd+A, C, X, V, Z."));
    ui.text_input(root, "Notes", notes, 780., true);
}
