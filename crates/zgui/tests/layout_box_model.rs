//! The box model the layout engine follows: borders inside a box's size
//! (CSS `box-sizing: border-box`), overlays stacking like grid items in one
//! area, scroll views and inputs whose default sizes yield to a stretching
//! parent, and `absolute` views positioned against their containing block.
//! Each case runs on the ordinary path and on the taffy one (a child with
//! layout options sends its parent there).
use zgui::{
    compose::{ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers},
    layout::Length,
    scene::{Color, Insets, Rect},
    widgets::Ui,
};

const RED: Color = Color(255, 0, 0, 255);

fn rect(ui: &Ui, handle: &ViewHandle, id: &str) -> Rect {
    ui.scene
        .borrow()
        .bounds(handle.find(id).unwrap_or_else(|| panic!("no `{id}`")))
}
/// A child that sends its parent through taffy without taking room.
fn advanced() -> View {
    div().hidden()
}
fn mount(view: View) -> (Ui, ViewHandle) {
    let mut ui = Ui::new(600., 400.);
    let handle = ui.mount(view);
    ui.prepare_frame();
    (ui, handle)
}

// Borders --------------------------------------------------------------------

#[test]
fn borders_sit_inside_a_box_and_its_content_inside_them() {
    for taffy in [false, true] {
        let sized = column()
            .id("sized")
            .w(100.)
            .h(60.)
            .p(5.)
            .border(2.)
            .border_color(RED)
            .child(div().id("sized_child").w_full().h_full())
            .when(taffy, |v| v.child(advanced()));
        let fit = row()
            .id("fit")
            .p(5.)
            .border(3.)
            .border_color(RED)
            .child(div().id("fit_child").w(20.).h(10.))
            .when(taffy, |v| v.child(advanced()));
        // Per side, a transparent one too: it still takes its width.
        let sides = column()
            .id("sides")
            .border_edges(Insets {
                top: 1.,
                right: 2.,
                bottom: 3.,
                left: 4.,
            })
            .border_color(Color(0, 0, 0, 0))
            .child(div().id("sides_child").w(10.).h(10.))
            .when(taffy, |v| v.child(advanced()));
        // Each box its own size: not stretched across the column.
        let (ui, h) = mount(column().items_start().child(sized).child(fit).child(sides));
        let case = format!("taffy {taffy}");
        assert_eq!(
            rect(&ui, &h, "sized"),
            Rect::new(0., 0., 100., 60.),
            "{case}"
        );
        // Inside the border and the padding: 100 - 2 * (2 + 5).
        assert_eq!(
            rect(&ui, &h, "sized_child"),
            Rect::new(7., 7., 86., 46.),
            "{case}"
        );
        // Fit to content: the content, its padding and its border.
        assert_eq!(rect(&ui, &h, "fit"), Rect::new(0., 60., 36., 26.), "{case}");
        assert_eq!(
            rect(&ui, &h, "fit_child"),
            Rect::new(8., 68., 20., 10.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "sides"),
            Rect::new(0., 86., 16., 14.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "sides_child"),
            Rect::new(4., 87., 10., 10.),
            "{case}"
        );
    }
}

#[test]
fn paint_only_rings_take_no_room() {
    let (ui, h) = mount(
        column()
            .child(
                div()
                    .id("over")
                    .border(4.)
                    .border_over(true)
                    .child(div().id("over_child").w(10.).h(10.)),
            )
            .child(
                div()
                    .id("outlined")
                    .outline(3., RED)
                    .child(div().id("outlined_child").w(10.).h(10.)),
            )
            .child(
                div()
                    .id("stroked")
                    .stroke(2., RED, zgui::style::StrokeAlign::Inside)
                    .child(div().id("stroked_child").w(10.).h(10.)),
            ),
    );
    assert_eq!(rect(&ui, &h, "over"), Rect::new(0., 0., 10., 10.));
    assert_eq!(rect(&ui, &h, "over_child"), Rect::new(0., 0., 10., 10.));
    assert_eq!(
        rect(&ui, &h, "outlined_child"),
        Rect::new(0., 10., 10., 10.)
    );
    assert_eq!(rect(&ui, &h, "stroked_child"), Rect::new(0., 20., 10., 10.));
}

#[test]
fn border_width_changes_relayout_and_colour_changes_do_not() {
    let mut ui = Ui::new(300., 200.);
    let width = ui.signal(1_f32);
    let colour = ui.signal(RED);
    let (read_width, read_colour) = (width.clone(), colour.clone());
    let h = ui.mount(
        column()
            .id("box")
            .reactive_style(move || {
                Styles::new()
                    .border(read_width.get())
                    .border_color(read_colour.get())
            })
            .child(div().id("child").w(10.).h(10.)),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    assert_eq!(rect(&ui, &h, "child"), Rect::new(1., 1., 10., 10.));
    colour.set(Color(0, 255, 0, 255));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    width.set(3.);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
    assert_eq!(rect(&ui, &h, "box"), Rect::new(0., 0., 16., 16.));
    assert_eq!(rect(&ui, &h, "child"), Rect::new(3., 3., 10., 10.));
}

#[test]
fn absolute_insets_count_from_inside_the_border_and_flow_from_inside_the_padding() {
    let (ui, h) = mount(
        column()
            .id("frame")
            .w(100.)
            .h(80.)
            .p(10.)
            .border(2.)
            .border_color(RED)
            .child(div().id("pinned").absolute().left(0.).top(0.).w(5.).h(5.))
            .child(
                div()
                    .id("filled")
                    .absolute()
                    .left(0.)
                    .right(0.)
                    .bottom(0.)
                    .h(4.),
            )
            .child(div().id("static").absolute().w(5.).h(5.)),
    );
    // As CSS: insets from the padding box, the static place in the content box.
    assert_eq!(rect(&ui, &h, "pinned"), Rect::new(2., 2., 5., 5.));
    assert_eq!(rect(&ui, &h, "filled"), Rect::new(2., 74., 96., 4.));
    assert_eq!(rect(&ui, &h, "static"), Rect::new(12., 12., 5., 5.));
}

#[test]
fn a_bordered_input_keeps_its_text_inside_the_border_and_focus_moves_nothing() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::from("hello"));
    let h = ui.mount(
        column()
            .child(text_input("plain", value.clone()).id("plain"))
            .child(
                text_input("framed", value.clone())
                    .id("framed")
                    .border(3.)
                    .border_color(RED),
            ),
    );
    ui.prepare_frame();
    let viewport = |ui: &Ui, id: &str| {
        let scene = ui.scene.borrow();
        let root = h.find(id).unwrap();
        scene.bounds(scene.children(root)[0])
    };
    let framed = rect(&ui, &h, "framed");
    let inner = viewport(&ui, "framed");
    // Its padding (8, 6) inside its border.
    assert_eq!(inner.x - framed.x, 11.);
    assert_eq!(inner.y - framed.y, 9.);
    assert_eq!(inner.width, framed.width - 22.);
    // The default focus ring is an inset stroke: nothing moves.
    let before = viewport(&ui, "plain");
    assert!(ui.input.focus(&ui.scene, h.find("plain")));
    ui.prepare_frame();
    assert_eq!(viewport(&ui, "plain"), before);
    // A bordered input's ring recolours its own border: nothing moves either.
    assert!(ui.input.focus(&ui.scene, h.find("framed")));
    ui.prepare_frame();
    assert_eq!(viewport(&ui, "framed"), inner);
}

// Overlays -------------------------------------------------------------------

#[test]
fn overlay_children_stretch_by_default_unless_they_size_themselves() {
    for taffy in [false, true] {
        let (ui, h) = mount(
            overlay()
                .id("stack")
                .w(200.)
                .h(100.)
                .p(10.)
                .child(div().id("fill").bg(RED))
                .child(div().id("wide").h(20.))
                .child(div().id("tall").w(30.))
                .child(div().id("fixed").w(30.).h(20.))
                .when(taffy, |v| v.child(advanced())),
        );
        let case = format!("taffy {taffy}");
        assert_eq!(
            rect(&ui, &h, "fill"),
            Rect::new(10., 10., 180., 80.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "wide"),
            Rect::new(10., 10., 180., 20.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "tall"),
            Rect::new(10., 10., 30., 80.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "fixed"),
            Rect::new(10., 10., 30., 20.),
            "{case}"
        );
    }
}

#[test]
fn overlay_items_align_across_and_justify_aligns_down() {
    for taffy in [false, true] {
        for (items, justify, expected) in [
            ("center", "center", Rect::new(85., 40., 30., 20.)),
            ("end", "end", Rect::new(170., 80., 30., 20.)),
            ("start", "end", Rect::new(0., 80., 30., 20.)),
            ("center", "start", Rect::new(85., 0., 30., 20.)),
            ("stretch", "center", Rect::new(0., 40., 200., 20.)),
        ] {
            let mut stack = overlay().w(200.).h(100.);
            stack = match items {
                "center" => stack.items_center(),
                "end" => stack.items_end(),
                "start" => stack.items_start(),
                _ => stack.items_stretch(),
            };
            stack = match justify {
                "center" => stack.justify_center(),
                "end" => stack.justify_end(),
                _ => stack.justify_start(),
            };
            // Sized across only where it is not stretched.
            let icon = match items {
                "stretch" => div().id("icon").h(20.),
                _ => div().id("icon").w(30.).h(20.),
            };
            let (ui, h) = mount(stack.child(icon).when(taffy, |v| v.child(advanced())));
            assert_eq!(
                rect(&ui, &h, "icon"),
                expected,
                "taffy {taffy}, items {items}, justify {justify}"
            );
        }
    }
}

#[test]
fn an_overlay_is_as_large_as_its_largest_child_and_sizes_children_against_itself() {
    for taffy in [false, true] {
        let (ui, h) = mount(
            column().child(
                overlay()
                    .id("stack")
                    .items_start()
                    .justify_start()
                    .p(4.)
                    .child(div().id("a").w(50.).h(10.))
                    .child(div().id("b").w(20.).h(30.))
                    .when(taffy, |v| v.child(advanced())),
            ),
        );
        let case = format!("taffy {taffy}");
        assert_eq!(
            rect(&ui, &h, "stack"),
            Rect::new(0., 0., 58., 38.),
            "{case}"
        );
        assert_eq!(rect(&ui, &h, "b"), Rect::new(4., 4., 20., 30.), "{case}");
    }
    // Percentages count from its own box.
    let (ui, h) = mount(
        overlay()
            .w(200.)
            .h(100.)
            .items_start()
            .justify_start()
            .child(div().id("half").w_percent(50.).h_percent(25.)),
    );
    assert_eq!(rect(&ui, &h, "half"), Rect::new(0., 0., 100., 25.));
}

#[test]
fn an_overlay_pane_in_a_row_shows_a_growing_child() {
    // The preview pane: an overlay taking a row's remaining room, holding
    // content that asks to fill it.
    for taffy in [false, true] {
        let (ui, h) = mount(
            row()
                .w(600.)
                .h(300.)
                .items_stretch()
                .child(div().w(100.))
                .child(
                    overlay()
                        .id("pane")
                        .flex_grow(1.)
                        .child(div().id("page").flex_grow(1.).bg(RED))
                        .when(taffy, |v| v.child(advanced())),
                ),
        );
        let case = format!("taffy {taffy}");
        assert_eq!(
            rect(&ui, &h, "pane"),
            Rect::new(100., 0., 500., 300.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "page"),
            Rect::new(100., 0., 500., 300.),
            "{case}"
        );
    }
}

// Scroll views ---------------------------------------------------------------

#[test]
fn an_unsized_scroll_fills_a_stretching_parent_and_its_content_fills_it() {
    let mut ui = Ui::new(600., 800.);
    let offset = ui.signal(0.);
    let across = ui.signal(0.);
    let h = ui.mount(
        // Each box at its own size, stacked.
        column()
            .items_start()
            .child(
                column().id("pane").w(190.).h(300.).items_stretch().child(
                    scroll(offset.clone())
                        .id("list")
                        .flex_grow(1.)
                        .child(text("word ".repeat(60)).id("words").text_wrap(true)),
                ),
            )
            .child(
                column()
                    .child(scroll(offset.clone()).id("default").child(div().h(10.)))
                    .child(scroll(offset).id("explicit").w(100.).h(50.).child(div())),
            )
            .child(
                row()
                    .id("strip")
                    .w(200.)
                    .h(70.)
                    .items_stretch()
                    .child(scroll_x(across).id("across").child(div().id("chip").w(40.))),
            ),
    );
    ui.prepare_frame();
    // Across the pane (no longer 320 wide and clipped by it), down its height.
    assert_eq!(rect(&ui, &h, "list"), Rect::new(0., 0., 190., 300.));
    let words = rect(&ui, &h, "words");
    assert!(
        words.width <= 190.,
        "wrapped at the scroll's width: {words:?}"
    );
    assert!(words.height > 30., "on several lines: {words:?}");
    // Where nothing sizes it, 320 by 240; its own sizes hold.
    assert_eq!(rect(&ui, &h, "default").width, 320.);
    assert_eq!(rect(&ui, &h, "default").height, 240.);
    assert_eq!(rect(&ui, &h, "explicit").width, 100.);
    assert_eq!(rect(&ui, &h, "explicit").height, 50.);
    // A horizontal one fills a stretching row's height, its content too.
    assert_eq!(rect(&ui, &h, "across").height, 70.);
    assert_eq!(rect(&ui, &h, "chip").height, 70.);
}

#[test]
fn an_unsized_horizontal_scroll_is_as_tall_as_what_it_holds() {
    // A tab strip in a column: its tabs' height, not a default 240, and
    // what follows it sits right under it.
    let mut ui = Ui::new(600., 400.);
    let across = ui.signal(0.);
    let h = ui.mount(
        column()
            .w(300.)
            .h(400.)
            .child(
                scroll_x(across)
                    .id("strip")
                    .child(div().id("tab").w(500.).h(33.)),
            )
            .child(div().id("panel").h(20.)),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "strip").height, 33.);
    assert_eq!(rect(&ui, &h, "panel").y, 33.);
}

// Inputs ---------------------------------------------------------------------

#[test]
fn inputs_fit_rows_whatever_their_text() {
    for long in [false, true] {
        let mut ui = Ui::new(600., 400.);
        let value = ui.signal(if long { "x".repeat(400) } else { "x".into() });
        let h = ui.mount(
            column()
                .child(
                    row()
                        .id("bar")
                        .w(200.)
                        .h(36.)
                        .child(div().w(50.).shrink_0())
                        .child(text_input("grow", value.clone()).id("grow").flex_grow(1.)),
                )
                .child(
                    row()
                        .w(200.)
                        .h(36.)
                        .child(div().w(50.).shrink_0())
                        .child(text_input("narrow", value.clone()).id("narrow")),
                )
                .child(
                    row().w(200.).h(36.).child(div().w(50.).shrink_0()).child(
                        text_input("rigid", value.clone())
                            .id("rigid")
                            .flex_shrink(0.),
                    ),
                )
                .child(
                    column()
                        .w(300.)
                        .items_stretch()
                        .child(text_input("wide", value.clone()).id("wide")),
                )
                .child(row().child(text_input("alone", value).id("alone"))),
        );
        ui.prepare_frame();
        let case = format!("long {long}");
        assert_eq!(rect(&ui, &h, "grow").width, 150., "{case}");
        assert_eq!(rect(&ui, &h, "narrow").width, 150., "{case}");
        assert_eq!(rect(&ui, &h, "rigid").width, 240., "{case}");
        assert_eq!(rect(&ui, &h, "wide").width, 300., "{case}");
        // Unconstrained, its default width, never its text's.
        assert_eq!(rect(&ui, &h, "alone").width, 240., "{case}");
    }
}

#[test]
fn a_narrow_input_scrolls_its_text_to_keep_the_caret_in_view() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("word ".repeat(40));
    let h = ui.mount(
        row()
            .w(120.)
            .child(text_input("field", value.clone()).id("field").flex_grow(1.)),
    );
    ui.prepare_frame();
    let field = h.find("field").unwrap();
    assert!(ui.input.focus(&ui.scene, Some(field)));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    let scene = ui.scene.borrow();
    let viewport = scene.bounds(scene.parent(editor.caret).unwrap());
    let caret = scene.bounds(editor.caret);
    assert_eq!(scene.bounds(field).width, 120.);
    assert!(
        caret.x >= viewport.x && caret.x + caret.width <= viewport.x + viewport.width,
        "caret {caret:?} outside {viewport:?}"
    );
}

// Containing blocks ----------------------------------------------------------

fn card(id: &str, content: View) -> View {
    column()
        .id(id)
        .w(200.)
        .h(100.)
        .p(10.)
        .border(2.)
        .border_color(RED)
        .relative()
        .child(div().h(30.))
        .child(column().id("middle").p(5.).child(content))
}

#[test]
fn absolute_views_position_against_the_nearest_relative_ancestor() {
    for taffy in [false, true] {
        let badge = div()
            .id("badge")
            .absolute()
            .right(0.)
            .top(Length::Percent(0.5))
            .w(10.)
            .h(10.);
        let fill = div()
            .id("fill")
            .absolute()
            .left(4.)
            .right(4.)
            .bottom(0.)
            .h(6.);
        let content = column()
            .id("inner")
            .child(badge)
            .child(fill)
            .child(div().id("flow").w(20.).h(20.))
            .when(taffy, |v| v.child(advanced()));
        let (ui, h) = mount(column().child(div().h(50.)).child(card("card", content)));
        let case = format!("taffy {taffy}");
        // The card's padding box: (2, 52) to (198, 148).
        assert_eq!(
            rect(&ui, &h, "badge"),
            Rect::new(188., 100., 10., 10.),
            "{case}"
        );
        assert_eq!(
            rect(&ui, &h, "fill"),
            Rect::new(6., 142., 188., 6.),
            "{case}"
        );
        // In flow, untouched: inside the card, below the spacer, padded.
        assert_eq!(
            rect(&ui, &h, "flow"),
            Rect::new(17., 97., 20., 20.),
            "{case}"
        );
    }
}

#[test]
fn without_a_relative_ancestor_or_past_an_absolute_one_the_parent_contains() {
    let (ui, h) = mount(
        column()
            .child(div().h(50.))
            .child(
                column().id("outer").w(200.).h(100.).child(
                    column()
                        .id("parent")
                        .ml(20.)
                        .w(60.)
                        .h(40.)
                        .child(div().id("plain").absolute().right(0.).top(0.).w(10.).h(10.)),
                ),
            )
            .child(
                column().w(200.).h(100.).relative().child(
                    column()
                        .id("floating")
                        .absolute()
                        .left(30.)
                        .top(10.)
                        .w(80.)
                        .h(40.)
                        .child(
                            column().w_full().h_full().child(
                                div()
                                    .id("held")
                                    .absolute()
                                    .right(0.)
                                    .bottom(0.)
                                    .w(10.)
                                    .h(10.),
                            ),
                        ),
                ),
            ),
    );
    assert_eq!(rect(&ui, &h, "plain"), Rect::new(70., 50., 10., 10.));
    // An absolute ancestor bounds the search: the floating box's column,
    // the parent, contains it, not the relative box further out.
    assert_eq!(rect(&ui, &h, "floating"), Rect::new(30., 160., 80., 40.));
    assert_eq!(rect(&ui, &h, "held"), Rect::new(100., 190., 10., 10.));
}

#[test]
fn an_absolute_view_inside_a_switch_region_uses_the_relative_ancestor() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let read = shown.clone();
    let h =
        ui.mount(
            column().child(div().h(20.)).child(
                column()
                    .id("panel")
                    .w(300.)
                    .h(200.)
                    .relative()
                    .child(switch(
                        move || read.get(),
                        |on, _| match on {
                            true => div()
                                .id("toast")
                                .absolute()
                                .left(10.)
                                .right(10.)
                                .bottom(12.)
                                .h(30.),
                            false => div(),
                        },
                    )),
            ),
        );
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "toast"), Rect::new(10., 178., 280., 30.));
    shown.set(false);
    ui.prepare_frame();
    assert!(h.find("toast").is_none());
    shown.set(true);
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "toast"), Rect::new(10., 178., 280., 30.));
}

#[test]
fn containing_blocks_follow_relative_changes_resizes_and_hidden_ancestors() {
    let mut ui = Ui::new(400., 300.);
    let relative = ui.signal(true);
    let width = ui.signal(200_f32);
    let hidden = ui.signal(false);
    let (read_relative, read_width, read_hidden) =
        (relative.clone(), width.clone(), hidden.clone());
    let h = ui.mount(
        column()
            .id("block")
            .h(100.)
            .reactive_style(move || {
                let styles = Styles::new().w(read_width.get());
                match read_relative.get() {
                    true => styles.relative(),
                    false => styles,
                }
            })
            .child(
                column()
                    .id("parent")
                    .w(50.)
                    .h(50.)
                    .reactive_style(move || match read_hidden.get() {
                        true => Styles::new().hidden(),
                        false => Styles::new().flex(),
                    })
                    .child(
                        column().child(
                            div()
                                .id("tag")
                                .absolute()
                                .right(0.)
                                .bottom(0.)
                                .w(10.)
                                .h(10.),
                        ),
                    ),
            ),
    );
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "tag"), Rect::new(190., 90., 10., 10.));
    width.set(300.);
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "tag"), Rect::new(290., 90., 10., 10.));
    // No longer relative: its parent (the unsized inner column) contains it.
    relative.set(false);
    ui.prepare_frame();
    let tag = rect(&ui, &h, "tag");
    assert!(tag.x < 50. && tag.y < 50., "{tag:?}");
    relative.set(true);
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "tag"), Rect::new(290., 90., 10., 10.));
    // Under a hidden ancestor it is hidden too.
    hidden.set(true);
    ui.prepare_frame();
    assert!(!ui.scene.borrow().layout_visible(h.find("tag").unwrap()));
    hidden.set(false);
    ui.prepare_frame();
    assert_eq!(rect(&ui, &h, "tag"), Rect::new(290., 90., 10., 10.));
    // Settled: nothing left to lay out.
    ui.scene.borrow_mut().flush();
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
}

// Depth ----------------------------------------------------------------------

#[test]
fn a_tree_thousands_deep_lays_out_on_a_small_stack() {
    // A debug build's frames are large, and a thread's stack can be small
    // (512 KB here): every recursion over the tree needs room made for it
    // (`zgui::deep`).
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let mut view = text("leaf").id("leaf").text_wrap(true);
            for _ in 0..2_500 {
                view = column().child(view);
            }
            let (ui, handle) = mount(view);
            assert_eq!(rect(&ui, &handle, "leaf").x, 0.);
        })
        .unwrap()
        .join()
        .unwrap();
}

// Empty boxes ----------------------------------------------------------------

/// An empty box is as large as its padding and border, within its limits,
/// unless sized: a gap between nothing adds nothing, in a row, a column or
/// an overlay, and resizing it lays out again.
#[test]
fn empty_boxes_are_their_padding_or_their_size() {
    let (ui, handle) = mount(
        column()
            .items_start()
            .child(row().id("row").gap(9.).p(3.))
            .child(column().id("column").gap(9.).px(2.).py(5.).border(1.))
            .child(overlay().id("overlay").p(4.).min_w(30.))
            .child(div().id("sized").size(12., 7.).p(1.))
            .child(row().id("capped").p(20.).max_h(25.)),
    );
    let size = |id: &str| {
        let r = rect(&ui, &handle, id);
        (r.width, r.height)
    };
    assert_eq!(size("row"), (6., 6.));
    assert_eq!(size("column"), (6., 12.));
    assert_eq!(size("overlay"), (30., 8.));
    assert_eq!(size("sized"), (12., 7.));
    assert_eq!(size("capped"), (40., 25.));
}
