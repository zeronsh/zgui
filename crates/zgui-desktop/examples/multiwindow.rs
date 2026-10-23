//! Popups past the window's edges: a menu with a submenu, a select, a
//! popover with a text field, a context menu and a tooltip, each near an
//! edge of a small window. On macOS they show in windows of their own, with
//! AppKit's menu and popover material (the theme's defaults there), open as
//! AppKit's do (a menu on press: drag to an item and release to choose it)
//! and fade as they close; elsewhere they stay inside the window. The
//! chips at the bottom drag out to other apps: text into Mail or Notes, a
//! link into Safari, a file into Finder.
//!
//! `--open=file|recent|select|details` opens one at start and `--at=X,Y`
//! places the window (physical pixels), for screenshots.
use zgui::{
    compose::{Placement, PlacementEdge, PlacementSide, prelude::*},
    input::DragItem,
    reactive::Signal,
};
use zgui_desktop::{Application, WindowOptions};

/// A menu sized as AppKit's: a rounded panel, tight rows.
fn mac_menu(label: &str, open: Signal<bool>, anchor: View) -> View {
    menu(label, open, anchor)
        .w(220.)
        .p(5.)
        .gap(0.)
        .rounded(12.)
        .text_size(13.)
}
fn item(label: &str) -> View {
    menu_item(label).h(24.).px(10.).py(0.).rounded(5.)
}
fn separator() -> View {
    div().h(1.).mx(10.).my(5.).bg(rgba(0x80808040))
}
fn toolbar_button(label: &str) -> View {
    button().h(28.).px(12.).text_size(13.).child(text(label))
}

fn file_menu(open: Signal<bool>, recent: Signal<bool>) -> View {
    mac_menu("File", open, toolbar_button("File")).children([
        item("New Window"),
        item("Open…"),
        submenu("Open Recent", recent)
            .trigger(
                item("Open Recent").child(
                    row()
                        .w_full()
                        .justify_between()
                        .child(text("Open Recent"))
                        .child(text("›")),
                ),
            )
            .w(240.)
            .p(5.)
            .rounded(12.)
            .text_size(13.)
            .children(
                [
                    "Quarterly report.numbers",
                    "Launch plan.pages",
                    "notes.md",
                    "zgui",
                ]
                .map(item),
            ),
        separator(),
        item("Close Window"),
        item("Save"),
        item("Save As…"),
        separator(),
        item("Print…"),
    ])
}

/// A pop-up button: its menu opens with the chosen size over the button.
fn select(open: Signal<bool>, chosen: Signal<String>) -> View {
    let label = chosen.clone();
    let anchor = button().h(28.).w(150.).px(10.).text_size(13.).child(
        row()
            .w_full()
            .justify_between()
            .child(text_signal(move || label.get()))
            .child(text("⌃⌄").text_size(10.)),
    );
    let over = chosen.clone();
    let sizes = [
        "Extra small",
        "Small",
        "Medium",
        "Large",
        "Extra large",
        "Huge",
        "Enormous",
    ];
    mac_menu("Size", open, anchor)
        .w(150.)
        .open_over(move || over.get())
        .children(sizes.map(|size| {
            let (chosen, check) = (chosen.clone(), chosen.clone());
            menu_item(size)
                .id(size)
                .h(24.)
                .px(6.)
                .py(0.)
                .rounded(5.)
                .child(
                    row()
                        .gap(4.)
                        .child(
                            text_signal(move || {
                                if check.get() == size { "✓" } else { "" }.to_string()
                            })
                            .w(14.),
                        )
                        .child(text(size)),
                )
                .on_click(move || {
                    chosen.set(size.to_string());
                })
        }))
}

fn details(open: Signal<bool>, name: Signal<String>) -> View {
    let anchor = toolbar_button("Details…").on_click({
        let open = open.clone();
        move || {
            open.set(true);
        }
    });
    popover("Details", open, anchor)
        .placement(Placement {
            side: PlacementSide::Right,
            align: PlacementEdge::Center,
            offset: 8.,
        })
        .w(260.)
        .p(14.)
        .gap(10.)
        .rounded(14.)
        .text_size(13.)
        .child(text("Rename").text_size(15.))
        .child(text_input("Name", name).h(28.))
        .child(
            text("Keys reach this field while the window keeps its focus.")
                .text_wrap(true)
                .text_size(12.)
                .opacity(0.7),
        )
}

/// A chip that drags out to other apps, carrying `export`.
fn chip(label: &'static str, export: DragItem) -> View {
    let look = |view: View| {
        view.h(28.)
            .px(12.)
            .rounded(8.)
            .items_center()
            .justify_center()
            .text_size(12.)
    };
    look(div())
        .bg(rgba(0x3d7ef733))
        .child(text(label))
        .on_drag(move || label)
        .drag_preview(move |label: &&'static str| {
            look(div())
                .bg(rgba(0x3d7ef7e0))
                .text_color(rgb(0xffffff))
                .child(text(*label))
        })
        .drag_export(move |_: &&'static str| vec![export.clone()])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let open_at_start =
        std::env::args().find_map(|arg| arg.strip_prefix("--open=").map(String::from));
    let at = std::env::args().find_map(|arg| {
        let (x, y) = arg.strip_prefix("--at=")?.split_once(',')?;
        Some((x.parse().ok()?, y.parse().ok()?))
    });
    // A file to drag into Finder.
    let path = std::env::temp_dir().join("Dragged from zgui.txt");
    std::fs::write(&path, "Dragged out of a zgui window.\n")?;
    Application::new()
        .window(WindowOptions {
            title: "Popups past the window".into(),
            width: 520.,
            height: 340.,
            bounds: at.map(|(x, y)| zgui_desktop::WindowBounds::new(1040, 680).at(x, y)),
            ..Default::default()
        })
        .run(move |cx| {
            let ui = &cx.ui;
            let file = ui.signal(false);
            let recent = ui.signal(false);
            let size = ui.signal(false);
            let chosen = ui.signal(String::from("Medium"));
            let details_open = ui.signal(false);
            let name = ui.signal(String::from("Untitled"));
            let said = ui.signal(String::from("Right-click (or control-click) here"));
            let shown = said.clone();
            let context = div()
                .flex_grow(1.)
                .w_full()
                .rounded(10.)
                .bg(rgba(0x80808018))
                .items_center()
                .justify_center()
                .child(text_signal(move || shown.get()).text_size(13.).opacity(0.7))
                .context_menu(move |_| {
                    ["Copy", "Paste", "Duplicate", "Move to Trash"]
                        .map(|label| {
                            let said = said.clone();
                            item(label).on_click(move || {
                                said.set(format!("Chose “{label}”"));
                            })
                        })
                        .into_iter()
                        .collect()
                })
                .context_menu_style(
                    zgui::style::Styles::new()
                        .w(200.)
                        .p(5.)
                        .gap(0.)
                        .rounded(12.)
                        .text_size(13.),
                );
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(14.)
                    .gap(12.)
                    .child(
                        row()
                            .w_full()
                            .gap(8.)
                            .items_center()
                            .child(file_menu(file.clone(), recent.clone()))
                            .child(div().flex_grow(1.))
                            .child(toolbar_button("ⓘ").tooltip("Tooltips show above the title bar"))
                            .child(details(details_open.clone(), name)),
                    )
                    .child(context)
                    .child(
                        row()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(row().gap(8.).children([
                                chip(
                                    "A line of text",
                                    DragItem::Text("Dragged out of a zgui window.".into()),
                                ),
                                chip(
                                    "A link",
                                    DragItem::Url("https://github.com/zeronsh/zgui".into()),
                                ),
                                chip("A file", DragItem::File(path.clone())),
                            ]))
                            .child(select(size.clone(), chosen)),
                    ),
            );
            if let Some(which) = open_at_start {
                let opens: Vec<_> = [
                    ("file", file),
                    ("recent", recent),
                    ("select", size),
                    ("details", details_open),
                ]
                .into_iter()
                .filter(|(name, _)| which == *name || (which == "recent" && *name == "file"))
                .map(|(_, open)| open)
                .collect();
                cx.tasks.spawn(async move {
                    for open in opens {
                        zgui::timer::sleep(std::time::Duration::from_millis(600)).await;
                        open.set(true);
                    }
                });
            }
        })
}
