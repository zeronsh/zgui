use zgx::scene::{Color, Effects, Layout, NodeKind, Rect, Scene, Style, Transform};

fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Style::default()
    }
}

#[test]
fn transparent_ancestor_filter_does_not_expand_background_damage() {
    let mut scene = Scene::new(240., 200.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let background = scene.append(
        scene.root(),
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(5., 5.),
    );
    scene.set_transform(background, Transform { x: 50., y: 50. });
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(200., 200.),
    );
    let blur = scene.append(
        group,
        NodeKind::Rect(Color(255, 255, 255, 100)),
        fixed(100., 100.),
    );
    scene.set_transform(blur, Transform { x: 40., y: 40. });
    scene.set_effects(
        blur,
        Effects {
            blur_radius: 10.,
            ..Effects::default()
        },
    );
    scene.set_effects(
        group,
        Effects {
            opacity: 0.,
            ..Effects::default()
        },
    );
    scene.flush();
    scene.set_kind(background, NodeKind::Rect(Color(0, 255, 0, 255)));
    assert_eq!(scene.flush().damage, vec![Rect::new(50., 50., 5., 5.)]);

    // Revealing the retained filter must repaint its complete influence area:
    // its own bounds, as a backdrop filter samples nothing outside them.
    let influence = Rect::new(40., 40., 100., 100.);
    scene.set_effects(group, Effects::default());
    assert_eq!(scene.flush().damage, vec![influence]);
    scene.set_kind(background, NodeKind::Rect(Color(0, 0, 255, 255)));
    assert_eq!(scene.flush().damage, vec![influence]);

    // Hiding repairs the former pixels even though subsequent frames exclude it.
    scene.set_effects(
        group,
        Effects {
            opacity: 0.,
            ..Effects::default()
        },
    );
    assert_eq!(scene.flush().damage, vec![influence]);
    scene.set_kind(background, NodeKind::Rect(Color(255, 0, 0, 255)));
    assert_eq!(scene.flush().damage, vec![Rect::new(50., 50., 5., 5.)]);
    assert!(scene.flush().is_idle());
}

#[test]
fn directly_transparent_filter_is_excluded_but_positive_opacity_is_not() {
    let mut scene = Scene::new(200., 200.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let background = scene.append(
        scene.root(),
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(5., 5.),
    );
    let blur = scene.append(
        scene.root(),
        NodeKind::Rect(Color(255, 255, 255, 100)),
        fixed(100., 100.),
    );
    scene.set_effects(
        blur,
        Effects {
            opacity: 0.,
            blur_radius: 10.,
            ..Effects::default()
        },
    );
    scene.flush();
    scene.set_kind(background, NodeKind::Rect(Color(0, 255, 0, 255)));
    assert_eq!(scene.flush().damage, vec![Rect::new(0., 0., 5., 5.)]);
    scene.set_effects(
        blur,
        Effects {
            opacity: 0.001,
            blur_radius: 10.,
            ..Effects::default()
        },
    );
    scene.flush();
    scene.set_kind(background, NodeKind::Rect(Color(0, 0, 255, 255)));
    assert_eq!(scene.flush().damage, vec![Rect::new(0., 0., 100., 100.)]);
}

#[test]
fn hidden_nodes_damage_nothing_until_shown() {
    use zgx::layout::{Display, LayoutOptions};
    let hidden = |style: Style| Style {
        layout_options: Some(std::sync::Arc::new(LayoutOptions {
            display: Some(Display::None),
            ..Default::default()
        })),
        ..style
    };
    let mut scene = Scene::new(200., 200.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        hidden(fixed(50., 50.)),
    );
    let dot = scene.append(group, NodeKind::Rect(Color(255, 0, 0, 255)), fixed(6., 6.));
    scene.set_transform(group, Transform { x: 20., y: 20. });
    scene.flush();
    // An animation inside a hidden spinner: nothing on screen changes.
    for opacity in [0.2, 0.6, 1.0] {
        scene.set_effects(
            dot,
            Effects {
                opacity,
                ..Effects::default()
            },
        );
        assert!(scene.flush().damage.is_empty());
    }
    // Shown, it damages where it lands; hidden again, where it was.
    scene.set_style(group, fixed(50., 50.));
    assert_eq!(scene.flush().damage, vec![Rect::new(20., 20., 6., 6.)]);
    scene.set_style(group, hidden(fixed(50., 50.)));
    assert_eq!(scene.flush().damage, vec![Rect::new(20., 20., 6., 6.)]);
}
