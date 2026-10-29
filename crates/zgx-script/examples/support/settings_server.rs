//! The settings example's API, served in memory as a fetcher: one person's
//! settings, read with `GET /settings` and replaced with `PUT /settings`,
//! each answer after `latency`. A name of "taken" is refused (409), as a
//! server checking uniqueness would; `fail(n)` makes the next `n` requests
//! fail (a network down) and `hold(true)` keeps answers back until
//! `hold(false)`, for a test to see what shows while one is out.
// Shared with its tests, which use what the example does not.
#![allow(dead_code)]
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx_script::{Request, Response, ScriptHost, Value};

struct Inner {
    settings: Value,
    failing: u32,
    held: bool,
    saves: u32,
}

/// The server's settings and switches; clones share them.
#[derive(Clone)]
pub struct SettingsServer(Rc<RefCell<Inner>>);

impl SettingsServer {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Inner {
            settings: Value::record([
                ("name", Value::from("Ada Lovelace")),
                ("email", "ada@example.com".into()),
                ("theme", "system".into()),
                ("notifications", true.into()),
            ]),
            failing: 0,
            held: false,
            saves: 0,
        })))
    }
    /// Fail the next `n` requests.
    pub fn fail(&self, n: u32) {
        self.0.borrow_mut().failing = n;
    }
    /// Keep answers back (`true`) until released (`false`).
    pub fn hold(&self, held: bool) {
        self.0.borrow_mut().held = held;
    }
    /// A field of what it holds, as text.
    pub fn field(&self, name: &str) -> String {
        self.0
            .borrow()
            .settings
            .field(name)
            .map(Value::to_string)
            .unwrap_or_default()
    }
    /// How many saves it took.
    pub fn saves(&self) -> u32 {
        self.0.borrow().saves
    }
    /// Serve `host`'s `ui.fetch`, answering each request after `latency`
    /// (and once no longer held).
    pub fn serve(&self, host: &ScriptHost, latency: Duration) {
        let server = self.clone();
        host.fetcher(move |request: Request| {
            let server = server.clone();
            async move {
                zgx::timer::sleep(latency).await;
                while server.0.borrow().held {
                    zgx::timer::sleep(Duration::from_millis(1)).await;
                }
                server.answer(&request)
            }
        });
    }
    fn answer(&self, request: &Request) -> Result<Response, String> {
        {
            let mut inner = self.0.borrow_mut();
            if inner.failing > 0 {
                inner.failing -= 1;
                return Err("The network is unreachable".into());
            }
        }
        let path = request
            .url
            .strip_prefix("https://settings.example/api")
            .unwrap_or("");
        match (request.method.as_str(), path) {
            ("GET", "/settings") => Ok(json(200, &self.0.borrow().settings)),
            ("PUT", "/settings") => {
                let Some(body) = request
                    .body
                    .as_deref()
                    .and_then(|body| Value::from_json(std::str::from_utf8(body).ok()?))
                else {
                    return Ok(error(400, "Send the settings as JSON"));
                };
                if body.field("name").map(Value::to_string).as_deref() == Some("taken") {
                    return Ok(error(409, "Someone already has that name"));
                }
                let mut inner = self.0.borrow_mut();
                inner.settings = body.clone();
                inner.saves += 1;
                Ok(json(200, &body))
            }
            _ => Ok(error(404, "Not found")),
        }
    }
}

fn json(status: u16, value: &Value) -> Response {
    Response {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: value.to_json().into_bytes(),
    }
}
fn error(status: u16, message: &str) -> Response {
    json(status, &Value::record([("error", Value::from(message))]))
}
