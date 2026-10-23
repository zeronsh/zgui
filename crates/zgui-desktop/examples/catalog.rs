//! The component catalogue in Rust: the controls, overlays and containers
//! built from `zgui::compose`, each with what it does from the keyboard.
//! The Luau catalogue (`crates/zgui-script/examples/catalog`) shows the
//! same through `ui.*`.
//! Run: cargo run -p zgui-desktop --example catalog
use std::rc::Rc;
use zgui::compose::{SelectOption, StepperOptions, Tab, TabsOptions, prelude::*};
use zgui_desktop::{Application, WindowOptions};

/// A titled card with a hint and its controls in a row.
fn section(title: &str, hint: &str, controls: Vec<View>) -> View {
    column()
        .gap(10.)
        .p(16.)
        .rounded(10.)
        .bg(tok::SURFACE_BASE)
        .border(1.)
        .border_color(tok::BORDER_SUBTLE)
        .child(text(title).font_weight(600))
        .child(
            text(hint)
                .text_color(tok::TEXT_SECONDARY)
                .text_size(tok::FONT_SIZE_SM),
        )
        .child(row().gap(12.).items_center().flex_wrap().children(controls))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Components".into(),
            width: 900.,
            height: 760.,
            ..Default::default()
        })
        .run(|cx| {
            let ui = &mut cx.ui;
            let range = ui.signal(1usize);
            let copies = ui.signal(2.);
            let volume = ui.signal(0.5);
            let busy = ui.signal(f32::NAN);
            let notify = ui.signal(true);
            let menu_open = ui.signal(false);
            let wrap = ui.signal(true);
            let size = ui.signal(1usize);
            let documents = ui.signal(vec![
                Tab::new("readme", "README.md").closable(),
                Tab::new("main", "main.rs").closable(),
                Tab::new("cargo", "Cargo.toml").closable(),
                Tab::new("notes", "Notes"),
            ]);
            let document = ui.signal("readme".to_owned());
            let fruit = ui.signal("banana".to_owned());
            let details = ui.signal(false);
            let faq = ui.signal(Some(0usize));

            let shown = range.clone();
            let controls = column()
                .gap(12.)
                .child(section(
                    "Segmented control",
                    "One Tab stop; the arrows move the choice.",
                    vec![
                        segmented("Range", range, ["Day", "Week", "Month"]),
                        text_signal(move || {
                            format!("Showing a {}", ["day", "week", "month"][shown.get()])
                        })
                        .text_color(tok::TEXT_SECONDARY),
                    ],
                ))
                .child(section(
                    "Stepper",
                    "Up and Down step, Page Up and Down ten, Home and End to the ends.",
                    vec![stepper(
                        "Copies",
                        copies,
                        StepperOptions {
                            min: 1.,
                            max: 99.,
                            ..Default::default()
                        },
                    )],
                ))
                .child(section(
                    "Slider and progress",
                    "Arrows step by 5%; Shift or Page Up and Down by 50%. NaN progress is indeterminate.",
                    vec![
                        slider("Volume", volume, 0. ..=1.).step(0.05),
                        progress("Syncing", busy).w(160.),
                    ],
                ))
                .child(section(
                    "Checkbox and buttons",
                    "Space toggles; disabled controls dim; tooltips show on keyboard focus.",
                    vec![
                        checkbox("Notify me", notify),
                        button()
                            .child(text("Save"))
                            .tooltip("Save the document (⌘S)"),
                        button().disabled(true).child(text("Disabled")),
                    ],
                ));

            let choosing = section(
                "Tabs and select",
                "Tabs: arrows choose, Delete closes, ⌘⇧ and an arrow (or a drag) reorders. Select: typing chooses while closed.",
                vec![
                    column().w(520.).h(160.).child(tabs(
                        "Documents",
                        documents,
                        document,
                        TabsOptions {
                            reorderable: true,
                            ..Default::default()
                        },
                        |id, _| {
                            column()
                                .p(12.)
                                .child(text(format!("Editing {id}")).text_color(tok::TEXT_SECONDARY))
                        },
                    )),
                    select(
                        "Fruit",
                        fruit,
                        vec![
                            SelectOption::new("apple", "Apple"),
                            SelectOption::new("banana", "Banana"),
                            SelectOption::new("blueberry", "Blueberry").disabled(),
                            SelectOption::new("cherry", "Cherry"),
                        ],
                        "Choose a fruit",
                    ),
                ],
            );
            let item = |i: usize, label: &str| {
                let (checked, set) = (size.clone(), size.clone());
                menu_item(label)
                    .menu_radio()
                    .menu_checked_when(move || checked.get() == i)
                    .on_click(move || {
                        set.set(i);
                    })
            };
            let wrapped = wrap.clone();
            let overlays = section(
                "Menus and context menus",
                "Arrows, typeahead and Escape; Shift+F10 or the Menu key opens a context menu.",
                vec![
                    menu("View", menu_open, button().child(text("View ▾")))
                        .match_anchor_width()
                        .child(
                            menu_item("Word Wrap")
                                .menu_checked_when(move || wrapped.get())
                                .shortcut("⌥Z")
                                .keep_open()
                                .on_click(move || {
                                    wrap.set(!wrap.get());
                                }),
                        )
                        .child(separator(false))
                        .children([item(0, "Small"), item(1, "Medium"), item(2, "Large")]),
                    div()
                        .focusable(true)
                        .p(8.)
                        .rounded(6.)
                        .border(1.)
                        .border_color(tok::BORDER)
                        .tooltip("Right-click, Shift+F10 or the Menu key")
                        .context_menu(|_| {
                            vec![
                                menu_item("Rename").shortcut("↩"),
                                menu_item("Duplicate").shortcut("⌘D"),
                            ]
                        })
                        .child(text("Report.pdf")),
                ],
            );

            let sections: Vec<(String, Rc<dyn Fn() -> View>)> = vec![
                ("Shipping".into(), Rc::new(|| text("Ships in two days."))),
                ("Payment".into(), Rc::new(|| text("Cards and transfers."))),
                ("Returns".into(), Rc::new(|| text("Thirty days."))),
            ];
            let containers = section(
                "Disclosure and accordion",
                "Enter or Space toggles a section; an accordion keeps one open.",
                vec![
                    column()
                        .w(360.)
                        .gap(4.)
                        .child(disclosure(details, text("Advanced settings"), || {
                            text("Hidden until opened.")
                        }))
                        .child(separator(false))
                        .child(accordion(faq, sections)),
                ],
            );

            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .bg(tok::SURFACE_SUNKEN)
                    .child(
                        row()
                            .p(16.)
                            .child(text("Components").text_size(tok::FONT_SIZE_XL).font_weight(700)),
                    )
                    .child(
                        scroll(cx.ui.signal(0.)).grow().child(
                            column()
                                .gap(16.)
                                .px(16.)
                                .pb(24.)
                                .child(controls)
                                .child(choosing)
                                .child(overlays)
                                .child(containers),
                        ),
                    ),
            );
        })
}
