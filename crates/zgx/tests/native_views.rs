//! Native views (web pages) as zgx nodes, headless: placements from layout,
//! the base/overlay split of paint around them, glass over them, and where
//! pointer input goes.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    native_view::{NativeViews, Plane, PlaneSplit, PointerRoute},
    scene::Rect,
    web_view::{WebView, WebViewBackend, WebViewCommand, WebViewEvent, WebViewOptions},
    widgets::Ui,
};

fn supported() -> NativeViews {
    let natives = NativeViews::new();
    natives.set_supported(true);
    natives
}
fn split(ui: &Ui, natives: &NativeViews, previous: &PlaneSplit) -> (PlaneSplit, Vec<Rect>) {
    let scene = ui.scene.borrow();
    PlaneSplit::compute(&scene, &natives.nodes(), previous)
}

/// A browser-like window: a sidebar, a toolbar, the page, then a right
/// inspector, a popover over the page edge and a floating composer.
fn browser(web: &WebView, popover: bool) -> View {
    let page = web_view(web).id("page").w(600.).h(500.).rounded(10.);
    let mut root = overlay()
        .w(1000.)
        .h(600.)
        .child(
            row()
                .w_full()
                .h_full()
                .child(div().id("sidebar").w(200.).h_full().bg(rgb(0x202020)))
                .child(
                    column()
                        .w(600.)
                        .h_full()
                        .child(div().id("toolbar").w_full().h(100.).bg(rgb(0x303030)))
                        .child(page),
                )
                .child(div().id("inspector").w(200.).h_full().bg(rgb(0x252525))),
        )
        .child(
            div()
                .id("composer")
                .absolute()
                .left(300.)
                .top(520.)
                .w(400.)
                .h(60.)
                .rounded(16.)
                .blur(20.)
                .bg(rgba(0xffffff30)),
        );
    if popover {
        root = root
            .child(
                div()
                    .id("popover")
                    .absolute()
                    .left(150.)
                    .top(120.)
                    .w(200.)
                    .h(150.)
                    .rounded(12.)
                    .bg(rgba(0x10101080))
                    .on_click(|| {}),
            )
            // Over the popover's part beyond the page, not over the page.
            .child(
                div()
                    .id("badge")
                    .absolute()
                    .left(160.)
                    .top(130.)
                    .w(20.)
                    .h(20.)
                    .bg(rgb(0xff0000)),
            );
    }
    root
}

#[test]
fn a_native_node_is_placed_where_layout_puts_it_clipped_and_rounded() {
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(natives.clone(), browser(&web, false)));
    ui.prepare_frame();
    assert_eq!(natives.nodes(), vec![view.find("page").unwrap()]);
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let page = &split.natives()[0];
    assert_eq!(page.frame, Rect::new(200., 100., 600., 500.));
    assert_eq!(page.size, (600., 500.));
    assert_eq!(page.opacity, 1.);
    // Its own corners round it.
    assert_eq!(page.rounded.len(), 1);
    assert_eq!(page.rounded[0].radii, [10.; 4]);
    assert_eq!(page.visible(), Some(Rect::new(200., 100., 600., 500.)));
    // In a window zoomed to 125%, the platform places it in the zoom's
    // points: larger, and sized (not drawn scaled), its corners with it.
    let zoomed = split.zoomed(1.25);
    let page = &zoomed.natives()[0];
    assert_eq!(page.frame, Rect::new(250., 125., 750., 625.));
    assert_eq!(page.size, (750., 625.));
    assert_eq!(page.rounded[0].radii, [12.5; 4]);
    // The composer's glass over it, at (300, 520) 400 by 60.
    assert_eq!(split.backdrops()[0].rect, Rect::new(300., 520., 400., 60.));
    assert_eq!(zoomed.backdrops()[0].rect, Rect::new(375., 650., 500., 75.));
}

#[test]
fn scrolling_clipping_scaling_and_hiding_follow_the_node() {
    let mut ui = Ui::new(800., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let offset = ui.signal(0.0_f32);
    let shown = ui.signal(true);
    let opacity = ui.signal(1.0_f32);
    let scale = ui.signal(1.0_f32);
    let page = {
        let shown = shown.clone();
        let opacity = opacity.clone();
        let scale = scale.clone();
        web_view(&web)
            .id("page")
            .w(400.)
            .h(300.)
            .reactive_style(move || {
                let styles = Styles::default().opacity(opacity.get()).scale(scale.get());
                if shown.get() { styles } else { styles.hidden() }
            })
    };
    let view = ui.mount(provide(
        natives.clone(),
        column().p(50.).child(
            div().w(500.).h(200.).rounded(20.).overflow_hidden().child(
                scroll(offset.clone()).w(500.).h(200.).child(
                    column()
                        .child(div().h(100.))
                        .child(page)
                        .child(div().h(2000.)),
                ),
            ),
        ),
    ));
    ui.prepare_frame();
    let node = view.find("page").unwrap();
    let place = |ui: &Ui| {
        let scene = ui.scene.borrow();
        PlaneSplit::compute(&scene, &natives.nodes(), &PlaneSplit::default())
            .0
            .native(node)
            .cloned()
    };
    let at_rest = place(&ui).unwrap();
    assert_eq!(at_rest.frame, Rect::new(50., 150., 400., 300.));
    // The scroll viewport clips it, and the rounded ancestor rounds it.
    assert_eq!(at_rest.visible(), Some(Rect::new(50., 150., 400., 100.)));
    assert_eq!(at_rest.rounded.len(), 1);
    assert_eq!(at_rest.rounded[0].rect, Rect::new(50., 50., 500., 200.));
    assert_eq!(at_rest.rounded[0].radii, [20.; 4]);

    offset.set(80.);
    ui.prepare_frame();
    let scrolled = place(&ui).unwrap();
    assert_eq!(scrolled.frame, Rect::new(50., 70., 400., 300.));
    assert_eq!(scrolled.visible(), Some(Rect::new(50., 70., 400., 180.)));

    opacity.set(0.5);
    ui.prepare_frame();
    assert_eq!(place(&ui).unwrap().opacity, 0.5);

    // Scaled about its centre: the box on screen shrinks, its size stays.
    opacity.set(1.);
    scale.set(0.5);
    ui.prepare_frame();
    let scaled = place(&ui).unwrap();
    assert_eq!(scaled.size, (400., 300.));
    assert_eq!(scaled.frame, Rect::new(150., 145., 200., 150.));

    scale.set(1.);
    shown.set(false);
    ui.prepare_frame();
    assert!(place(&ui).is_none(), "hidden nodes place no view");
    shown.set(true);
    offset.set(1000.);
    ui.prepare_frame();
    let gone = place(&ui);
    assert!(
        gone.as_ref().is_none_or(|gone| gone.visible().is_none()),
        "scrolled out of view: {gone:?}"
    );
}

#[test]
fn only_paint_after_and_over_a_native_view_goes_on_the_overlay() {
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(natives.clone(), browser(&web, true)));
    ui.prepare_frame();
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let id = |name: &str| view.find(name).unwrap();
    // Before the page in paint order: under it.
    assert!(!split.in_overlay(id("sidebar")));
    assert!(!split.in_overlay(id("toolbar")));
    // After it but beside it: stays on the base plane, keeping zgx's blur.
    assert!(!split.in_overlay(id("inspector")));
    // Over it.
    assert!(split.in_overlay(id("popover")));
    assert!(split.in_overlay(id("composer")));
    // Not over the page, but over overlay paint (the popover's overhang).
    assert!(split.in_overlay(id("badge")));
    assert!(split.has_overlay());
    let bounds = split.overlay_bounds().unwrap();
    assert!(bounds.x <= 150. && bounds.y <= 120. && bounds.x + bounds.width >= 700.);

    // Renderers keep their plane's items.
    let scene = ui.scene.borrow();
    let items = scene.layer_items(None);
    let mut base = items.clone();
    split.filter(&mut base, Plane::Base);
    let mut overlay = items.clone();
    split.filter(&mut overlay, Plane::Overlay);
    let names = |items: &[zgx::scene::PaintItem<'_>]| -> Vec<NodeIdName> {
        items.iter().map(|item| NodeIdName(item.id)).collect()
    };
    #[derive(PartialEq, Debug)]
    struct NodeIdName(zgx::scene::NodeId);
    assert!(names(&base).contains(&NodeIdName(id("sidebar"))));
    assert!(!names(&base).contains(&NodeIdName(id("popover"))));
    assert!(names(&overlay).contains(&NodeIdName(id("popover"))));
    assert!(!names(&overlay).contains(&NodeIdName(id("inspector"))));
    assert_eq!(base.len() + overlay.len(), items.len());
}

#[test]
fn glass_over_a_native_view_gets_a_platform_backdrop_shaped_to_the_node() {
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(natives.clone(), browser(&web, true)));
    ui.prepare_frame();
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let composer = view.find("composer").unwrap();
    let backdrops = split.backdrops();
    assert_eq!(backdrops.len(), 1);
    let glass = &backdrops[0];
    assert_eq!(glass.node, composer);
    assert_eq!(glass.rect, Rect::new(300., 520., 400., 60.));
    assert_eq!(glass.radii, [16.; 4]);
    assert_eq!(glass.sigma, 20.);
    // Nothing of the overlay's own lies under it: its overlay blur pass
    // is dropped (the platform blurs the page and the base plane).
    let scene = ui.scene.borrow();
    let mut items = scene.layer_items(None);
    split.filter(&mut items, Plane::Overlay);
    let item = items.iter().find(|item| item.id == composer).unwrap();
    assert_eq!(item.effects.blur_radius, 0.);
    let mut base = scene.layer_items(None);
    split.filter(&mut base, Plane::Base);
    assert!(base.iter().all(|item| item.id != composer));
}

#[test]
fn glass_scaled_over_a_native_view_keeps_its_platform_backdrop() {
    // A transform makes the glass a layer of its own, whose contents the
    // split walks with the root's effects taken off; its blur is still
    // over the page, as a selector's pill is while it scales to a width.
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(
        natives.clone(),
        overlay()
            .w(1000.)
            .h(600.)
            .child(web_view(&web).id("page").w_full().h_full())
            .child(
                div()
                    .id("pill")
                    .absolute()
                    .left(100.)
                    .top(100.)
                    .w(100.)
                    .h(40.)
                    .rounded(20.)
                    .blur(12.)
                    .bg(rgba(0xffffff30))
                    .transform_origin(0., 0.)
                    .scale_xy(2., 1.),
            ),
    ));
    ui.prepare_frame();
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let pill = view.find("pill").unwrap();
    let backdrops = split.backdrops();
    assert_eq!(backdrops.len(), 1, "the scaled glass keeps its backdrop");
    let glass = &backdrops[0];
    assert_eq!(glass.node, pill);
    assert_eq!(glass.rect, Rect::new(100., 100., 200., 40.));
    assert_eq!(glass.radii, [20.; 4], "corners scale by the lesser axis");
    assert_eq!(glass.sigma, 12. * 2f32.sqrt());
    assert!(split.in_overlay(pill));
    let (again, damage) = PlaneSplit::compute(&ui.scene.borrow(), &natives.nodes(), &split);
    assert_eq!(again.backdrops(), split.backdrops());
    assert!(damage.is_empty(), "a still pill adds no damage");
}

#[test]
fn without_native_views_everything_is_base_and_nothing_is_computed() {
    let mut ui = Ui::new(400., 300.);
    let natives = supported();
    ui.mount(provide(
        natives.clone(),
        column().child(div().w(100.).h(100.).bg(rgb(0xff0000))),
    ));
    ui.prepare_frame();
    let (split, damage) = split(&ui, &natives, &PlaneSplit::default());
    assert!(split.is_empty() && !split.has_overlay() && damage.is_empty());
    let scene = ui.scene.borrow();
    let mut items = scene.layer_items(None);
    let count = items.len();
    split.filter(&mut items, Plane::Base);
    assert_eq!(items.len(), count);
    split.filter(&mut items, Plane::Overlay);
    assert!(items.is_empty());
}

#[test]
fn moving_paint_onto_a_native_view_damages_both_planes_there() {
    let mut ui = Ui::new(800., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let x = ui.signal(500.0_f32);
    let card = {
        let x = x.clone();
        div()
            .id("card")
            .absolute()
            .top(10.)
            .w(100.)
            .h(50.)
            .bg(rgb(0x00ff00))
            .reactive_style(move || Styles::default().left(x.get()))
    };
    let view = ui.mount(provide(
        natives.clone(),
        overlay()
            .w(800.)
            .h(600.)
            .child(web_view(&web).id("page").w(300.).h(300.))
            .child(card),
    ));
    ui.prepare_frame();
    let card = view.find("card").unwrap();
    let (first, _) = split(&ui, &natives, &PlaneSplit::default());
    assert!(!first.in_overlay(card));
    x.set(250.);
    ui.prepare_frame();
    let (second, damage) = split(&ui, &natives, &first);
    assert!(second.in_overlay(card));
    assert!(damage.contains(&Rect::new(250., 10., 100., 50.)));
    x.set(500.);
    ui.prepare_frame();
    let (third, damage) = split(&ui, &natives, &second);
    assert!(!third.in_overlay(card));
    assert!(damage.contains(&Rect::new(250., 10., 100., 50.)));
}

#[test]
fn pointer_input_goes_to_the_page_unless_zgx_is_drawn_or_interactive_there() {
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(
        natives.clone(),
        browser(&web, true)
            // A transparent wrapper over everything, inert: input passes.
            .child(div().absolute().left(0.).top(0.).w(1000.).h(600.)),
    ));
    ui.prepare_frame();
    let page = view.find("page").unwrap();
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let scene = ui.scene.borrow();
    let route = |x, y| split.route(&scene, &ui.input, x, y);
    assert_eq!(route(500., 350.), PointerRoute::Native(page));
    // Over the popover and the glass composer: zgx.
    assert_eq!(route(250., 200.), PointerRoute::Ui);
    assert_eq!(route(500., 550.), PointerRoute::Ui);
    // Off the page: zgx.
    assert_eq!(route(100., 300.), PointerRoute::Ui);
    assert_eq!(route(900., 300.), PointerRoute::Ui);
    // The page's own rounded box is hit as a box; outside its clip, zgx.
    assert_eq!(route(500., 50.), PointerRoute::Ui);
}

#[test]
fn an_interactive_overlay_in_front_takes_input_and_a_modal_scope_blocks_the_page() {
    let mut ui = Ui::new(800., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let open = ui.signal(false);
    let view = ui.mount(provide(
        natives.clone(),
        overlay()
            .w(800.)
            .h(600.)
            .child(web_view(&web).id("page").w(800.).h(600.))
            .child(
                modal("Dialog", open.clone())
                    .id("dialog")
                    .w(200.)
                    .h(100.)
                    .bg(rgb(0x333333)),
            ),
    ));
    ui.prepare_frame();
    let page = view.find("page").unwrap();
    let at = |ui: &Ui| {
        let (split, _) = {
            let scene = ui.scene.borrow();
            PlaneSplit::compute(&scene, &natives.nodes(), &PlaneSplit::default())
        };
        let scene = ui.scene.borrow();
        split.route(&scene, &ui.input, 700., 500.)
    };
    assert_eq!(at(&ui), PointerRoute::Native(page));
    open.set(true);
    ui.prepare_frame();
    // The modal's backdrop listens everywhere (dismissal), and its scope
    // leaves the page outside: clicks there go to zgx.
    assert_eq!(at(&ui), PointerRoute::Ui);
    open.set(false);
    ui.prepare_frame();
    assert_eq!(at(&ui), PointerRoute::Native(page));
}

#[test]
fn unmounting_unregisters_and_unsupported_windows_show_a_placeholder() {
    let mut ui = Ui::new(400., 300.);
    let natives = NativeViews::new();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    web.load_url("https://example.com/");
    let shown = ui.signal(true);
    let view = {
        let web = web.clone();
        let shown = shown.clone();
        ui.mount(provide(
            natives.clone(),
            switch(
                move || shown.get(),
                move |shown, _| {
                    if shown {
                        web_view(&web).id("page").w(200.).h(100.)
                    } else {
                        div()
                    }
                },
            ),
        ))
    };
    ui.prepare_frame();
    let page = view.find("page").unwrap();
    assert_eq!(natives.nodes(), vec![page]);
    // Unsupported here: the node holds a labelled box.
    let scene = ui.scene.borrow();
    assert!(!scene.children(page).is_empty());
    drop(scene);
    shown.set(false);
    ui.prepare_frame();
    assert!(natives.is_empty());
    // The controller (and its queued navigation) lives on.
    assert_eq!(web.url(), "https://example.com/");
}

struct Recorder(RefCell<Vec<String>>);
impl WebViewBackend for Recorder {
    fn run(&self, command: WebViewCommand) {
        self.0.borrow_mut().push(format!("{command:?}"));
        if let WebViewCommand::Evaluate {
            reply: Some(reply), ..
        } = command
        {
            zgx::native_view::post(move || reply(Ok("42".into())));
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn commands_queue_until_a_page_attaches_and_events_arrive_when_delivered() {
    let ui = Ui::new(100., 100.);
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    web.load_url("https://a.test/");
    web.set_src("https://a.test/");
    web.set_src("https://b.test/");
    web.go_back();
    let backend = Rc::new(Recorder(RefCell::new(Vec::new())));
    web.attach(backend.clone());
    assert_eq!(
        *backend.0.borrow(),
        vec![
            "LoadUrl(\"https://a.test/\")".to_string(),
            "LoadUrl(\"https://b.test/\")".to_string(),
            "GoBack".to_string(),
        ]
    );
    assert!(web.is_attached());
    let result = Rc::new(RefCell::new(None));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _subscription = {
        let seen = seen.clone();
        web.on_event(move |event| seen.borrow_mut().push(event.clone()))
    };
    {
        let result = result.clone();
        web.evaluate_js("6 * 7", move |value| *result.borrow_mut() = Some(value));
    }
    let weak = web.downgrade();
    zgx::native_view::post(move || {
        let web = weak.upgrade().unwrap();
        web.update_state(|state| state.title = "B".into());
        web.emit(WebViewEvent::TitleChanged { title: "B".into() });
    });
    assert!(result.borrow().is_none() && seen.borrow().is_empty());
    assert!(zgx::native_view::deliver_posted());
    assert_eq!(*result.borrow(), Some(Ok("42".to_string())));
    assert_eq!(web.title(), "B");
    assert_eq!(
        *seen.borrow(),
        vec![WebViewEvent::TitleChanged { title: "B".into() }]
    );
}

#[test]
fn a_layer_far_from_native_views_stays_on_the_base_plane() {
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(
        natives.clone(),
        row()
            .w(1000.)
            .h(600.)
            .child(
                // A spinner in a sidebar: a layer (it turns) away from the page.
                div().w(200.).h(600.).child(
                    div()
                        .id("spinner")
                        .w(10.)
                        .h(10.)
                        .bg(zgx::scene::Color(255, 0, 0, 255)),
                ),
            )
            .child(web_view(&web).w(800.).h(600.)),
    ));
    ui.prepare_frame();
    let (first, _) = split(&ui, &natives, &PlaneSplit::default());
    // It becomes a layer after the first split: later splits walk only what
    // reaches the page, so never visit it.
    let spinner = view.find("spinner").unwrap();
    ui.scene.borrow_mut().set_isolated(spinner, true);
    ui.prepare_frame();
    let (later, _) = split(&ui, &natives, &first);
    let scene = ui.scene.borrow();
    let item = scene
        .layer_items(None)
        .into_iter()
        .find(|item| item.id == spinner)
        .expect("painted");
    assert!(item.isolated);
    assert!(later.keeps(&item, Plane::Base));
    assert!(!later.keeps(&item, Plane::Overlay));
}

#[test]
fn a_painted_pointer_transparent_layer_leaves_the_page_its_input() {
    // A tinted scrim over the page that lets the pointer through: it paints
    // on the overlay plane, but the page still takes the pointer under it.
    let mut ui = Ui::new(1000., 600.);
    let natives = supported();
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let view = ui.mount(provide(
        natives.clone(),
        browser(&web, false).child(
            div()
                .absolute()
                .left(0.)
                .top(0.)
                .w(1000.)
                .h(600.)
                .bg(rgba(0x00000020))
                .pointer_events(false),
        ),
    ));
    ui.prepare_frame();
    let page = view.find("page").unwrap();
    let (split, _) = split(&ui, &natives, &PlaneSplit::default());
    let scene = ui.scene.borrow();
    assert_eq!(
        split.route(&scene, &ui.input, 500., 350.),
        PointerRoute::Native(page)
    );
}

#[test]
fn closing_a_page_drops_its_platform_page_while_handles_remain() {
    let ui = Ui::new(100., 100.);
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    // A handle a script still holds (until it collects it).
    let kept = web.clone();
    let backend = Rc::new(Recorder(RefCell::new(Vec::new())));
    web.attach(backend.clone());
    assert_eq!(Rc::strong_count(&backend), 2);
    web.close();
    assert!(kept.is_closed() && !kept.is_attached());
    assert_eq!(Rc::strong_count(&backend), 1, "the page let go of it");
    // Nothing reaches a page, and none attaches again.
    kept.load_url("https://late.test/");
    let again = Rc::new(Recorder(RefCell::new(Vec::new())));
    kept.attach(again.clone());
    assert!(!kept.is_attached());
    assert!(backend.0.borrow().is_empty() && again.0.borrow().is_empty());
}

#[test]
fn a_hollow_box_over_a_page_takes_the_pointer_only_while_it_shows_something() {
    use zgx::native_view::NativeContent;
    // A full-size box holding a page kept mounted (an app's settings) over
    // a web page: while what it holds is hidden or invisible, the web page
    // takes the pointer; shown, it does (it has no listeners of its own).
    let mut ui = Ui::new(800., 600.);
    let natives = supported();
    // 0: hidden, 1: invisible, 2: shown.
    let shown = ui.signal(0_u8);
    let mode = shown.clone();
    let view = ui.mount(provide(
        natives.clone(),
        overlay()
            .w(800.)
            .h(600.)
            .child(
                zgx::web_view::native_view(NativeContent::Platform(Rc::new(())))
                    .id("page")
                    .w_full()
                    .h_full(),
            )
            .child(
                column()
                    .absolute()
                    .left(0.)
                    .top(0.)
                    .w(800.)
                    .h(600.)
                    .hollow()
                    .child(column().w_full().h_full().bg(rgb(0x202020)).reactive_style(
                        move || match mode.get() {
                            0 => Styles::new().hidden(),
                            1 => Styles::new().invisible(),
                            _ => Styles::new(),
                        },
                    )),
            ),
    ));
    let page = view.find("page").unwrap();
    let at = |ui: &Ui| {
        ui.prepare_frame();
        let (split, _) = split(ui, &natives, &PlaneSplit::default());
        let scene = ui.scene.borrow();
        split.route(&scene, &ui.input, 400., 300.)
    };
    assert_eq!(at(&ui), PointerRoute::Native(page), "hidden");
    for (mode, route) in [
        (2, PointerRoute::Ui),
        (1, PointerRoute::Native(page)),
        (2, PointerRoute::Ui),
        (0, PointerRoute::Native(page)),
    ] {
        shown.set(mode);
        assert_eq!(at(&ui), route, "mode {mode}");
    }
}
