//! The inbox example's API, served in memory as a fetcher: 500 messages
//! searched and paged (`GET /messages?q=…&cursor=n`, 40 a page), one
//! thread (`GET /threads/{id}`), and the folders' order (`PUT /folders`).
//! Each answer comes after `latency`; `fail(n)` fails the next `n`, and
//! `hold(true)` keeps answers back, for tests.
#![allow(dead_code)]
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx_script::{Request, Response, ScriptHost, Value};

const PAGE: usize = 40;

struct Inner {
    messages: Vec<(u32, String, String, String)>,
    folders: Vec<String>,
    failing: u32,
    held: bool,
    asked: Vec<String>,
}

#[derive(Clone)]
pub struct InboxServer(Rc<RefCell<Inner>>);

impl InboxServer {
    pub fn new() -> Self {
        let senders = ["Ada", "Grace", "Edsger", "Barbara", "Alan", "Katherine"];
        let topics = ["Lunch", "Release notes", "Review", "Budget", "Offsite", "Bug triage"];
        let messages = (1..=500u32)
            .map(|id| {
                let from = senders[id as usize % senders.len()].to_string();
                let subject = format!("{} #{id}", topics[id as usize % topics.len()]);
                let body = format!("{from} wrote about {subject}.");
                (id, from, subject, body)
            })
            .collect();
        Self(Rc::new(RefCell::new(Inner {
            messages,
            folders: vec!["Inbox".into(), "Work".into(), "Personal".into(), "Receipts".into()],
            failing: 0,
            held: false,
            asked: Vec::new(),
        })))
    }
    pub fn fail(&self, n: u32) {
        self.0.borrow_mut().failing = n;
    }
    pub fn hold(&self, held: bool) {
        self.0.borrow_mut().held = held;
    }
    /// The requests answered, as `"GET /messages?q=lunch&cursor=0"`.
    pub fn asked(&self) -> Vec<String> {
        self.0.borrow().asked.clone()
    }
    pub fn folders(&self) -> Vec<String> {
        self.0.borrow().folders.clone()
    }
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
        let path = request.url.strip_prefix("https://mail.example/api").unwrap_or("");
        self.0.borrow_mut().asked.push(format!("{} {path}", request.method));
        {
            let mut inner = self.0.borrow_mut();
            if inner.failing > 0 {
                inner.failing -= 1;
                return Err("The network is unreachable".into());
            }
        }
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let param = |name: &str| {
            query
                .split('&')
                .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
                .map(|v| v.replace("%20", " ").replace('+', " "))
                .unwrap_or_default()
        };
        let inner = self.0.borrow();
        match (request.method.as_str(), route) {
            ("GET", "/messages") => {
                let q = param("q").to_lowercase();
                let cursor: usize = param("cursor").parse().unwrap_or(0);
                let found: Vec<_> = inner
                    .messages
                    .iter()
                    .filter(|(_, from, subject, _)| {
                        q.is_empty()
                            || subject.to_lowercase().contains(&q)
                            || from.to_lowercase().contains(&q)
                    })
                    .collect();
                let page: Vec<Value> = found
                    .iter()
                    .skip(cursor)
                    .take(PAGE)
                    .map(|(id, from, subject, _)| {
                        Value::record([
                            ("id", Value::Num(*id as f64)),
                            ("from", from.as_str().into()),
                            ("subject", subject.as_str().into()),
                        ])
                    })
                    .collect();
                let next = cursor + page.len();
                let more = next < found.len();
                Ok(json(
                    200,
                    &Value::record([
                        ("messages", Value::List(page.into())),
                        ("next", if more { Value::Num(next as f64) } else { Value::Nil }),
                    ]),
                ))
            }
            ("GET", thread) if thread.starts_with("/threads/") => {
                let id: u32 = thread["/threads/".len()..].parse().unwrap_or(0);
                match inner.messages.iter().find(|m| m.0 == id) {
                    Some((id, from, subject, body)) => Ok(json(
                        200,
                        &Value::record([
                            ("id", Value::Num(*id as f64)),
                            ("from", from.as_str().into()),
                            ("subject", subject.as_str().into()),
                            ("body", body.as_str().into()),
                        ]),
                    )),
                    None => Ok(error(404, "No such thread")),
                }
            }
            ("GET", "/folders") => Ok(json(
                200,
                &Value::List(inner.folders.iter().map(|f| Value::from(f.as_str())).collect()),
            )),
            ("PUT", "/folders") => {
                drop(inner);
                let body = request
                    .body
                    .as_deref()
                    .and_then(|b| Value::from_json(std::str::from_utf8(b).ok()?));
                let Some(Value::List(list)) = body else {
                    return Ok(error(400, "Send the folders as a list"));
                };
                self.0.borrow_mut().folders = list.iter().map(Value::to_string).collect();
                Ok(json(200, &Value::List(list)))
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
