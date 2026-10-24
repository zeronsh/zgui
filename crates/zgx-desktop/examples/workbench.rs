//! A small, responsive notes workspace exercising the public framework API.
//! Run: cargo run -p zgx-desktop --example workbench
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};
use zgx::{
    components::{Dialog, Popover, ScrollView, VirtualListView},
    input::{EventPhase, InputEvent, NodeInput},
    scene::{Align, Color, Justify, Layout, NodeId, NodeKind, QuadStyle, Style, Transform},
    semantics::{Role, SemanticNode, Semantics},
    widgets::{Ui, fixed},
};
use zgx_desktop::{Application, WindowOptions};

const TEXT: Color = Color(225, 233, 246, 255);
const MUTED: Color = Color(146, 163, 187, 255);
const PANEL: Color = Color(24, 33, 48, 255);
const BORDER: Color = Color(49, 65, 87, 255);

fn label(
    ui: &mut Ui,
    parent: NodeId,
    text: &str,
    size: f32,
    color: Color,
    width: f32,
    height: f32,
) -> NodeId {
    let node = ui.label(parent, text, fixed(width, height));
    ui.scene.borrow_mut().set_kind(
        node,
        NodeKind::Text {
            text: text.into(),
            color,
            font_size: size,
        },
    );
    node
}
fn panel(ui: &mut Ui, parent: NodeId, style: Style) -> (NodeId, NodeId) {
    let root = ui.container(
        parent,
        Layout::Overlay,
        Style {
            align: Align::Stretch,
            justify: Justify::Stretch,
            ..style
        },
    );
    ui.scene.borrow_mut().append(
        root,
        NodeKind::Quad(QuadStyle {
            fill: PANEL,
            radius: 12.0,
            border_color: BORDER,
            border_width: 1.0,
            ..QuadStyle::default()
        }),
        Style::default(),
    );
    let body = ui.container(
        root,
        Layout::Column,
        Style {
            padding: 20.0,
            gap: 12.0,
            ..Style::default()
        },
    );
    (root, body)
}
struct AccessibleRow {
    semantics: Rc<RefCell<Semantics>>,
    node: NodeId,
}
impl Drop for AccessibleRow {
    fn drop(&mut self) {
        self.semantics.borrow_mut().remove(self.node);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx — Notes workspace".into(),
            width: 1100.0,
            height: 850.0,
            ..WindowOptions::default()
        })
        .run(build)
}
fn build(cx: &mut zgx_desktop::WindowContext) {
    let viewport = cx.viewport.clone();
    let tasks = cx.tasks.clone();
    let ui = &mut cx.ui;
    {
        use zgx::theme::{DARK, Theme, ThemeKind, tok};
        let surface = Color(37, 51, 72, 255);
        ui.theme().register(
            Theme::new("workbench", ThemeKind::Dark)
                .extending(DARK)
                .with(tok::BACKGROUND, Color(13, 19, 29, 255))
                .with(tok::SURFACE_RAISED, surface)
                .with(tok::SURFACE_OVERLAY, surface)
                // The hover colour the widgets always had, over this surface.
                .with(tok::BUTTON_HOVER, Color(47, 64, 88, 255))
                .with(tok::ACCENT, Color(89, 158, 237, 255))
                .with(tok::TEXT_PRIMARY, TEXT),
        );
        ui.theme().set("workbench").unwrap();
    }
    let root = ui.root();
    ui.scene
        .borrow_mut()
        .set_kind(root, NodeKind::Container(Layout::Overlay));
    let shell = ui.container(
        root,
        Layout::Column,
        Style {
            padding: 24.0,
            gap: 20.0,
            ..fixed(1100.0, 850.0)
        },
    );
    let header = ui.container(
        shell,
        Layout::Row,
        Style {
            gap: 16.0,
            align: Align::Center,
            ..fixed(1052.0, 54.0)
        },
    );
    let brand = ui.container(
        header,
        Layout::Column,
        Style {
            gap: 3.0,
            flex_grow: 1.0,
            ..Style::default()
        },
    );
    label(ui, brand, "Notes workspace", 25.0, TEXT, 380.0, 31.0);
    label(
        ui,
        brand,
        "LOCAL DRAFTS  /  RETAINED VIEWS",
        11.0,
        MUTED,
        380.0,
        18.0,
    );
    let menu_anchor = ui.button(header, "Workspace ▾", 160.0, || {});
    let main = ui.container(
        shell,
        Layout::Row,
        Style {
            gap: 20.0,
            ..fixed(1052.0, 710.0)
        },
    );
    let (sidebar, sidebar_body) = panel(ui, main, fixed(290.0, 710.0));
    label(ui, sidebar_body, "LIBRARY", 12.0, MUTED, 248.0, 18.0);
    label(
        ui,
        sidebar_body,
        "10,000 local notes",
        17.0,
        TEXT,
        248.0,
        24.0,
    );
    let selected = ui.signal(0usize);
    let select = selected.clone();
    let runtime = ui.runtime.clone();
    let scene = ui.scene.clone();
    let input = ui.input.clone();
    let semantics = ui.semantics.clone();
    let list = VirtualListView::mount(
        ui,
        sidebar_body,
        250.0,
        600.0,
        10_000,
        58.0,
        2,
        |i| i,
        move |index, _, scope| {
            let row = scope.root();
            semantics.borrow_mut().set(
                row,
                SemanticNode::new(Role::Button, format!("Open note {:04}", index + 1)),
            );
            scope.retain(AccessibleRow {
                semantics: semantics.clone(),
                node: row,
            });
            scene
                .borrow_mut()
                .set_kind(row, NodeKind::Container(Layout::Overlay));
            let bg = scope.append(NodeKind::Quad(QuadStyle::default()), fixed(246.0, 54.0));
            let title = format!("Note {:04}", index + 1);
            let text = scope.text(fixed(218.0, 24.0), TEXT, 14.0, move || title.clone());
            scene
                .borrow_mut()
                .set_transform(text, Transform { x: 12.0, y: 6.0 });
            let sub = scope.text(fixed(218.0, 18.0), MUTED, 11.0, move || {
                format!(
                    "{} · private draft",
                    if index % 3 == 0 {
                        "Research"
                    } else {
                        "Personal"
                    }
                )
            });
            scene
                .borrow_mut()
                .set_transform(sub, Transform { x: 12.0, y: 29.0 });
            let focus = runtime.signal(false);
            let current = select.clone();
            let paint_scene = scene.clone();
            let focused = focus.clone();
            scope.effect(move || {
                let active = current.get() == index;
                paint_scene.borrow_mut().set_kind(
                    bg,
                    NodeKind::Quad(QuadStyle {
                        fill: if active {
                            Color(38, 65, 96, 255)
                        } else {
                            PANEL
                        },
                        radius: 8.0,
                        border_width: if focused.get() { 1.0 } else { 0.0 },
                        border_color: Color(120, 180, 245, 255),
                        ..QuadStyle::default()
                    }),
                );
            });
            let current = select.clone();
            scope.retain(input.register(
                row,
                NodeInput {
                    focusable: true,
                    ..NodeInput::default()
                },
                move |cx| {
                    if cx.phase != EventPhase::Target {
                        return;
                    }
                    match cx.event {
                        InputEvent::Activate => {
                            current.set(index);
                        }
                        InputEvent::Focus => {
                            focus.set(true);
                        }
                        InputEvent::Blur => {
                            focus.set(false);
                        }
                        _ => {}
                    }
                },
            ));
        },
    );
    let (details, details_body) = panel(
        ui,
        main,
        Style {
            flex_grow: 1.0,
            min_width: Some(480.0),
            height: Some(710.0),
            ..Style::default()
        },
    );
    let detail_scroll = ScrollView::mount(ui, details_body, 430.0, 670.0);
    detail_scroll.set_content_height(640.0);
    let details_body = detail_scroll.content;
    {
        let mut scene = ui.scene.borrow_mut();
        let mut style = scene.style(details_body);
        style.gap = 12.0;
        scene.set_style(details_body, style);
    }
    let heading = ui.signal(String::new());
    let draft = ui.signal(String::new());
    let status = ui.signal(String::from(
        "Session-only notes · save before switching with autosave off",
    ));
    let drafts: Rc<RefCell<HashMap<usize, String>>> = Rc::new(RefCell::new(HashMap::new()));
    let active_note = Rc::new(Cell::new(0usize));
    let stored = drafts.clone();
    let active = active_note.clone();
    let h = heading.clone();
    let d = draft.clone();
    let selection = selected.clone();
    ui.bind(details, move || {
            let index = selection.get();
            h.set(format!("Note {:04}", index+1));
            active.set(index);
            let value = stored.borrow_mut().entry(index).or_insert_with(|| format!("A place for an idea.\n\nThis is note {}. Try selecting text, composing with your IME,\nand editing with Ctrl/Cmd+Z undo.\n\nScroll the library: only visible rows stay mounted.",index+1)).clone();
            d.set(value);
        });
    let title = ui.label_signal(details_body, heading, fixed(430.0, 32.0));
    {
        let mut s = ui.scene.borrow_mut();
        if let NodeKind::Text { text, .. } = s.kind(title).clone() {
            s.set_kind(
                title,
                NodeKind::Text {
                    text,
                    color: TEXT,
                    font_size: 24.0,
                },
            );
        }
    }
    label(ui, details_body, "Draft", 12.0, MUTED, 430.0, 20.0);
    let editor = ui.text_input(details_body, "Note draft", draft.clone(), 430.0, true);
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    let count = ui.label(details_body, "", fixed(430.0, 20.0));
    let text = draft.clone();
    let scene = ui.scene.clone();
    ui.bind(count, move || {
        text.with(|value| {
            scene.borrow_mut().set_text(
                count,
                format!("{} characters  ·  Unicode editing", value.chars().count()),
            );
        });
    });
    let options = ui.container(
        details_body,
        Layout::Row,
        Style {
            gap: 12.0,
            ..Style::default()
        },
    );
    let highlighted = ui.signal(false);
    ui.checkbox(options, "Highlight note", highlighted.clone(), 180.0);
    let background = ui.scene.borrow().children(details)[0];
    let scene = ui.scene.clone();
    ui.bind(details, move || {
        scene.borrow_mut().set_kind(
            background,
            NodeKind::Quad(QuadStyle {
                fill: PANEL,
                radius: 12.0,
                border_color: if highlighted.get() {
                    Color(89, 158, 237, 255)
                } else {
                    BORDER
                },
                border_width: if highlighted.get() { 2.0 } else { 1.0 },
                ..QuadStyle::default()
            }),
        );
    });
    let autosave = ui.signal(true);
    ui.checkbox(options, "Autosave", autosave.clone(), 210.0);
    let enabled = autosave.clone();
    let text = draft.clone();
    let stored = drafts.clone();
    let active = active_note.clone();
    ui.bind(details, move || {
        if enabled.get() {
            text.with(|value| {
                stored.borrow_mut().insert(active.get(), value.clone());
            });
        }
    });
    label(
        ui,
        details_body,
        "Review progress",
        12.0,
        MUTED,
        430.0,
        20.0,
    );
    let intensity = ui.signal(0.65);
    ui.slider(
        details_body,
        "Review progress",
        intensity.clone(),
        0.0..=1.0,
        430.0,
    );
    ui.progress(details_body, "Review progress", intensity, 430.0);
    let buttons = ui.container(
        details_body,
        Layout::Row,
        Style {
            gap: 12.0,
            ..Style::default()
        },
    );
    let saved = status.clone();
    let saved_text = draft.clone();
    let selection = selected.clone();
    let stored = drafts.clone();
    ui.button(buttons, "Save draft", 140.0, move || {
        let status = saved.clone();
        let value = saved_text.get();
        let bytes = value.len();
        stored.borrow_mut().insert(selection.get(), value);
        let note = selection.get() + 1;
        status.set("Saving…".into());
        tasks.spawn(async move {
            zgx::timer::sleep(Duration::from_millis(160)).await;
            status.set(format!("Saved note {note:04} · {bytes} bytes in memory"));
        });
    });
    let reset = ui.button(buttons, "Reset draft", 140.0, || {});
    ui.label_signal(
        details_body,
        status.clone(),
        Style {
            text_wrap: true,
            ..fixed(430.0, 42.0)
        },
    );
    label(
        ui,
        details_body,
        "Tip: Tab navigates controls. Escape closes overlays.",
        11.0,
        MUTED,
        430.0,
        20.0,
    );
    let overlay = ui.container(root, Layout::Overlay, fixed(1100.0, 850.0));
    let dialog = Dialog::mount(ui, overlay, "Reset draft?", 440.0, 210.0, true);
    let dialog_content = ui.container(
        dialog.body,
        Layout::Column,
        Style {
            padding: 24.0,
            gap: 14.0,
            ..fixed(440.0, 210.0)
        },
    );
    label(
        ui,
        dialog_content,
        "Reset this draft?",
        22.0,
        TEXT,
        390.0,
        30.0,
    );
    label(
        ui,
        dialog_content,
        "The current draft will be replaced with an empty note.",
        13.0,
        MUTED,
        390.0,
        42.0,
    );
    let row = ui.container(
        dialog_content,
        Layout::Row,
        Style {
            gap: 12.0,
            ..Style::default()
        },
    );
    let close = dialog.clone();
    ui.button(row, "Keep editing", 160.0, move || close.close());
    let close = dialog.clone();
    let value = draft.clone();
    let note = status.clone();
    ui.button(row, "Reset", 120.0, move || {
        value.set(String::new());
        note.set("Draft reset".into());
        close.close();
    });
    let show = dialog.clone();
    ui.on_event(reset, true, move |cx| {
        if cx.phase == EventPhase::Target && cx.event == InputEvent::Activate {
            show.show();
        }
    });
    let menu = Popover::mount(ui, overlay, menu_anchor, 230.0, 120.0);
    let value = autosave.clone();
    menu.item(ui, "Toggle autosave", move || {
        value.update(|on| *on = !*on);
    });
    let message = status.clone();
    menu.item(ui, "About this workspace", move || {
        message.set("Built with zgx · GPU retained rendering · local demo".into());
    });
    let window = cx.window.clone();
    menu.item(ui, "Close window", move || window.close());
    let show = menu.clone();
    let scene = ui.scene.clone();
    let input = ui.input.clone();
    // Positioning uses a fresh layout before opening; clone only narrow
    // services, never the Ui owner, into retained callbacks.
    ui.on_event(menu_anchor, true, move |event| {
        if event.phase == EventPhase::Target && event.event == InputEvent::Activate {
            show.show_with(&scene, &input);
        }
    });
    let scene = ui.scene.clone();
    let scroll = list.scroll.clone();
    let modal = dialog.clone();
    ui.bind(shell, move || {
        let (w, h) = viewport.get();
        let width = w.max(820.0);
        let height = h.max(600.0);
        let main_height = (height - 122.0).max(478.0);
        let mut s = scene.borrow_mut();
        s.set_style(
            shell,
            Style {
                padding: 24.0,
                gap: 20.0,
                ..fixed(width, height)
            },
        );
        s.set_style(
            header,
            Style {
                gap: 16.0,
                align: Align::Center,
                ..fixed(width - 48.0, 54.0)
            },
        );
        s.set_style(
            main,
            Style {
                gap: 20.0,
                ..fixed(width - 48.0, main_height)
            },
        );
        s.set_style(
            sidebar,
            Style {
                align: Align::Stretch,
                justify: Justify::Stretch,
                ..fixed(290.0, main_height)
            },
        );
        s.set_style(
            details,
            Style {
                align: Align::Stretch,
                justify: Justify::Stretch,
                flex_grow: 1.0,
                min_width: Some(480.0),
                height: Some(main_height),
                ..Style::default()
            },
        );
        s.set_style(overlay, fixed(w, h));
        drop(s);
        scroll.resize(250.0, (main_height - 112.0).max(100.0));
        detail_scroll.resize(430.0, (main_height - 40.0).max(100.0));
        modal.resize_to_viewport();
    });
    if let Ok(seconds) = zgx::env::var("ZGX_SECONDS")
        .unwrap_or_default()
        .parse::<f64>()
        && seconds > 0.0
    {
        let window = cx.window.clone();
        cx.tasks.spawn(async move {
            zgx::timer::sleep(Duration::from_secs_f64(seconds)).await;
            window.close();
        });
    }
}
