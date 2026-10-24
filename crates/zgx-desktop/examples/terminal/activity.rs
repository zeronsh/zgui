//! What a session's program is doing, as the session server sees it: from
//! everything it prints, whether a window shows it or not. The agents
//! sidebar lists every session's activity, so it knows about agents in tabs
//! the user has not opened (and while the window is closed).
//!
//! A program that reports its status (the Program Status Protocol, OSC
//! 7501) is taken at its word. For one that does not (Claude Code and Codex
//! today) it is worked out from what it shows: a progress report (OSC 9;4),
//! the spinner Claude Code and Codex turn before their title while they
//! work, and the notifications (OSC 9, 777) and bell with which they ask for
//! the user or say they are done.
use zgx::terminal::{ProgramState, ProgramStatus};

/// What a session's program last said to get the user's attention.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub text: String,
    pub at_ms: u64,
    /// It asks to do something (needs permission), rather than reporting.
    pub permission: bool,
}

/// One session's activity, kept by its reader as output arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {
    working: bool,
    /// When `working` last changed (Unix ms).
    since_ms: u64,
    /// It finished working and nobody has looked since.
    done: bool,
    /// When the user last looked at it (a window focused its pane).
    seen_ms: u64,
    note: Option<Note>,
    /// The last progress report (OSC 9;4): busy, and whether it failed.
    progress: Option<bool>,
}

/// A session's status for the sidebar: the protocol's words, what for (a
/// blocked program's `kind`), and since when (Unix ms).
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub state: &'static str,
    pub kind: Option<String>,
    pub since_ms: u64,
}

impl Activity {
    pub fn new(now_ms: u64) -> Self {
        Self {
            since_ms: now_ms,
            ..Default::default()
        }
    }
    /// After output: what the title (and a progress report) say about work.
    /// Returns whether anything changed.
    pub fn observe(&mut self, title: Option<&[u8]>, now_ms: u64) -> bool {
        let working = self.progress == Some(false) || title.and_then(title_working) == Some(true);
        if working == self.working {
            return false;
        }
        self.working = working;
        self.since_ms = now_ms;
        // Finishing makes it done until someone looks; starting again
        // answers what it said before.
        self.done = !working;
        if working {
            self.note = None;
        }
        true
    }
    /// A progress report (OSC 9;4 `state`; `value`): 0 ends it, 2 is an
    /// error, 1, 3 and 4 are work under way (set, indeterminate, paused).
    pub fn progress(&mut self, state: u8) -> bool {
        let progress = match state {
            0 => None,
            2 => Some(true),
            _ => Some(false),
        };
        let changed = self.progress != progress;
        self.progress = progress;
        changed
    }
    /// A desktop notification (`text`) or a bell (`"bell"`).
    pub fn note(&mut self, text: &str, now_ms: u64) -> bool {
        let lower = text.to_lowercase();
        let permission = ["permission", "approv", "allow", "confirm", "needs your"]
            .iter()
            .any(|word| lower.contains(word));
        self.note = Some(Note {
            text: text.to_owned(),
            at_ms: now_ms,
            permission,
        });
        true
    }
    /// The user looked (a window focused its pane): what it finished and
    /// said is answered.
    pub fn seen(&mut self, now_ms: u64) -> bool {
        let changed = self.done || self.note.is_some();
        self.done = false;
        self.note = None;
        self.seen_ms = now_ms;
        changed
    }
    pub fn note_text(&self) -> Option<&str> {
        self.note.as_ref().map(|note| note.text.as_str())
    }
    /// Its status: a report wins, then a failed progress report, a request
    /// for permission, work, something finished or said, and rest.
    pub fn status(&self, reported: Option<&ProgramStatus>) -> Status {
        let status = |state: &'static str, kind: Option<String>, since_ms: u64| Status {
            state,
            kind,
            since_ms,
        };
        if let Some(reported) = reported {
            if reported.child_blocked && reported.state != ProgramState::Error {
                return status("blocked", reported.kind.clone(), reported.since_ms);
            }
            // "done" means unseen: once someone has looked, it rests.
            if reported.state == ProgramState::Done && self.seen_ms >= reported.since_ms {
                return status("idle", None, reported.since_ms);
            }
            return status(
                reported.state.name(),
                reported.kind.clone(),
                reported.since_ms,
            );
        }
        if self.progress == Some(true) {
            return status("error", None, self.since_ms);
        }
        if let Some(note) = self.note.as_ref().filter(|note| note.permission) {
            return status("blocked", Some("permission".into()), note.at_ms);
        }
        if self.working {
            return status("working", None, self.since_ms);
        }
        if let Some(note) = &self.note {
            return status("done", None, note.at_ms);
        }
        if self.done {
            return status("done", None, self.since_ms);
        }
        status("idle", None, self.since_ms)
    }
}

/// What a title's leading symbol says: a spinner turns while the program
/// works (Claude Code's, which shows ✳ at rest, or the braille one Codex
/// and others turn). `None` when it has neither.
pub fn title_working(title: &[u8]) -> Option<bool> {
    let title = String::from_utf8_lossy(title);
    let glyph = title.split_whitespace().next()?;
    match glyph {
        "✳" => Some(false),
        "◐" | "◑" | "✶" | "✻" | "✽" | "✢" | "·" => Some(true),
        "⠋" | "⠙" | "⠹" | "⠸" | "⠼" | "⠴" | "⠦" | "⠧" | "⠇" | "⠏" => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spinner_in_the_title_is_work_and_its_end_is_done_until_seen() {
        let mut a = Activity::new(0);
        assert_eq!(a.status(None).state, "idle");
        assert!(a.observe(Some("◐ Fix the build".as_bytes()), 10));
        assert_eq!(
            a.status(None),
            Status {
                state: "working",
                kind: None,
                since_ms: 10
            }
        );
        // The spinner turning is still work: nothing changes.
        assert!(!a.observe(Some("◑ Fix the build".as_bytes()), 20));
        assert!(a.observe(Some("✳ Fix the build".as_bytes()), 30));
        assert_eq!(
            a.status(None),
            Status {
                state: "done",
                kind: None,
                since_ms: 30
            }
        );
        assert!(a.seen(40));
        assert_eq!(a.status(None).state, "idle");
        // A title without a spinner says nothing about work.
        assert!(!a.observe(Some(b"vim notes.md"), 50));
        assert!(a.observe(Some("⠋ Writing tests".as_bytes()), 60));
        assert_eq!(a.status(None).state, "working");
    }

    #[test]
    fn notes_ask_for_the_user_or_report_and_work_answers_them() {
        let mut a = Activity::new(0);
        a.note("Claude needs your permission to use Bash", 5);
        assert_eq!(
            a.status(None),
            Status {
                state: "blocked",
                kind: Some("permission".into()),
                since_ms: 5
            }
        );
        assert!(a.observe(Some("◐ Running".as_bytes()), 6));
        assert_eq!(a.note_text(), None, "starting again answers it");
        a.observe(Some("✳ Running".as_bytes()), 7);
        a.note("bell", 8);
        assert_eq!(
            a.status(None),
            Status {
                state: "done",
                kind: None,
                since_ms: 8
            }
        );
        a.seen(9);
        assert_eq!(a.status(None).state, "idle");
    }

    #[test]
    fn progress_and_reports_take_precedence() {
        let mut a = Activity::new(0);
        assert!(a.progress(1));
        a.observe(None, 1);
        assert_eq!(a.status(None).state, "working");
        assert!(a.progress(2));
        assert_eq!(a.status(None).state, "error");
        // A program's own report wins, and its "done" rests once seen.
        let reported = ProgramStatus {
            state: ProgramState::Done,
            since_ms: 100,
            ..Default::default()
        };
        assert_eq!(a.status(Some(&reported)).state, "done");
        a.seen(150);
        assert_eq!(a.status(Some(&reported)).state, "idle");
        let blocked = ProgramStatus {
            state: ProgramState::Working,
            child_blocked: true,
            kind: Some("question".into()),
            since_ms: 200,
            ..Default::default()
        };
        assert_eq!(
            a.status(Some(&blocked)),
            Status {
                state: "blocked",
                kind: Some("question".into()),
                since_ms: 200
            }
        );
    }
}
