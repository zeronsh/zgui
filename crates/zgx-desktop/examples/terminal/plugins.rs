//! zterm's plugins, after MoonLoader's: a folder per plugin in the plugins
//! directory, found when zterm starts (and when the panel palette opens),
//! each running on its own (a sandboxed Luau VM; one that fails is left out).
//! Unlike MoonLoader's scripts, a plugin says up front what it needs, in its
//! manifest, and runs only once the user allows it:
//!
//! ```text
//! plugins/notes/plugin.json   {
//!                               "name": "Notes",
//!                               "version": "1.0.0",
//!                               "author": "Ada",
//!                               "description": "A notepad per pane.",
//!                               "permissions": ["clipboard"],
//!                               "panels": [{ "id": "notes", "title": "Notes",
//!                                            "description": "A notepad" }]
//!                             }
//! plugins/notes/init.luau     ui.contribute("panel:notes", Notes {})
//! ```
//!
//! A panel the plugin declares opens from ⌘K as a pane; the plugin's
//! component, contributed to `panel:<id>`, gets the pane in `props.slot`.
//! What the user decided is kept in `plugins.json` beside the plugins
//! directory: a plugin that asks for more than it was allowed (a new
//! version) asks again.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use zgx_script::{Capability, Plugin, Request, Response, Value};

/// What a plugin may ask for: its name in a manifest, what the user is
/// told, and what it lets the plugin do.
pub const PERMISSIONS: &[(&str, &str)] = &[
    ("network", "Connect to the internet"),
    ("clipboard", "Read and change your clipboard"),
    ("files", "Open and save files you choose"),
    (
        "terminals",
        "See your terminals and what runs in them, and type into them",
    ),
    ("web", "Show web pages"),
];
fn capabilities(permission: &str) -> Vec<Capability> {
    match permission {
        "network" => vec![Capability::Fetch],
        "clipboard" => vec![Capability::Clipboard],
        "files" => vec![Capability::Files],
        "terminals" => vec![Capability::Terminals],
        "web" => vec![Capability::Web],
        _ => vec![],
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: String,
    pub title: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub panels: Vec<Panel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Allowed: it runs.
    Enabled,
    /// Waiting for the user to allow it (new, or asking for more).
    Pending,
    /// The user did not allow it, or turned it off.
    Off,
    /// Its manifest does not read.
    Broken,
}
impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Enabled => "enabled",
            Status::Pending => "pending",
            Status::Off => "off",
            Status::Broken => "broken",
        }
    }
}

/// A plugin found in the plugins directory.
#[derive(Clone, Debug)]
pub struct Found {
    /// Its folder's name: what zterm calls it.
    pub id: String,
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    pub status: Status,
    /// Why it is broken.
    pub error: Option<String>,
}

/// What the user decided about a plugin: whether it may run, and the
/// permissions they allowed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decision {
    pub allowed: bool,
    pub permissions: Vec<String>,
}

fn text(value: &serde_json::Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}
/// A manifest's contents, or what is wrong with it.
pub fn parse(json: &str) -> Result<Manifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("plugin.json: {e}"))?;
    let name = text(&value, "name");
    if name.is_empty() {
        return Err("plugin.json has no name".into());
    }
    let permissions: Vec<String> = value["permissions"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if let Some(unknown) = permissions
        .iter()
        .find(|p| !PERMISSIONS.iter().any(|(known, _)| known == p))
    {
        return Err(format!(
            "plugin.json asks for `{unknown}`, which zterm does not know (it knows {})",
            PERMISSIONS
                .iter()
                .map(|(p, _)| *p)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let panels = value["panels"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|panel| {
                    let id = text(panel, "id");
                    (!id.is_empty()).then(|| Panel {
                        title: Some(text(panel, "title"))
                            .filter(|t| !t.is_empty())
                            .unwrap_or_else(|| id.clone()),
                        description: text(panel, "description"),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        name,
        version: text(&value, "version"),
        author: text(&value, "author"),
        description: text(&value, "description"),
        permissions,
        panels,
    })
}

/// Where plugins live.
pub fn dir() -> PathBuf {
    crate::session::dir().join("plugins")
}
fn decisions_file() -> PathBuf {
    crate::session::dir().join("plugins.json")
}

pub fn read_decisions(path: &Path) -> HashMap<String, Decision> {
    let Some(value) = std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
    else {
        return HashMap::new();
    };
    value
        .as_object()
        .map(|plugins| {
            plugins
                .iter()
                .map(|(id, decision)| {
                    (
                        id.clone(),
                        Decision {
                            allowed: decision["allowed"].as_bool().unwrap_or(false),
                            permissions: decision["permissions"]
                                .as_array()
                                .map(|list| {
                                    list.iter()
                                        .filter_map(|p| p.as_str().map(str::to_owned))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
fn write_decisions(path: &Path, decisions: &HashMap<String, Decision>) {
    let mut out = serde_json::Map::new();
    for (id, decision) in decisions {
        out.insert(
            id.clone(),
            serde_json::json!({ "allowed": decision.allowed, "permissions": decision.permissions }),
        );
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, serde_json::Value::Object(out).to_string()).is_ok() {
        let _ = std::fs::rename(&temp, path);
    }
}

/// Whether a plugin runs, waits for the user or stays off: it runs once
/// allowed everything it asks for.
pub fn status(manifest: &Manifest, decision: Option<&Decision>) -> Status {
    match decision {
        None => Status::Pending,
        Some(decision) if !decision.allowed => Status::Off,
        Some(decision)
            if manifest
                .permissions
                .iter()
                .all(|p| decision.permissions.contains(p)) =>
        {
            Status::Enabled
        }
        // A new version asks for more: ask again.
        Some(_) => Status::Pending,
    }
}

/// Every plugin in `dir`, by name, and how it stands.
pub fn scan(dir: &Path, decisions: &HashMap<String, Decision>) -> Vec<Found> {
    let mut found: Vec<Found> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let id = path.file_name()?.to_str()?.to_owned();
            if !path.is_dir() || id.starts_with('.') {
                return None;
            }
            let manifest = std::fs::read_to_string(path.join("plugin.json"))
                .map_err(|e| format!("plugin.json: {e}"))
                .and_then(|json| parse(&json));
            let entry_missing = !path.join("init.luau").is_file();
            Some(match manifest {
                Ok(_) if entry_missing => Found {
                    id,
                    dir: path,
                    manifest: None,
                    status: Status::Broken,
                    error: Some("it has no init.luau".into()),
                },
                Ok(manifest) => Found {
                    status: status(&manifest, decisions.get(&id)),
                    id,
                    dir: path,
                    manifest: Some(manifest),
                    error: None,
                },
                Err(error) => Found {
                    id,
                    dir: path,
                    manifest: None,
                    status: Status::Broken,
                    error: Some(error),
                },
            })
        })
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// The plugins that run, for `ScriptHost::set_plugins`.
pub fn enabled(found: &[Found]) -> Vec<Plugin> {
    found
        .iter()
        .filter(|plugin| plugin.status == Status::Enabled)
        .filter_map(|plugin| {
            let manifest = plugin.manifest.as_ref()?;
            Some(Plugin {
                name: plugin.id.clone(),
                entry: plugin.dir.join("init.luau"),
                grants: manifest
                    .permissions
                    .iter()
                    .flat_map(|p| capabilities(p))
                    .collect(),
            })
        })
        .collect()
}

/// The plugins as the interface shows them (`plugins`): each with its
/// status, what it asks for (in words) and its panels.
pub fn published(found: &[Found]) -> Value {
    let json: Vec<serde_json::Value> = found
        .iter()
        .map(|plugin| {
            let manifest = plugin.manifest.clone().unwrap_or(Manifest {
                name: plugin.id.clone(),
                version: String::new(),
                author: String::new(),
                description: String::new(),
                permissions: vec![],
                panels: vec![],
            });
            serde_json::json!({
                "id": plugin.id,
                "name": manifest.name,
                "version": manifest.version,
                "author": manifest.author,
                "description": manifest.description,
                "status": plugin.status.name(),
                "error": plugin.error,
                "permissions": manifest.permissions.iter().map(|p| serde_json::json!({
                    "id": p,
                    "text": PERMISSIONS.iter().find(|(known, _)| known == p).map(|(_, text)| *text),
                })).collect::<Vec<_>>(),
                "panels": manifest.panels.iter().map(|panel| serde_json::json!({
                    "id": panel.id,
                    "kind": format!("panel:{}/{}", plugin.id, panel.id),
                    "title": panel.title,
                    "description": panel.description,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Value::from_json(&serde_json::Value::Array(json).to_string()).unwrap_or(Value::Nil)
}

/// The user's answer about plugin `id`: allowed (with what it asks for
/// now) or not.
pub fn decide(id: &str, allowed: bool) {
    let file = decisions_file();
    let mut decisions = read_decisions(&file);
    let asked = std::fs::read_to_string(dir().join(id).join("plugin.json"))
        .ok()
        .and_then(|json| parse(&json).ok())
        .map(|manifest| manifest.permissions)
        .unwrap_or_default();
    decisions.insert(
        id.to_owned(),
        Decision {
            allowed,
            permissions: if allowed { asked } else { vec![] },
        },
    );
    write_decisions(&file, &decisions);
}

/// Every plugin, as of now.
pub fn find() -> Vec<Found> {
    let dir = dir();
    let _ = std::fs::create_dir_all(&dir);
    scan(&dir, &read_decisions(&decisions_file()))
}

/// The `network` permission's transport (`ui.fetch`): the system's curl,
/// over http and https only. Blocking: run it on a thread.
pub fn fetch(request: &Request) -> Result<Response, String> {
    use std::{io::Write, process::Stdio};
    let unsafe_text = |s: &str| s.contains(['\r', '\n']);
    if !request.method.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("`{}` is not an HTTP method", request.method));
    }
    if request
        .headers
        .iter()
        .any(|(name, value)| unsafe_text(name) || unsafe_text(value) || name.contains(':'))
    {
        return Err("a header has a line break or a colon in its name".into());
    }
    let mut curl = std::process::Command::new("/usr/bin/curl");
    curl.args(["-sS", "-i", "--proto", "=http,https", "--max-time", "60"])
        .args(["-X", &request.method]);
    for (name, value) in &request.headers {
        curl.arg("-H").arg(format!("{name}: {value}"));
    }
    if request.body.is_some() {
        curl.args(["--data-binary", "@-"]);
    }
    curl.arg("--url")
        .arg(&request.url)
        .stdin(if request.body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = curl.spawn().map_err(|e| format!("curl: {e}"))?;
    // Fed from its own thread: a large body and a large answer would
    // otherwise wait on each other.
    let feeding = child
        .stdin
        .take()
        .zip(request.body.clone())
        .map(|(mut stdin, body)| {
            std::thread::spawn(move || {
                let _ = stdin.write_all(&body);
            })
        });
    let output = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if let Some(feeding) = feeding {
        let _ = feeding.join();
    }
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(error.trim().trim_start_matches("curl: ").to_owned());
    }
    response(&output.stdout)
}
/// curl's `-i` output: the head (after any `100 Continue`), then the body.
fn response(mut raw: &[u8]) -> Result<Response, String> {
    loop {
        let end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("the answer has no head")?;
        let head = String::from_utf8_lossy(&raw[..end]);
        raw = &raw[end + 4..];
        let mut lines = head.split("\r\n");
        let status: u16 = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or("the answer has no status")?;
        if (100..200).contains(&status) {
            continue;
        }
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect();
        return Ok(Response {
            status,
            headers,
            body: raw.to_vec(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reads_past_continue() {
        let r = response(
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/2 404\r\ncontent-type: text/plain\r\n\r\nnope\r\n\r\n",
        )
        .unwrap();
        assert_eq!(r.status, 404);
        assert_eq!(r.headers, [("content-type".into(), "text/plain".into())]);
        assert_eq!(r.body, b"nope\r\n\r\n");
        let bad = Request {
            url: "https://example.com".into(),
            method: "GET\r\nX".into(),
            headers: vec![],
            body: None,
        };
        assert!(fetch(&bad).is_err());
    }

    fn manifest(permissions: &[&str]) -> Manifest {
        Manifest {
            name: "Notes".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
            panels: vec![],
        }
    }

    #[test]
    fn a_manifest_names_its_permissions_and_panels() {
        let m = parse(
            r#"{ "name": "Notes", "version": "1.2", "permissions": ["clipboard", "network"],
                 "panels": [{ "id": "notes", "title": "Notes", "description": "A pad" }, { "title": "no id" }] }"#,
        )
        .unwrap();
        assert_eq!(m.permissions, ["clipboard", "network"]);
        assert_eq!(m.panels.len(), 1, "a panel without an id is left out");
        assert!(
            parse(r#"{ "name": "X", "permissions": ["root"] }"#)
                .unwrap_err()
                .contains("`root`")
        );
        assert!(parse(r#"{ "version": "1" }"#).is_err(), "it needs a name");
    }

    #[test]
    fn a_plugin_runs_once_allowed_all_it_asks_for() {
        let m = manifest(&["clipboard"]);
        assert_eq!(status(&m, None), Status::Pending);
        let allowed = Decision {
            allowed: true,
            permissions: vec!["clipboard".into()],
        };
        assert_eq!(status(&m, Some(&allowed)), Status::Enabled);
        let denied = Decision::default();
        assert_eq!(status(&m, Some(&denied)), Status::Off);
        // A new version asks for more: the user is asked again.
        let more = manifest(&["clipboard", "network"]);
        assert_eq!(status(&more, Some(&allowed)), Status::Pending);
    }

    #[test]
    fn the_example_plugins_read() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/terminal-plugins");
        let found = scan(&dir, &HashMap::new());
        let ids: Vec<_> = found.iter().map(|f| (f.id.as_str(), f.status)).collect();
        assert_eq!(
            ids,
            [
                ("editor", Status::Pending),
                ("hacker-news", Status::Pending),
                ("notes", Status::Pending)
            ],
            "{found:?}"
        );
        let published = published(&found).to_json();
        assert!(published.contains("Connect to the internet"), "{published}");
        assert!(published.contains("panel:notes/notes"), "{published}");
    }

    #[test]
    fn scanning_finds_folders_and_what_is_wrong_with_them() {
        let dir = std::env::temp_dir().join(format!("zterm-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (id, manifest, entry) in [
            (
                "good",
                r#"{ "name": "Good", "permissions": ["files"] }"#,
                true,
            ),
            ("noentry", r#"{ "name": "No entry" }"#, false),
            ("bad", "not json", true),
        ] {
            std::fs::create_dir_all(dir.join(id)).unwrap();
            std::fs::write(dir.join(id).join("plugin.json"), manifest).unwrap();
            if entry {
                std::fs::write(dir.join(id).join("init.luau"), "").unwrap();
            }
        }
        let mut decisions = HashMap::new();
        decisions.insert(
            "good".to_string(),
            Decision {
                allowed: true,
                permissions: vec!["files".into()],
            },
        );
        let found = scan(&dir, &decisions);
        let status: Vec<_> = found.iter().map(|f| (f.id.as_str(), f.status)).collect();
        assert_eq!(
            status,
            [
                ("bad", Status::Broken),
                ("good", Status::Enabled),
                ("noentry", Status::Broken)
            ]
        );
        let plugins = enabled(&found);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].grants, [Capability::Files]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
