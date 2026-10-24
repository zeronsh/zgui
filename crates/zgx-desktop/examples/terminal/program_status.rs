//! The Program Status Protocol (OSC 7501, revision 0.4, as Rex defines it:
//! https://www.superlogical.com/rex/docs/build/program-status): programs
//! report what they are doing as `OSC 7501 ; key=value:key=value ST`, and the
//! terminal keeps one record per `id` (none: the root). The session server
//! keeps a session's records and reports its root with the session's process.
use std::collections::HashMap;
use zgx::terminal::{ProgramState, ProgramStatus};

/// Records per terminal at most; past it the least recently updated goes.
const RECORDS: usize = 256;

/// One report's keys, as the protocol keeps them per record.
#[derive(Clone, Debug, PartialEq)]
struct Record {
    state: ProgramState,
    kind: Option<String>,
    progress: Option<u8>,
    app: Option<String>,
    title: Option<String>,
    msg: Option<String>,
    /// For dropping `working` and `blocked` when the reporting program
    /// leaves the foreground (its process group, as the terminal saw it).
    group: i32,
    /// Update order, for the least-recently-updated rule.
    touched: u64,
}

/// A terminal's records.
#[derive(Default)]
pub struct Records {
    records: HashMap<String, Record>,
    clock: u64,
    /// When the root last changed state (Unix ms).
    since_ms: u64,
}

/// What a feature query (`OSC 7501 ; ?`) is answered with.
pub const QUERY_REPLY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";

impl Records {
    /// An OSC 7501 payload (what follows `7501;`) from a program in process
    /// group `group`. A query answers with [`QUERY_REPLY`] (returns true).
    pub fn report(&mut self, payload: &[u8], group: i32, now_ms: u64) -> bool {
        if payload.len() > 4096 - 8 {
            return false;
        }
        let text = String::from_utf8_lossy(payload);
        if text.trim() == "?" {
            return true;
        }
        let mut keys: HashMap<&str, &str> = HashMap::new();
        for pair in text.split(':') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let key_ok =
                !key.is_empty() && key.len() <= 16 && key.bytes().all(|b| b.is_ascii_lowercase());
            let value_ok = value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b));
            if key_ok && value_ok {
                keys.insert(key, value);
            }
        }
        let Some(state) = keys.get("state") else {
            return false;
        };
        let id = keys.get("id").copied().unwrap_or("");
        if !valid_id(id) {
            return false;
        }
        if *state == "clear" {
            self.records.retain(|other, _| {
                !(other == id || id.is_empty() || other.starts_with(&format!("{id}/")))
            });
            if id.is_empty() {
                self.since_ms = now_ms;
            }
            return false;
        }
        let Some(state) = ProgramState::parse(state) else {
            return false;
        };
        let text_of =
            |key: &str, encoded_max: usize, decoded_max: usize| -> Result<Option<String>, ()> {
                let Some(value) = keys.get(key) else {
                    return Ok(None);
                };
                if value.len() > encoded_max {
                    return Err(());
                }
                let bytes = decode_base64(value).ok_or(())?;
                let text = String::from_utf8(bytes).map_err(|_| ())?;
                if text.len() > decoded_max
                    || text.chars().any(|c| {
                        let c = c as u32;
                        c < 0x20 || c == 0x7f || (0x80..=0x9f).contains(&c)
                    })
                {
                    return Err(());
                }
                Ok(Some(text))
            };
        let (Ok(title), Ok(msg)) = (text_of("title", 256, 192), text_of("msg", 2732, 2048)) else {
            return false;
        };
        let app = match keys.get("app") {
            Some(app)
                if app.len() <= 32
                    && !app.is_empty()
                    && app
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b)) =>
            {
                Some(app.to_string())
            }
            Some(_) => return false,
            None => None,
        };
        let busy = matches!(state, ProgramState::Working | ProgramState::Blocked);
        let progress = keys
            .get("progress")
            .and_then(|p| p.parse::<u8>().ok())
            .filter(|p| *p <= 100 && busy);
        let kind = keys
            .get("kind")
            .filter(|k| {
                state == ProgramState::Blocked && ["permission", "question", "auth"].contains(k)
            })
            .map(|k| k.to_string());
        self.clock += 1;
        if id.is_empty() && self.records.get("").map(|r| r.state) != Some(state) {
            self.since_ms = now_ms;
        }
        self.records.insert(
            id.to_string(),
            Record {
                state,
                kind,
                progress,
                app,
                title,
                msg,
                group,
                touched: self.clock,
            },
        );
        if self.records.len() > RECORDS
            && let Some(oldest) = self
                .records
                .iter()
                .min_by_key(|(_, record)| record.touched)
                .map(|(id, _)| id.clone())
        {
            self.records.remove(&oldest);
        }
        false
    }
    /// A new shell prompt began (OSC 133 `A`): `working` and `blocked`
    /// records go.
    pub fn prompt(&mut self, now_ms: u64) {
        self.drop_busy(|_| true, now_ms);
    }
    /// The foreground is now process group `group`: records of programs no
    /// longer in front lose `working` and `blocked`, as when they exit.
    pub fn foreground(&mut self, group: i32, now_ms: u64) {
        self.drop_busy(|record| record.group != group, now_ms);
    }
    fn drop_busy(&mut self, gone: impl Fn(&Record) -> bool, now_ms: u64) {
        let root_before = self.records.get("").map(|r| r.state);
        self.records.retain(|_, record| {
            !(matches!(record.state, ProgramState::Working | ProgramState::Blocked) && gone(record))
        });
        if self.records.get("").map(|r| r.state) != root_before {
            self.since_ms = now_ms;
        }
    }
    /// The root record, and whether a record under it is blocked.
    pub fn status(&self) -> Option<ProgramStatus> {
        let root = self.records.get("");
        let child_blocked = self
            .records
            .iter()
            .any(|(id, record)| !id.is_empty() && record.state == ProgramState::Blocked);
        // Children alone (a root never reported) still count: a worker blocked.
        if root.is_none() && !child_blocked {
            return None;
        }
        let app = root.and_then(|r| r.app.clone()).or_else(|| {
            // Records inherit the nearest ancestor's app; the root has none.
            self.records.values().find_map(|r| r.app.clone())
        });
        Some(ProgramStatus {
            state: root.map_or(ProgramState::Blocked, |r| r.state),
            kind: root.and_then(|r| r.kind.clone()),
            progress: root.and_then(|r| r.progress),
            app,
            title: root.and_then(|r| r.title.clone()),
            msg: root.and_then(|r| r.msg.clone()),
            child_blocked,
            since_ms: self.since_ms,
        })
    }
}

fn valid_id(id: &str) -> bool {
    if id.is_empty() {
        return true;
    }
    id.len() <= 128
        && id.split('/').count() <= 8
        && id.split('/').all(|segment| {
            (1..=32).contains(&segment.len())
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b))
        })
}

/// Standard base64, padding optional.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in text.bytes().filter(|b| *b != b'=') {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_replace_the_root_and_decode_text() {
        let mut records = Records::default();
        // "Apply 3 to add, 1 to change, 0 to destroy?", from the spec.
        let report = b"state=blocked:kind=permission:app=terraform:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/";
        assert!(!records.report(report, 7, 1000));
        let status = records.status().unwrap();
        assert_eq!(status.state, ProgramState::Blocked);
        assert_eq!(status.kind.as_deref(), Some("permission"));
        assert_eq!(status.app.as_deref(), Some("terraform"));
        assert_eq!(
            status.msg.as_deref(),
            Some("Apply 3 to add, 1 to change, 0 to destroy?")
        );
        assert_eq!(status.since_ms, 1000);
        // A report replaces the record whole: no msg now.
        records.report(b"state=working:progress=40", 7, 2000);
        let status = records.status().unwrap();
        assert_eq!(
            (status.state, status.progress, status.msg),
            (ProgramState::Working, Some(40), None)
        );
    }

    #[test]
    fn invalid_reports_are_ignored_and_queries_answered() {
        let mut records = Records::default();
        assert!(records.report(b"?", 1, 0));
        records.report(b"state=sleeping", 1, 0);
        records.report(b"state=working:id=a//b", 1, 0);
        // A control character in decoded text discards the report.
        records.report(b"state=working:msg=YQpi", 1, 0);
        assert_eq!(records.status(), None);
        // Malformed pairs are skipped, the rest kept; progress needs a busy state.
        records.report(b"state=done:junk:progress=50", 1, 0);
        let status = records.status().unwrap();
        assert_eq!((status.state, status.progress), (ProgramState::Done, None));
    }

    #[test]
    fn prompts_and_exits_drop_busy_records_but_keep_done() {
        let mut records = Records::default();
        records.report(b"state=working", 5, 0);
        records.report(b"state=blocked:id=worker:kind=question", 5, 0);
        assert!(records.status().unwrap().child_blocked);
        // The program left the foreground (the shell, group 2, is back).
        records.foreground(2, 10);
        assert_eq!(records.status(), None);
        records.report(b"state=done", 5, 20);
        records.prompt(30);
        assert_eq!(records.status().unwrap().state, ProgramState::Done);
        records.report(b"state=clear", 5, 40);
        assert_eq!(records.status(), None);
    }
}
