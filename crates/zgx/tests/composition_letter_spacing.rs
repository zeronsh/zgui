use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, widgets::Ui};

#[test]
fn inherited_spacing_reaches_components_and_slots_without_rebuilding() {
    let mut ui = Ui::new(500., 400.);
    let spacing = ui.signal(2.);
    let read = spacing.clone();
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let mounted = ui.mount(
        column()
            .text_size(10.)
            .reactive_style(move || Styles::new().letter_spacing(read.get()))
            .child(component(move |cx| {
                count.set(count.get() + 1);
                let body = cx.slot(|_| text("abcd").id("slot"));
                // Each text its own width, to measure its spacing.
                column()
                    .items_start()
                    .child(text("abcd").id("inherited"))
                    .child(text("abcd").letter_spacing(0.).id("reset"))
                    .child(
                        component(|_| text("abcd").letter_spacing(7.))
                            .letter_spacing(0.)
                            .id("wrapper"),
                    )
                    .child(body)
            })),
    );
    ui.prepare_frame();
    let inherited = mounted.find("inherited").unwrap();
    let slot = mounted.find("slot").unwrap();
    let reset = mounted.find("reset").unwrap();
    let wrapper = mounted.find("wrapper").unwrap();
    let normal = ui.scene.borrow().bounds(reset).width;
    let expanded = ui.scene.borrow().bounds(inherited).width;
    assert!(expanded > normal);
    assert_eq!(ui.scene.borrow().bounds(slot).width, expanded);
    assert_eq!(ui.scene.borrow().bounds(wrapper).width, normal);
    spacing.set(-1.);
    ui.prepare_frame();
    let contracted = ui.scene.borrow().bounds(inherited).width;
    assert!(contracted < normal);
    assert_eq!(ui.scene.borrow().bounds(slot).width, contracted);
    assert_eq!(ui.scene.borrow().bounds(reset).width, normal);
    assert_eq!(mounted.find("inherited"), Some(inherited));
    assert_eq!(builds.get(), 1);
}

#[test]
fn normalized_equivalent_spacing_is_idle_and_sparse_patch_restores_inheritance() {
    let mut ui = Ui::new(500., 400.);
    let mode = ui.signal(0);
    let read = mode.clone();
    let mounted =
        ui.mount(
            column().text_size(10.).letter_spacing(2.).child(
                text("abcd")
                    .id("text")
                    .reactive_style(move || match read.get() {
                        0 => Styles::new().letter_spacing(f32::NAN),
                        1 => Styles::new().letter_spacing(f32::INFINITY),
                        2 => Styles::new().letter_spacing(-0.),
                        _ => Styles::new(),
                    }),
            ),
        );
    ui.prepare_frame();
    let node = mounted.find("text").unwrap();
    let normal = ui.scene.borrow().bounds(node).width;
    ui.scene.borrow_mut().flush();
    for next in [1, 2] {
        mode.set(next);
        ui.prepare_frame();
        let idle = ui.scene.borrow_mut().flush();
        assert_eq!(idle.layout_nodes, 0);
        assert!(idle.damage.is_empty());
        assert_eq!(ui.scene.borrow().bounds(node).width, normal);
    }
    mode.set(3);
    ui.prepare_frame();
    assert!(ui.scene.borrow().bounds(node).width > normal);
    assert_eq!(
        Styles::new().letter_spacing(f32::NAN),
        Styles::new().letter_spacing(0.)
    );
    assert_eq!(
        Styles::new().letter_spacing(-0.),
        Styles::new().letter_spacing(0.)
    );
}

#[test]
fn editor_spacing_changes_caret_and_wrapping_without_changing_selection_or_model() {
    let mut ui = Ui::new(500., 400.);
    let spacing = ui.signal(0.);
    let read = spacing.clone();
    let value = ui.signal("abcdefghijklmnop".to_owned());
    let mounted = ui.mount(
        column()
            .text_size(10.)
            .line_height(20.)
            .reactive_style(move || Styles::new().letter_spacing(read.get()))
            .child(
                text_area("Editor", value.clone())
                    .id("editor")
                    .size(100., 180.)
                    .p(5.)
                    .text_wrap(true),
            ),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 3);
    editor.refresh();
    ui.prepare_frame();
    let normal = ui.scene.borrow().bounds(editor.caret);
    spacing.set(3.);
    ui.prepare_frame();
    let expanded = ui.scene.borrow().bounds(editor.caret);
    assert!(expanded.x > normal.x);
    assert_eq!(expanded.y, normal.y);
    assert_eq!(editor.editor.borrow().selection().focus, 3);
    editor.editor.borrow_mut().set_selection(0, 12);
    editor.refresh();
    ui.prepare_frame();
    let wrapped = ui.scene.borrow().bounds(editor.caret);
    spacing.set(0.);
    ui.prepare_frame();
    let unwrapped = ui.scene.borrow().bounds(editor.caret);
    assert!(wrapped.y > unwrapped.y);
    assert_eq!(editor.editor.borrow().selection().anchor, 0);
    assert_eq!(editor.editor.borrow().selection().focus, 12);
    assert_eq!(value.get(), "abcdefghijklmnop");
    assert_eq!(mounted.find("editor"), Some(node));
}

#[test]
fn editor_shaping_cache_invalidates_spacing_but_reuses_equivalent_zero() {
    use zgx::text_layout::{FallbackTextLayout, FontStyle, TextLayout};
    let mut ui = Ui::new(400., 300.);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            count.set(count.get() + 1);
            Box::new(FallbackTextLayout::with_font(text, size, width, font))
        },
    );
    let spacing = ui.signal(0.);
    let read = spacing.clone();
    let value = ui.signal("sample text".to_owned());
    let mounted = ui.mount(
        text_area("Editor", value)
            .size(200., 100.)
            .reactive_style(move || Styles::new().letter_spacing(read.get())),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(mounted.node()));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    calls.set(0);
    for zero in [f32::NAN, f32::INFINITY, -0.] {
        spacing.set(zero);
        editor.refresh();
        ui.prepare_frame();
        assert_eq!(calls.get(), 0, "equivalent zero spacing reuses shaping");
    }
    spacing.set(2.);
    ui.prepare_frame();
    assert!(
        calls.replace(0) > 0,
        "spacing participates in shaping identity"
    );
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(calls.get(), 0);
}
