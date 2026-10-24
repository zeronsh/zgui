//! Boxes that grow open (a layout animation's negative margin) and rows of
//! wrapping text: a parent is always as tall as its children end, frame
//! after frame, so nothing hangs out of its box or under the next one.
use zgx::{compose::prelude::*, scene::Align, scene::NodeId, scene::Rect, widgets::Ui};

fn bounds(ui: &Ui, path: &[usize]) -> Rect {
    let scene = ui.scene.borrow();
    let mut id: NodeId = scene.root();
    for index in path {
        id = scene.children(id)[*index];
    }
    scene.bounds(id)
}

/// A row of a price and a shrinking column of a title and wrapping text,
/// then another row, as a list of picks lays them out.
fn picks(line_height: Option<f32>) -> View {
    let mut why = text("Cordless, but it's the only portable one that handles frozen fruit.")
        .text_wrap(true)
        .text_size(18.);
    if let Some(pixels) = line_height {
        why = why.line_height(pixels);
    }
    column()
        .gap(8.)
        .items_stretch()
        .child(
            row()
                .gap(12.)
                .items_start()
                .child(text("£34.99").w(44.).flex_shrink(0.))
                .child(
                    column()
                        .grow()
                        .flex_shrink(1.)
                        .min_w(0.)
                        .gap(2.)
                        .child(
                            row()
                                .gap(4.)
                                .items_center()
                                .child(text("Ninja Blast BC151")),
                        )
                        .child(why),
                ),
        )
        .child(row().child(text("£49.00")))
}

#[test]
fn rows_are_as_tall_as_the_wrapped_text_they_shrink_to() {
    for line_height in [None, Some(28.)] {
        // Directly in a stretching column, and inside a layout box growing
        // open (its bottom margin animating from negative to nothing) in a
        // shrink-to-fit presence wrapper.
        for animated in [false, true] {
            let mut ui = Ui::new(400., 800.);
            let margin = ui.signal(-60_f32);
            let read = margin.clone();
            let content = match animated {
                false => picks(line_height),
                true => overlay().w_full().child(
                    div().w_full().child(
                        column().items_stretch().child(
                            column()
                                .items_stretch()
                                .reactive_style(move || Styles::new().mb(read.get()))
                                .child(
                                    column()
                                        .items_stretch()
                                        .opacity(0.5)
                                        .child(picks(line_height)),
                                ),
                        ),
                    ),
                ),
            };
            let _view = ui.mount(column().w(300.).items_stretch().child(content));
            let picks_path: &[usize] = match animated {
                false => &[0, 0],
                true => &[0, 0, 0, 0, 0, 0, 0],
            };
            for frame in 0..4 {
                ui.prepare_frame();
                let path = [picks_path, &[0]].concat();
                let (row, next) = (
                    bounds(&ui, &path),
                    bounds(&ui, &[picks_path, &[1]].concat()),
                );
                let column = bounds(&ui, &[&path[..], &[1]].concat());
                let why = bounds(&ui, &[&path[..], &[1, 1]].concat());
                let case =
                    format!("line height {line_height:?}, animated {animated}, frame {frame}");
                assert_eq!(row.width, 300., "{case}");
                assert_eq!(column.width, 244., "{case}");
                assert_eq!(row.height, column.height, "{case}");
                assert_eq!(why.y + why.height, column.y + column.height, "{case}");
                assert_eq!(next.y, row.y + row.height + 8., "{case}");
                margin.set(margin.get() + 20.);
            }
        }
    }
}

/// A box (9 px padding) around a growing region: a child growing open by a
/// negative bottom margin, styled by `kid`, then a 48 px child, 12 px apart.
fn growing_box(
    ui: &mut Ui,
    kid: impl Fn(View, zgx::reactive::Signal<f32>) -> View,
) -> (zgx::reactive::Signal<f32>, zgx::compose::ViewHandle) {
    let margin = ui.signal(-75_f32);
    let grown = kid(column().pt(3.).child(row().h(60.).w(100.)), margin.clone());
    let region = column()
        .items_start()
        .gap(12.)
        .grow()
        .child(grown)
        .child(column().h(48.));
    let view = ui.mount(
        column()
            .items_stretch()
            .child(column().p(9.).items_stretch().child(region)),
    );
    (margin, view)
}

/// A case: its name, how the growing child is styled, and the box's height
/// for a margin.
type Case = (
    &'static str,
    fn(View, zgx::reactive::Signal<f32>) -> View,
    fn(f32) -> f32,
);

#[test]
fn boxes_grow_open_with_their_children_whatever_lays_them_out() {
    // Stretched by `align_self` (so laid out by taffy), plain, and clipped
    // with a negative margin at both ends beside a stretched sibling.
    let cases: [Case; 3] = [
        (
            "align_self stretch",
            |kid, margin| {
                kid.align_self(Align::Stretch)
                    .reactive_style(move || Styles::new().mb(margin.get()))
            },
            |margin| 141. + margin,
        ),
        (
            "plain",
            |kid, margin| kid.reactive_style(move || Styles::new().mb(margin.get())),
            |margin| 141. + margin,
        ),
        (
            "clipped",
            |kid, margin| {
                kid.overflow_hidden()
                    .align_self(Align::Start)
                    .reactive_style(move || Styles::new().my(margin.get() / 5.))
            },
            |margin| 141. + 2. * margin / 5.,
        ),
    ];
    for (name, kid, expected) in cases {
        let mut ui = Ui::new(400., 600.);
        let (margin, _view) = growing_box(&mut ui, kid);
        for frame in 0..5 {
            ui.prepare_frame();
            let height = bounds(&ui, &[0, 0]).height;
            assert_eq!(height, expected(margin.get()), "{name}, frame {frame}");
            margin.set(margin.get() + 25.);
        }
    }
}

/// Bars growing in a moved, clipping card (a dashboard's sparkline): each
/// repaints where it was and where it is now, cut by the card's clip, and
/// nothing outside the card repaints.
#[test]
fn growing_siblings_damage_their_old_and_new_boxes_within_the_clip() {
    use zgx::scene::{Color, Insets, Layout, NodeKind, Scene, Style, Transform};
    let mut scene = Scene::new(400., 300.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let card = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(120.),
            height: Some(60.),
            clip: true,
            ..Style::default()
        },
    );
    scene.set_transform(card, Transform { x: 50.5, y: 40. });
    let bar = |height: f32, index: usize| Style {
        absolute: true,
        width: Some(6.),
        height: Some(height),
        margin: Insets {
            left: 10. * index as f32,
            top: 50. - height,
            ..Insets::default()
        },
        ..Style::default()
    };
    let bars: Vec<_> = (0..12)
        .map(|i| scene.append(card, NodeKind::Rect(Color(9, 9, 9, 255)), bar(20., i)))
        .collect();
    scene.flush();
    let before: Vec<Rect> = bars.iter().map(|id| scene.bounds(*id)).collect();
    for (i, id) in bars.iter().enumerate() {
        // Some past the card's bottom edge: cut there.
        scene.set_style(*id, bar(10. + 7. * i as f32, i));
    }
    let report = scene.flush();
    let clip = Rect::new(50.5, 40., 120., 60.);
    let covered = |x: f32, y: f32| {
        report
            .damage
            .iter()
            .any(|d| d.x <= x && x <= d.x + d.width && d.y <= y && y <= d.y + d.height)
    };
    for (i, id) in bars.iter().enumerate() {
        for rect in [before[i], scene.bounds(*id)] {
            let Some(rect) = rect.intersection(clip) else {
                continue;
            };
            for (x, y) in [
                (rect.x, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x + rect.width / 2., rect.y + rect.height / 2.),
            ] {
                assert!(covered(x, y), "bar {i} at {rect:?} not repainted");
            }
        }
    }
    for d in &report.damage {
        // Whole pixels around the clip.
        assert!(
            d.x >= 50. && d.y >= 40. && d.x + d.width <= 171. && d.y + d.height <= 100.,
            "{d:?} repaints outside the card"
        );
    }
}

/// A box growing in place (a bar) repaints where it was and where it is
/// as one area, whole points out: the union of the two, as two damaged
/// apart would merge. A box that jumps clear of where it was repaints the
/// two apart.
#[test]
fn a_box_growing_in_place_damages_its_old_and_new_box_as_one() {
    use zgx::scene::{Color, Insets, Layout, NodeKind, Scene, Style};
    let mut scene = Scene::new(400., 300.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let bar = |height: f32, left: f32| Style {
        absolute: true,
        width: Some(6.5),
        height: Some(height),
        margin: Insets {
            left,
            top: 100.25 - height,
            ..Insets::default()
        },
        ..Style::default()
    };
    let id = scene.append(
        scene.root(),
        NodeKind::Rect(Color(9, 9, 9, 255)),
        bar(20., 10.),
    );
    scene.flush();
    scene.set_style(id, bar(30.5, 10.));
    let grown = scene.flush();
    // From y 80.25 (then 69.75) to 100.25, x 10 to 16.5: whole points out.
    assert_eq!(grown.damage, vec![Rect::new(10., 69., 7., 32.)]);
    scene.set_style(id, bar(30.5, 200.));
    let jumped = scene.flush();
    let mut damage = jumped.damage.clone();
    damage.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(
        damage,
        vec![Rect::new(10., 69., 7., 32.), Rect::new(200., 69., 7., 32.)]
    );
}
