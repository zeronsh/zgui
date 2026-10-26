//! The other end of devtools ([`crate::inspect`, with the `inspect`
//! feature]): finding this user's running apps and asking one, for `zgx
//! inspect` and its MCP server.
use crate::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

/// Where this user's apps put their sockets: a directory only they may
/// enter (`$TMPDIR/zgx-inspect-<uid>`, mode 0700; `%LOCALAPPDATA%` on
/// Windows).
pub fn socket_dir() -> PathBuf {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { getuid() };
        std::env::temp_dir().join(format!("zgx-inspect-{uid}"))
    }
    #[cfg(not(unix))]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("zgx-inspect")
    }
}

/// A running app serving devtools: its process and what it said it is.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    /// Its process id.
    pub pid: u32,
    /// What it runs (its script).
    pub app: String,
}

/// The apps serving devtools now, by process id; files of apps gone are
/// left out (and removed).
pub fn apps() -> Vec<App> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(socket_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(about) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| Value::from_json(&t))
        else {
            continue;
        };
        let pid = about.field("pid").and_then(Value::as_f64).unwrap_or(0.) as u32;
        let app = about.field("app").map(Value::to_string).unwrap_or_default();
        if connect(pid).is_ok() {
            out.push(App { pid, app });
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
    out.sort_by_key(|a| a.pid);
    out
}

/// The app to ask: `pid`, or the only one running.
pub fn pick(pid: Option<u32>) -> Result<u32, String> {
    if let Some(pid) = pid {
        return Ok(pid);
    }
    let apps = apps();
    match apps.as_slice() {
        [one] => Ok(one.pid),
        [] => Err(
            "no app is serving devtools: run it with `zgx-run --inspect` (or ZGX_INSPECT_SOCKET=1)"
                .into(),
        ),
        many => Err(format!(
            "{} apps serve devtools; say which with --app:\n{}",
            many.len(),
            many.iter()
                .map(|a| format!("  {} {}", a.pid, a.app))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type Stream = std::net::TcpStream;

#[cfg(unix)]
fn connect(pid: u32) -> std::io::Result<(Stream, Option<String>)> {
    Ok((
        Stream::connect(socket_dir().join(format!("{pid}.sock")))?,
        None,
    ))
}
#[cfg(not(unix))]
fn connect(pid: u32) -> std::io::Result<(Stream, Option<String>)> {
    let about = std::fs::read_to_string(socket_dir().join(format!("{pid}.json")))?;
    let about = Value::from_json(&about).ok_or_else(|| std::io::Error::other("unreadable"))?;
    let port = about.field("port").and_then(Value::as_f64).unwrap_or(0.) as u16;
    let token = about.field("token").map(Value::to_string);
    Ok((Stream::connect(("127.0.0.1", port))?, token))
}

/// Ask app `pid` one request (a JSON object): its answer, a JSON line.
pub fn request(pid: u32, request: &Value) -> Result<String, String> {
    let (stream, token) = connect(pid).map_err(|e| format!("app {pid}: {e}"))?;
    let request = match (token, request) {
        (Some(token), Value::Map(fields)) => {
            let mut fields: Vec<_> = fields.iter().cloned().collect();
            fields.push(("token".into(), Value::from(token.as_str())));
            Value::record(fields)
        }
        (_, request) => request.clone(),
    };
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writeln!(writer, "{}", request.to_json()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.is_empty() {
        return Err(format!("app {pid} closed the connection"));
    }
    Ok(line.trim_end().to_owned())
}
