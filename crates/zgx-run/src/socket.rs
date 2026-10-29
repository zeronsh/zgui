//! The connector `run` gives a script (`ui.socket`): WebSockets, `ws://`
//! and `wss://` (TLS by rustls with Mozilla's roots), each connection on a
//! thread of its own that sleeps until the network or the script has
//! something for it: an open, quiet connection costs no wakeups. What the
//! script sends is taken as the connection can write it, so a slow peer
//! fills the script's `buffered`, not memory here. A host that brings its
//! own transport replaces it (`ScriptHost::socket_connector`).
use polling::{Event, Events, Poller};
use std::{
    collections::VecDeque,
    future::Future,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tungstenite::{
    Error as WsError, Message as WsMessage, WebSocket, client::IntoClientRequest, http::HeaderValue,
};
use zgx_script::{Message, ScriptHost, Socket, SocketRequest};

/// Give `host` the default connector, as `run` does before a host's own
/// setup (which may replace it).
pub fn install(host: &ScriptHost) {
    host.socket_connector(Connection::start);
}

/// The most bytes handed to the connection's thread and not yet written:
/// past it, the script's messages wait in its own queue (`buffered`).
const IN_FLIGHT: usize = 64 * 1024;

/// What the thread and the script's task tell each other.
#[derive(Default)]
struct Shared {
    /// Heard from the network, for the script.
    heard: VecDeque<Heard>,
    /// From the script, for the network.
    outgoing: VecDeque<Message>,
    /// Bytes in `outgoing`, and the thread's write not yet flushed.
    in_flight: usize,
    /// The script stopped wanting the connection: the thread closes it.
    closed: bool,
    /// The script's task, waiting for something heard or room to send.
    waker: Option<Waker>,
}
enum Heard {
    Opened,
    Message(Message),
    Ended(Result<(), String>),
}

/// One connection: its thread talks to the network, this future to the
/// script's `Socket`.
struct Connection {
    shared: Arc<Mutex<Shared>>,
    poller: Arc<Poller>,
    socket: Socket,
    /// The script's next message, while waiting for one.
    next: Option<Pin<Box<dyn Future<Output = Option<Message>>>>>,
}

impl Connection {
    fn start(request: SocketRequest, socket: Socket) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let poller = Arc::new(Poller::new().expect("a poller for the connection"));
        let (heard, woken) = (shared.clone(), poller.clone());
        std::thread::Builder::new()
            .name("zgx-run socket".into())
            .spawn(move || {
                let ended = run(&request, &heard, &woken);
                tell(&heard, Heard::Ended(ended));
            })
            .expect("a thread for the connection");
        Self {
            shared,
            poller,
            socket,
            next: None,
        }
    }
}

impl Future for Connection {
    type Output = Result<(), String>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let heard = self.shared.lock().unwrap().heard.pop_front();
            match heard {
                // Outside the lock: the script's handlers run in these.
                Some(Heard::Opened) => self.socket.opened(),
                Some(Heard::Message(message)) => self.socket.receive(message),
                Some(Heard::Ended(result)) => return Poll::Ready(result),
                None => break,
            }
        }
        // Take what the script sent while the thread has room for it.
        while self.shared.lock().unwrap().in_flight < IN_FLIGHT {
            let socket = self.socket.clone();
            let next = self.next.get_or_insert_with(|| Box::pin(socket.next()));
            match next.as_mut().poll(cx) {
                Poll::Pending => break,
                Poll::Ready(message) => {
                    self.next = None;
                    let mut shared = self.shared.lock().unwrap();
                    match message {
                        Some(message) => {
                            shared.in_flight += len(&message);
                            shared.outgoing.push_back(message);
                        }
                        None => shared.closed = true,
                    }
                    let closed = shared.closed;
                    drop(shared);
                    let _ = self.poller.notify();
                    if closed {
                        break;
                    }
                }
            }
        }
        let mut shared = self.shared.lock().unwrap();
        if shared.heard.is_empty() {
            shared.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        drop(shared);
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

impl Drop for Connection {
    /// The script closed it, reloaded or went: the thread closes the
    /// connection and stops.
    fn drop(&mut self) {
        self.shared.lock().unwrap().closed = true;
        let _ = self.poller.notify();
    }
}

fn len(message: &Message) -> usize {
    match message {
        Message::Text(text) => text.len(),
        Message::Binary(bytes) => bytes.len(),
    }
}

/// Hand `heard` to the script's task, waking it.
fn tell(shared: &Mutex<Shared>, heard: Heard) {
    let waker = {
        let mut shared = shared.lock().unwrap();
        shared.heard.push_back(heard);
        shared.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// The connection's transport: TCP, or TLS over it.
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}
impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(tcp) => tcp,
            Stream::Tls(tls) => tls.get_ref(),
        }
    }
    /// TLS has written less than it was given: wait until it can write.
    fn wants_write(&self) -> bool {
        match self {
            Stream::Plain(_) => false,
            Stream::Tls(tls) => tls.conn.wants_write(),
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(tcp) => tcp.read(buf),
            Stream::Tls(tls) => tls.read(buf),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(tcp) => tcp.write(buf),
            Stream::Tls(tls) => tls.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Plain(tcp) => tcp.flush(),
            Stream::Tls(tls) => tls.flush(),
        }
    }
}

/// TLS as browsers check it: Mozilla's roots, ring's cryptography.
fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            Arc::new(
                rustls::ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .expect("ring's protocol versions")
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

/// Connect, then carry messages each way until either side ends it.
fn run(request: &SocketRequest, shared: &Mutex<Shared>, poller: &Poller) -> Result<(), String> {
    let mut ws = connect(request)?;
    if shared.lock().unwrap().closed {
        let _ = ws.close(None);
        return Ok(());
    }
    ws.get_ref()
        .tcp()
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    // SAFETY: the stream outlives its registration: it is deleted below,
    // before `ws` drops.
    unsafe { poller.add(ws.get_ref().tcp(), Event::readable(0)) }.map_err(|e| e.to_string())?;
    tell(shared, Heard::Opened);
    let ended = carry(&mut ws, shared, poller);
    let _ = poller.delete(ws.get_ref().tcp());
    ended
}

/// Open `request`'s connection: TCP to its host (ten seconds at most per
/// address), TLS for `wss://`, then the WebSocket handshake.
fn connect(request: &SocketRequest) -> Result<WebSocket<Stream>, String> {
    let mut http = request
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| e.to_string())?;
    if !request.protocols.is_empty() {
        let protocols =
            HeaderValue::from_str(&request.protocols.join(", ")).map_err(|e| e.to_string())?;
        http.headers_mut()
            .insert("Sec-WebSocket-Protocol", protocols);
    }
    let uri = http.uri().clone();
    let secure = match uri.scheme_str() {
        Some("wss") => true,
        Some("ws") => false,
        other => return Err(format!("not a WebSocket URL (scheme {other:?})")),
    };
    let host = uri.host().ok_or("the URL has no host")?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = uri.port_u16().unwrap_or(if secure { 443 } else { 80 });
    let mut last = None;
    let mut tcp = None;
    for address in (host, port).to_socket_addrs().map_err(|e| e.to_string())? {
        match TcpStream::connect_timeout(&address, Duration::from_secs(10)) {
            Ok(stream) => {
                tcp = Some(stream);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let tcp = tcp.ok_or_else(|| match last {
        Some(e) => e.to_string(),
        None => format!("{host} has no address"),
    })?;
    let _ = tcp.set_nodelay(true);
    // The handshake waits at most 30 seconds for the peer.
    tcp.set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let stream = if secure {
        let name =
            rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(|e| e.to_string())?;
        let connection =
            rustls::ClientConnection::new(tls_config(), name).map_err(|e| e.to_string())?;
        Stream::Tls(Box::new(rustls::StreamOwned::new(connection, tcp)))
    } else {
        Stream::Plain(tcp)
    };
    let (ws, _) = tungstenite::client(http, stream).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(e) => e.to_string(),
        tungstenite::HandshakeError::Interrupted(_) => "the handshake was interrupted".into(),
    })?;
    ws.get_ref()
        .tcp()
        .set_read_timeout(None)
        .map_err(|e| e.to_string())?;
    Ok(ws)
}

fn would_block(error: &WsError) -> bool {
    matches!(error, WsError::Io(e) if e.kind() == io::ErrorKind::WouldBlock)
}

/// The open connection's loop: read what arrived, write what the script
/// sent, then sleep until the socket or the script wakes it.
fn carry(
    ws: &mut WebSocket<Stream>,
    shared: &Mutex<Shared>,
    poller: &Poller,
) -> Result<(), String> {
    let mut events = Events::new();
    let mut closing = false;
    loop {
        loop {
            match ws.read() {
                Ok(WsMessage::Text(text)) => tell(
                    shared,
                    Heard::Message(Message::Text(text.as_str().to_owned())),
                ),
                Ok(WsMessage::Binary(bytes)) => {
                    tell(shared, Heard::Message(Message::Binary(bytes.to_vec())))
                }
                Ok(WsMessage::Close(_)) | Err(WsError::ConnectionClosed) => return Ok(()),
                Ok(_) => {}
                Err(e) if would_block(&e) => break,
                Err(WsError::AlreadyClosed) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        }
        let (outgoing, closed) = {
            let mut shared = shared.lock().unwrap();
            (std::mem::take(&mut shared.outgoing), shared.closed)
        };
        let mut written = 0;
        for message in outgoing {
            written += len(&message);
            let message = match message {
                Message::Text(text) => WsMessage::text(text),
                Message::Binary(bytes) => WsMessage::binary(bytes),
            };
            match ws.write(message) {
                Ok(()) => {}
                Err(e) if would_block(&e) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if closed && !closing {
            closing = true;
            match ws.close(None) {
                Ok(()) => {}
                Err(e) if would_block(&e) => {}
                Err(WsError::AlreadyClosed | WsError::ConnectionClosed) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        }
        // A write the socket did not take all of: flushed once it can.
        let unflushed = match ws.flush() {
            Ok(()) => ws.get_ref().wants_write(),
            Err(e) if would_block(&e) => true,
            Err(WsError::ConnectionClosed) if closing => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        // Room for more once what was taken is on its way.
        let waker = {
            let mut shared = shared.lock().unwrap();
            shared.in_flight = shared.in_flight.saturating_sub(written);
            if unflushed {
                shared.in_flight = shared.in_flight.max(IN_FLIGHT);
            } else if shared.in_flight >= IN_FLIGHT {
                shared.in_flight = shared.outgoing.iter().map(len).sum();
            }
            shared.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        let interest = if unflushed {
            Event::all(0)
        } else {
            Event::readable(0)
        };
        poller
            .modify(ws.get_ref().tcp(), interest)
            .map_err(|e| e.to_string())?;
        events.clear();
        // A closing handshake the peer never answers ends after 5 s.
        let wait = closing.then(|| Duration::from_secs(5));
        match poller.wait(&mut events, wait) {
            Ok(0) if closing && !shared.lock().unwrap().outgoing.is_empty() => {}
            Ok(0) if closing => return Ok(()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}
