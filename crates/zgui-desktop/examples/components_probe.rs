//! The catalogue's components on one screen, for the Linux accessibility
//! probe (`scripts/components_atspi_smoke.py`): a segmented control, a
//! stepper, a select, a date picker, a time field, tabs, a tree left to
//! right and one right to left, and a data grid. Each change prints the
//! state it reaches (`COMPONENTS …`), which the probe reads beside the
//! accessibility tree.
//! Run: cargo run -p zgui-desktop --example components_probe
use zgui::compose::prelude::*;
use zgui::compose::{SelectOption, StepperOptions, Tab, TabsOptions, TreeChildren, TreeItem};
use zgui_desktop::{Application, WindowOptions};

fn files(parent: Option<&str>) -> TreeChildren {
    TreeChildren::Ready(match parent {
        None => vec![TreeItem::parent("a", "Alpha"), TreeItem::leaf("b", "Beta")],
        Some("a") => vec![TreeItem::leaf("a1", "Apple")],
        Some(_) => Vec::new(),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgui components probe".into(),
            width: 900.,
            height: 700.,
            ..Default::default()
        })
        .run(|cx| {
            let ui = &mut cx.ui;
            let size = ui.signal(0_usize);
            let quantity = ui.signal(1_f64);
            let fruit = ui.signal("apple".to_owned());
            let due = ui.signal(CalendarDate::new(2026, 10, 23));
            let start = ui.signal(TimeOfDay::new(8, 30));
            let tab = ui.signal("one".to_owned());
            let tabs_list = ui.signal(vec![
                Tab::new("one", "One"),
                Tab::new("two", "Two"),
                Tab::new("three", "Three"),
            ]);
            let open = ui.signal(Vec::<String>::new());
            let open_rtl = ui.signal(Vec::<String>::new());
            let chosen = ui.signal(None::<String>);
            let chosen_rtl = ui.signal(None::<String>);
            let columns = ui.signal(vec![
                GridColumn::new("name", "Name", 140.).sortable(),
                GridColumn::new("qty", "Quantity", 100.).sortable().align_end(),
            ]);
            let sort = ui.signal(None::<GridSort>);
            let current = ui.signal(None::<GridCell>);
            let selection = ui.signal(GridSelection::default());
            let source = GridSource::new(
                || 20,
                |r, c| match c {
                    "name" => format!("Item {r}"),
                    _ => ((r * 7) % 20).to_string(),
                },
            )
            .selection(selection.clone());
            let calendar_options = CalendarOptions {
                today: CalendarDate::new(2026, 10, 23),
                ..Default::default()
            };
            let printed = (
                size.clone(),
                quantity.clone(),
                fruit.clone(),
                due.clone(),
                start.clone(),
                tab.clone(),
                open.clone(),
                open_rtl.clone(),
                selection.clone(),
                sort.clone(),
            );
            let view = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(16.)
                    .gap(12.)
                    .child(
                        row()
                            .gap(12.)
                            .items_center()
                            .child(segmented(
                                "Probe size",
                                size,
                                ["Small", "Medium", "Large"],
                            ))
                            .child(stepper(
                                "Probe quantity",
                                quantity,
                                StepperOptions {
                                    min: 0.,
                                    max: 10.,
                                    step: 1.,
                                    decimals: 0,
                                },
                            ))
                            .child(select(
                                "Probe fruit",
                                fruit,
                                vec![
                                    SelectOption::new("apple", "Apple"),
                                    SelectOption::new("pear", "Pear"),
                                ],
                                "Fruit",
                            ))
                            .child(date_picker("Probe due", due, calendar_options))
                            .child(time_field("Probe start", start, TimeOptions::default())),
                    )
                    .child(
                        column().h(110.).shrink_0().child(tabs(
                            "Probe tabs",
                            tabs_list,
                            tab,
                            TabsOptions::default(),
                            |id, _| text(format!("Panel {id}")),
                        )),
                    )
                    .child(
                        row()
                            .gap(12.)
                            .h(220.)
                            .shrink_0()
                            .child(
                                column()
                                    .w(180.)
                                    .h_full()
                                    .child(tree("Probe files", files, open, chosen, |_| {})),
                            )
                            .child(column().w(180.).h_full().rtl().child(tree(
                                "Probe files right to left",
                                files,
                                open_rtl,
                                chosen_rtl,
                                |_| {},
                            )))
                            .child(column().w(260.).h_full().child(data_grid(
                                "Probe grid",
                                columns,
                                sort,
                                current,
                                source,
                            ))),
                    ),
            );
            let (size, quantity, fruit, due, start, tab, open, open_rtl, selection, sort) = printed;
            cx.ui.bind(view.node(), move || {
                println!(
                    "COMPONENTS size={} quantity={} fruit={} due={} start={} tab={} open={:?} open_rtl={:?} selection={:?} sort={:?}",
                    size.get(),
                    quantity.get(),
                    fruit.get(),
                    due.get().map(|d| d.to_string()).unwrap_or_default(),
                    start.get().map(|t| t.to_string()).unwrap_or_default(),
                    tab.get(),
                    open.get(),
                    open_rtl.get(),
                    selection.get().rows().collect::<Vec<_>>(),
                    sort.get().map(|s| format!("{}{}", s.column, if s.descending { "-" } else { "+" })),
                );
            });
        })
}
