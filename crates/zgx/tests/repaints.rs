//! What a renderer patching last frame's paint relies on: the nodes each
//! flush repainted, with what changed (`Scene::repaints`), and each paint
//! item worked out alone from what it inherited (`Scene::item_in`), as the
//! walk works it out.
use zgx::{
    compose::prelude::*,
    reactive::Signal,
    scene::{Color, Inherited, PaintItem, Scene, repaint},
    widgets::Ui,
};

fn same(a: &PaintItem<'_>, b: &PaintItem<'_>) -> bool {
    a.id == b.id
        && a.isolated == b.isolated
        && a.bounds == b.bounds
        && std::ptr::eq(a.kind, b.kind)
        && std::ptr::eq(a.font, b.font)
        && a.text_options == b.text_options
        && a.effects == b.effects
        && a.clip == b.clip
        && a.border_pass == b.border_pass
        && a.extras == b.extras
}

/// Every item the walk records that `item_in` works out alone comes out as
/// the walk has it; returns how many did.
fn alone_as_walked(scene: &Scene) -> usize {
    let mut inherited: Vec<Inherited> = Vec::new();
    let items = scene.layer_items_recorded(&mut inherited);
    assert_eq!(items.len(), inherited.len());
    let mut alone = 0;
    for (item, from) in items.iter().zip(&inherited) {
        if let Some(again) = scene.item_in(item.id, from) {
            assert!(same(item, &again), "{:?} alone differs", item.id);
            alone += 1;
        }
    }
    // As the plain walk has them, in the same order.
    let plain = scene.layer_items(None);
    assert_eq!(plain.len(), items.len());
    assert!(plain.iter().zip(&items).all(|(a, b)| same(a, b)));
    alone
}

fn dashboard(live: Signal<u32>) -> View {
    let card = |index: u32| {
        let (value, bar) = (live.clone(), live.clone());
        div()
            .absolute()
            .ml(10. + 130. * index as f32)
            .mt(20.)
            .size(120., 100.)
            .rounded(6.)
            .border(1.)
            .border_color(Color(40, 50, 60, 255))
            .bg(Color(20, 30, 40, 255))
            .child(
                text_signal(move || format!("{} ms", value.get() * (index + 1)))
                    .absolute()
                    .ml(8.)
                    .mt(6.)
                    .size(100., 20.),
            )
            .children((0..4).map(move |b| {
                let bar = bar.clone();
                div()
                    .absolute()
                    .ml(8. + 5. * b as f32)
                    .w(3.)
                    .bg(Color(90, 170, 255, 255))
                    .reactive_style(move || {
                        let height = 4. + ((bar.get() + b) % 7) as f32 * 3.5;
                        Styles::new().mt(90. - height).h(height)
                    })
            }))
    };
    div()
        .size(600., 400.)
        .bg(Color(10, 12, 16, 255))
        .children((0..3).map(card))
        .child(
            div()
                .absolute()
                .ml(10.)
                .mt(140.)
                .size(300., 100.)
                .rounded(8.)
                .overflow_hidden()
                .opacity(0.8)
                .child(
                    text("clipped and faded")
                        .absolute()
                        .ml(4.)
                        .mt(4.)
                        .size(200., 20.),
                )
                .child(
                    div()
                        .absolute()
                        .ml(250.)
                        .mt(80.)
                        .size(80., 40.)
                        .bg(Color(200, 80, 80, 255)),
                ),
        )
}

#[test]
fn items_alone_come_out_as_the_walk_has_them() {
    let mut ui = Ui::new(600., 400.);
    let live = ui.signal(1_u32);
    ui.mount(dashboard(live.clone()));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let first = alone_as_walked(&ui.scene.borrow());
    // The cards' panels, texts and bars, and the clipped container's own
    // children, at least.
    assert!(first >= 3 * 6, "{first} worked out alone");
    for tick in 2..6 {
        live.set(tick);
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        assert_eq!(alone_as_walked(&ui.scene.borrow()), first);
    }
}

/// A scene no renderer reads repaints from keeps none: flushes then cost
/// nothing more, and what the list covers says it holds nothing.
#[test]
fn repaints_are_kept_only_while_a_renderer_reads_them() {
    let mut ui = Ui::new(600., 400.);
    let live = ui.signal(1_u32);
    ui.mount(dashboard(live.clone()));
    for tick in 2..8 {
        live.set(tick);
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    }
    let scene = ui.scene.borrow();
    let (covered, repaints) = scene.repaints();
    assert!(repaints.is_empty() && covered.is_empty(), "{covered:?}");
}

#[test]
fn a_flush_names_the_nodes_it_repainted() {
    let mut ui = Ui::new(600., 400.);
    let live = ui.signal(1_u32);
    ui.mount(dashboard(live.clone()));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let before = *ui.scene.borrow().repaints().0.end();
    // What the walk had before the change, to work changed items out from.
    let recorded: Vec<(zgx::scene::NodeId, Inherited)> = {
        let scene = ui.scene.borrow();
        let mut inherited = Vec::new();
        let items = scene.layer_items_recorded(&mut inherited);
        items.iter().map(|item| item.id).zip(inherited).collect()
    };
    live.set(2);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let scene = ui.scene.borrow();
    let (covered, repaints) = scene.repaints();
    assert_eq!(covered, before + 1..=before + 1, "the one flush since");
    let after = before + 1;
    // Three texts changed and bars moved; containers whose layout alone was
    // redone (the cards, the root) are not named.
    let texts = repaints
        .iter()
        .filter(|(id, _)| matches!(scene.kind(*id), zgx::scene::NodeKind::Text { .. }))
        .count();
    assert_eq!(texts, 3);
    let moved = repaints
        .iter()
        .filter(|(_, flags)| flags & repaint::MOVED != 0)
        .count();
    assert!(moved >= 3 * 4 - 3, "{moved} moved");
    assert!(
        repaints
            .iter()
            .all(|(id, _)| scene.children(*id).is_empty())
    );
    // Each changed node, worked out alone from what it inherited before,
    // is what the walk has now: its ancestors did not change.
    let now = scene.layer_items(None);
    for (id, _) in repaints {
        let from = &recorded.iter().find(|(r, _)| r == id).expect("recorded").1;
        let alone = scene.item_in(*id, from).expect("plain");
        let walked = now.iter().find(|item| item.id == *id).expect("painted");
        assert!(same(&alone, walked));
    }
    drop(scene);
    // A flush with nothing to do names nothing, and still counts.
    ui.scene.borrow_mut().flush();
    let (covered, count) = {
        let scene = ui.scene.borrow();
        let (covered, repaints) = scene.repaints();
        (covered, repaints.len())
    };
    assert_eq!((covered, count), (after + 1..=after + 1, 0));
    // Unread, repaints add up: the list then covers both flushes.
    live.set(3);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    live.set(4);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let scene = ui.scene.borrow();
    let (covered, repaints) = scene.repaints();
    assert_eq!(covered, after + 2..=after + 3);
    let texts = repaints
        .iter()
        .filter(|(id, _)| matches!(scene.kind(*id), zgx::scene::NodeKind::Text { .. }))
        .count();
    assert_eq!(texts, 6, "each flush's three texts");
}
