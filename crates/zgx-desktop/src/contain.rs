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

/// `run` for a frame: an error it returns skips it just as a panic does
/// (`None`), but for a lost device, which the host answers with a new
/// renderer. Its panics go to `report` (the banner); its errors are the
/// renderer going on with less (`queue_issue`, `IssueKind::Degraded`),
/// each said once.
pub(crate) fn run_frame<T>(
    fault: &mut Option<Fault>,
    revision: u64,
    now: Instant,
    what: &str,
    mut report: impl FnMut(&str),
    piece: impl FnOnce() -> Result<T, zgx_gpu::GpuError>,
) -> Option<Result<T, zgx_gpu::GpuError>> {
    let said = fault.as_ref().map(|last| last.message.clone());
    let done = run(fault, revision, now, what, &mut report, piece);
    match done {
        Some(Err(error)) if !error.is_device_lost() => {
            *fault = said.map(|message| Fault {
                at: now,
                revision,
                message,
            });
            failed(fault, revision, now, what, &error);
            None
        }
        done => done,
    }
}

/// `error` ended `what`: a degradation (`queue_issue`) unless it is the
/// last one said, which `fault` then holds. `true`: it is news.
pub(crate) fn failed(
    fault: &mut Option<Fault>,
    revision: u64,
    now: Instant,
    what: &str,
    error: &zgx_gpu::GpuError,
) -> bool {
    let message = error.to_string();
    let news = fault.as_ref().is_none_or(|last| last.message != message);
    if news {
        queue_issue(format!("{what} was skipped: {message}"));
    }
    *fault = Some(Fault {
        at: now,
        revision,
        message,
    });
    news
}

thread_local! {
    /// What was contained where no window host is at hand (a popup's
    /// frame, drawn by its host; layout that didn't settle, heard as it
    /// prepares), for the window's banner (`take_reported`).
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
thread_local! {
    /// What a popup's renderer went on without, for the window that has a
    /// runtime to tell (`report_issues`).
    static ISSUES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// Keep `issue` of a frame drawn where no `Runtime` is at hand.
pub(crate) fn queue_issue(issue: String) {
    ISSUES.with(|issues| issues.borrow_mut().push(issue));
}
/// Tell `runtime`'s listeners what `queue_issue` kept, as what the
/// renderer went on without (`IssueKind::Degraded`). Its window takes them
/// as it draws, before another window can.
pub(crate) fn report_issues(runtime: &zgx::reactive::Runtime) {
    for issue in ISSUES.with(|issues| std::mem::take(&mut *issues.borrow_mut())) {
        zgx::issues::report_kind(runtime, zgx::issues::IssueKind::Degraded, &issue);
    }
}

/// What a window's listener heard (`window_issues`), for its banner.
pub(crate) type WindowIssues =
    std::rc::Rc<std::cell::RefCell<Vec<(zgx::issues::IssueKind, String)>>>;
/// Listen to a window's `runtime`: each issue logged once, and kept for
/// that window's banner alone (another window's never shows there).
pub(crate) fn window_issues(
    runtime: &zgx::reactive::Runtime,
) -> (zgx::issues::IssueListener, WindowIssues) {
    let issues = WindowIssues::default();
    let kept = issues.clone();
    let listener = zgx::issues::on_issue_kind(runtime, move |kind, issue| {
        match kind {
            zgx::issues::IssueKind::Degraded => {
                if logged_first(issue) {
                    eprintln!("zgx: degraded: {issue}");
                }
            }
            _ => eprintln!("zgx: {issue}"),
        }
        kept.borrow_mut().push((kind, issue.to_string()));
    });
    (listener, issues)
}
thread_local! {
    /// Degradations logged so far, by any window (`logged_first`).
    static LOGGED: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::default();
}
/// Whether `issue` is a degradation no window has logged yet: two windows
/// missing the same budget log it once, not once each.
fn logged_first(issue: &str) -> bool {
    LOGGED.with(|logged| logged.borrow_mut().insert(issue.to_string()))
}
/// What a window's banner says of what it heard since the last call: its
/// errors, and what the renderer went on without in `debug` builds alone.
pub(crate) fn banner(issues: &WindowIssues, debug: bool) -> Vec<String> {
    let heard = std::mem::take(&mut *issues.borrow_mut());
    heard
        .into_iter()
        .filter(|(kind, _)| *kind != zgx::issues::IssueKind::Degraded || debug)
        .map(|(_, message)| message)
        .collect()
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

    /// A frame's error skips it as a panic does, reported once, and it is
    /// tried again once what it draws changes, a second on.
    #[test]
    fn a_failed_frame_is_reported_once_and_tried_again() {
        let (start, mut fault) = (Instant::now(), None);
        let (banners, runs) = (Cell::new(0), Cell::new(0));
        for frame in 0..120u64 {
            let done = run_frame(
                &mut fault,
                frame,
                start + Duration::from_millis(frame * 1000 / 60),
                "the window's frame",
                |_| banners.set(banners.get() + 1),
                || {
                    runs.set(runs.get() + 1);
                    Err::<(), _>(zgx_gpu::GpuError("frame geometry".into()))
                },
            );
            assert!(done.is_none());
        }
        assert_eq!(runs.get(), 2, "the first frame, then a second on");
        assert_eq!(banners.get(), 0, "not a panic: no banner of its own");
        assert_eq!(
            degradations(),
            ["the window's frame was skipped: frame geometry"]
        );
    }

    /// A popup's issue reaches the runtime's listeners, and only once.
    #[test]
    fn a_queued_issue_reaches_the_listeners_once() {
        let runtime = zgx::reactive::Runtime::new();
        let heard = std::rc::Rc::new(Cell::new(0));
        let hears = heard.clone();
        let _listener = zgx::issues::on_issue(&runtime, move |issue| {
            assert_eq!(issue, "a layer went coarser");
            hears.set(hears.get() + 1);
        });
        queue_issue("a layer went coarser".into());
        report_issues(&runtime);
        report_issues(&runtime);
        assert_eq!(heard.get(), 1);
    }

    /// A frame's error past `run_frame` (a present, a plane) is news once.
    #[test]
    fn a_repeated_error_is_news_once() {
        let (now, mut fault) = (Instant::now(), None);
        let error = zgx_gpu::GpuError("surface acquisition: Timeout".into());
        let fail = |fault: &mut Option<Fault>| failed(fault, 1, now, "the window's frame", &error);
        assert!(fail(&mut fault));
        assert!(!fail(&mut fault));
        assert_eq!(degradations().len(), 1);
    }

    /// A budget miss drawing window A: logged, heard by A's listeners as a
    /// degradation, in A's banner in debug builds alone, and never in B's.
    /// An error (a script's) is in A's banner either way.
    #[test]
    fn a_degradation_stays_in_its_window_and_out_of_release_banners() {
        let (a, b) = (zgx::reactive::Runtime::new(), zgx::reactive::Runtime::new());
        let (_a_listener, a_issues) = window_issues(&a);
        let (_b_listener, b_issues) = window_issues(&b);
        let kinds = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let hears = kinds.clone();
        let _app = zgx::issues::on_issue_kind(&a, move |kind, _| hears.borrow_mut().push(kind));
        queue_issue("the images one frame draws exceed the 64 MiB budget".into());
        report_issues(&a);
        assert!(banner(&b_issues, true).is_empty(), "nothing in B's banner");
        let (debug, release) = (a_issues.clone(), a_issues.clone());
        assert!(
            banner(&release, false).is_empty(),
            "nothing in A's in release"
        );
        queue_issue("a layer went coarser".into());
        report_issues(&a);
        assert_eq!(
            banner(&debug, true),
            ["a layer went coarser"],
            "A's, in debug"
        );
        zgx::issues::report(&a, "a script failed");
        assert_eq!(banner(&a_issues, false), ["a script failed"]);
        use zgx::issues::IssueKind::{Degraded, Error};
        assert_eq!(
            *kinds.borrow(),
            [Degraded, Degraded, Error],
            "apps hear each, by kind"
        );
    }

    /// The same degradation in two windows is logged once in the process;
    /// another is logged too.
    #[test]
    fn a_degradation_is_logged_once_however_many_windows_have_it() {
        assert!(logged_first("a layer went coarser in two windows"));
        assert!(!logged_first("a layer went coarser in two windows"));
        assert!(logged_first("an image drawn softer"));
    }

    /// What the queue holds, as degradations heard by a runtime.
    fn degradations() -> Vec<String> {
        let runtime = zgx::reactive::Runtime::new();
        let heard = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let hears = heard.clone();
        let _listener = zgx::issues::on_issue_kind(&runtime, move |kind, issue| {
            assert_eq!(kind, zgx::issues::IssueKind::Degraded);
            hears.borrow_mut().push(issue.to_string());
        });
        report_issues(&runtime);
        heard.take()
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
