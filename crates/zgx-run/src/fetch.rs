//! The fetcher `run` gives a script (`ui.fetch`, remote images): HTTP and
//! HTTPS through ureq, TLS by rustls with Mozilla's roots, each request on
//! a thread of its own so the window never waits for the network. The
//! body reaches the script as it arrives (`on_chunk`, `on_event`); a
//! cancelled fetch stops reading and closes its connection. A host that
//! brings its own transport replaces it (`ScriptHost::fetcher`).
use std::{
    future::Future,
    io::Read,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll, Waker},
};
use zgx_script::{Body, Request, Response, ScriptHost};

/// Give `host` the default fetcher, as `run` does before a host's own
/// setup (which may replace it).
pub fn install(host: &ScriptHost) {
    host.streaming_fetcher(Fetching::start);
}

/// An answer's status and headers.
type Head = (u16, Vec<(String, String)>);
/// What a request's thread has heard, for the task awaiting it.
#[derive(Default)]
struct Heard {
    /// The status and headers, or why there are none.
    head: Option<Result<Head, String>>,
    /// Body read since the task last took it.
    pieces: Vec<u8>,
    /// The body ended, or broke off (`failed`).
    ended: bool,
    failed: Option<String>,
    /// The task stopped listening: the thread stops reading.
    cancelled: bool,
    waker: Option<Waker>,
}
type Shared = Arc<Mutex<Heard>>;

/// One request: its thread reads, this future hands what it read to the
/// script's body and answers with the head once the body ends.
struct Fetching {
    shared: Shared,
    body: Body,
}
impl Fetching {
    fn start(request: Request, body: Body) -> Self {
        let shared = Shared::default();
        let heard = shared.clone();
        std::thread::Builder::new()
            .name("zgx-run fetch".into())
            .spawn(move || read(request, &heard))
            .expect("a thread for the request");
        Self { shared, body }
    }
}
impl Future for Fetching {
    type Output = Result<Response, String>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let (pieces, done) = {
                let mut heard = self.shared.lock().unwrap();
                let pieces = std::mem::take(&mut heard.pieces);
                let done = pieces.is_empty() && (heard.ended || matches!(heard.head, Some(Err(_))));
                if pieces.is_empty() && !done {
                    heard.waker = Some(cx.waker().clone());
                    return Poll::Pending;
                }
                (pieces, done)
            };
            if !pieces.is_empty() {
                // Outside the lock: the script's handlers run in it.
                self.body.write(&pieces);
                if !self.body.is_open() {
                    self.shared.lock().unwrap().cancelled = true;
                    return Poll::Ready(Err("cancelled".into()));
                }
                continue;
            }
            debug_assert!(done);
            let mut heard = self.shared.lock().unwrap();
            if let Some(failed) = heard.failed.take() {
                return Poll::Ready(Err(failed));
            }
            return Poll::Ready(match heard.head.take() {
                Some(Ok((status, headers))) => Ok(Response {
                    status,
                    headers,
                    body: Vec::new(),
                }),
                Some(Err(error)) => Err(error),
                None => Err("the request ended without an answer".into()),
            });
        }
    }
}
impl Drop for Fetching {
    /// A task cancelled (or the script reloaded) drops this: the thread
    /// stops at its next read.
    fn drop(&mut self) {
        self.shared.lock().unwrap().cancelled = true;
    }
}

/// The agent every request goes through: connections are kept for reuse.
/// A status other than 2xx is an answer the script reads (`ok` false), not
/// an error.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(concat!("zgx-run/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(std::time::Duration::from_secs(30)))
            .build()
            .into()
    })
}

/// Make `request` and read its answer into `heard`, waking its task as
/// each part arrives, until it ends or the task stops listening.
fn read(request: Request, heard: &Mutex<Heard>) {
    let tell = |change: &mut dyn FnMut(&mut Heard)| -> bool {
        let waker = {
            let mut heard = heard.lock().unwrap();
            change(&mut heard);
            if heard.cancelled {
                return false;
            }
            heard.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        true
    };
    let response = match send(request) {
        Ok(response) => response,
        Err(error) => {
            tell(&mut |heard| heard.head = Some(Err(error.clone())));
            return;
        }
    };
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
            (name.as_str().to_owned(), value)
        })
        .collect();
    let status = response.status().as_u16();
    if !tell(&mut |heard| heard.head = Some(Ok((status, headers.clone())))) {
        return;
    }
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                tell(&mut |heard| heard.ended = true);
                return;
            }
            Ok(n) => {
                if !tell(&mut |heard| heard.pieces.extend_from_slice(&buffer[..n])) {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                tell(&mut |heard| {
                    heard.failed = Some(error.to_string());
                    heard.ended = true;
                });
                return;
            }
        }
    }
}

/// `request` sent, its answer's head read.
fn send(request: Request) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut builder = ureq::http::Request::builder()
        .method(request.method.to_ascii_uppercase().as_str())
        .uri(&request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    let sent = match request.body {
        // No body at all (a GET's), not an empty one.
        None => builder.body(()).map(|request| agent().run(request)),
        Some(body) => builder.body(body).map(|request| agent().run(request)),
    };
    sent.map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
