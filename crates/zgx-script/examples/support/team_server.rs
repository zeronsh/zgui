//! The team example's API, served in memory as a fetcher: people listed,
//! added, changed and removed, each answer after `latency`. An email two
//! people would share is refused, as a server would, and `fail(n)` makes
//! the next `n` requests fail (a network down) to show the app's errors.
// Shared with its tests, which use what the example does not.
#![allow(dead_code)]
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx_script::{Request, Response, ScriptHost, Value};

#[derive(Default)]
struct Inner {
    members: Vec<Value>,
    made: u32,
    failing: u32,
    asked: Vec<String>,
}

/// The server's people and its switches; clones share them.
#[derive(Clone, Default)]
pub struct TeamServer(Rc<RefCell<Inner>>);

impl TeamServer {
    /// A server holding `people` (name, email, role).
    pub fn new(people: &[(&str, &str, &str)]) -> Self {
        let server = Self::default();
        for (name, email, role) in people {
            let member = server.member(None, name, email, role);
            server.0.borrow_mut().members.push(member);
        }
        server
    }
    fn member(&self, id: Option<&str>, name: &str, email: &str, role: &str) -> Value {
        let id = match id {
            Some(id) => id.to_string(),
            None => {
                let mut inner = self.0.borrow_mut();
                inner.made += 1;
                format!("m{}", inner.made)
            }
        };
        Value::record([
            ("id", Value::from(id.as_str())),
            ("name", name.into()),
            ("email", email.into()),
            ("role", role.into()),
        ])
    }
    /// Fail the next `n` requests.
    pub fn fail(&self, n: u32) {
        self.0.borrow_mut().failing = n;
    }
    /// The requests answered so far, as `"GET /members"`.
    pub fn asked(&self) -> Vec<String> {
        self.0.borrow().asked.clone()
    }
    /// How many people it holds.
    pub fn len(&self) -> usize {
        self.0.borrow().members.len()
    }
    /// Serve `host`'s `ui.fetch`, answering each request after `latency`.
    pub fn serve(&self, host: &ScriptHost, latency: Duration) {
        let server = self.clone();
        host.fetcher(move |request: Request| {
            let server = server.clone();
            async move {
                zgx::timer::sleep(latency).await;
                server.answer(&request)
            }
        });
    }
    fn answer(&self, request: &Request) -> Result<Response, String> {
        let path = request
            .url
            .strip_prefix("https://team.example/api")
            .unwrap_or("");
        self.0
            .borrow_mut()
            .asked
            .push(format!("{} {path}", request.method));
        {
            let mut inner = self.0.borrow_mut();
            if inner.failing > 0 {
                inner.failing -= 1;
                return Err("The network is unreachable".into());
            }
        }
        let body = request
            .body
            .as_deref()
            .and_then(|body| Value::from_json(std::str::from_utf8(body).ok()?));
        let field = |name: &str| {
            body.as_ref()
                .and_then(|body| body.field(name))
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        let id = path.strip_prefix("/members/");
        match (request.method.as_str(), id) {
            ("GET", None) => Ok(json(
                200,
                &Value::List(self.0.borrow().members.clone().into()),
            )),
            ("POST", None) | ("PUT", Some(_)) => {
                let email = field("email").to_lowercase();
                let taken = self.0.borrow().members.iter().any(|m| {
                    m.field("email")
                        .is_some_and(|e| e.to_string().to_lowercase() == email)
                        && m.field("id").map(Value::to_string).as_deref() != id
                });
                if taken {
                    return Ok(error(409, "Someone already has that email"));
                }
                let member = self.member(id, &field("name"), &field("email"), &field("role"));
                let mut inner = self.0.borrow_mut();
                match id {
                    Some(id) => {
                        let Some(at) = inner.members.iter().position(|m| {
                            m.field("id").map(Value::to_string).as_deref() == Some(id)
                        }) else {
                            return Ok(error(404, "No one has that id"));
                        };
                        inner.members[at] = member.clone();
                    }
                    None => inner.members.push(member.clone()),
                }
                Ok(json(if id.is_some() { 200 } else { 201 }, &member))
            }
            ("DELETE", Some(id)) => {
                let mut inner = self.0.borrow_mut();
                let before = inner.members.len();
                inner
                    .members
                    .retain(|m| m.field("id").map(Value::to_string).as_deref() != Some(id));
                if inner.members.len() == before {
                    return Ok(error(404, "No one has that id"));
                }
                Ok(Response {
                    status: 204,
                    headers: vec![],
                    body: vec![],
                })
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
    let body = Value::record([("error", Value::from(message))]);
    json(status, &body)
}
