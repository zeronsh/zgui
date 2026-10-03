//! Owned operating-system application activation and open-URL callbacks.
use std::{fmt, sync::Arc};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationEvent {
    OpenUrls(Vec<String>),
    Reopen,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEventError(pub String);
impl fmt::Display for ApplicationEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ApplicationEventError {}
pub(crate) type Handler = Arc<dyn Fn(ApplicationEvent) + Send + Sync>;
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
        arg::PropMap,
        blocking::{Connection, stdintf::org_freedesktop_dbus::RequestNameReply},
        channel::MatchingReceiver,
        message::MatchRule,
    };
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };
    pub(crate) struct NativeApplicationEvents {
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl NativeApplicationEvents {
        pub(crate) fn new(id: &str, handler: Handler) -> Result<Self, ApplicationEventError> {
            validate_id(id)?;
            let id = id.to_owned();
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let (ready, started) = mpsc::sync_channel(1);
            let thread=thread::Builder::new().name("zgui-application-events".into()).spawn(move || {
                let setup=(||->Result<_,ApplicationEventError>{
                    let connection=Connection::new_session().map_err(|e|ApplicationEventError(e.to_string()))?;
                    let reply=connection.request_name(&id,false,false,true).map_err(|e|ApplicationEventError(e.to_string()))?;
                    if reply!=RequestNameReply::PrimaryOwner {return Err(ApplicationEventError(format!("application ID {id} is already owned")))}
                    let mut routes=dbus_crossroads::Crossroads::new();
                    let interface=routes.register("org.freedesktop.Application",|builder| {
                        builder.method("Activate",("platform_data",),(),|_:&mut dbus_crossroads::Context,handler:&mut Handler,(_,): (PropMap,)| {
                            handler(ApplicationEvent::Reopen);Ok(())
                        });
                        builder.method("Open",("uris","platform_data"),(),|_:&mut dbus_crossroads::Context,handler:&mut Handler,(urls,_):(Vec<String>,PropMap)| {
                            if urls.len()>256 || urls.iter().any(|url|url.len()>65536 || !crate::url::valid_url(url)) {
                                return Err(dbus::MethodErr::invalid_arg("invalid URL batch"));
                            }
                            handler(ApplicationEvent::OpenUrls(urls));Ok(())
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
                        while !stopped.load(Ordering::Acquire) {
                            if connection.process(Duration::from_millis(100)).is_err(){break}
                        }
                    }
                    Err(error)=>{let _=ready.send(Err(error));}
                }
            }).map_err(|e|ApplicationEventError(e.to_string()))?;
            match started.recv_timeout(Duration::from_secs(3)) {
                Ok(Ok(())) => Ok(Self {
                    stop,
                    thread: Some(thread),
                }),
                result => {
                    stop.store(true, Ordering::Release);
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
            self.stop.store(true, Ordering::Release);
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
                (self.ivars().handler)(ApplicationEvent::Reopen);
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
                (self.ivars().handler)(ApplicationEvent::OpenUrls(urls));
            }
        }
    }
    pub(crate) struct NativeApplicationEvents {
        manager: Retained<NSAppleEventManager>,
        _handler: Retained<EventHandler>,
    }
    impl NativeApplicationEvents {
        pub(crate) fn new(id: &str, handler: Handler) -> Result<Self, ApplicationEventError> {
            validate_id(id)?;
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                ApplicationEventError("application events require the main thread".into())
            })?;
            // Winit owns its NSApplicationDelegate and downcasts that object
            // during event delivery. Subscribe to AppleEvents without replacing it.
            let handler = EventHandler::alloc(mtm).set_ivars(State { handler });
            // SAFETY: NSObject initializer on our allocated main-thread class.
            let handler: Retained<EventHandler> = unsafe { msg_send![super(handler), init] };
            let manager = NSAppleEventManager::sharedAppleEventManager();
            let service = Self {
                manager,
                _handler: handler,
            };
            service.install();
            Ok(service)
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
                // SAFETY: Selectors have Apple's two-descriptor handler signature;
                // this service retains the receiver until unregistered.
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
    use super::*;

    pub(crate) struct NativeApplicationEvents;
    impl NativeApplicationEvents {
        pub(crate) fn new(id: &str, _handler: Handler) -> Result<Self, ApplicationEventError> {
            validate_id(id)?;
            Err(ApplicationEventError(
                "Windows URL activation is not registered by zgui; handle launch arguments in the application".into(),
            ))
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
    fn windows_activation_is_explicitly_unsupported() {
        let result = NativeApplicationEvents::new("org.example.Editor", Arc::new(|_| {}));
        let Err(error) = result else {
            panic!("Windows activation must not silently succeed without a backend");
        };
        assert!(error.0.contains("Windows URL activation"));
    }

    #[test]
    fn application_ids_are_valid_bus_names_and_paths() {
        assert!(validate_id("org.example.Editor").is_ok());
        for id in ["single", "org..app", "org.9app", "org.app/bad", "org.app\0"] {
            assert!(validate_id(id).is_err());
        }
    }
}
