//! What a running app holds, by what holds it (`zgx inspect memory`). Each
//! holder adds its lines only when asked: nothing here is kept or counted
//! between reports, so an app that never asks pays nothing.
//!
//! The window's host holds what the interface can't reach (its renderer, the
//! process's own figures): the report's reader awaits [`from_host`], and the
//! host answers once the task that asked has run ([`asked`], [`answer`]).
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    task::{Poll, Waker},
};

/// One holder of memory: entries, bytes, or both, as it can tell.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// What it is part of: `process`, `luau`, `images`, `text`, `gpu`…
    pub area: &'static str,
    /// What it is within that.
    pub name: String,
    /// How many it holds.
    pub entries: Option<usize>,
    /// The bytes it holds.
    pub bytes: Option<usize>,
}

/// Every holder's lines, in the order they were added.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The lines.
    pub lines: Vec<Line>,
}
impl Report {
    /// Adds a line.
    pub fn add(
        &mut self,
        area: &'static str,
        name: impl Into<String>,
        entries: Option<usize>,
        bytes: Option<usize>,
    ) {
        self.lines.push(Line {
            area,
            name: name.into(),
            entries,
            bytes,
        });
    }
}

thread_local! {
    static HOSTED: Cell<bool> = const { Cell::new(false) };
    static ASKED: Cell<bool> = const { Cell::new(false) };
    static WAITING: RefCell<Option<Waker>> = const { RefCell::new(None) };
    static ANSWER: RefCell<Option<Report>> = const { RefCell::new(None) };
}

/// This thread's windows have a host that answers [`from_host`].
pub fn set_hosted() {
    HOSTED.set(true);
}
/// Whether a report waits for the host's lines: it answers ([`answer`]).
#[inline]
pub fn asked() -> bool {
    ASKED.get()
}
/// The host's lines, for [`from_host`].
pub fn answer(report: Report) {
    ASKED.set(false);
    ANSWER.set(Some(report));
    if let Some(waker) = WAITING.take() {
        waker.wake();
    }
}
/// The host's lines, once it has answered; none without a host (`TestApp`).
pub async fn from_host() -> Report {
    if !HOSTED.get() {
        return Report::default();
    }
    ASKED.set(true);
    poll_fn(|context| match ANSWER.take() {
        Some(report) => Poll::Ready(report),
        None => {
            WAITING.set(Some(context.waker().clone()));
            Poll::Pending
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_answers_what_a_report_waits_for() {
        use std::{future::Future, pin::pin, task::Context};
        // Without a host there is nothing to wait for.
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(
            pin!(from_host()).poll(&mut context),
            Poll::Ready(Report::default())
        );
        assert!(!asked());
        set_hosted();
        let mut report = pin!(from_host());
        assert!(report.as_mut().poll(&mut context).is_pending());
        assert!(asked());
        let mut lines = Report::default();
        lines.add("process", "footprint", None, Some(1));
        answer(lines.clone());
        assert!(!asked());
        assert_eq!(report.poll(&mut context), Poll::Ready(lines));
    }
}
