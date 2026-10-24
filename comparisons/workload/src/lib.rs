//! Identical bounded deterministic work and geometry for all three adapters.
use std::{fmt::Write, ops::Range, time::Duration};
pub const WIDTH: f32 = 960.0;
pub const HEIGHT: f32 = 720.0;
pub const ROW_HEIGHT: f32 = 28.0;
pub const ROW_COUNT: usize = 100_000;
pub const VIEWPORT_HEIGHT: f32 = 400.0;
pub const PERIOD: Duration = Duration::from_nanos(16_666_667);
pub const TOKEN: &str = "The quick brown fox streams a token. ";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Stream,
    Scroll,
    Both,
}
/// `std::env::var(key)`, or for one release the name the variable had before
/// the rename to zgx.
fn var(key: &str) -> Result<String, std::env::VarError> {
    std::env::var(key).or_else(|_| std::env::var(key.replace("ZGX_", "ZGUI_"))) // rename-to-zgx: keep
}
impl Mode {
    pub fn from_env() -> Self {
        match var("ZGX_MODE").as_deref() {
            Ok("idle") => Self::Idle,
            Ok("stream") => Self::Stream,
            Ok("scroll") => Self::Scroll,
            _ => Self::Both,
        }
    }
}
pub struct Workload {
    pub text: String,
    pub scroll_offset: f32,
    pub frames: u64,
    pub mode: Mode,
    pub seconds: f64,
}
impl Default for Workload {
    fn default() -> Self {
        Self::from_env()
    }
}
impl Workload {
    pub fn from_env() -> Self {
        let mut work = Self {
            text: String::with_capacity(8256),
            scroll_offset: 0.,
            frames: 0,
            mode: Mode::from_env(),
            seconds: var("ZGX_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.)
                .unwrap_or(30.),
        };
        let initial_ticks = var("ZGX_INITIAL_TICKS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            .min(100_000);
        let mode = work.mode;
        work.mode = Mode::Both;
        for _ in 0..initial_ticks {
            work.tick();
        }
        work.mode = mode;
        work
    }
    pub fn tick(&mut self) {
        self.frames += 1;
        if matches!(self.mode, Mode::Stream | Mode::Both) {
            write!(self.text, "{:06} {TOKEN}", self.frames).unwrap();
            if self.text.len() > 8192 {
                self.text.drain(..self.text.len() - 8192);
            }
        }
        if matches!(self.mode, Mode::Scroll | Mode::Both) {
            self.scroll_offset =
                (self.scroll_offset + 14.) % (ROW_COUNT as f32 * ROW_HEIGHT - VIEWPORT_HEIGHT);
        }
    }
    pub fn row_range(&self) -> Range<usize> {
        let first = (self.scroll_offset / ROW_HEIGHT) as usize;
        let end = ((self.scroll_offset as f64 + VIEWPORT_HEIGHT as f64) / ROW_HEIGHT as f64).ceil()
            as usize;
        first.saturating_sub(2)..end.saturating_add(2).min(ROW_COUNT)
    }
    pub fn visible_text(&self) -> String {
        self.text.as_bytes()[self.text.len().saturating_sub(735)..]
            .chunks(105)
            .take(7)
            .map(|s| std::str::from_utf8(s).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
pub fn row_label(index: usize) -> String {
    format!("Row {index:06} — retained virtual item")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_range_uses_two_rows_of_overscan_on_each_side() {
        let mut work = Workload {
            text: String::new(),
            scroll_offset: 0.,
            frames: 0,
            mode: Mode::Idle,
            seconds: 1.,
        };
        assert_eq!(work.row_range(), 0..17);
        work.scroll_offset = 280.;
        assert_eq!(work.row_range(), 8..27);
        work.scroll_offset = ROW_COUNT as f32 * ROW_HEIGHT - VIEWPORT_HEIGHT;
        assert_eq!(work.row_range().end, ROW_COUNT);
        assert!(work.row_range().len() <= 19);
    }
    #[test]
    fn bounded_and_deterministic() {
        let mut w = Workload::from_env();
        w.mode = Mode::Both;
        for _ in 0..10_000 {
            let before = w.visible_text();
            w.tick();
            assert!(w.text.len() <= 8192);
            assert!(w.row_range().len() <= 19);
            assert_ne!(before, w.visible_text());
        }
        assert_eq!(w.scroll_offset, 140_000.);
    }
}

/// A continuously animated scene for comparing per-frame cost at the display
/// refresh rate: every dot's opacity and every shimmer character's colour
/// change on every frame, like loaders and skeleton shimmers.
pub mod animation {
    pub const COLUMNS: usize = 12;
    pub const ROWS: usize = 4;
    pub const DOT: f32 = 14.;
    pub const GAP: f32 = 12.;
    pub const ORIGIN: (f32, f32) = (40., 120.);
    pub const SHIMMER: &str = "Reading workspace files and streaming tokens";
    pub const SHIMMER_ORIGIN: (f32, f32) = (40., 260.);
    pub const DOT_COLOR: u32 = 0x9ad2ff;
    pub const BACKGROUND: u32 = 0x10141c;

    pub fn dot_position(index: usize) -> (f32, f32) {
        let (column, row) = (index % COLUMNS, index / COLUMNS);
        (
            ORIGIN.0 + column as f32 * (DOT + GAP),
            ORIGIN.1 + row as f32 * (DOT + GAP),
        )
    }
    /// A 750 ms blink with a per-dot phase offset.
    pub fn dot_opacity(index: usize, seconds: f32) -> f32 {
        let phase = (seconds / 0.75 - index as f32 * 0.07).rem_euclid(1.);
        if phase < 0.45 {
            1. - 0.9 * (1. - (1. - phase / 0.45).powi(3))
        } else if phase < 0.92 {
            0.1
        } else {
            1.
        }
    }
    /// White text whose alpha follows a light band sweeping every 3.4 s.
    pub fn shimmer_alpha(index: usize, count: usize, seconds: f32) -> u8 {
        let last = (count.max(2) - 1) as f32;
        let centre = -0.4 + 1.8 * (seconds / 3.4).rem_euclid(1.);
        let x = (1. - (index as f32 / last - centre).abs() / 0.36).clamp(0., 1.);
        (90. + 165. * x * x * (3. - 2. * x)) as u8
    }
}

/// A busy, mostly static window with one small continuous animation: the
/// case damage-based rendering is for. Only the spinner changes each frame.
pub mod busy {
    pub const GRID_COLUMNS: usize = 8;
    pub const GRID_ROWS: usize = 30;
    pub const CELL: (f32, f32) = (112., 16.);
    pub const GRID_ORIGIN: (f32, f32) = (12., 56.);
    pub const PARAGRAPH_ORIGIN: (f32, f32) = (12., 548.);
    pub const PARAGRAPH_LINES: usize = 20;
    pub const LINE_HEIGHT: f32 = 8.4;
    pub const SPINNER_CENTRE: (f32, f32) = (920., 28.);
    pub const SPINNER_DOTS: usize = 12;
    pub const SPINNER_RADIUS: f32 = 14.;
    pub const DOT: f32 = 5.;

    pub fn cell_position(index: usize) -> (f32, f32) {
        let (column, row) = (index % GRID_COLUMNS, index / GRID_COLUMNS);
        (
            GRID_ORIGIN.0 + column as f32 * (CELL.0 + 5.),
            GRID_ORIGIN.1 + row as f32 * CELL.1,
        )
    }
    pub fn cell_label(index: usize) -> String {
        format!("row {:02} · item {:03}", index / GRID_COLUMNS, index)
    }
    pub fn cell_color(index: usize) -> u32 {
        if (index / GRID_COLUMNS + index % GRID_COLUMNS).is_multiple_of(2) {
            0x1b2330
        } else {
            0x202a3a
        }
    }
    pub fn paragraph_line(index: usize) -> String {
        format!(
            "{index:02}  The quick brown fox jumps over the lazy dog while the renderer keeps every static glyph retained."
        )
    }
    pub fn dot_position(index: usize) -> (f32, f32) {
        let angle = index as f32 / SPINNER_DOTS as f32 * std::f32::consts::TAU;
        (
            SPINNER_CENTRE.0 + SPINNER_RADIUS * angle.cos() - DOT / 2.,
            SPINNER_CENTRE.1 + SPINNER_RADIUS * angle.sin() - DOT / 2.,
        )
    }
    /// A light chasing around the ring once every 0.9 s.
    pub fn dot_opacity(index: usize, seconds: f32) -> f32 {
        let head = (seconds / 0.9).rem_euclid(1.) * SPINNER_DOTS as f32;
        let behind = (head - index as f32).rem_euclid(SPINNER_DOTS as f32);
        (1. - behind / SPINNER_DOTS as f32).powi(2).max(0.12)
    }
}

/// A heavy live dashboard: most of a larger window changes every tick. Stat
/// tiles, service cards with a value, a progress bar and a scrolling
/// sparkline each, a scrolling status table and a streaming log, all derived
/// from `live` (advanced by `stream` and `both`) and `scroll` (advanced by
/// `scroll` and `both`) so every adapter draws the same pixels.
pub mod heavy {
    use super::{Mode, PERIOD};
    use std::{collections::VecDeque, fmt::Write, ops::Range};
    pub const WIDTH: f32 = 1280.;
    pub const HEIGHT: f32 = 800.;
    /// Installed on every Mac, so no adapter falls back to another face.
    pub const FONT: &str = "Arial";
    pub const BACKGROUND: u32 = 0x0e1218;
    pub const PANEL: u32 = 0x161c26;
    pub const HEADER: u32 = 0x1c2430;
    pub const STRIPE: u32 = 0x19202b;
    pub const BORDER: u32 = 0x263041;
    pub const TRACK: u32 = 0x243042;
    pub const TEXT: u32 = 0xe5edf7;
    pub const MUTED: u32 = 0x8b98ab;
    pub const ACCENT: u32 = 0x5eb1ff;

    pub const TITLE: &str = "Fleet overview";
    pub const TITLE_ORIGIN: (f32, f32) = (16., 14.);

    pub const TILES: usize = 6;
    pub const TILE: (f32, f32) = (200., 60.);
    pub const TILE_ORIGIN: (f32, f32) = (16., 48.);
    pub const TILE_STEP: f32 = 208.;
    /// Label and value positions inside a tile.
    pub const TILE_LABEL: (f32, f32) = (12., 8.);
    pub const TILE_VALUE: (f32, f32) = (12., 26.);

    pub const CARD_COLUMNS: usize = 6;
    pub const CARD_ROWS: usize = 5;
    pub const CARDS: usize = CARD_COLUMNS * CARD_ROWS;
    pub const CARD: (f32, f32) = (120., 104.);
    pub const CARD_ORIGIN: (f32, f32) = (16., 120.);
    pub const CARD_STEP: (f32, f32) = (128., 112.);
    /// Inside a card: title, value, progress track and the sparkline's bars,
    /// bottom-aligned on `SPARK_BASE`.
    pub const CARD_TITLE: (f32, f32) = (8., 6.);
    pub const CARD_VALUE: (f32, f32) = (8., 20.);
    pub const PROGRESS: (f32, f32, f32, f32) = (8., 46., 104., 4.);
    pub const BARS: usize = 26;
    pub const BAR_X: f32 = 8.;
    pub const BAR_WIDTH: f32 = 3.;
    pub const BAR_STEP: f32 = 4.;
    pub const SPARK_BASE: f32 = 96.;
    pub const SPARK_HEIGHT: f32 = 42.;

    pub const TABLE_ORIGIN: (f32, f32) = (792., 120.);
    pub const TABLE: (f32, f32) = (472., 552.);
    pub const TABLE_HEADER: f32 = 28.;
    pub const TABLE_VIEWPORT: f32 = TABLE.1 - TABLE_HEADER;
    pub const TABLE_ROW: f32 = 24.;
    pub const TABLE_ROWS: usize = 100_000;
    pub const TABLE_OVERSCAN: usize = 2;
    /// Column x offsets and titles; text sits `CELL_TOP` below a row's top.
    pub const COLUMNS: [(f32, &str); 5] = [
        (10., "ID"),
        (84., "Worker"),
        (238., "Status"),
        (312., "Latency"),
        (386., "Throughput"),
    ];
    pub const CELL_TOP: f32 = 4.;
    pub const PILL: (f32, f32, f32, f32) = (238., 4., 64., 16.);
    pub const PILL_TEXT: (f32, f32) = (8., 1.);

    pub const LOG_ORIGIN: (f32, f32) = (16., 684.);
    pub const LOG: (f32, f32) = (1248., 100.);
    pub const LOG_TEXT: (f32, f32) = (12., 8.);
    pub const LOG_LINES: usize = 5;
    pub const LOG_LINE_HEIGHT: f32 = 16.;

    pub struct Heavy {
        pub frames: u64,
        pub live: u64,
        pub scroll: f32,
        pub log: VecDeque<String>,
        pub mode: Mode,
        pub seconds: f64,
    }
    impl Heavy {
        pub fn from_env() -> Self {
            let work = super::Workload::from_env();
            let mut heavy = Self {
                frames: 0,
                live: 0,
                scroll: 0.,
                log: VecDeque::with_capacity(LOG_LINES + 1),
                mode: work.mode,
                seconds: work.seconds,
            };
            for _ in 0..LOG_LINES {
                heavy.push_log();
            }
            heavy
        }
        pub fn period() -> std::time::Duration {
            PERIOD
        }
        /// Whether the workload runs now. With `ZGX_PARITY` set to `even`
        /// or `odd`, only in Unix seconds of that parity, so two apps take
        /// turns and WindowServer's CPU in each second is one app's
        /// (`scripts/windowserver_pair.py`); otherwise always.
        pub fn turn() -> bool {
            static PARITY: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
            let parity = PARITY.get_or_init(|| match super::var("ZGX_PARITY").as_deref() {
                Ok("even") => Some(0),
                Ok("odd") => Some(1),
                _ => None,
            });
            parity.is_none_or(|parity| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                now.as_secs() % 2 == parity
            })
        }
        pub fn tick(&mut self) {
            self.frames += 1;
            if matches!(self.mode, Mode::Stream | Mode::Both) {
                self.live += 1;
                self.push_log();
            }
            if matches!(self.mode, Mode::Scroll | Mode::Both) {
                self.scroll =
                    (self.scroll + 14.) % (TABLE_ROWS as f32 * TABLE_ROW - TABLE_VIEWPORT);
            }
        }
        fn push_log(&mut self) {
            let (live, node) = (self.live, self.live as usize % CARDS);
            let mut line = String::with_capacity(160);
            write!(
                line,
                "{live:06}  {}  latency {}  p99 {:.1} ms  queue {:>3}  {}  checksum {:08x}",
                card_title(node),
                card_value(node, live),
                8. + 180. * sample(node + 300, live),
                (sample(node + 600, live) * 400.) as u32,
                status(node, live).0,
                (live as u32).wrapping_mul(2_654_435_761),
            )
            .unwrap();
            self.log.push_back(line);
            if self.log.len() > LOG_LINES {
                self.log.pop_front();
            }
        }
        pub fn visible_log(&self) -> String {
            self.log
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        }
        /// Table rows to build, with `TABLE_OVERSCAN` rows on each side.
        pub fn row_range(&self) -> Range<usize> {
            row_range(self.scroll)
        }
    }
    pub fn row_range(scroll: f32) -> Range<usize> {
        let first = (scroll / TABLE_ROW) as usize;
        let end = ((scroll + TABLE_VIEWPORT) / TABLE_ROW).ceil() as usize;
        first.saturating_sub(TABLE_OVERSCAN)..(end + TABLE_OVERSCAN).min(TABLE_ROWS)
    }
    /// A smooth pseudo-random series in [0.02, 1], one per `series`.
    pub fn sample(series: usize, t: u64) -> f32 {
        let x = t as f32 * 0.09 + series as f32 * 1.7;
        (0.5 + 0.28 * x.sin() + 0.14 * (x * 2.3 + series as f32).sin() + 0.08 * (x * 5.1).sin())
            .clamp(0.02, 1.)
    }
    pub fn tile_label(tile: usize) -> &'static str {
        [
            "Requests / s",
            "Error rate",
            "p50 latency",
            "p99 latency",
            "Active workers",
            "Queue depth",
        ][tile]
    }
    pub fn tile_position(tile: usize) -> (f32, f32) {
        (TILE_ORIGIN.0 + tile as f32 * TILE_STEP, TILE_ORIGIN.1)
    }
    pub fn tile_value(tile: usize, live: u64) -> String {
        let s = sample(900 + tile, live);
        match tile {
            0 => format!("{:.0}", 12_000. + 40_000. * s),
            1 => format!("{:.2}%", 3. * s),
            2 => format!("{:.1} ms", 4. + 30. * s),
            3 => format!("{:.1} ms", 40. + 200. * s),
            4 => format!("{}", 800 + (s * 400.) as u32),
            _ => format!("{}", (s * 5_000.) as u32),
        }
    }
    pub fn card_position(card: usize) -> (f32, f32) {
        let (column, row) = (card % CARD_COLUMNS, card / CARD_COLUMNS);
        (
            CARD_ORIGIN.0 + column as f32 * CARD_STEP.0,
            CARD_ORIGIN.1 + row as f32 * CARD_STEP.1,
        )
    }
    pub fn card_title(card: usize) -> String {
        format!("node-{card:02}")
    }
    pub fn card_value(card: usize, live: u64) -> String {
        format!("{:.1} ms", 4. + 96. * sample(card, live))
    }
    /// The progress fill's width.
    pub fn card_progress(card: usize, live: u64) -> f32 {
        PROGRESS.2 * sample(card + 1000, live)
    }
    /// Bar `bar`'s height: the series advances one sample per tick, so the
    /// whole sparkline scrolls left.
    pub fn bar_height(card: usize, bar: usize, live: u64) -> f32 {
        (SPARK_HEIGHT * sample(card, live + bar as u64)).max(2.)
    }
    pub fn bar_x(bar: usize) -> f32 {
        BAR_X + bar as f32 * BAR_STEP
    }
    pub fn row_id(row: usize) -> String {
        format!("#{row:06}")
    }
    pub fn row_worker(row: usize) -> String {
        format!(
            "worker-{:05}.{}",
            row % 100_000,
            ["iad", "sfo", "fra", "nrt"][row % 4]
        )
    }
    /// A status label and its pill colour.
    pub fn status(series: usize, live: u64) -> (&'static str, u32) {
        let s = sample(series + 5000, live / 20);
        if s > 0.8 {
            ("degraded", 0x8a5a12)
        } else if s < 0.12 {
            ("down", 0x8a2230)
        } else {
            ("healthy", 0x1d6b4a)
        }
    }
    pub fn row_latency(row: usize, live: u64) -> String {
        format!("{:.1} ms", 2. + 120. * sample(row + 2000, live))
    }
    pub fn row_throughput(row: usize, live: u64) -> String {
        format!("{:.2} MB/s", 0.2 + 48. * sample(row + 3000, live))
    }
    pub fn row_color(row: usize) -> u32 {
        if row.is_multiple_of(2) {
            PANEL
        } else {
            STRIPE
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn geometry_fits_the_window_and_the_series_is_bounded() {
            let (x, y) = card_position(CARDS - 1);
            assert!(x + CARD.0 <= TABLE_ORIGIN.0 && y + CARD.1 <= LOG_ORIGIN.1);
            assert!(bar_x(BARS - 1) + BAR_WIDTH <= CARD.0 - BAR_X);
            assert!(TABLE_ORIGIN.0 + TABLE.0 <= WIDTH && LOG_ORIGIN.1 + LOG.1 <= HEIGHT);
            for t in 0..5_000 {
                let s = sample(t as usize % 97, t);
                assert!((0.02..=1.).contains(&s));
            }
            assert_eq!(row_range(0.).len(), 24);
        }
        #[test]
        fn modes_advance_only_their_parts() {
            let mut heavy = Heavy::from_env();
            heavy.mode = Mode::Scroll;
            heavy.tick();
            assert_eq!((heavy.live, heavy.scroll), (0, 14.));
            heavy.mode = Mode::Stream;
            let before = heavy.visible_log();
            heavy.tick();
            assert_eq!((heavy.live, heavy.scroll), (1, 14.));
            assert_ne!(before, heavy.visible_log());
            assert_eq!(heavy.log.len(), LOG_LINES);
        }
    }
}
