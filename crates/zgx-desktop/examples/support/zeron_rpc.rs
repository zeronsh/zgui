//! A small client for the running Zeron engine's local IPC: one WebSocket at
//! `ws://127.0.0.1:27654` (or `ZERON_IPC_PORT`), no auth, one JSON object per
//! message. Requests are `{"id","method","params"}`; replies are
//! `{"id","ok"}` or `{"id","err"}`; watches stream `{"id","item"}` until
//! `{"id","done"}` and stop with `{"id","cancel":true}`. See Zeron's
//! `crates/rpc/src/lib.rs` and `crates/engine/src/rpc.rs`.
//!
//! A background thread owns the socket; the UI thread routes frames by id.

use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    task::{Poll, Waker},
    time::Duration,
};

pub fn engine_url() -> String {
    let port = std::env::var("ZERON_IPC_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(27654);
    format!("ws://127.0.0.1:{port}")
}

#[derive(Default)]
struct Inbox {
    frames: VecDeque<Value>,
    closed: Option<String>,
    waker: Option<Waker>,
}

/// Frames from the network thread, awaited on the UI thread.
#[derive(Clone)]
struct Frames(Arc<Mutex<Inbox>>);

impl Frames {
    fn push(&self, frame: Option<Value>, closed: Option<String>) {
        let waker = {
            let mut inbox = self.0.lock().unwrap();
            inbox.frames.extend(frame);
            if closed.is_some() {
                inbox.closed = closed;
            }
            inbox.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    /// The next batch of frames, or why the connection ended.
    fn next(&self) -> impl Future<Output = Result<Vec<Value>, String>> + '_ {
        std::future::poll_fn(move |cx| {
            let mut inbox = self.0.lock().unwrap();
            if !inbox.frames.is_empty() {
                return Poll::Ready(Ok(inbox.frames.drain(..).collect()));
            }
            if let Some(reason) = inbox.closed.clone() {
                return Poll::Ready(Err(reason));
            }
            inbox.waker = Some(cx.waker().clone());
            Poll::Pending
        })
    }
}

/// Connect on a background thread. Resolves once the socket is open.
pub fn connect() -> impl Future<Output = Result<Rpc, String>> {
    let frames = Frames(Arc::default());
    let (tx, rx) = mpsc::channel::<String>();
    let opened = Arc::new(Mutex::new((None::<Result<(), String>>, None::<Waker>)));
    let (thread_frames, thread_opened) = (frames.clone(), opened.clone());
    let open = move |result: Result<(), String>| {
        let waker = {
            let mut state = thread_opened.lock().unwrap();
            state.0 = Some(result);
            state.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    };
    std::thread::Builder::new()
        .name("zeron-ipc".into())
        .spawn(move || {
            // A long session's snapshot can pass tungstenite's 16 MiB default
            // in one message; local IPC allows up to 256 MiB.
            const LIMIT: usize = 256 << 20;
            let config = tungstenite::protocol::WebSocketConfig::default()
                .max_message_size(Some(LIMIT))
                .max_frame_size(Some(LIMIT));
            let connected = tungstenite::client::connect_with_config(engine_url(), Some(config), 3);
            let mut socket = match connected {
                Ok((socket, _)) => socket,
                Err(error) => {
                    open(Err(error.to_string()));
                    return;
                }
            };
            if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
                // Short reads let this thread interleave outgoing requests.
                let _ = stream.set_read_timeout(Some(Duration::from_millis(15)));
                let _ = stream.set_nodelay(true);
            }
            open(Ok(()));
            loop {
                loop {
                    match rx.try_recv() {
                        Ok(text) => {
                            if let Err(error) = socket.send(tungstenite::Message::text(text)) {
                                thread_frames.push(None, Some(error.to_string()));
                                return;
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        // The UI dropped the client: close quietly.
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        for line in text.as_str().lines().filter(|l| !l.trim().is_empty()) {
                            if let Ok(frame) = serde_json::from_str(line) {
                                thread_frames.push(Some(frame), None);
                            }
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        thread_frames.push(None, Some("Zeron closed the connection".into()));
                        return;
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => {
                        thread_frames.push(None, Some(error.to_string()));
                        return;
                    }
                }
            }
        })
        .expect("spawn the IPC thread");
    let mut client = Some((tx, frames));
    std::future::poll_fn(move |cx| {
        let mut state = opened.lock().unwrap();
        match state.0.take() {
            Some(Ok(())) => {
                let (tx, frames) = client.take().expect("polled after completion");
                Poll::Ready(Ok(Rpc(Rc::new(Inner {
                    tx,
                    frames,
                    next_id: Cell::new(1),
                    pending: RefCell::default(),
                }))))
            }
            Some(Err(error)) => Poll::Ready(Err(error)),
            None => {
                state.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    })
}

type Reply = Rc<RefCell<(Option<Result<Value, String>>, Option<Waker>)>>;

enum Pending {
    Call(Reply),
    /// `None` while its callback runs (it may start or stop other watches).
    Watch(Option<Box<dyn FnMut(Value)>>),
}

struct Inner {
    tx: mpsc::Sender<String>,
    frames: Frames,
    next_id: Cell<u64>,
    pending: RefCell<HashMap<u64, Pending>>,
}

#[derive(Clone)]
pub struct Rpc(Rc<Inner>);

impl Rpc {
    fn send(&self, method: &str, params: Value) -> u64 {
        let id = self.0.next_id.get();
        self.0.next_id.set(id + 1);
        let _ = self
            .0
            .tx
            .send(json!({ "id": id, "method": method, "params": params }).to_string());
        id
    }
    /// A unary call.
    pub fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>>>> {
        let reply: Reply = Rc::default();
        let id = self.send(method, params);
        self.0
            .pending
            .borrow_mut()
            .insert(id, Pending::Call(reply.clone()));
        Box::pin(std::future::poll_fn(move |cx| {
            let mut slot = reply.borrow_mut();
            match slot.0.take() {
                Some(result) => Poll::Ready(result),
                None => {
                    slot.1 = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        }))
    }
    /// A watch: `on_item` sees the current value, then every change, until
    /// the returned handle drops.
    pub fn watch(
        &self,
        method: &str,
        params: Value,
        on_item: impl FnMut(Value) + 'static,
    ) -> Watch {
        let id = self.send(method, params);
        self.0
            .pending
            .borrow_mut()
            .insert(id, Pending::Watch(Some(Box::new(on_item))));
        Watch {
            rpc: self.clone(),
            id,
        }
    }
    /// Route frames until the connection ends; returns why it ended.
    pub async fn pump(&self) -> String {
        loop {
            let frames = match self.0.frames.next().await {
                Ok(frames) => frames,
                Err(reason) => {
                    // Fail outstanding calls so their awaiters move on.
                    for (_, pending) in self.0.pending.borrow_mut().drain() {
                        if let Pending::Call(reply) = pending {
                            let mut slot = reply.borrow_mut();
                            slot.0 = Some(Err(reason.clone()));
                            if let Some(waker) = slot.1.take() {
                                waker.wake();
                            }
                        }
                    }
                    return reason;
                }
            };
            for frame in frames {
                self.route(frame);
            }
        }
    }
    fn route(&self, frame: Value) {
        let Some(id) = frame.get("id").and_then(Value::as_u64) else {
            return;
        };
        let result = if let Some(error) = frame.get("err") {
            Some(Err(error.as_str().unwrap_or("error").to_owned()))
        } else {
            frame.get("ok").map(|ok| Ok(ok.clone()))
        };
        let mut pending = self.0.pending.borrow_mut();
        match pending.get_mut(&id) {
            Some(Pending::Call(_)) => {
                if let (Some(Pending::Call(reply)), Some(result)) = (pending.remove(&id), result) {
                    drop(pending);
                    let mut slot = reply.borrow_mut();
                    slot.0 = Some(result);
                    if let Some(waker) = slot.1.take() {
                        waker.wake();
                    }
                }
            }
            Some(Pending::Watch(callback)) => {
                if frame.get("done").is_some() || frame.get("err").is_some() {
                    if let Some(error) = frame.get("err") {
                        eprintln!("zeron: watch {id} failed: {error}");
                    }
                    pending.remove(&id);
                    return;
                }
                let Some(item) = frame.get("item").cloned() else {
                    return;
                };
                let Some(mut on_item) = callback.take() else {
                    return;
                };
                drop(pending);
                on_item(item);
                if let Some(Pending::Watch(slot)) = self.0.pending.borrow_mut().get_mut(&id) {
                    *slot = Some(on_item);
                }
            }
            None => {}
        }
    }
}

/// Cancels its watch when dropped.
pub struct Watch {
    rpc: Rpc,
    id: u64,
}

impl Drop for Watch {
    fn drop(&mut self) {
        if self.rpc.0.pending.borrow_mut().remove(&self.id).is_some() {
            let _ = self
                .rpc
                .0
                .tx
                .send(json!({ "id": self.id, "cancel": true }).to_string());
        }
    }
}
