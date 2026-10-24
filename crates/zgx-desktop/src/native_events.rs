//! Owned operating-system application activation and open-URL callbacks.
use std::{fmt, sync::Arc};
/// What the operating system asks of the application as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationEvent {
    /// Open these URLs.
    OpenUrls(Vec<String>),
    /// Show a window again: the dock icon was clicked or the app relaunched.
    Reopen,
}
/// Why native application events could not be registered (an invalid
/// application ID, or the platform refused).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEventError(pub String);
impl fmt::Display for ApplicationEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ApplicationEventError {}
/// An event, and the activation token of the launch that sent it (Linux:
/// `activation-token` on Wayland, `desktop-startup-id` on X11), which lets
/// the app raise its window for it.
pub(crate) type Handler = Arc<dyn Fn(ApplicationEvent, Option<String>) + Send + Sync>;
pub(crate) fn validate_id(id: &str) -> Result<(), ApplicationEventError> {
    let pieces: Vec<_> = id.split('.').collect();
    if id.len() > 255
        || pieces.len() < 2
        || pieces.iter().any(|piece| {
            piece.is_empty()
                || !piece.as_bytes()[0].is_ascii_alphabetic()
                || !piece
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        })
    {
        Err(ApplicationEventError(
            "application ID must be a reverse-DNS identifier".into(),
        ))
    } else {
        Ok(())
    }
}
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use dbus::{
        arg::PropMap, blocking::stdintf::org_freedesktop_dbus::RequestNameReply,
        channel::MatchingReceiver, message::MatchRule,
    };
    use std::{sync::mpsc, thread, time::Duration};
    /// The token a launch hands over with its call (GApplication's
    /// `platform_data`): Wayland's activation token, or X11's startup id.
    fn launch_token(platform_data: &PropMap) -> Option<String> {
        ["activation-token", "desktop-startup-id"]
            .iter()
            .find_map(|key| Some(platform_data.get(*key)?.0.as_str()?.to_owned()))
    }
    pub(crate) struct NativeApplicationEvents {
        stop: Arc<crate::linux_dbus::Stop>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl NativeApplicationEvents {
        pub(crate) fn new(
            id: &str,
            handler: Handler,
        ) -> Result<Option<Self>, ApplicationEventError> {
            validate_id(id)?;
            let id = id.to_owned();
            let stop = Arc::new(crate::linux_dbus::Stop::new().ok_or_else(|| {
                ApplicationEventError("no event descriptor for the service's thread".into())
            })?);
            let stopped = stop.clone();
            let (ready, started) = mpsc::sync_channel(1);
            let thread=thread::Builder::new().name("zgx-application-events".into()).spawn(move || {
                let setup=(||->Result<_,ApplicationEventError>{
                    let connection=crate::linux_dbus::session().map_err(|e|ApplicationEventError(e.to_string()))?;
                    let reply=connection.request_name(&id,false,false,true).map_err(|e|ApplicationEventError(e.to_string()))?;
                    if reply!=RequestNameReply::PrimaryOwner {return Err(ApplicationEventError(format!("application ID {id} is already owned")))}
                    let mut routes=dbus_crossroads::Crossroads::new();
                    let interface=routes.register("org.freedesktop.Application",|builder| {
                        builder.method("Activate",("platform_data",),(),|_:&mut dbus_crossroads::Context,handler:&mut Handler,(platform_data,): (PropMap,)| {
                            handler(ApplicationEvent::Reopen,launch_token(&platform_data));Ok(())
                        });
                        builder.method("Open",("uris","platform_data"),(),|_:&mut dbus_crossroads::Context,handler:&mut Handler,(urls,platform_data):(Vec<String>,PropMap)| {
                            if urls.len()>256 || urls.iter().any(|url|url.len()>65536 || !crate::url::valid_url(url)) {
                                return Err(dbus::MethodErr::invalid_arg("invalid URL batch"));
                            }
                            handler(ApplicationEvent::OpenUrls(urls),launch_token(&platform_data));Ok(())
                        });
                    });
                    let path=format!("/{}",id.replace('.',"/").replace('-',"_"));
                    routes.insert(path,&[interface],handler);
                    connection.start_receive(MatchRule::new_method_call(),Box::new(move |message,connection| {
                        let _=routes.handle_message(message,connection);true
                    }));
                    Ok(connection)
                })();
                match setup {
                    Ok(connection)=>{
                        let _=ready.send(Ok(()));
                        // Asleep until a launch calls, or the app ends.
                        crate::linux_dbus::serve(&connection,&stopped);
                    }
                    Err(error)=>{let _=ready.send(Err(error));}
                }
            }).map_err(|e|ApplicationEventError(e.to_string()))?;
            match started.recv_timeout(Duration::from_secs(3)) {
                Ok(Ok(())) => Ok(Some(Self {
                    stop,
                    thread: Some(thread),
                })),
                result => {
                    stop.stop();
                    Err(match result {
                        Ok(Err(error)) => error,
                        _ => ApplicationEventError(
                            "application event service startup timed out".into(),
                        ),
                    })
                }
            }
        }
    }
    impl Drop for NativeApplicationEvents {
        fn drop(&mut self) {
            self.stop.stop();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2::{
        DeclaredClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
    };
    use objc2_foundation::{
        NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol,
    };

    const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
    const CORE: u32 = u32::from_be_bytes(*b"aevt");
    const REOPEN: u32 = u32::from_be_bytes(*b"rapp");
    const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
    const FILE_URL: u32 = u32::from_be_bytes(*b"furl");
    struct State {
        handler: Handler,
    }
    // SAFETY: an NSObject subclass, main-thread only (AppleEvents are
    // dispatched on the main thread), with nothing to clean up beyond its
    // ivars; each method has the AppleEvent handler signature
    // `-(void)handle:(NSAppleEventDescriptor *)event
    // withReplyEvent:(NSAppleEventDescriptor *)reply`.
    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = State]
        struct EventHandler;
        unsafe impl NSObjectProtocol for EventHandler {}
        impl EventHandler {
            #[unsafe(method(handleUrl:withReplyEvent:))]
            fn open_url(&self, event: &NSAppleEventDescriptor, _: &NSAppleEventDescriptor) {
                if let Some(value) = event.paramDescriptorForKeyword(DIRECT_OBJECT)
                    .and_then(|value| value.stringValue())
                {
                    self.deliver_urls(vec![value.to_string()]);
                }
            }
            #[unsafe(method(handleDocuments:withReplyEvent:))]
            fn open_documents(&self, event: &NSAppleEventDescriptor, _: &NSAppleEventDescriptor) {
                if let Some(list) = event.paramDescriptorForKeyword(DIRECT_OBJECT) {
                    if !(0..=256).contains(&list.numberOfItems()) { return; }
                    let urls = (1..=list.numberOfItems()).map(|index| {
                        list.descriptorAtIndex(index)
                            .and_then(|item| item.coerceToDescriptorType(FILE_URL))
                            .and_then(|item| item.stringValue())
                            .map(|url| url.to_string())
                    }).collect::<Option<Vec<_>>>();
                    // Preserve the batch as one transaction. A malformed entry
                    // must not silently turn a multi-document open into a subset.
                    if let Some(urls) = urls {
                        self.deliver_urls(urls);
                    }
                }
            }
            #[unsafe(method(handleReopen:withReplyEvent:))]
            fn reopen(&self, _: &NSAppleEventDescriptor, _: &NSAppleEventDescriptor) {
                (self.ivars().handler)(ApplicationEvent::Reopen, None);
            }
        }
    );
    impl EventHandler {
        fn deliver_urls(&self, urls: Vec<String>) {
            if !urls.is_empty()
                && urls.len() <= 256
                && urls
                    .iter()
                    .all(|url| url.len() <= 65536 && crate::url::valid_url(url))
            {
                (self.ivars().handler)(ApplicationEvent::OpenUrls(urls), None);
            }
        }
    }
    pub(crate) struct NativeApplicationEvents {
        manager: Retained<NSAppleEventManager>,
        _handler: Retained<EventHandler>,
    }
    impl NativeApplicationEvents {
        pub(crate) fn new(
            id: &str,
            handler: Handler,
        ) -> Result<Option<Self>, ApplicationEventError> {
            validate_id(id)?;
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                ApplicationEventError("application events require the main thread".into())
            })?;
            // Winit owns its NSApplicationDelegate and downcasts that object
            // during event delivery. Subscribe to AppleEvents without replacing it.
            let handler = EventHandler::alloc(mtm).set_ivars(State { handler });
            // SAFETY: NSObject's `init`, on a fresh allocation with its ivars set.
            let handler: Retained<EventHandler> = unsafe { msg_send![super(handler), init] };
            let manager = NSAppleEventManager::sharedAppleEventManager();
            let service = Self {
                manager,
                _handler: handler,
            };
            service.install();
            Ok(Some(service))
        }
        pub(crate) fn install(&self) {
            // NSApplication installs its standard AppleEvent routes while
            // launching. Reinstall at winit's resumed/DidFinishLaunching boundary
            // so our routes survive that initialization without touching delegates.
            for (class, id, selector) in [
                (GET_URL, GET_URL, sel!(handleUrl:withReplyEvent:)),
                (CORE, OPEN_DOCUMENTS, sel!(handleDocuments:withReplyEvent:)),
                (CORE, REOPEN, sel!(handleReopen:withReplyEvent:)),
            ] {
                // SAFETY: each selector is one of `EventHandler`'s, with the
                // two-descriptor handler signature. The manager keeps the
                // handler unretained: this service owns it and removes these
                // routes in `Drop`, before the handler goes.
                unsafe {
                    self.manager
                        .setEventHandler_andSelector_forEventClass_andEventID(
                            &self._handler,
                            selector,
                            class,
                            id,
                        );
                }
            }
        }
    }
    impl Drop for NativeApplicationEvents {
        fn drop(&mut self) {
            for (class, id) in [(GET_URL, GET_URL), (CORE, OPEN_DOCUMENTS), (CORE, REOPEN)] {
                self.manager
                    .removeEventHandlerForEventClass_andEventID(class, id);
            }
        }
    }
}
#[cfg(target_os = "windows")]
mod platform {
    //! One instance an application ID and session, as Windows has no
    //! launcher that routes to a running app (Linux's session bus, macOS's
    //! AppleEvents): the first claims a named pipe, and a later launch hands
    //! it its files and URLs (or asks it to reopen) and steps aside. The ID
    //! is also the process's AppUserModelID: its windows group under it on
    //! the taskbar, as an installer's shortcut names it.
    use super::*;
    use std::{
        ffi::OsString,
        path::Path,
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{CloseHandle, ERROR_PIPE_BUSY, GENERIC_WRITE, GetLastError, HANDLE},
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES,
                FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
            },
            System::{
                Pipes::{
                    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
                    GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
                },
                RemoteDesktop::ProcessIdToSessionId,
                Threading::GetCurrentProcessId,
            },
            UI::{
                Shell::{SetCurrentProcessExplicitAppUserModelID, UrlCreateFromPathW},
                WindowsAndMessaging::AllowSetForegroundWindow,
            },
        },
        core::{HSTRING, PWSTR},
    };

    /// The most a launch may hand over: the other platforms' batch limits.
    const MAX_MESSAGE: usize = 256 * 65537;

    pub(crate) struct NativeApplicationEvents {
        name: HSTRING,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    /// This session's pipe for `id`.
    fn pipe_name(id: &str) -> HSTRING {
        let mut session = 0;
        // SAFETY: reads this process's session into a local.
        let _ = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
        HSTRING::from(format!(r"\\.\pipe\{id}.{session}"))
    }

    /// The files and URLs among launch arguments, as a file association or a
    /// protocol handler passes them: existing paths as `file:` URLs (made
    /// absolute against this process's directory), and URLs. Flags and
    /// anything else are the application's own.
    pub(super) fn launch_urls(args: impl IntoIterator<Item = OsString>) -> Vec<String> {
        args.into_iter()
            .filter_map(|arg| {
                let text = arg.to_string_lossy();
                if text.starts_with('-') {
                    return None;
                }
                let path = Path::new(&arg);
                if path.exists() {
                    return file_url(&std::path::absolute(path).ok()?);
                }
                // `C:\x` is not a URL: a scheme is longer than a drive.
                let (scheme, _) = text.split_once(':')?;
                (scheme.len() > 1 && text.len() <= 65536 && crate::url::valid_url(&text))
                    .then(|| text.into_owned())
            })
            .take(256)
            .collect()
    }

    fn file_url(path: &Path) -> Option<String> {
        let mut url = vec![0u16; 32 * 1024];
        let mut length = url.len() as u32;
        // SAFETY: writes at most `length` units into `url`.
        unsafe {
            UrlCreateFromPathW(
                &HSTRING::from(path.as_os_str()),
                PWSTR(url.as_mut_ptr()),
                &mut length,
                0,
            )
        }
        .ok()?;
        Some(String::from_utf16_lossy(&url[..length as usize]))
    }

    /// What a launch's message asks: no URLs, a reopen.
    pub(super) fn event(message: &str) -> Option<ApplicationEvent> {
        if message.is_empty() {
            return Some(ApplicationEvent::Reopen);
        }
        let urls: Vec<String> = message.split('\n').map(str::to_owned).collect();
        (urls.len() <= 256
            && urls
                .iter()
                .all(|url| url.len() <= 65536 && crate::url::valid_url(url)))
        .then_some(ApplicationEvent::OpenUrls(urls))
    }

    /// A connection to the instance serving `name`, waiting while it is
    /// between two launches; `None` if none serves it.
    fn open(name: &HSTRING) -> Option<HANDLE> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            // SAFETY: opens a handle of ours, which the caller closes.
            match unsafe {
                CreateFileW(
                    name,
                    GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            } {
                Ok(pipe) => return Some(pipe),
                // SAFETY: reads this thread's last error; waits on the name.
                Err(_)
                    if unsafe { GetLastError() } == ERROR_PIPE_BUSY
                        && Instant::now() < deadline =>
                unsafe {
                    let _ = WaitNamedPipeW(name, 500);
                },
                Err(_) => return None,
            }
        }
    }

    /// Hand `urls` (none: reopen) to the instance serving `name`, letting it
    /// take the foreground; false if none answers.
    fn forward(name: &HSTRING, urls: &[String]) -> bool {
        let Some(pipe) = open(name) else {
            return false;
        };
        // SAFETY: our own handle to the pipe, closed before returning.
        unsafe {
            let mut server = 0;
            if GetNamedPipeServerProcessId(pipe, &mut server).is_ok() {
                let _ = AllowSetForegroundWindow(server);
            }
            let message = urls.join("\n");
            let mut written = 0;
            let sent = WriteFile(pipe, Some(message.as_bytes()), Some(&mut written), None).is_ok()
                && written as usize == message.len();
            let _ = CloseHandle(pipe);
            sent
        }
    }

    /// The pipe's one instance, or `None` when another process has it.
    fn claim(name: &HSTRING) -> Option<HANDLE> {
        // SAFETY: creates the pipe under our name, ours to close.
        let pipe = unsafe {
            CreateNamedPipeW(
                name,
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                64 * 1024,
                0,
                None,
            )
        };
        (!pipe.is_invalid()).then_some(pipe)
    }

    /// One launch's message, to the end of its writes.
    fn read(pipe: HANDLE) -> Option<String> {
        let mut message = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            let mut read = 0;
            // SAFETY: reads into `chunk`, at most its length.
            if unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut read), None) }.is_err()
                || read == 0
            {
                break;
            }
            message.extend_from_slice(&chunk[..read as usize]);
            if message.len() > MAX_MESSAGE {
                return None;
            }
        }
        String::from_utf8(message).ok()
    }

    impl NativeApplicationEvents {
        /// The running instance's events, or `None` when another instance
        /// serves `id` and this launch's files and URLs went to it.
        pub(crate) fn new(
            id: &str,
            handler: Handler,
        ) -> Result<Option<Self>, ApplicationEventError> {
            Self::serving(id, handler, launch_urls(std::env::args_os().skip(1)))
        }

        pub(super) fn serving(
            id: &str,
            handler: Handler,
            launch: Vec<String>,
        ) -> Result<Option<Self>, ApplicationEventError> {
            validate_id(id)?;
            // SAFETY: names this process before any window exists.
            unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(id)) }
                .map_err(|error| ApplicationEventError(error.to_string()))?;
            let name = pipe_name(id);
            let Some(pipe) = claim(&name) else {
                if forward(&name, &launch) {
                    return Ok(None);
                }
                return Err(ApplicationEventError(format!(
                    "application ID {id} is held by an instance that does not answer"
                )));
            };
            if !launch.is_empty() {
                handler(ApplicationEvent::OpenUrls(launch), None);
            }
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            // A HANDLE is not Send; the thread owns the pipe from here.
            let pipe = pipe.0 as usize;
            let thread = thread::Builder::new()
                .name("zgx-application-events".into())
                .spawn(move || {
                    let pipe = HANDLE(pipe as *mut _);
                    loop {
                        // SAFETY: our pipe; a launch that connected first
                        // reports an error and is connected all the same.
                        let _ = unsafe { ConnectNamedPipe(pipe, None) };
                        if stopped.load(Ordering::Acquire) {
                            break;
                        }
                        if let Some(event) = read(pipe).as_deref().and_then(event) {
                            handler(event, None);
                        }
                        // SAFETY: our pipe, listened on again.
                        let _ = unsafe { DisconnectNamedPipe(pipe) };
                    }
                    // SAFETY: our pipe, closed once.
                    let _ = unsafe { CloseHandle(pipe) };
                })
                .map_err(|error| ApplicationEventError(error.to_string()))?;
            Ok(Some(Self {
                name,
                stop,
                thread: Some(thread),
            }))
        }
    }

    impl Drop for NativeApplicationEvents {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            // Connect once to wake the server from its wait for a launch,
            // waiting out one it is still serving.
            if let Some(pipe) = open(&self.name) {
                // SAFETY: our own short-lived handle.
                let _ = unsafe { CloseHandle(pipe) };
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) use platform::NativeApplicationEvents;
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    fn a_second_windows_launch_hands_its_files_and_urls_to_the_first() {
        use std::sync::{Mutex, mpsc};
        let id = format!("org.zgx.test{}", std::process::id());
        let (send, receive) = mpsc::channel();
        let send = Mutex::new(send);
        let first = NativeApplicationEvents::serving(
            &id,
            Arc::new(move |event, _| send.lock().unwrap().send(event).unwrap()),
            vec!["https://example.com/launch".into()],
        )
        .unwrap()
        .expect("the first launch serves");
        let next = |launch: Vec<String>| {
            NativeApplicationEvents::serving(&id, Arc::new(|_, _| {}), launch)
                .unwrap()
                .is_none()
        };
        let wait = || {
            receive
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        };
        assert_eq!(
            wait(),
            ApplicationEvent::OpenUrls(vec!["https://example.com/launch".into()])
        );
        assert!(next(vec![
            "file:///C:/a%20b.txt".into(),
            "zgx-test:open".into()
        ]));
        assert_eq!(
            wait(),
            ApplicationEvent::OpenUrls(vec!["file:///C:/a%20b.txt".into(), "zgx-test:open".into()])
        );
        assert!(next(Vec::new()));
        assert_eq!(wait(), ApplicationEvent::Reopen);
        drop(first);
        // Once the first has gone, the next launch serves.
        let again = NativeApplicationEvents::serving(&id, Arc::new(|_, _| {}), Vec::new());
        assert!(again.unwrap().is_some());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_launch_arguments_are_files_and_urls_not_flags() {
        let file = std::env::current_exe().unwrap();
        let urls = platform::launch_urls([
            "--smoke-test".into(),
            file.into_os_string(),
            "myapp://open/thing".into(),
            r"C:\does\not\exist.txt".into(),
            "plain words".into(),
        ]);
        assert_eq!(urls.len(), 2, "{urls:?}");
        assert!(urls[0].starts_with("file:///"), "{urls:?}");
        assert_eq!(urls[1], "myapp://open/thing");
        assert_eq!(platform::event(""), Some(ApplicationEvent::Reopen));
        assert_eq!(platform::event("not a url"), None);
    }

    #[test]
    fn application_ids_are_valid_bus_names_and_paths() {
        assert!(validate_id("org.example.Editor").is_ok());
        for id in ["single", "org..app", "org.9app", "org.app/bad", "org.app\0"] {
            assert!(validate_id(id).is_err());
        }
    }
}
