//! The component catalogue in Rust: the controls, overlays and containers
//! built from `zgui::compose`, each with what it does from the keyboard.
//! The Luau catalogue (`crates/zgui-script/examples/catalog`) shows the
//! same through `ui.*`.
//! Run: cargo run -p zgui-desktop --example catalog
use std::rc::Rc;
use zgui::compose::{
    ComboItem, ComboboxOptions, PaletteCommand, SelectOption, StepperOptions, Tab, TabsOptions,
    Toast, ToastKind, Toaster, TreeChildren, TreeItem, prelude::*,
};
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
            let query = ui.signal(String::new());
            let city = ui.signal(None::<String>);
            let cities = ui.signal(Vec::<ComboItem>::new());
            let palette = ui.signal(false);
            let ran = ui.signal(String::from("Nothing run yet"));
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
            let (shown_ran, opener) = (ran.clone(), palette.clone());
            let searching = section(
                "Combobox and command palette",
                "The combobox suggests as typed (focus stays in the field); the palette filters commands by a fuzzy match.",
                vec![
                    component(move |cx| {
                        // The app filters; an async search would set the list later.
                        const CITIES: &[&str] = &[
                            "Amsterdam", "Athens", "Berlin", "Lisbon", "London", "Paris", "Prague", "Rome",
                        ];
                        let (q, list) = (query.clone(), cities.clone());
                        cx.retain(cx.runtime().effect(move || {
                            let q = q.get().to_lowercase();
                            list.set(
                                CITIES
                                    .iter()
                                    .filter(|c| c.to_lowercase().contains(&q))
                                    .map(|c| ComboItem::new(c.to_lowercase(), *c).detail("Europe"))
                                    .collect(),
                            );
                        }));
                        column().w(260.).child(combobox(
                            "City",
                            query.clone(),
                            city.clone(),
                            cities.clone(),
                            ComboboxOptions::default(),
                        ))
                    }),
                    button()
                        .on_click(move || {
                            opener.set(true);
                        })
                        .child(text("Command palette…")),
                    text_signal(move || shown_ran.get()).text_color(tok::TEXT_SECONDARY),
                    command_palette(
                        palette,
                        || {
                            vec![
                                PaletteCommand::new("open", "Open File…").detail("File").shortcut("⌘O"),
                                PaletteCommand::new("save", "Save").detail("File").shortcut("⌘S"),
                                PaletteCommand::new("theme", "Toggle Theme").detail("View"),
                                PaletteCommand::new("zoom", "Zoom In").detail("View").shortcut("⌘="),
                            ]
                        },
                        move |id| {
                            ran.set(format!("Ran {id}"));
                        },
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
                    button()
                        .on_click({
                            let toaster = Toaster::of(&cx.ui.runtime);
                            move || {
                                toaster.show(
                                    Toast::new("Saved")
                                        .body("Report.pdf")
                                        .kind(ToastKind::Success)
                                        .action("Undo", || {}),
                                );
                            }
                        })
                        .child(text("Toast")),
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

            let open_files = cx.ui.signal(vec!["src".to_owned()]);
            let current_file = cx.ui.signal(None::<String>);
            let tree_section = section(
                "Tree",
                "Up and Down move; Right opens then enters; Left closes then goes up; typing finds; Enter activates.",
                vec![column().w(320.).h(180.).child(tree(
                    "Files",
                    |parent| {
                        TreeChildren::Ready(match parent {
                            None => vec![
                                TreeItem::parent("src", "src"),
                                TreeItem::parent("docs", "docs"),
                                TreeItem::leaf("readme", "README.md"),
                            ],
                            Some("src") => vec![
                                TreeItem::leaf("main", "main.rs"),
                                TreeItem::parent("compose", "compose"),
                            ],
                            Some("compose") => (0..500)
                                .map(|i| TreeItem::leaf(format!("c{i}"), format!("view_{i}.rs")))
                                .collect(),
                            Some("docs") => vec![TreeItem::leaf("guide", "guide.md")],
                            Some(_) => Vec::new(),
                        })
                    },
                    open_files,
                    current_file,
                    |_| {},
                ))],
            );
            let grid_section = section(
                "Data grid",
                "Arrows move; Enter, F2 or typing edits, Escape drops; Up from the first row reaches the header, where Enter sorts, ⌘⇧ arrows move a column and Alt arrows resize it. Drag a header to move its column, its edge to resize it.",
                vec![component(|cx| {
                    let runtime = cx.runtime();
                    let products = runtime.signal(
                        (0..10_000)
                            .map(|i| {
                                (
                                    format!("SKU-{i:05}"),
                                    format!("Product {i}"),
                                    ((i * 37) % 1000) as f64 / 10.,
                                    (i * 13) % 250,
                                )
                            })
                            .collect::<Vec<_>>(),
                    );
                    let columns = runtime.signal(vec![
                        GridColumn::new("sku", "SKU", 110.),
                        GridColumn::new("name", "Name", 200.).sortable().editable(),
                        GridColumn::new("price", "Price", 90.).sortable().align_end(),
                        GridColumn::new("stock", "Stock", 80.).sortable().align_end(),
                    ]);
                    let sort = runtime.signal(None::<GridSort>);
                    // The app sorts: the rows' order as the header asks.
                    let order = runtime.signal(Rc::new(Vec::<usize>::new()));
                    {
                        let (products, sort, order) = (products.clone(), sort.clone(), order.clone());
                        cx.retain(runtime.effect(move || {
                            let by = sort.get();
                            let next = products.with(|p| {
                                let mut index: Vec<usize> = (0..p.len()).collect();
                                if let Some(by) = &by {
                                    index.sort_by(|&a, &b| {
                                        let o = match by.column.as_str() {
                                            "price" => p[a].2.total_cmp(&p[b].2),
                                            "stock" => p[a].3.cmp(&p[b].3),
                                            _ => p[a].1.cmp(&p[b].1),
                                        };
                                        if by.descending { o.reverse() } else { o }
                                    });
                                }
                                index
                            });
                            order.set(Rc::new(next));
                        }));
                    }
                    let row_of = |order: &zgui::reactive::Signal<Rc<Vec<usize>>>, r: usize| {
                        order.with(|o| o.get(r).copied().unwrap_or(r))
                    };
                    let (count, shown, shown_order) = (products.clone(), products.clone(), order.clone());
                    let (edited, edit_order) = (products.clone(), order.clone());
                    let toaster = Toaster::of(&runtime);
                    let source = GridSource::new(
                        move || count.with(Vec::len),
                        move |r, column| {
                            let i = row_of(&shown_order, r);
                            shown.with(|p| match column {
                                "sku" => p[i].0.clone(),
                                "name" => p[i].1.clone(),
                                "price" => format!("{:.2}", p[i].2),
                                _ => p[i].3.to_string(),
                            })
                        },
                    )
                    .on_edit(move |r, _, text| {
                        let i = edit_order.with_untracked(|o| o.get(r).copied().unwrap_or(r));
                        edited.update(|p| p[i].1 = text);
                    })
                    .on_activate(move |r| {
                        toaster.show(Toast::new(format!("Opened row {}", r + 1)));
                    });
                    column().w(560.).h(260.).child(data_grid(
                        "Products",
                        columns,
                        sort,
                        runtime.signal(None),
                        source,
                    ))
                })],
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
                                .child(searching)
                                .child(tree_section)
                                .child(grid_section)
                                .child(overlays)
                                .child(containers),
                        ),
                    )
                    .child(toast_host()),
            );
        })
}
