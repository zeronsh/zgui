//! A piece of a frame run apart from the rest (a window's render, a popup's,
//! the popup host's update): a panic in it is reported once and the piece
//! is skipped until what it draws changes, and then tried again at most
//! once a second. A panic that would repeat every frame (#950: a popup's
//! fade bands, every frame of a tween) costs one report and that piece's
//! frames, not the app: the turn's own containment (`WindowManager::
//! contain`) would end it after 20 in a second.
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

/// How soon a faulted piece may be tried again, once what it draws changed.
const RETRY: Duration = Duration::from_secs(1);

/// What a piece's last panic left: when, at what revision of what it draws,
/// and its message (reported once).
#[derive(Debug)]
pub(crate) struct Fault {
    at: Instant,
    revision: u64,
    message: String,
}

/// Run `piece` (named `what` in the report) unless it panicked at this
/// `revision` of what it draws, or less than a second ago: `None` where it
/// panicked or was skipped. A first panic, or one with another message, is
/// reported (stderr, and `report`, the window's banner); a repeat is not.
pub(crate) fn run<R>(
    fault: &mut Option<Fault>,
    revision: u64,
    now: Instant,
    what: &str,
    report: impl FnOnce(&str),
    piece: impl FnOnce() -> R,
) -> Option<R> {
    if let Some(last) = fault
        && (last.revision == revision || now.duration_since(last.at) < RETRY)
    {
        return None;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        #[cfg(debug_assertions)]
        if injected(what) {
            panic!("ZGX_PANIC_IN: {what}");
        }
        piece()
    })) {
        Ok(done) => {
            *fault = None;
            Some(done)
        }
        Err(payload) => {
            let message = message(payload.as_ref());
            if fault.as_ref().is_none_or(|last| last.message != message) {
                eprintln!(
                    "zgx: contained a panic in {what} ({message}); it is skipped until it \
                     changes, and the app keeps running"
                );
                report(&message);
            }
            *fault = Some(Fault {
                at: now,
                revision,
                message,
            });
            None
        }
    }
}

thread_local! {
    /// Panics contained where no window host is at hand (a popup's frame,
    /// drawn by its host), for the window's banner (`take_reported`).
    static REPORTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// Keep `message` for the window's banner (`run`'s `report` where only the
/// window host can show it).
pub(crate) fn queue(message: &str) {
    REPORTED.with(|reported| reported.borrow_mut().push(message.to_string()));
}
/// What `queue` kept since the last take.
pub(crate) fn take_reported() -> Vec<String> {
    REPORTED.with(|reported| std::mem::take(&mut *reported.borrow_mut()))
}

/// Debug builds: whether `ZGX_PANIC_IN` names `what` (`"a popup's frame"`,
/// `"the window's frame"`, `"the popups' update"`), which then panics each
/// time it runs: what `scripts/popup_windows_smoke.py` contains.
#[cfg(debug_assertions)]
fn injected(what: &str) -> bool {
    thread_local! {
        static IN: Option<String> = zgx::env::var("ZGX_PANIC_IN").ok();
    }
    IN.with(|piece| piece.as_deref() == Some(what))
}

/// A panic's message, as the panic hook printed it.
pub(crate) fn message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A piece that panics every frame is run once per change at most once
    /// a second, reported once, and the caller goes on each frame.
    #[test]
    fn a_piece_panicking_every_frame_is_reported_once_and_skipped() {
        let (start, mut fault) = (Instant::now(), None);
        let (runs, reports, frames) = (Cell::new(0), Cell::new(0), Cell::new(0));
        // 120 frames over two seconds, what it draws changing every frame.
        for frame in 0..120u64 {
            let now = start + Duration::from_millis(frame * 1000 / 60);
            let done = run(
                &mut fault,
                frame,
                now,
                "a popup's frame",
                |_| reports.set(reports.get() + 1),
                || {
                    runs.set(runs.get() + 1);
                    panic!("min > max");
                },
            );
            assert_eq!(done, None::<()>);
            frames.set(frames.get() + 1);
        }
        assert_eq!(frames.get(), 120, "every frame goes on");
        assert_eq!(
            runs.get(),
            2,
            "tried at the first frame, then once a second later"
        );
        assert_eq!(reports.get(), 1, "the same panic is reported once");
    }

    /// Unchanged, a faulted piece isn't tried again, however long; changed,
    /// it is, and one that works again runs every frame from then on.
    #[test]
    fn a_faulted_piece_runs_again_once_what_it_draws_changes() {
        let (start, mut fault) = (Instant::now(), None);
        let broken = Cell::new(true);
        let runs = Cell::new(0);
        let mut frame = |revision: u64, seconds: u64| {
            run(
                &mut fault,
                revision,
                start + Duration::from_secs(seconds),
                "the window's frame",
                |_| {},
                || {
                    runs.set(runs.get() + 1);
                    assert!(!broken.get(), "a bad box");
                },
            )
        };
        assert_eq!(frame(1, 0), None);
        assert_eq!(frame(1, 5), None, "unchanged: not tried again");
        assert_eq!(runs.get(), 1);
        broken.set(false);
        assert_eq!(frame(2, 6), Some(()), "changed: tried, and it works");
        assert_eq!(frame(2, 6), Some(()), "working: runs every frame");
        assert_eq!(runs.get(), 3);
    }

    /// Another panic in the same piece is reported too.
    #[test]
    fn a_different_panic_is_reported_again() {
        let (start, mut fault) = (Instant::now(), None);
        let reported = std::cell::RefCell::new(Vec::new());
        for (second, message) in [(0, "first"), (2, "first"), (4, "second")] {
            run(
                &mut fault,
                second,
                start + Duration::from_secs(second),
                "a popup's frame",
                |m| reported.borrow_mut().push(m.to_string()),
                || panic!("{message}"),
            );
        }
        assert_eq!(*reported.borrow(), ["first", "second"]);
    }
}
