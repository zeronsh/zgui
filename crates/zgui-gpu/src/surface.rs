//! Acquisition policy shared by the native renderer and fault-injection tests.
use crate::GpuError;

/// Result of presenting the retained target to a native window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationStatus {
    /// Presented.
    Presented,
    /// Retry on a later event-loop turn; the rendered target remains valid.
    /// Also returned when surface changes exhaust this turn's recovery budget.
    Timeout,
    /// Wait for visibility before retrying to avoid spinning a hidden window.
    Occluded,
    /// This renderer has no native surface.
    Offscreen,
}

pub(crate) enum Acquisition<T> {
    Frame(T),
    Timeout,
    Occluded,
    Lost,
    Outdated,
}
pub(crate) enum Acquired<T> {
    Frame(T),
    Skipped(PresentationStatus),
}
pub(crate) trait SurfaceSource {
    type Frame;
    fn acquire(&mut self) -> Result<Acquisition<Self::Frame>, GpuError>;
    fn recreate(&mut self) -> Result<(), GpuError>;
    fn configure(&mut self);
}

pub(crate) fn acquire<S: SurfaceSource>(source: &mut S) -> Result<Acquired<S::Frame>, GpuError> {
    for attempt in 0..2 {
        match source.acquire()? {
            Acquisition::Frame(frame) => return Ok(Acquired::Frame(frame)),
            Acquisition::Timeout => return Ok(Acquired::Skipped(PresentationStatus::Timeout)),
            Acquisition::Occluded => return Ok(Acquired::Skipped(PresentationStatus::Occluded)),
            Acquisition::Lost | Acquisition::Outdated if attempt == 1 => break,
            Acquisition::Lost => {
                source.recreate()?;
                source.configure();
            }
            Acquisition::Outdated => source.configure(),
        }
    }
    // Another resize or surface change can race the recovery above. Keep the
    // retained frame and yield to the host's bounded retry scheduler instead of
    // turning another recoverable acquisition result into an application error.
    // Recreation/validation errors still propagate through the `?` paths above.
    Ok(Acquired::Skipped(PresentationStatus::Timeout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Fake {
        outcomes: VecDeque<Result<Acquisition<u32>, GpuError>>,
        calls: Vec<&'static str>,
        recreate_fails: bool,
    }
    impl Fake {
        fn new(outcomes: impl IntoIterator<Item = Acquisition<u32>>) -> Self {
            Self {
                outcomes: outcomes.into_iter().map(Ok).collect(),
                calls: vec![],
                recreate_fails: false,
            }
        }
    }
    impl SurfaceSource for Fake {
        type Frame = u32;
        fn acquire(&mut self) -> Result<Acquisition<u32>, GpuError> {
            self.calls.push("acquire");
            self.outcomes
                .pop_front()
                .expect("unbounded acquisition retry")
        }
        fn recreate(&mut self) -> Result<(), GpuError> {
            self.calls.push("recreate");
            if self.recreate_fails {
                Err(GpuError("recreate failed".into()))
            } else {
                Ok(())
            }
        }
        fn configure(&mut self) {
            self.calls.push("configure");
        }
    }
    #[test]
    fn lost_recreates_before_configuring_and_acquires_replacement() {
        let mut source = Fake::new([Acquisition::Lost, Acquisition::Frame(17)]);
        assert!(matches!(acquire(&mut source).unwrap(), Acquired::Frame(17)));
        assert_eq!(
            source.calls,
            ["acquire", "recreate", "configure", "acquire"]
        );
    }
    #[test]
    fn outdated_preserves_surface_and_reconfigures() {
        let mut source = Fake::new([Acquisition::Outdated, Acquisition::Frame(19)]);
        assert!(matches!(acquire(&mut source).unwrap(), Acquired::Frame(19)));
        assert_eq!(source.calls, ["acquire", "configure", "acquire"]);
    }
    #[test]
    fn skipped_frames_are_distinct_and_do_not_retry_or_configure() {
        for (outcome, expected) in [
            (Acquisition::Timeout, PresentationStatus::Timeout),
            (Acquisition::Occluded, PresentationStatus::Occluded),
        ] {
            let mut source = Fake::new([outcome]);
            assert!(
                matches!(acquire(&mut source).unwrap(), Acquired::Skipped(status) if status == expected)
            );
            assert_eq!(source.calls, ["acquire"]);
        }
    }
    #[test]
    fn recovered_surface_can_skip_then_succeed_on_a_later_call() {
        let mut source = Fake::new([
            Acquisition::Lost,
            Acquisition::Timeout,
            Acquisition::Frame(23),
        ]);
        assert!(matches!(
            acquire(&mut source).unwrap(),
            Acquired::Skipped(PresentationStatus::Timeout)
        ));
        assert!(matches!(acquire(&mut source).unwrap(), Acquired::Frame(23)));
        assert_eq!(
            source.calls,
            ["acquire", "recreate", "configure", "acquire", "acquire"]
        );
    }
    #[test]
    fn repeated_surface_changes_yield_and_can_recover_on_a_later_turn() {
        for first_lost in [false, true] {
            for second in [Acquisition::Lost, Acquisition::Outdated] {
                let first = if first_lost {
                    Acquisition::Lost
                } else {
                    Acquisition::Outdated
                };
                let mut source = Fake::new([first, second, Acquisition::Frame(29)]);
                assert!(matches!(
                    acquire(&mut source).unwrap(),
                    Acquired::Skipped(PresentationStatus::Timeout)
                ));
                let expected = if first_lost {
                    vec!["acquire", "recreate", "configure", "acquire"]
                } else {
                    vec!["acquire", "configure", "acquire"]
                };
                assert_eq!(source.calls, expected);
                assert!(matches!(acquire(&mut source).unwrap(), Acquired::Frame(29)));
                assert_eq!(source.calls.len(), expected.len() + 1);
            }
        }
    }
    #[test]
    fn persistent_surface_changes_remain_bounded_on_each_host_retry() {
        let mut source = Fake::new((0..8).map(|_| Acquisition::Lost));
        for attempt in 1..=4 {
            assert!(matches!(
                acquire(&mut source).unwrap(),
                Acquired::Skipped(PresentationStatus::Timeout)
            ));
            assert_eq!(source.calls.len(), attempt * 4);
        }
        assert!(source.outcomes.is_empty());
    }
    #[test]
    fn recreation_and_acquisition_errors_propagate_without_retry() {
        let mut source = Fake::new([Acquisition::Lost]);
        source.recreate_fails = true;
        assert!(acquire(&mut source).is_err());
        assert_eq!(source.calls, ["acquire", "recreate"]);
        let mut source = Fake::new([]);
        source
            .outcomes
            .push_back(Err(GpuError("validation failed".into())));
        assert!(acquire(&mut source).is_err());
        assert_eq!(source.calls, ["acquire"]);
    }
}
