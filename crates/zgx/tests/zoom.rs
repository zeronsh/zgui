//! A window's zoom, as a browser's page zoom: Chrome's levels, Command-plus,
//! minus and 0 (Control elsewhere) for keys nothing else took, an app's own
//! keymap keeping them, and scroll views keeping what was at their top.
use std::{cell::Cell, rc::Rc};
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}
fn press(ui: &mut Ui, key: &str) -> bool {
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character(key.into()),
        modifiers: command(),
        repeat: false,
    })
    .default_prevented
}
/// A host that counts the zooms it is told of.
fn hosted(ui: &Ui) -> Rc<Cell<usize>> {
    let told = Rc::new(Cell::new(0));
    let count = told.clone();
    ui.on_zoom(move || count.set(count.get() + 1));
    told
}

#[test]
fn the_keys_step_through_chromes_levels_and_the_host_hears_each_change() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(column().child(text("page")));
    let told = hosted(&ui);
    assert_eq!(ui.zoom(), 1.);
    assert!(press(&mut ui, "="));
    assert_eq!(ui.zoom(), 1.1);
    assert!(press(&mut ui, "+"));
    assert_eq!(ui.zoom(), 1.25);
    assert!(press(&mut ui, "-"));
    assert!(press(&mut ui, "-"));
    assert!(press(&mut ui, "-"));
    assert_eq!(ui.zoom(), 0.9);
    assert!(press(&mut ui, "0"));
    assert_eq!(ui.zoom(), 1.);
    assert_eq!(told.get(), 6);
    // The same zoom again tells nothing; the ends hold.
    ui.set_zoom(1.);
    assert_eq!(told.get(), 6);
    ui.set_zoom(9.);
    assert_eq!(ui.zoom(), 5.);
    ui.zoom_in();
    assert_eq!(ui.zoom(), 5.);
    ui.set_zoom(0.01);
    assert_eq!(ui.zoom(), 0.25);
    ui.zoom_out();
    assert_eq!(ui.zoom(), 0.25);
    ui.set_zoom(f64::NAN);
    assert_eq!(ui.zoom(), 0.25);
}

#[test]
fn option_or_both_command_and_control_with_the_keys_are_the_apps() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(column().child(text("page")));
    let told = hosted(&ui);
    let key = |key: &str| Key::Character(key.into());
    let option = Modifiers {
        alt: true,
        ..command()
    };
    let shift = Modifiers {
        shift: true,
        ..command()
    };
    let both = Modifiers {
        meta: true,
        control: true,
        ..Default::default()
    };
    // A terminal's font size on Command-Option, its panes evened out on
    // Command-Control-=, say: never a zoom.
    for modifiers in [option, both] {
        for k in ["=", "-", "0"] {
            assert!(!zgx::zoom::is_zoom_key(&key(k), modifiers), "{k}");
            ui.dispatch(InputEvent::KeyDown {
                key: key(k),
                modifiers,
                repeat: false,
            });
        }
    }
    assert_eq!((ui.zoom(), told.get()), (1., 0));
    // Command-Shift-= is Command-+, in; Command-Shift-0 is no reset.
    assert!(zgx::zoom::is_zoom_key(&key("+"), shift));
    assert!(!zgx::zoom::is_zoom_key(&key("0"), shift));
    assert!(!zgx::zoom::is_zoom_key(&key("="), Modifiers::default()));
}

#[test]
fn an_apps_keymap_keeps_the_keys_and_zoom_can_be_turned_off() {
    #[derive(Debug)]
    struct Bigger;
    let mut ui = Ui::new(400., 300.);
    let asked = Rc::new(Cell::new(0));
    let seen = asked.clone();
    let view = ui.mount(
        column()
            .keymap(
                Keymap::new().bind(
                    KeyBinding::new(
                        [Keystroke::new(Key::Character("=".into()), command())],
                        Action::new(Bigger),
                    )
                    .unwrap(),
                ),
            )
            .on_action(move |_: &Bigger, cx| {
                seen.set(seen.get() + 1);
                cx.prevent_default();
            })
            .child(button().id("button").child(text("Go"))),
    );
    hosted(&ui);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("button"));
    press(&mut ui, "=");
    assert_eq!((asked.get(), ui.zoom()), (1, 1.), "the app's own");
    assert!(press(&mut ui, "-"), "unbound: the window's");
    assert_eq!(ui.zoom(), 0.9);
    ui.set_zoomable(false);
    assert!(!press(&mut ui, "0"));
    assert_eq!(ui.zoom(), 0.9);
    // No host (headless): the keys go on.
    let mut ui = Ui::new(400., 300.);
    ui.mount(column().child(text("page")));
    assert!(!press(&mut ui, "="));
    assert_eq!(ui.zoom(), 1.);
}

#[test]
fn a_scroll_view_keeps_what_was_at_its_top_through_a_zoom() {
    let mut ui = Ui::new(300., 200.);
    let offset = ui.signal(0_f32);
    let paragraphs: Vec<View> = (0..30)
        .map(|i| {
            text(format!("Paragraph {i}: {}", "words that wrap ".repeat(6)))
                .id(format!("p{i}"))
                .text_wrap(true)
        })
        .collect();
    let view = ui.mount(
        scroll(offset.clone())
            .w_full()
            .h(200.)
            .child(column().w_full().children(paragraphs)),
    );
    ui.prepare_frame();
    // Scrolled to paragraph 12's top.
    let top = |ui: &Ui, id: &str| {
        let scene = ui.scene.borrow();
        scene.bounds(view.find(id).unwrap()).y - scene.bounds(view.node()).y
    };
    offset.set(top(&ui, "p12"));
    ui.prepare_frame();
    assert!(top(&ui, "p12").abs() < 1.);
    let anchors = ui.zoom_anchors();
    // As a zoom to 150% leaves a 300-wide window: 200 points across.
    ui.scene.borrow_mut().resize(200., 133.);
    ui.prepare_frame();
    assert!(top(&ui, "p12").abs() > 20., "reflowed: it moved");
    ui.restore_zoom_anchors(anchors);
    assert!(
        top(&ui, "p12").abs() < 1.,
        "back at the top: {}",
        top(&ui, "p12")
    );
}

#[test]
fn a_control_wheel_nothing_takes_zooms_a_level_a_notch_and_scrolls_nothing() {
    let mut ui = Ui::new(300., 200.);
    let offset = ui.signal(0_f32);
    let rows: Vec<View> = (0..40).map(|i| text(format!("Row {i}"))).collect();
    let view = ui.mount(
        scroll(offset.clone())
            .w_full()
            .h(200.)
            .child(column().w_full().children(rows)),
    );
    ui.prepare_frame();
    let told = hosted(&ui);
    assert!(ui.wheel_zooms());
    // The host's Control-wheel: to the nodes first, then the zoom.
    let wheel = |ui: &mut Ui, delta_y: f32, notches: f32| {
        let taken = zgx::zoom::wheel(|| {
            ui.dispatch(InputEvent::Scroll {
                x: 100.,
                y: 100.,
                delta_x: 0.,
                delta_y,
            })
            .default_prevented
        });
        if !taken {
            ui.zoom_wheel(notches);
        }
        taken
    };
    // Up a notch: in a level, the scroll view where it was.
    assert!(!wheel(&mut ui, -40., 1.));
    assert_eq!((ui.zoom(), offset.get(), told.get()), (1.1, 0., 1));
    // A touchpad's fractions add up to a notch.
    wheel(&mut ui, 16., -0.4);
    wheel(&mut ui, 16., -0.4);
    assert_eq!(ui.zoom(), 1.1);
    wheel(&mut ui, 16., -0.4);
    assert_eq!(ui.zoom(), 1.);
    // A plain wheel still scrolls.
    ui.dispatch(InputEvent::Scroll {
        x: 100.,
        y: 100.,
        delta_x: 0.,
        delta_y: 40.,
    });
    assert_eq!(offset.get(), 40.);
    // What takes the wheel itself (a canvas's zoom) keeps it.
    ui.on_event(view.node(), false, |cx| {
        if matches!(cx.event, InputEvent::Scroll { .. }) {
            cx.prevent_default();
        }
    });
    assert!(wheel(&mut ui, -40., 1.));
    assert_eq!(ui.zoom(), 1.);
    // Off, the host does not ask.
    ui.set_zoomable(false);
    assert!(!ui.wheel_zooms());
}
