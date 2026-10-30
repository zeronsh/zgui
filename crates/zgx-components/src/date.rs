//! Dates and times: a calendar, a date picker (a button opening one, as
//! shadcn's and the system's), and a time field of hour and minute
//! segments, as WAI-ARIA's date picker dialog and spin buttons. The
//! calendar is one Tab stop keeping focus, its current day the active
//! descendant: Left and Right move a day (mirrored right to left), Up and
//! Down a week, Home and End to the week's ends, Page Up and Down a month
//! (with Shift a year); Enter or Space chooses. A time field's segments
//! are each a spin button: Up and Down step, digits type, Left and Right
//! move between them, Backspace clears one.
//!
//! The dates are the proleptic Gregorian calendar's, with no time zone;
//! "today" is the local one where the host says what the local offset is
//! ([`zgx::local_time::set_offset`]: zgx-desktop does), else UTC's.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::{
        Context, NodeHandle, View, button, column, component, div, popover, row, text, text_signal,
    },
    input::{EventContext, EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Color, Direction, NodeId},
    semantics::{Live, Role},
    style::{Styled, Styles},
    text_layout::{FontFeatures, TextAlign},
    theme::tok,
};

/// A day of the calendar: a year, a month (1 to 12) and a day of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    /// The year (negative before 1 BC's 0).
    pub year: i32,
    /// 1 for January.
    pub month: u8,
    /// From 1.
    pub day: u8,
}

/// Whether `year` has a 29 February.
pub fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
/// How many days `month` (1 to 12) of `year` has.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl CalendarDate {
    /// The day, if there is one: `new(2026, 2, 30)` is none.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month))
            .then_some(Self { year, month, day })
    }
    /// The day `days` after 1 January 1970 (before it, negative).
    pub fn from_days(days: i64) -> Self {
        // Howard Hinnant's `civil_from_days`.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }
    /// Days since 1 January 1970.
    pub fn days(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let yoe = year.rem_euclid(400);
        let mp = (i64::from(self.month) + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }
    /// Its day of the week: 0 for Sunday to 6 for Saturday.
    pub fn weekday(self) -> u8 {
        (self.days() + 4).rem_euclid(7) as u8
    }
    /// The day `n` days on (back, negative).
    pub fn add_days(self, n: i64) -> Self {
        Self::from_days(self.days() + n)
    }
    /// The same day `n` months on, or the month's last if it has fewer.
    pub fn add_months(self, n: i32) -> Self {
        let months = self.year * 12 + i32::from(self.month) - 1 + n;
        let (year, month) = (months.div_euclid(12), (months.rem_euclid(12) + 1) as u8);
        Self {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }
    /// Its month's first day.
    pub fn first_of_month(self) -> Self {
        Self { day: 1, ..self }
    }
    /// Today, here (see [`zgx::local_time`]).
    pub fn today() -> Self {
        Self::from_days(zgx::local_time::now_seconds().div_euclid(86_400))
    }
    /// A date written `2026-10-23` (ISO 8601's).
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().splitn(3, '-');
        let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
        Self::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
    }
    /// It clamped to `min` and `max`.
    pub fn clamp_to(self, min: Option<Self>, max: Option<Self>) -> Self {
        let low = min.map_or(self, |m| self.max(m));
        max.map_or(low, |m| low.min(m))
    }
}
impl std::fmt::Display for CalendarDate {
    /// ISO 8601's `2026-10-23`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// A time of day: an hour (0 to 23) and a minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay {
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
}
impl TimeOfDay {
    /// The time, if there is one.
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then_some(Self { hour, minute })
    }
    /// Now, here (see [`zgx::local_time`]).
    pub fn now() -> Self {
        let seconds = zgx::local_time::now_seconds().rem_euclid(86_400);
        Self {
            hour: (seconds / 3600) as u8,
            minute: (seconds / 60 % 60) as u8,
        }
    }
    /// A time written `09:30` (24-hour).
    pub fn parse(text: &str) -> Option<Self> {
        let (hour, minute) = text.trim().split_once(':')?;
        Self::new(hour.parse().ok()?, minute.parse().ok()?)
    }
}
impl std::fmt::Display for TimeOfDay {
    /// `09:30`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// How a [`calendar`] or [`date_picker`] shows and limits its days.
#[derive(Clone)]
pub struct CalendarOptions {
    /// The first day that can be chosen.
    pub min: Option<CalendarDate>,
    /// The last.
    pub max: Option<CalendarDate>,
    /// The week's first day: 0 for Sunday, 1 for Monday.
    pub first_weekday: u8,
    /// Days that can't be chosen (weekends, booked ones).
    pub disabled: Option<Rc<dyn Fn(CalendarDate) -> bool>>,
    /// The months' names, January first (English unless set).
    pub month_names: Rc<[String; 12]>,
    /// The weekdays' names, Sunday first; the calendar's header shows
    /// their first two letters.
    pub weekday_names: Rc<[String; 7]>,
    /// How a date picker's button writes its date (ISO 8601's unless set).
    pub format: Option<Rc<dyn Fn(CalendarDate) -> String>>,
    /// The day marked today (the local one unless set).
    pub today: Option<CalendarDate>,
}
impl Default for CalendarOptions {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            first_weekday: 0,
            disabled: None,
            month_names: Rc::new(MONTHS.map(String::from)),
            weekday_names: Rc::new(WEEKDAYS.map(String::from)),
            format: None,
            today: None,
        }
    }
}
impl CalendarOptions {
    fn allows(&self, date: CalendarDate) -> bool {
        self.min.is_none_or(|m| date >= m)
            && self.max.is_none_or(|m| date <= m)
            && !self.disabled.as_ref().is_some_and(|d| d(date))
    }
    fn caption(&self, date: CalendarDate) -> String {
        format!(
            "{} {}",
            self.month_names[usize::from(date.month) - 1],
            date.year
        )
    }
    /// "Friday, 23 October 2026".
    fn spoken(&self, date: CalendarDate) -> String {
        format!(
            "{}, {} {} {}",
            self.weekday_names[usize::from(date.weekday())],
            date.day,
            self.month_names[usize::from(date.month) - 1],
            date.year
        )
    }
    /// The first day the month's grid shows: its week's first.
    fn first_shown(&self, month: CalendarDate) -> CalendarDate {
        let first = month.first_of_month();
        let back = (i64::from(first.weekday()) - i64::from(self.first_weekday)).rem_euclid(7);
        first.add_days(-back)
    }
}

/// A day's cell size, and the weeks a month's grid shows.
const DAY: f32 = 34.;
const WEEKS: usize = 6;

/// What a calendar's parts share.
struct Month {
    value: Signal<Option<CalendarDate>>,
    /// The current day, which the grid shows the month of.
    cursor: Signal<CalendarDate>,
    focused: Signal<bool>,
    rtl: Signal<bool>,
    options: CalendarOptions,
    today: CalendarDate,
    grid: RefCell<Option<NodeHandle>>,
    /// The day cells, by their place in the grid.
    cells: RefCell<[Option<NodeId>; WEEKS * 7]>,
    choose: Rc<dyn Fn(CalendarDate)>,
}
impl Month {
    fn date_at(&self, index: usize) -> CalendarDate {
        self.options
            .first_shown(self.cursor.get())
            .add_days(index as i64)
    }
    fn go(&self, to: CalendarDate) {
        self.cursor
            .set(to.clamp_to(self.options.min, self.options.max));
    }
    fn describe_active(&self) {
        let Some(handle) = self.grid.borrow().clone() else {
            return;
        };
        let cursor = self.cursor.with_untracked(|c| *c);
        let index = (cursor.days() - self.options.first_shown(cursor).days()) as usize;
        let target = self.cells.borrow().get(index).copied().flatten();
        handle.update_semantics(|node| node.active_descendant = target);
    }
    fn read_direction(&self) {
        let rtl = self.grid.borrow().as_ref().is_some_and(|h| {
            h.scene()
                .is_some_and(|s| s.borrow().direction(h.node()) == Direction::Rtl)
        });
        if self.rtl.with_untracked(|r| *r != rtl) {
            self.rtl.set(rtl);
        }
    }
}

/// A month of days to choose `value` from (two-way), the month changing
/// with its arrows or the keyboard; `label` names it.
pub fn calendar(
    label: impl Into<String>,
    value: Signal<Option<CalendarDate>>,
    options: CalendarOptions,
) -> View {
    let label = label.into();
    component(move |cx| {
        // A label that changes (a translation, a state), where the caller
        // gave one (`FollowedLabel::around`).
        let followed = zgx::compose::FollowedLabel::take(cx);
        let today = options.today.unwrap_or_else(CalendarDate::today);
        let start = value
            .with_untracked(|v| v.unwrap_or(today))
            .clamp_to(options.min, options.max);
        let cursor = cx.state(start);
        // The value set from elsewhere: its month shows.
        {
            let (value, cursor) = (value.clone(), cursor.clone());
            cx.retain(cx.runtime().effect(move || {
                if let Some(v) = value.get() {
                    cursor.set(v);
                }
            }));
        }
        let chosen = value.clone();
        month_view(
            cx,
            label,
            followed,
            value,
            cursor,
            options,
            today,
            Rc::new(move |d: CalendarDate| {
                chosen.set(Some(d));
            }),
        )
    })
}

/// The calendar's caption, arrows and grid.
#[allow(clippy::too_many_arguments)]
fn month_view(
    cx: &mut Context,
    label: String,
    followed: Option<zgx::compose::Followed>,
    value: Signal<Option<CalendarDate>>,
    cursor: Signal<CalendarDate>,
    options: CalendarOptions,
    today: CalendarDate,
    choose: Rc<dyn Fn(CalendarDate)>,
) -> View {
    let runtime = cx.runtime();
    let month = Rc::new(Month {
        value,
        cursor,
        focused: runtime.signal(false),
        rtl: runtime.signal(false),
        options,
        today,
        grid: RefCell::default(),
        cells: RefCell::new([None; WEEKS * 7]),
        choose,
    });
    {
        let month = month.clone();
        cx.retain(runtime.effect(move || {
            month.cursor.with(|_| ());
            month.describe_active();
        }));
    }
    // A month back or on; the glyphs point the way it goes, right to left
    // too. Not Tab stops: Page Up and Down do the same from the grid.
    let arrow = |on: bool| {
        let (go, glyph) = (month.clone(), month.clone());
        button()
            .accessible_label(if on { "Next month" } else { "Previous month" })
            .focusable(false)
            .w(28.)
            .h(28.)
            .p(0.)
            .rounded(6.)
            .border(0.)
            .bg(Color(0, 0, 0, 0))
            .hover(|s| s.bg(tok::HOVER))
            .items_center()
            .justify_center()
            .on_click(move || {
                let now = go.cursor.with_untracked(|c| *c);
                go.go(now.add_months(if on { 1 } else { -1 }));
            })
            .child(text_signal(move || {
                (if on != glyph.rtl.get() { "›" } else { "‹" }).into()
            }))
    };
    let caption = {
        let month = month.clone();
        text_signal(move || month.options.caption(month.cursor.get()))
            .grow()
            .font_weight(600)
            .text_align(TextAlign::Center)
            .live(Live::Polite)
    };
    let header = {
        let names = month.options.weekday_names.clone();
        let first = month.options.first_weekday;
        row().role(Role::Row).children((0..7).map(|i| {
            let name = names[(usize::from(first) + i) % 7].clone();
            div()
                .role(Role::ColumnHeader)
                .accessible_label(name.clone())
                .w(DAY)
                .h(24.)
                .items_center()
                .justify_center()
                .child(
                    text(name.chars().take(2).collect::<String>())
                        .text_size(tok::FONT_SIZE_SM)
                        .text_color(tok::TEXT_SECONDARY)
                        .accessible_hidden(),
                )
        }))
    };
    let weeks = (0..WEEKS).map(|w| {
        row()
            .role(Role::Row)
            .children((0..7).map(|d| day_cell(month.clone(), w * 7 + d)))
    });
    let (keys, mount) = (month.clone(), month.clone());
    column()
        .gap(6.)
        .p(4.)
        .child(
            row()
                .items_center()
                .gap(4.)
                .child(arrow(false))
                .child(caption)
                .child(arrow(true)),
        )
        .child(
            column()
                .role(Role::Grid)
                .accessible_label_or(label, followed.as_ref())
                .focusable(true)
                .rounded(6.)
                .on_mount(move |handle| {
                    *mount.grid.borrow_mut() = Some(handle);
                    mount.read_direction();
                    mount.describe_active();
                })
                .on_event(move |cx| calendar_keys(&keys, cx))
                .child(header)
                .children(weeks),
        )
}

/// The calendar's keys and focus.
fn calendar_keys(month: &Rc<Month>, cx: &mut EventContext) {
    if cx.phase == EventPhase::Capture {
        return;
    }
    let own = cx.target == cx.current_target;
    match &cx.event {
        InputEvent::Focus if own => {
            month.read_direction();
            month.focused.set(true);
            return;
        }
        InputEvent::Blur if own => {
            month.focused.set(false);
            return;
        }
        InputEvent::PointerDown { .. } => {
            cx.focus();
            return;
        }
        _ => {}
    }
    let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
        return;
    };
    month.read_direction();
    let now = month.cursor.with_untracked(|c| *c);
    let (back, on) = if month.rtl.with_untracked(|r| *r) {
        (Key::ArrowRight, Key::ArrowLeft)
    } else {
        (Key::ArrowLeft, Key::ArrowRight)
    };
    let into_week =
        (i64::from(now.weekday()) - i64::from(month.options.first_weekday)).rem_euclid(7);
    match key {
        k if *k == on => month.go(now.add_days(1)),
        k if *k == back => month.go(now.add_days(-1)),
        Key::ArrowDown => month.go(now.add_days(7)),
        Key::ArrowUp => month.go(now.add_days(-7)),
        Key::Home => month.go(now.add_days(-into_week)),
        Key::End => month.go(now.add_days(6 - into_week)),
        Key::PageDown => month.go(now.add_months(if modifiers.shift { 12 } else { 1 })),
        Key::PageUp => month.go(now.add_months(if modifiers.shift { -12 } else { -1 })),
        Key::Enter => {
            if month.options.allows(now) {
                (month.choose)(now);
            }
        }
        Key::Character(s) if s == " " => {
            if month.options.allows(now) {
                (month.choose)(now);
            }
        }
        _ => return,
    }
    cx.prevent_default();
    cx.stop_propagation();
}

/// The grid's `index`th day: its number, marked chosen, today, current,
/// outside the month or not to be chosen.
fn day_cell(month: Rc<Month>, index: usize) -> View {
    let (number, label, selected, disabled, styled, mount, press) = (
        month.clone(),
        month.clone(),
        month.clone(),
        month.clone(),
        month.clone(),
        month.clone(),
        month,
    );
    div()
        .role(Role::Cell)
        .w(DAY)
        .h(DAY)
        .items_center()
        .justify_center()
        .rounded(DAY / 2.)
        .accessible_label_when(move || label.options.spoken(label.date_at(index)))
        .selected_when(move || {
            let date = selected.date_at(index);
            selected.value.with(|v| *v == Some(date))
        })
        .disabled_when(move || !disabled.options.allows(disabled.date_at(index)))
        .reactive_style(move || {
            let date = styled.date_at(index);
            let chosen = styled.value.with(|v| *v == Some(date));
            let current = styled.cursor.with(|c| *c == date);
            let in_month = styled.cursor.with(|c| c.month == date.month);
            let mut styles = Styles::new().text_color(if chosen {
                tok::ACCENT_TEXT
            } else if !styled.options.allows(date) {
                tok::TEXT_DISABLED
            } else if in_month {
                tok::TEXT_PRIMARY
            } else {
                tok::TEXT_TERTIARY
            });
            styles = if chosen {
                styles.bg(tok::ACCENT)
            } else {
                styles.bg(Color(0, 0, 0, 0))
            };
            styles = if current && styled.focused.get() {
                styles.border(2.).border_color(tok::FOCUS_RING)
            } else if date == styled.today {
                styles.border(1.).border_color(tok::BORDER_STRONG)
            } else {
                styles.border(0.)
            };
            styles
        })
        .on_mount(move |handle| {
            mount.cells.borrow_mut()[index] = Some(handle.node());
            mount.describe_active();
        })
        .on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            if let InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } = cx.event
            {
                let date = press.date_at(index);
                if press.options.allows(date) {
                    press.cursor.set(date);
                    (press.choose)(date);
                }
            }
        })
        .child(text_signal(move || number.date_at(index).day.to_string()).accessible_hidden())
}

/// A button showing `value` (two-way) that opens a calendar to choose it
/// from: chosen, the calendar closes and focus is back on the button.
/// Down opens it too. `label` names both.
pub fn date_picker(
    label: impl Into<String>,
    value: Signal<Option<CalendarDate>>,
    options: CalendarOptions,
) -> View {
    let label = label.into();
    component(move |cx| {
        // A label that changes (a translation, a state), where the caller
        // gave one (`FollowedLabel::around`).
        let followed = zgx::compose::FollowedLabel::take(cx);
        let runtime = cx.runtime();
        let open = runtime.signal(false);
        let today = options.today.unwrap_or_else(CalendarDate::today);
        let cursor = runtime.signal(
            value
                .with_untracked(|v| v.unwrap_or(today))
                .clamp_to(options.min, options.max),
        );
        // Each time it opens: at the chosen day, else today.
        {
            let (open, value, cursor, options) =
                (open.clone(), value.clone(), cursor.clone(), options.clone());
            cx.retain(runtime.effect(move || {
                if open.get() {
                    let at = value
                        .with_untracked(|v| v.unwrap_or(today))
                        .clamp_to(options.min, options.max);
                    cursor.set(at);
                }
            }));
        }
        let shown = {
            let (value, format) = (value.clone(), options.format.clone());
            move || {
                value.get().map(|d| match &format {
                    Some(format) => format(d),
                    None => d.to_string(),
                })
            }
        };
        let (named, written, opens, keyed) = (shown.clone(), shown, open.clone(), open.clone());
        let (name, spoken) = (label.clone(), followed.clone());
        let trigger = button()
            .w_full()
            .justify_between()
            .accessible_label_when(move || {
                let name = spoken.as_ref().map_or_else(|| name.clone(), |f| f.get());
                match named() {
                    Some(date) => format!("{name}: {date}"),
                    None => name,
                }
            })
            .gap(8.)
            .flex_row()
            .items_center()
            .on_click(move || {
                opens.set(!opens.with_untracked(|o| *o));
            })
            .on_event(move |cx| {
                if cx.phase != EventPhase::Capture
                    && matches!(
                        cx.event,
                        InputEvent::KeyDown {
                            key: Key::ArrowDown,
                            ..
                        }
                    )
                {
                    keyed.set(true);
                    cx.prevent_default();
                    cx.stop_propagation();
                }
            })
            .child(
                text_signal(move || written().unwrap_or_else(|| "Pick a date".into()))
                    .reactive_style({
                        let value = value.clone();
                        move || {
                            Styles::new().text_color(if value.with(Option::is_some) {
                                tok::TEXT_PRIMARY
                            } else {
                                tok::TEXT_TERTIARY
                            })
                        }
                    }),
            )
            .child(
                text("▾")
                    .text_size(10.)
                    .text_color(tok::TEXT_SECONDARY)
                    .accessible_hidden(),
            );
        let (close, chosen) = (open.clone(), value.clone());
        let panel = month_view(
            cx,
            label.clone(),
            followed.clone(),
            value,
            cursor,
            options,
            today,
            Rc::new(move |d: CalendarDate| {
                chosen.set(Some(d));
                close.set(false);
            }),
        );
        // What the picker is given (its `id`, its size) lands on this box,
        // which the trigger fills: on the popover it would reach the panel.
        column().child(popover(label.clone(), open, trigger).lazy().child(panel))
    })
}

/// How a [`time_field`] steps and shows its hours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeOptions {
    /// What Up and Down step the minutes by.
    pub minute_step: u8,
    /// Hours 1 to 12 and an AM/PM segment, rather than 0 to 23.
    pub hour12: bool,
}
impl Default for TimeOptions {
    fn default() -> Self {
        Self {
            minute_step: 1,
            hour12: false,
        }
    }
}

/// Which of a time field's segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Segment {
    Hour,
    Minute,
    Meridiem,
}

/// What a time field's segments share.
struct Clock {
    value: Signal<Option<TimeOfDay>>,
    hour: Signal<Option<u8>>,
    minute: Signal<Option<u8>>,
    options: TimeOptions,
    segments: RefCell<Vec<NodeHandle>>,
    /// Digits typed into a segment so far.
    typed: RefCell<String>,
}
impl Clock {
    fn order(&self) -> Vec<Segment> {
        let mut order = vec![Segment::Hour, Segment::Minute];
        if self.options.hour12 {
            order.push(Segment::Meridiem);
        }
        order
    }
    fn focus(&self, segment: usize) {
        let handle = self.segments.borrow().get(segment).cloned();
        if let Some(handle) = handle {
            handle.focus();
        }
    }
    /// Its text as it shows: `09`, `--`, `PM`.
    fn shown(&self, segment: Segment) -> String {
        match segment {
            Segment::Hour => self.hour.get().map_or("--".into(), |h| {
                let h = if self.options.hour12 {
                    (h + 11) % 12 + 1
                } else {
                    h
                };
                format!("{h:02}")
            }),
            Segment::Minute => self.minute.get().map_or("--".into(), |m| format!("{m:02}")),
            Segment::Meridiem => self
                .hour
                .get()
                .map_or("--".into(), |h| (if h < 12 { "AM" } else { "PM" }).into()),
        }
    }
    /// Step a segment by `by`, wrapping (a blank one starts at its first).
    fn step(&self, segment: Segment, by: i32) {
        match segment {
            Segment::Hour => {
                let next = self
                    .hour
                    .with_untracked(|h| h.map_or(0, |h| (i32::from(h) + by).rem_euclid(24)));
                self.hour.set(Some(next as u8));
            }
            Segment::Minute => {
                let step = i32::from(self.options.minute_step.max(1));
                let next = self.minute.with_untracked(|m| {
                    m.map_or(0, |m| {
                        // To the step's next multiple, as a stepped field does.
                        let m = i32::from(m);
                        let snapped = if by > 0 {
                            (m / step + 1) * step
                        } else {
                            (m + step - 1) / step * step - step
                        };
                        snapped.rem_euclid(60)
                    })
                });
                self.minute.set(Some(next as u8));
            }
            Segment::Meridiem => {
                let next = self.hour.with_untracked(|h| h.map_or(0, |h| (h + 12) % 24));
                self.hour.set(Some(next));
            }
        }
    }
}

/// A time of day in `value` (two-way: none until each segment has one),
/// in hour and minute segments (and AM/PM, `hour12`). `label` names it.
pub fn time_field(
    label: impl Into<String>,
    value: Signal<Option<TimeOfDay>>,
    options: TimeOptions,
) -> View {
    let label = label.into();
    component(move |cx| {
        let followed = zgx::compose::FollowedLabel::take(cx);
        let runtime = cx.runtime();
        let now = value.with_untracked(|v| *v);
        let clock = Rc::new(Clock {
            value: value.clone(),
            hour: runtime.signal(now.map(|t| t.hour)),
            minute: runtime.signal(now.map(|t| t.minute)),
            options,
            segments: RefCell::default(),
            typed: RefCell::default(),
        });
        // The segments to the value, and the value to them (not clearing
        // what is typed while one is blank).
        {
            let clock = clock.clone();
            cx.retain(runtime.effect(move || {
                let next = match (clock.hour.get(), clock.minute.get()) {
                    (Some(hour), Some(minute)) => TimeOfDay::new(hour, minute),
                    _ => None,
                };
                clock.value.set(next);
            }));
        }
        {
            let clock = clock.clone();
            cx.retain(runtime.effect(move || {
                let given = clock.value.get();
                let parts = (
                    clock.hour.with_untracked(|h| *h),
                    clock.minute.with_untracked(|m| *m),
                );
                match given {
                    Some(t) => {
                        clock.hour.set(Some(t.hour));
                        clock.minute.set(Some(t.minute));
                    }
                    None if parts.0.is_some() && parts.1.is_some() => {
                        clock.hour.set(None);
                        clock.minute.set(None);
                    }
                    None => {}
                }
            }));
        }
        let order = clock.order();
        let mut field = row()
            .role(Role::Group)
            .accessible_label_or(label.clone(), followed.as_ref())
            .direction(Direction::Ltr)
            .items_center()
            .gap(1.)
            .px(6.)
            .h(28.)
            .rounded(6.)
            .bg(tok::INPUT_BG)
            .border(1.)
            .border_color(tok::BORDER);
        for (i, segment) in order.iter().copied().enumerate() {
            if segment == Segment::Minute {
                field = field.child(
                    text(":")
                        .text_color(tok::TEXT_SECONDARY)
                        .accessible_hidden(),
                );
            }
            if segment == Segment::Meridiem {
                field = field.child(div().w(4.));
            }
            field = field.child(time_segment(clock.clone(), segment, i, order.len()));
        }
        field
    })
}

/// One segment of a time field, the `index`th of `count`.
fn time_segment(clock: Rc<Clock>, segment: Segment, index: usize, count: usize) -> View {
    let (min, max) = match segment {
        Segment::Hour if clock.options.hour12 => (1., 12.),
        Segment::Hour => (0., 23.),
        Segment::Minute => (0., 59.),
        Segment::Meridiem => (0., 1.),
    };
    let (shown, spoken, numeric, mount, keys) = (
        clock.clone(),
        clock.clone(),
        clock.clone(),
        clock.clone(),
        clock,
    );
    div()
        .role(Role::SpinButton)
        .accessible_label(match segment {
            Segment::Hour => "Hours",
            Segment::Minute => "Minutes",
            Segment::Meridiem => "AM/PM",
        })
        .focusable(true)
        .px(2.)
        .rounded(3.)
        .focus(|s| s.bg(tok::SELECTION).text_color(tok::SELECTION_TEXT))
        .value_when(move || spoken.shown(segment))
        .numeric_when(min, max, move || match segment {
            Segment::Hour => numeric.hour.get().map_or(f64::NAN, |h| {
                f64::from(if numeric.options.hour12 {
                    (h + 11) % 12 + 1
                } else {
                    h
                })
            }),
            Segment::Minute => numeric.minute.get().map_or(f64::NAN, f64::from),
            Segment::Meridiem => numeric
                .hour
                .get()
                .map_or(f64::NAN, |h| f64::from(u8::from(h >= 12))),
        })
        .on_mount(move |handle| {
            let mut segments = mount.segments.borrow_mut();
            if segments.len() <= index {
                segments.resize(index + 1, handle.clone());
            }
            segments[index] = handle;
        })
        .on_event(move |cx| time_keys(&keys, segment, index, count, cx))
        .child(
            text_signal(move || shown.shown(segment))
                .font_features(FontFeatures::new([(*b"tnum", 1)])),
        )
}

/// A time segment's keys.
fn time_keys(
    clock: &Rc<Clock>,
    segment: Segment,
    index: usize,
    count: usize,
    cx: &mut EventContext,
) {
    if cx.phase == EventPhase::Capture || cx.target != cx.current_target {
        return;
    }
    match &cx.event {
        InputEvent::Focus | InputEvent::Blur => {
            clock.typed.borrow_mut().clear();
            return;
        }
        InputEvent::PointerDown { .. } => {
            cx.focus();
            return;
        }
        InputEvent::Increment => clock.step(segment, 1),
        InputEvent::Decrement => clock.step(segment, -1),
        InputEvent::KeyDown { key, .. } => match key {
            Key::ArrowUp => clock.step(segment, 1),
            Key::ArrowDown => clock.step(segment, -1),
            Key::PageUp => clock.step(segment, if segment == Segment::Hour { 6 } else { 10 }),
            Key::PageDown => clock.step(segment, if segment == Segment::Hour { -6 } else { -10 }),
            Key::ArrowRight if index + 1 < count => clock.focus(index + 1),
            Key::ArrowLeft if index > 0 => clock.focus(index - 1),
            Key::Backspace | Key::Delete => {
                clock.typed.borrow_mut().clear();
                match segment {
                    Segment::Minute => clock.minute.set(None),
                    _ => clock.hour.set(None),
                };
            }
            Key::Character(s) if segment == Segment::Meridiem => {
                let pm = match s.to_lowercase().as_str() {
                    "a" => false,
                    "p" => true,
                    _ => return,
                };
                let hour = clock.hour.with_untracked(|h| h.unwrap_or(0));
                clock.hour.set(Some(hour % 12 + if pm { 12 } else { 0 }));
            }
            Key::Character(s) if s.len() == 1 && s.as_bytes()[0].is_ascii_digit() => {
                type_digit(clock, segment, index, count, s.as_bytes()[0] - b'0');
            }
            _ => return,
        },
        _ => return,
    }
    cx.prevent_default();
    cx.stop_propagation();
}

/// A digit typed into a segment: the first sets it; a second makes two
/// digits, and the next segment takes focus once no more can follow.
fn type_digit(clock: &Rc<Clock>, segment: Segment, index: usize, count: usize, digit: u8) {
    let typed = {
        let mut typed = clock.typed.borrow_mut();
        typed.push(char::from(b'0' + digit));
        typed.clone()
    };
    let number: u8 = typed.parse().unwrap_or(0);
    let (first, last) = match segment {
        Segment::Hour if clock.options.hour12 => (1, 12),
        Segment::Hour => (0, 23),
        _ => (0, 59),
    };
    let value = if number > last { digit } else { number };
    match segment {
        Segment::Hour if clock.options.hour12 => {
            let pm = clock.hour.with_untracked(|h| h.is_some_and(|h| h >= 12));
            let h12 = value.max(first);
            clock.hour.set(Some(h12 % 12 + if pm { 12 } else { 0 }));
        }
        Segment::Hour => {
            clock.hour.set(Some(value));
        }
        _ => {
            clock.minute.set(Some(value));
        }
    }
    // Done when two digits are in, or no second could follow the first.
    let done = typed.len() >= 2 || u16::from(value) * 10 > u16::from(last);
    if done {
        clock.typed.borrow_mut().clear();
        if index + 1 < count {
            clock.focus(index + 1);
        }
    } else if number > last {
        *clock.typed.borrow_mut() = char::from(b'0' + digit).to_string();
    }
}
