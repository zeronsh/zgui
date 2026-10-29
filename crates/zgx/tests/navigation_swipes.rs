//! A trackpad's swipe between pages (`Ui::set_navigation_swipes`): a
//! gesture that starts across where nothing under it scrolls that way, over
//! a view that takes navigation, is the host's to track, and scrolls and
//! stretches nothing; one that a scroll view takes scrolls as before.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Navigation, ScrollGesture},
    reactive::Signal,
    scene::NodeId,
    widgets::Ui,
};

struct Page {
    /// The row's offset across.
    offset: Signal<f32>,
    /// The row's content.
    content: NodeId,
    went: Rc<RefCell<Vec<Navigation>>>,
}

/// A 300-wide page (navigating if `navigates`) holding a 200-wide row that
/// scrolls across 600 of content, above a plain box 100 high.
fn mount(ui: &mut Ui, navigates: bool) -> Page {
    ui.set_elastic_scrolling(true);
    ui.set_navigation_swipes(true);
    let offset = ui.signal(0.);
    let went = Rc::new(RefCell::new(Vec::new()));
    let page = column()
        .w(300.)
        .h(200.)
        .child(
            scroll_x(offset.clone())
                .id("row")
                .w(200.)
                .h(100.)
                .child(div().w(600.).h(100.)),
        )
        .child(div().w(300.).h(100.));
    let log = went.clone();
    let page = if navigates {
        page.on_navigate(move |navigation| log.borrow_mut().push(navigation))
    } else {
        page
    };
    let view = ui.mount(page);
    ui.prepare_frame();
    let row = view.find("row").unwrap();
    let content = {
        let scene = ui.scene.borrow();
        scene.children(scene.children(row)[0])[0]
    };
    Page {
        offset,
        content,
        went,
    }
}

/// How far the row's content shows from where its offset puts it.
fn stretch(ui: &Ui, page: &Page) -> f32 {
    ui.scene.borrow().transform(page.content).x + page.offset.get()
}

fn scroll(ui: &mut Ui, x: f32, y: f32, delta_x: f32, delta_y: f32) {
    ui.dispatch(InputEvent::Scroll {
        x,
        y,
        delta_x,
        delta_y,
    });
    ui.prepare_frame();
}

fn phase(ui: &mut Ui, phase: ScrollGesture) {
    ui.dispatch(InputEvent::ScrollGesture(phase));
    ui.prepare_frame();
}

#[test]
fn a_swipe_across_where_nothing_scrolls_is_the_hosts_to_track() {
    let mut ui = Ui::new(400., 300.);
    let page = mount(&mut ui, true);
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 150., 150., -20., 2.);
    assert!(ui.take_navigation_swipe(), "over the plain box: a swipe");
    assert!(!ui.take_navigation_swipe(), "heard once");
    // The rest of the gesture is the swipe's: over the row, it neither
    // scrolls nor stretches it.
    scroll(&mut ui, 100., 50., -30., 0.);
    scroll(&mut ui, 100., 50., 40., 0.);
    assert_eq!(page.offset.get(), 0.);
    assert_eq!(stretch(&ui, &page), 0.);
    phase(&mut ui, ScrollGesture::Ended);
    assert!(page.went.borrow().is_empty(), "the host navigates, later");
    // The next gesture scrolls again.
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 100., 50., 30., 0.);
    assert!(!ui.take_navigation_swipe());
    assert_eq!(page.offset.get(), 30.);
}

#[test]
fn a_scroll_view_takes_what_it_can_scroll_and_swipes_start_at_its_edge() {
    let mut ui = Ui::new(400., 300.);
    let page = mount(&mut ui, true);
    // Toward its end: the row scrolls.
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 100., 50., 30., 0.);
    assert!(!ui.take_navigation_swipe());
    assert_eq!(page.offset.get(), 30.);
    // Back past its start in the same gesture: it stretches, as AppKit's do;
    // a swipe starts a gesture, never in the middle of one.
    scroll(&mut ui, 100., 50., -30., 0.);
    scroll(&mut ui, 100., 50., -30., 0.);
    assert!(!ui.take_navigation_swipe());
    assert_eq!(page.offset.get(), 0.);
    assert!(stretch(&ui, &page) != 0., "stretched at its start");
    phase(&mut ui, ScrollGesture::Ended);
    ui.advance_interactions(std::time::Instant::now() + std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(stretch(&ui, &page), 0.);
    // At its start, a gesture toward it is a swipe: no stretch.
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 100., 50., -20., 0.);
    assert!(ui.take_navigation_swipe(), "at its edge: a swipe");
    assert_eq!(stretch(&ui, &page), 0.);
    assert_eq!(page.offset.get(), 0.);
}

#[test]
fn with_nothing_that_navigates_or_swipes_off_it_scrolls_and_stretches() {
    let mut ui = Ui::new(400., 300.);
    let page = mount(&mut ui, false);
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 100., 50., -20., 0.);
    assert!(!ui.take_navigation_swipe(), "nothing navigates");
    assert!(stretch(&ui, &page) > 0., "the row stretches as before");
    phase(&mut ui, ScrollGesture::Ended);

    let mut ui = Ui::new(400., 300.);
    let page = mount(&mut ui, true);
    ui.set_navigation_swipes(false);
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 100., 50., -20., 0.);
    assert!(!ui.take_navigation_swipe(), "swipes off");
    assert!(stretch(&ui, &page) > 0.);
    phase(&mut ui, ScrollGesture::Ended);
    // A mouse's wheel (no gesture) never swipes.
    ui.set_navigation_swipes(true);
    scroll(&mut ui, 150., 150., -20., 0.);
    assert!(!ui.take_navigation_swipe(), "a wheel");
}

#[test]
fn a_gesture_that_starts_down_never_becomes_a_swipe() {
    let mut ui = Ui::new(400., 300.);
    let _page = mount(&mut ui, true);
    phase(&mut ui, ScrollGesture::Began);
    scroll(&mut ui, 150., 150., 2., 20.);
    scroll(&mut ui, 150., 150., -30., 0.);
    assert!(!ui.take_navigation_swipe());
    phase(&mut ui, ScrollGesture::Ended);
    // Nor does its momentum.
    phase(&mut ui, ScrollGesture::MomentumBegan);
    scroll(&mut ui, 150., 150., -30., 0.);
    assert!(!ui.take_navigation_swipe());
}

#[test]
fn navigating_reaches_the_nearest_view_that_navigates_under_the_point() {
    let mut ui = Ui::new(400., 300.);
    let (outer, inner) = (
        Rc::new(RefCell::new(Vec::new())),
        Rc::new(RefCell::new(Vec::new())),
    );
    let (outer_log, inner_log) = (outer.clone(), inner.clone());
    ui.mount(
        column()
            .w(300.)
            .h(200.)
            .on_navigate(move |navigation| outer_log.borrow_mut().push(navigation))
            .child(
                div()
                    .w(300.)
                    .h(100.)
                    .on_navigate(move |navigation| inner_log.borrow_mut().push(navigation)),
            )
            .child(div().w(300.).h(100.)),
    );
    ui.prepare_frame();
    assert!(ui.input.navigate(&ui.scene, 50., 50., Navigation::Back));
    assert!(ui.input.navigate(&ui.scene, 50., 150., Navigation::Forward));
    assert_eq!(*inner.borrow(), [Navigation::Back], "the nearest takes it");
    assert_eq!(*outer.borrow(), [Navigation::Forward]);
    assert!(
        !ui.input.navigate(&ui.scene, 350., 250., Navigation::Back),
        "nothing there navigates"
    );
}
