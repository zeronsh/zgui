use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, scene::Effects, widgets::Ui};

#[test]
fn reactive_nonfinite_effects_normalize_before_equality_and_restore_sparse_base() {
    for (invalid, opacity) in [(f32::NAN, 1.), (f32::INFINITY, 1.), (f32::NEG_INFINITY, 0.)] {
        let mut ui = Ui::new(200., 160.);
        let tick = ui.signal(0);
        let apply = ui.signal(true);
        let read_tick = tick.clone();
        let read_apply = apply.clone();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let view = ui.mount(
            div()
                .w(80.)
                .h(60.)
                .opacity(0.4)
                .blur(6.)
                .edge_fade(8.)
                .reactive_style(move || {
                    read_tick.get();
                    count.set(count.get() + 1);
                    if read_apply.get() {
                        Styles::new()
                            .opacity(invalid)
                            .blur(invalid)
                            .edge_fade(invalid)
                    } else {
                        Styles::new()
                    }
                }),
        );
        ui.prepare_frame();
        let expected = Effects {
            backdrop_material: zgx::scene::BackdropMaterial::Blur,
            backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill_blend: zgx::scene::BlendMode::Normal,
            opacity,
            blur_radius: 0.,
            edge_fade: 0.,
            filter_blur: 0.,
            brightness: 1.,
            contrast: 1.,
            saturate: 1.,
        };
        assert_eq!(ui.scene.borrow().effects(view.node()), expected);
        ui.scene.borrow_mut().flush();
        let initial_calls = calls.get();
        for value in 1..=3 {
            tick.set(value);
            ui.prepare_frame();
            assert_eq!(calls.get(), initial_calls + value);
            assert_eq!(ui.scene.borrow().effects(view.node()), expected);
            let frame = ui.scene.borrow_mut().flush();
            assert!(
                frame.is_idle(),
                "equal normalized effects should remain idle: {frame:?}"
            );
            assert_eq!(frame.layout_nodes, 0);
            assert_eq!(frame.composite_nodes, 0);
            assert!(frame.damage.is_empty());
        }
        apply.set(false);
        ui.prepare_frame();
        assert_eq!(
            ui.scene.borrow().effects(view.node()),
            Effects {
                backdrop_material: zgx::scene::BackdropMaterial::Blur,
                backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
                backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
                backdrop_fill_blend: zgx::scene::BlendMode::Normal,
                opacity: 0.4,
                blur_radius: 6.,
                edge_fade: 8.,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            }
        );
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!(frame.layout_nodes, 0);
        assert!(!frame.damage.is_empty());
        apply.set(true);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow().effects(view.node()), expected);
        ui.scene.borrow_mut().flush();
        assert!(ui.scene.borrow_mut().flush().is_idle());
    }
}

/// A style re-run that changes only a box's size (a bar growing) writes no
/// paint: what else set the node's translation and effects since (a script
/// animation) stays. A style that sets paint again still applies it.
#[test]
fn a_size_only_restyle_leaves_paint_set_elsewhere() {
    use zgx::scene::Transform;
    let mut ui = Ui::new(200., 160.);
    let width = ui.signal(40_f32);
    let fade = ui.signal(None::<f32>);
    let (read_width, read_fade) = (width.clone(), fade.clone());
    let view = ui.mount(div().h(20.).reactive_style(move || {
        let styles = Styles::new().w(read_width.get());
        match read_fade.get() {
            Some(opacity) => styles.opacity(opacity),
            None => styles,
        }
    }));
    ui.prepare_frame();
    // Two size changes: the second takes the size-only path.
    for w in [50., 60.] {
        width.set(w);
        ui.prepare_frame();
    }
    let node = view.node();
    let moved = Transform { x: 12., y: 3. };
    {
        let mut scene = ui.scene.borrow_mut();
        scene.set_transform(node, moved);
        let effects = Effects {
            backdrop_material: zgx::scene::BackdropMaterial::Blur,
            backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill_blend: zgx::scene::BlendMode::Normal,
            brightness: 0.5,
            ..scene.effects(node)
        };
        scene.set_effects(node, effects);
    }
    width.set(70.);
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    assert_eq!(scene.layout_bounds(node).width, 70.);
    assert_eq!(scene.transform(node), moved);
    assert_eq!(scene.effects(node).brightness, 0.5);
    drop(scene);
    fade.set(Some(0.5));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().effects(node).opacity, 0.5);
}
