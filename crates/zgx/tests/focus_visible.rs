use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    widgets::Ui,
};

/// Everything a focus ring could change about a node's drawing.
fn look(ui: &Ui, node: NodeId) -> String {
    let scene = ui.scene.borrow();
    format!(
        "{:?} {:?} {:?}",
        scene.style(node),
        scene.effects(node),
        scene.kind(node)
    )
}

fn settle(ui: &mut Ui) {
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
}

#[test]
fn a_clicked_button_shows_no_focus_ring_but_a_tabbed_one_does() {
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(
        column()
            .child(button().id("a").size(80., 30.).child(text("A")))
            .child(button().id("b").size(80., 30.).child(text("B"))),
    );
    let (a, b) = (view.find("a").unwrap(), view.find("b").unwrap());
    settle(&mut ui);
    let resting = look(&ui, a);

    ui.dispatch(InputEvent::PointerDown {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerMove { x: 250., y: 150. });
    settle(&mut ui);
    assert_eq!(ui.input.focused(), Some(a));
    assert_eq!(
        look(&ui, a),
        resting,
        "clicking focuses the button without drawing its ring"
    );

    let tab = |ui: &mut Ui, shift: bool| {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
            repeat: false,
        })
    };
    tab(&mut ui, false);
    tab(&mut ui, true);
    settle(&mut ui);
    assert_eq!(ui.input.focused(), Some(a));
    assert_ne!(
        look(&ui, a),
        resting,
        "focus from the keyboard draws the ring"
    );
    assert_ne!(Some(b), ui.input.focused());
}

#[test]
fn a_key_pressed_on_a_clicked_button_shows_its_ring_but_a_shortcut_does_not() {
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(column().child(button().id("a").size(80., 30.).child(text("A"))));
    let a = view.find("a").unwrap();
    settle(&mut ui);
    let resting = look(&ui, a);
    ui.dispatch(InputEvent::PointerDown {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerMove { x: 250., y: 150. });
    settle(&mut ui);
    assert_eq!(ui.input.focused(), Some(a));
    assert_eq!(look(&ui, a), resting);

    let key = |ui: &mut Ui, key: Key, modifiers: Modifiers| {
        ui.dispatch(InputEvent::KeyDown {
            key: key.clone(),
            modifiers,
            repeat: false,
        });
        ui.dispatch(InputEvent::KeyUp { key, modifiers });
        settle(ui);
    };
    let meta = Modifiers {
        meta: true,
        ..Default::default()
    };
    key(&mut ui, Key::Character("c".into()), meta);
    assert_eq!(look(&ui, a), resting, "a shortcut chord draws no ring");

    key(&mut ui, Key::Space, Modifiers::default());
    assert_eq!(ui.input.focused(), Some(a));
    assert_ne!(
        look(&ui, a),
        resting,
        "Space activates the button with its ring showing"
    );
}

#[test]
fn an_editor_shows_its_focus_style_however_it_was_focused() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal(String::new());
    let view = ui.mount(column().child(text_input("Name", value).id("field").w(200.).h(30.)));
    let field = view.find("field").unwrap();
    settle(&mut ui);
    let resting = look(&ui, field);
    ui.dispatch(InputEvent::PointerDown {
        x: 20.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 20.,
        y: 10.,
        button: PointerButton::Primary,
    });
    settle(&mut ui);
    assert_ne!(look(&ui, field), resting);
}
