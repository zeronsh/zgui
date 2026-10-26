//! Devtools' tapes of wheel input (`Ui::wheel_tape`): what a window received
//! from the platform, recorded with its times and phases, and played back
//! through the window's own input path, as if received again. A trackpad's
//! fingers and momentum, a wheel's notches: resampling, the trackpad's
//! phases and elastic edges then run as they do for the platform's events,
//! and nothing reaches the desktop.
//!
//! A tape is text, an event a line: milliseconds since the first, its kind
//! (`pixel` motion in points, or wheel `lines`), its motion across and down
//! as the platform reported it, and its phase:
//!
//! ```text
//! # zgx wheel tape 1
//! 0.000 pixel 0 -2.5 started
//! 8.312 pixel 0 -6.75 moved
//! ```
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use winit::event::{MouseScrollDelta, TouchPhase};

const HEADER: &str = "# zgx wheel tape 1";

/// One event of a tape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Entry {
    /// Since the tape's first event.
    pub after: Duration,
    /// Pixel motion in points (`true`), or wheel lines.
    pub pixels: bool,
    pub delta: (f32, f32),
    pub phase: TouchPhase,
}

fn phase_name(phase: TouchPhase) -> &'static str {
    match phase {
        TouchPhase::Started => "started",
        TouchPhase::Moved => "moved",
        TouchPhase::Ended => "ended",
        TouchPhase::Cancelled => "cancelled",
    }
}

/// `entries` as a tape's text.
pub(crate) fn format(entries: &[Entry]) -> String {
    let mut out = String::from(HEADER);
    for e in entries {
        out.push_str(&format!(
            "\n{:.3} {} {} {} {}",
            e.after.as_secs_f64() * 1e3,
            if e.pixels { "pixel" } else { "lines" },
            e.delta.0,
            e.delta.1,
            phase_name(e.phase)
        ));
    }
    out.push('\n');
    out
}

/// A tape's events, in order; what is wrong with it, at its line.
pub(crate) fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || {
            format!(
                "tape line {}: `{line}` is not `ms pixel|lines dx dy phase`",
                i + 1
            )
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        let [ms, kind, dx, dy, phase] = words[..] else {
            return Err(bad());
        };
        let number = |w: &str| w.parse::<f32>().ok().filter(|v| v.is_finite());
        let ms = ms.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.);
        let (Some(ms), Some(dx), Some(dy)) = (ms, number(dx), number(dy)) else {
            return Err(bad());
        };
        let pixels = match kind {
            "pixel" => true,
            "lines" => false,
            _ => return Err(bad()),
        };
        let phase = match phase {
            "started" => TouchPhase::Started,
            "moved" => TouchPhase::Moved,
            "ended" => TouchPhase::Ended,
            "cancelled" => TouchPhase::Cancelled,
            _ => return Err(bad()),
        };
        let after = Duration::from_secs_f64(ms / 1e3);
        if entries
            .last()
            .is_some_and(|last: &Entry| last.after > after)
        {
            return Err(format!("tape line {}: times go back", i + 1));
        }
        entries.push(Entry {
            after,
            pixels,
            delta: (dx, dy),
            phase,
        });
    }
    if entries.is_empty() {
        return Err("the tape holds no events".into());
    }
    Ok(entries)
}

/// A window's recording, and what it plays back.
#[derive(Default)]
pub(crate) struct Tape {
    /// Recording since then, what was received so far.
    recording: Option<(Option<Instant>, Vec<Entry>)>,
    /// To deliver, each at its time, at a point in the window.
    playing: VecDeque<(Instant, Entry)>,
    pub at: (f32, f32),
}

impl Tape {
    pub fn record(&mut self) {
        self.recording = Some((None, Vec::new()));
    }
    /// The recording so far, as text, and recording stops.
    pub fn stop(&mut self) -> Result<String, String> {
        match self.recording.take() {
            Some((_, entries)) if !entries.is_empty() => Ok(format(&entries)),
            Some(_) => Err("nothing was received while recording".into()),
            None => Err("not recording".into()),
        }
    }
    /// What the window received at `now`, kept if recording. `delta` is the
    /// platform's, pixels in points.
    pub fn received(
        &mut self,
        now: Instant,
        delta: MouseScrollDelta,
        scale: f64,
        phase: TouchPhase,
    ) {
        let Some((start, entries)) = &mut self.recording else {
            return;
        };
        let start = *start.get_or_insert(now);
        let (pixels, delta) = match delta {
            MouseScrollDelta::PixelDelta(p) => {
                let p = p.to_logical::<f32>(scale);
                (true, (p.x, p.y))
            }
            MouseScrollDelta::LineDelta(x, y) => (false, (x, y)),
        };
        entries.push(Entry {
            after: now.saturating_duration_since(start),
            pixels,
            delta,
            phase,
        });
    }
    /// Play `entries` from `now`, at `at`; whatever was playing stops.
    pub fn play(&mut self, now: Instant, entries: Vec<Entry>, at: (f32, f32)) {
        self.at = at;
        self.playing = entries.into_iter().map(|e| (now + e.after, e)).collect();
    }
    /// When the next event is due.
    pub fn deadline(&self) -> Option<Instant> {
        self.playing.front().map(|(at, _)| *at)
    }
    /// The next event due by `now`, as the platform would report it.
    pub fn due(&mut self, now: Instant, scale: f64) -> Option<(MouseScrollDelta, TouchPhase)> {
        if self.playing.front().is_none_or(|(at, _)| *at > now) {
            return None;
        }
        let (_, e) = self.playing.pop_front()?;
        let delta = if e.pixels {
            MouseScrollDelta::PixelDelta(
                winit::dpi::LogicalPosition::new(e.delta.0 as f64, e.delta.1 as f64)
                    .to_physical(scale),
            )
        } else {
            MouseScrollDelta::LineDelta(e.delta.0, e.delta.1)
        };
        Some((delta, e.phase))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    #[test]
    fn a_recording_plays_back_as_received() {
        let start = Instant::now();
        let mut tape = Tape::default();
        tape.record();
        let ms = |n: u64| start + Duration::from_millis(n);
        tape.received(
            ms(5),
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0., -5.)),
            2.,
            TouchPhase::Started,
        );
        tape.received(
            ms(13),
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(1., -13.5)),
            2.,
            TouchPhase::Moved,
        );
        tape.received(
            ms(30),
            MouseScrollDelta::LineDelta(0., -1.),
            2.,
            TouchPhase::Moved,
        );
        let text = tape.stop().unwrap();
        assert_eq!(
            text,
            "# zgx wheel tape 1\n0.000 pixel 0 -2.5 started\n8.000 pixel 0.5 -6.75 moved\n25.000 lines 0 -1 moved\n"
        );
        let entries = parse(&text).unwrap();
        tape.play(start, entries, (10., 20.));
        assert_eq!(tape.deadline(), Some(start));
        // At another scale: the same points.
        assert_eq!(
            tape.due(start, 1.),
            Some((
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0., -2.5)),
                TouchPhase::Started
            ))
        );
        assert_eq!(tape.due(ms(7), 1.), None, "not due yet");
        assert_eq!(
            tape.due(ms(8), 1.),
            Some((
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.5, -6.75)),
                TouchPhase::Moved
            ))
        );
        assert_eq!(
            tape.due(ms(40), 1.),
            Some((MouseScrollDelta::LineDelta(0., -1.), TouchPhase::Moved))
        );
        assert_eq!(tape.deadline(), None, "played");
        assert_eq!(tape.stop(), Err("not recording".into()));
    }

    #[test]
    fn a_bad_tape_says_where() {
        assert!(parse("").unwrap_err().contains("no events"));
        assert!(
            parse("0 pixel 1 2 started\nx")
                .unwrap_err()
                .contains("line 2")
        );
        assert!(
            parse("5 pixel 0 1 moved\n2 pixel 0 1 moved")
                .unwrap_err()
                .contains("back")
        );
        assert!(parse("0 pixel 0 NaN moved").is_err());
        assert!(parse("0 drag 0 1 moved").is_err());
    }
}
