//! Tabs dragged between windows, and torn off into windows of their own
//! (macOS): drag a tab onto the other window's strip to move it there, or
//! drop it outside every window to open it in a new one, the tab under the
//! pointer where it was grabbed. A window's only tab moves the window
//! itself, and dropped on the other window's strip, merges into it.
//! Elsewhere a tab drags within its strip.
use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{DragPhase, InputEvent},
    reactive::Signal,
};
use zgx_desktop::{Application, WindowBounds, WindowContext, WindowFactory, WindowOptions};

#[derive(Clone, PartialEq)]
struct Tab {
    id: u64,
    title: String,
}
impl Tab {
    fn new(title: &str) -> Self {
        thread_local! {
            static NEXT: Cell<u64> = const { Cell::new(0) };
        }
        let id = NEXT.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        Self {
            id,
            title: title.into(),
        }
    }
}
/// A tab being dragged. `kept`: dropped back on its own window's strip.
#[derive(Clone)]
struct TabDrag {
    tab: Tab,
    kept: Rc<Cell<bool>>,
}

/// The strip's height, a tab's top within it, and the strip's padding.
const STRIP: f32 = 38.;
const TAB_TOP: f32 = 6.;
const STRIP_PAD: f32 = 10.;

fn tab_chip(title: &str) -> View {
    div()
        .h(STRIP - TAB_TOP)
        .w(150.)
        .px(12.)
        .items_center()
        .justify_center()
        .rounded(8.)
        .bg(rgba(0x80808060))
        .text_size(13.)
        .child(text(title))
}

/// A window's tabs: dragged within its strip, onto another window's, or out
/// of every window (into a window of their own, under the pointer).
fn tab_strip(
    tabs: Signal<Vec<Tab>>,
    active: Signal<u64>,
    window: zgx_desktop::WindowHandle,
    windows: WindowFactory,
    // A tab hovering the strip lights it, to drop there.
    hovered: Signal<bool>,
    // Whether its window is fullscreen: a tab torn off it opens fullscreen
    // too, in a Space of its own, as Safari's do.
    fullscreen: Signal<bool>,
) -> View {
    let (ids, accept, shown) = (tabs.clone(), tabs.clone(), active.clone());
    let lit = hovered.clone();
    row()
        .w_full()
        .h(STRIP)
        .px(STRIP_PAD)
        .gap(4.)
        .items_end()
        .reactive_style(move || {
            zgx::style::Styles::new().bg(if lit.get() { rgba(0x3d7ef726) } else { rgba(0) })
        })
        .on_event(move |cx| {
            if let InputEvent::Drag(event) = &cx.event
                && event.payload.downcast_ref::<TabDrag>().is_some()
            {
                match event.phase {
                    DragPhase::Over => {
                        hovered.set(true);
                    }
                    DragPhase::Leave | DragPhase::Drop | DragPhase::End => {
                        hovered.set(false);
                    }
                    _ => {}
                }
            }
        })
        .child(
            keyed(
                move || ids.get().iter().map(|tab| tab.id).collect(),
                move |id, _| {
                    let title = tabs
                        .with_untracked(|tabs| tabs.iter().find(|tab| tab.id == id).cloned())
                        .map_or_else(String::new, |tab| tab.title);
                    let label = title.clone();
                    let (selected, chosen) = (active.clone(), active.clone());
                    let remove = {
                        let (window, active) = (window.clone(), active.clone());
                        move |tabs: &Signal<Vec<Tab>>, id: u64| {
                            tabs.update(|tabs| tabs.retain(|tab| tab.id != id));
                            // Its last tab gone, the window goes; its shown
                            // one, the first shows.
                            match tabs.with_untracked(|tabs| tabs.first().map(|tab| tab.id)) {
                                None => window.close(),
                                Some(first) if active.with_untracked(|id| *id) == id => {
                                    active.set(first);
                                }
                                Some(_) => {}
                            }
                        }
                    };
                    let (removed, torn, picked, alone) =
                        (tabs.clone(), tabs.clone(), tabs.clone(), tabs.clone());
                    let remove_torn = remove.clone();
                    let windows = windows.clone();
                    let fullscreen = fullscreen.clone();
                    div()
                        .h(STRIP - TAB_TOP)
                        .w(150.)
                        .px(12.)
                        .items_center()
                        .justify_center()
                        .rounded(8.)
                        .text_size(13.)
                        .reactive_style(move || {
                            zgx::style::Styles::new().bg(if selected.get() == id {
                                rgba(0x80808040)
                            } else {
                                rgba(0x80808014)
                            })
                        })
                        .child(text(&title))
                        .on_click(move || {
                            chosen.set(id);
                        })
                        .on_drag(move || TabDrag {
                            tab: picked.with_untracked(|tabs| {
                                tabs.iter().find(|tab| tab.id == id).cloned().unwrap()
                            }),
                            kept: Rc::default(),
                        })
                        .drag_preview(move |_: &TabDrag| tab_chip(&label))
                        // A window's only tab moves the window itself.
                        .drag_moves_window(move |_: &TabDrag| {
                            alone.with_untracked(|tabs| tabs.len() == 1)
                        })
                        .on_drag_end(move |drag: &TabDrag, accepted| {
                            if accepted && !drag.kept.get() {
                                remove(&removed, drag.tab.id);
                            }
                        })
                        .on_drag_outside(move |drag: &TabDrag, event| {
                            // A window of its own, the tab under the pointer
                            // where it was grabbed.
                            let tab = drag.tab.clone();
                            let factory = windows.clone();
                            let options = match fullscreen.with_untracked(|on| *on) {
                                true => WindowOptions {
                                    fullscreen: true,
                                    ..options(&tab.title, None)
                                },
                                false => options(
                                    &tab.title,
                                    Some((
                                        (STRIP_PAD + event.grab_x) as f64,
                                        (TAB_TOP + event.grab_y) as f64,
                                    )),
                                ),
                            };
                            windows.open(options, move |cx| document(cx, factory, vec![tab]));
                            remove_torn(&torn, drag.tab.id);
                        })
                },
            )
            .flex_row()
            .gap(4.)
            .h_full()
            .items_end(),
        )
        .child(div().flex_grow(1.).h_full())
        .on_drop(move |drag: &TabDrag, _| {
            let tab = drag.tab.clone();
            if accept.with_untracked(|tabs| tabs.contains(&tab)) {
                // Back on its own strip: it stays, moved to the end.
                drag.kept.set(true);
                accept.update(|tabs| {
                    tabs.retain(|other| other.id != tab.id);
                    tabs.push(tab.clone());
                });
            } else {
                accept.update(|tabs| tabs.push(tab.clone()));
            }
            shown.set(tab.id);
        })
}

fn document(cx: &mut WindowContext, windows: WindowFactory, tabs: Vec<Tab>) {
    let first = tabs[0].id;
    let tabs = cx.ui.signal(tabs);
    let active = cx.ui.signal(first);
    let title = {
        let (tabs, active) = (tabs.clone(), active.clone());
        move || {
            let id = active.get();
            tabs.get()
                .into_iter()
                .find(|tab| tab.id == id)
                .map_or_else(String::new, |tab| tab.title)
        }
    };
    cx.render(
        column()
            .w_full()
            .h_full()
            .child(tab_strip(
                tabs,
                active,
                cx.window.clone(),
                windows,
                cx.ui.signal(false),
                cx.window_info.fullscreen.clone(),
            ))
            .child(
                column()
                    .flex_grow(1.)
                    .w_full()
                    .p(20.)
                    .gap(8.)
                    .child(text_signal(title).text_size(22.))
                    .child(
                        text("Drag a tab to the other window, or out of both.")
                            .text_size(13.)
                            .opacity(0.6),
                    ),
            ),
    );
}

fn options(title: &str, under_pointer: Option<(f64, f64)>) -> WindowOptions {
    WindowOptions {
        title: title.into(),
        width: 520.,
        height: 300.,
        under_pointer,
        // Tabs of its own, so none of AppKit's (as Safari's).
        tabbing: zgx_desktop::Tabbing::Disallowed,
        ..Default::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let second = WindowOptions {
        bounds: Some(WindowBounds::new(1040, 600).at(1300, 300)),
        ..options("Two", None)
    };
    Application::new()
        .window(WindowOptions {
            bounds: Some(WindowBounds::new(1040, 600).at(200, 300)),
            ..options("One", None)
        })
        .run(move |cx| {
            let windows = cx.windows.clone();
            let factory = windows.clone();
            document(cx, windows, vec![Tab::new("Inbox"), Tab::new("Drafts")]);
            factory.open(second, {
                let factory = factory.clone();
                move |cx| document(cx, factory, vec![Tab::new("Archive")])
            });
        })
}
