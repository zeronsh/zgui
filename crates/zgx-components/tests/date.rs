//! Dates and times: the calendar arithmetic, a calendar's keyboard (days,
//! weeks, months, years, limits, right to left) and what assistive
//! technology hears, a date picker opening and closing as a day is chosen,
//! and a time field's segments stepping, typed in and cleared.
use zgx::{
    compose::{ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::{Direction, NodeId},
    semantics::Role,
    widgets::Ui,
};
use zgx_components::*;

fn date(year: i32, month: u8, day: u8) -> CalendarDate {
    CalendarDate::new(year, month, day).unwrap()
}

#[test]
fn the_calendar_counts_days_weeks_and_months() {
    assert_eq!(CalendarDate::from_days(0), date(1970, 1, 1));
    assert_eq!(date(1970, 1, 1).weekday(), 4, "a Thursday");
    assert_eq!(date(2026, 10, 23).weekday(), 5, "a Friday");
    assert_eq!(
        date(2000, 3, 1).days() - date(2000, 2, 28).days(),
        2,
        "2000 leaps"
    );
    assert_eq!(
        date(1900, 3, 1).days() - date(1900, 2, 28).days(),
        1,
        "1900 doesn't"
    );
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(CalendarDate::new(2026, 2, 29), None);
    assert_eq!(CalendarDate::new(2026, 13, 1), None);
    // Round trips, either side of the epoch.
    for days in [-800_000, -1, 0, 59, 365, 11_016, 20_384, 2_000_000] {
        assert_eq!(CalendarDate::from_days(days).days(), days);
    }
    assert_eq!(date(1969, 12, 31).days(), -1);
    // A month on keeps the day where the month has it.
    assert_eq!(date(2026, 1, 31).add_months(1), date(2026, 2, 28));
    assert_eq!(date(2026, 1, 15).add_months(-1), date(2025, 12, 15));
    assert_eq!(date(2026, 3, 1).add_months(-14), date(2025, 1, 1));
    assert_eq!(date(2026, 12, 31).add_days(1), date(2027, 1, 1));
    assert_eq!(CalendarDate::parse("2026-10-23"), Some(date(2026, 10, 23)));
    assert_eq!(CalendarDate::parse("2026-02-30"), None);
    assert_eq!(date(2026, 1, 5).to_string(), "2026-01-05");
    assert_eq!(TimeOfDay::parse("09:05"), TimeOfDay::new(9, 5));
    assert_eq!(TimeOfDay::new(9, 5).unwrap().to_string(), "09:05");
    assert_eq!(TimeOfDay::new(24, 0), None);
}

struct Fixture {
    ui: Ui,
    view: ViewHandle,
    value: Signal<Option<CalendarDate>>,
}
fn options() -> CalendarOptions {
    CalendarOptions {
        today: Some(date(2026, 10, 23)),
        min: Some(date(2026, 1, 1)),
        max: Some(date(2027, 6, 30)),
        disabled: Some(std::rc::Rc::new(|d: CalendarDate| d.weekday() == 0)),
        ..Default::default()
    }
}
fn with_calendar(build: impl FnOnce(Signal<Option<CalendarDate>>) -> View) -> Fixture {
    let mut ui = Ui::new(400., 420.);
    let value = ui.signal(None::<CalendarDate>);
    let view = ui.mount(
        column()
            .id("root")
            .w(400.)
            .h(420.)
            .child(build(value.clone())),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    Fixture { ui, view, value }
}
fn key_with(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
    ui.prepare_frame();
}
fn key(ui: &mut Ui, key: Key) {
    key_with(ui, key, Modifiers::default());
}
fn with_role(ui: &Ui, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(n) = pending.pop() {
        if semantics.get(n).is_some_and(|s| s.role == role) {
            out.push(n);
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    out
}
/// The calendar's current day, as assistive technology hears it.
fn active(ui: &Ui) -> String {
    let grid = with_role(ui, Role::Grid)[0];
    let semantics = ui.semantics.borrow();
    let day = semantics.get(grid).unwrap().active_descendant.unwrap();
    semantics.get(day).unwrap().label.clone()
}
fn focus_grid(f: &mut Fixture) {
    let grid = with_role(&f.ui, Role::Grid)[0];
    f.ui.input.focus(&f.ui.scene, Some(grid));
    f.ui.prepare_frame();
}

#[test]
fn a_calendars_keys_move_by_day_week_month_and_year_within_its_limits() {
    let mut f = with_calendar(|value| calendar("Due", value, options()));
    focus_grid(&mut f);
    assert_eq!(active(&f.ui), "Friday, 23 October 2026", "at today");
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(active(&f.ui), "Saturday, 24 October 2026");
    key(&mut f.ui, Key::ArrowDown);
    assert_eq!(active(&f.ui), "Saturday, 31 October 2026");
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(
        active(&f.ui),
        "Sunday, 1 November 2026",
        "into the next month"
    );
    let caption = |ui: &Ui| {
        let scene = ui.scene.borrow();
        let semantics = ui.semantics.borrow();
        let mut pending = vec![scene.root()];
        while let Some(n) = pending.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n)
                && text.contains(" 20")
                && semantics.get(n).is_none_or(|s| !s.hidden)
            {
                return text.to_string();
            }
            pending.extend(scene.children(n).iter().rev().copied());
        }
        String::new()
    };
    assert_eq!(caption(&f.ui), "November 2026");
    // A Sunday: not to be chosen.
    key(&mut f.ui, Key::Enter);
    assert_eq!(f.value.get(), None);
    key(&mut f.ui, Key::End);
    assert_eq!(active(&f.ui), "Saturday, 7 November 2026", "the week's end");
    key(&mut f.ui, Key::Home);
    assert_eq!(active(&f.ui), "Sunday, 1 November 2026");
    key(&mut f.ui, Key::PageDown);
    assert_eq!(active(&f.ui), "Tuesday, 1 December 2026");
    key_with(
        &mut f.ui,
        Key::PageDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(
        active(&f.ui),
        "Wednesday, 30 June 2027",
        "a year on: the last day allowed"
    );
    key_with(
        &mut f.ui,
        Key::PageUp,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    key_with(
        &mut f.ui,
        Key::PageUp,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(
        active(&f.ui),
        "Thursday, 1 January 2026",
        "two back: the first allowed"
    );
    key(&mut f.ui, Key::Character(" ".into()));
    assert_eq!(f.value.get(), Some(date(2026, 1, 1)));
    // The chosen day says so.
    let grid = with_role(&f.ui, Role::Grid)[0];
    let semantics = f.ui.semantics.borrow();
    let day = semantics.get(grid).unwrap().active_descendant.unwrap();
    assert_eq!(semantics.get(day).unwrap().selected, Some(true));
}

#[test]
fn right_to_left_a_calendars_arrows_mirror() {
    let mut f = with_calendar(|value| calendar("Due", value, options()));
    let root = f.view.find("root").unwrap();
    f.ui.scene
        .borrow_mut()
        .set_direction(root, Some(Direction::Rtl));
    f.ui.prepare_frame();
    focus_grid(&mut f);
    key(&mut f.ui, Key::ArrowLeft);
    assert_eq!(active(&f.ui), "Saturday, 24 October 2026", "Left goes on");
}

#[test]
fn a_click_chooses_a_day_and_the_arrows_change_the_month() {
    let mut f = with_calendar(|value| calendar("Due", value, options()));
    let button = PointerButton::Primary;
    let click = |ui: &mut Ui, node: NodeId| {
        let b = ui.scene.borrow().bounds(node);
        let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
        ui.dispatch(InputEvent::PointerDown { x, y, button });
        ui.dispatch(InputEvent::PointerUp { x, y, button });
        ui.prepare_frame();
    };
    let day = |ui: &Ui, label: &str| {
        with_role(ui, Role::Cell)
            .into_iter()
            .find(|n| ui.semantics.borrow().get(*n).unwrap().label == label)
            .unwrap()
    };
    let twelfth = day(&f.ui, "Monday, 12 October 2026");
    click(&mut f.ui, twelfth);
    assert_eq!(f.value.get(), Some(date(2026, 10, 12)));
    let next = with_role(&f.ui, Role::Button)
        .into_iter()
        .find(|n| f.ui.semantics.borrow().get(*n).unwrap().label == "Next month")
        .unwrap();
    click(&mut f.ui, next);
    let fifth = day(&f.ui, "Thursday, 5 November 2026");
    click(&mut f.ui, fifth);
    assert_eq!(f.value.get(), Some(date(2026, 11, 5)));
}

#[test]
fn a_date_picker_opens_at_its_day_and_closes_as_one_is_chosen() {
    let mut f = with_calendar(|value| date_picker("Due date", value, options()));
    f.value.set(Some(date(2026, 3, 10)));
    f.ui.prepare_frame();
    let trigger = with_role(&f.ui, Role::Button)[0];
    assert_eq!(
        f.ui.semantics.borrow().get(trigger).unwrap().label,
        "Due date: 2026-03-10"
    );
    f.ui.input.focus(&f.ui.scene, Some(trigger));
    f.ui.prepare_frame();
    key(&mut f.ui, Key::ArrowDown);
    f.ui.prepare_frame();
    let grid = with_role(&f.ui, Role::Grid)[0];
    assert_eq!(
        f.ui.input.focused(),
        Some(grid),
        "opened, focus on its days"
    );
    assert_eq!(active(&f.ui), "Tuesday, 10 March 2026");
    key(&mut f.ui, Key::ArrowRight);
    key(&mut f.ui, Key::Enter);
    f.ui.prepare_frame();
    assert_eq!(f.value.get(), Some(date(2026, 3, 11)));
    let expanded = |ui: &Ui| ui.semantics.borrow().get(trigger).unwrap().expanded;
    assert_eq!(expanded(&f.ui), Some(false), "closed");
    assert_eq!(
        f.ui.input.focused(),
        Some(trigger),
        "focus back on the button"
    );
    // Escape closes it, choosing nothing.
    key(&mut f.ui, Key::ArrowDown);
    key(&mut f.ui, Key::ArrowRight);
    key(&mut f.ui, Key::Escape);
    f.ui.prepare_frame();
    assert_eq!(f.value.get(), Some(date(2026, 3, 11)));
    assert_eq!(expanded(&f.ui), Some(false));
}

#[test]
fn a_time_fields_segments_step_take_digits_and_clear() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal(TimeOfDay::new(9, 30));
    ui.mount(time_field(
        "Start",
        value.clone(),
        TimeOptions {
            minute_step: 15,
            hour12: true,
        },
    ));
    ui.prepare_frame();
    let segments = with_role(&ui, Role::SpinButton);
    assert_eq!(segments.len(), 3, "hours, minutes, AM/PM");
    let said = |ui: &Ui, n: NodeId| ui.semantics.borrow().get(n).unwrap().value.clone();
    assert_eq!(said(&ui, segments[0]).as_deref(), Some("09"));
    assert_eq!(said(&ui, segments[2]).as_deref(), Some("AM"));
    ui.input.focus(&ui.scene, Some(segments[1]));
    ui.prepare_frame();
    key(&mut ui, Key::ArrowUp);
    assert_eq!(value.get(), TimeOfDay::new(9, 45), "a step of 15");
    key(&mut ui, Key::ArrowUp);
    assert_eq!(value.get(), TimeOfDay::new(9, 0), "wrapping");
    key(&mut ui, Key::ArrowRight);
    assert_eq!(ui.input.focused(), Some(segments[2]));
    key(&mut ui, Key::Character("p".into()));
    assert_eq!(value.get(), TimeOfDay::new(21, 0));
    // Hours typed: "1" waits for a second digit, "1" then makes 11 and
    // moves on.
    ui.input.focus(&ui.scene, Some(segments[0]));
    ui.prepare_frame();
    key(&mut ui, Key::Character("1".into()));
    assert_eq!(value.get(), TimeOfDay::new(13, 0), "1 PM so far");
    assert_eq!(ui.input.focused(), Some(segments[0]));
    key(&mut ui, Key::Character("1".into()));
    assert_eq!(value.get(), TimeOfDay::new(23, 0));
    assert_eq!(ui.input.focused(), Some(segments[1]), "on to the minutes");
    key(&mut ui, Key::Character("7".into()));
    assert_eq!(
        value.get(),
        TimeOfDay::new(23, 7),
        "7: no second digit can follow"
    );
    assert_eq!(ui.input.focused(), Some(segments[2]), "so on to AM/PM");
    key(&mut ui, Key::ArrowLeft);
    key(&mut ui, Key::Backspace);
    assert_eq!(value.get(), None, "a blank segment: no time");
    assert_eq!(said(&ui, segments[1]).as_deref(), Some("--"));
    // Set from elsewhere, the segments follow.
    value.set(TimeOfDay::new(0, 5));
    ui.prepare_frame();
    assert_eq!(said(&ui, segments[0]).as_deref(), Some("12"));
    assert_eq!(said(&ui, segments[2]).as_deref(), Some("AM"));
}
