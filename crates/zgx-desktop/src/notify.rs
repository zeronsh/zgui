//! System notifications, their clicks, and the app icon's badge
//! (`Platform::notify`, `Platform::next_notification_click`,
//! `Platform::set_badge`), each where its platform keeps them:
//! - macOS: the Notification Center (`UNUserNotificationCenter`), which
//!   takes an app run from a bundle (an `Info.plist` with a
//!   `CFBundleIdentifier`) and asks the person once whether it may; its
//!   delegate hears a click. The badge is the Dock tile's.
//! - Windows: a toast (`ToastNotificationManager`) for the app's identity
//!   (`Application::application_id`): set as the process's AppUserModelID
//!   and registered under `HKCU\Software\Classes\AppUserModelId\<id>`
//!   with its `DisplayName`, as an unpackaged app's toasts need. A toast's
//!   `Activated` is its click. The badge is an overlay on the taskbar
//!   button (`ITaskbarList3::SetOverlayIcon`): the count drawn here.
//! - Linux: the desktop's notification server (`org.freedesktop.
//!   Notifications`), on a connection of its own that stays open, as a
//!   server tells a click (`ActionInvoked`) to the connection that posted;
//!   and the count docks read from a launcher entry
//!   (`com.canonical.Unity.LauncherEntry`), matched to the app's desktop
//!   file (`Application::desktop_id`).
//!
//! Anything else says why not (`NotifyError::Unsupported`).
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    task::{Poll, Waker},
};
use zgx::platform::{Notification, NotifyError, Pending};

/// Who the app is to the system: its application id (reverse-DNS) and its
/// desktop file id, as `Application` was given them; and its name.
#[derive(Clone, Debug, Default)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Identity {
    pub application_id: Option<String>,
    pub desktop_id: Option<String>,
    pub name: String,
}
static IDENTITY: OnceLock<Identity> = OnceLock::new();

/// Record who the app is, once, as it starts.
pub(crate) fn set_identity(identity: Identity) {
    let _ = IDENTITY.set(identity);
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn identity() -> Identity {
    IDENTITY.get().cloned().unwrap_or_default()
}

/// Clicked notifications' tags, from whatever thread the system tells one
/// on, for the main thread's `next_click`.
struct Clicks {
    tags: VecDeque<String>,
    waker: Option<Waker>,
}
static CLICKS: Mutex<Clicks> = Mutex::new(Clicks {
    tags: VecDeque::new(),
    waker: None,
});

/// The notification tagged `tag` was clicked.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos", windows)),
    allow(dead_code)
)]
fn clicked(tag: String) {
    let waker = {
        let mut clicks = CLICKS.lock().unwrap();
        clicks.tags.push_back(tag);
        clicks.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// The next notification clicked: its tag.
pub(crate) fn next_click() -> Pending<Option<String>> {
    Box::pin(std::future::poll_fn(|cx| {
        let mut clicks = CLICKS.lock().unwrap();
        match clicks.tags.pop_front() {
            Some(tag) => Poll::Ready(Some(tag)),
            None => {
                clicks.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }))
}

/// A count as the badge shows it.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn count_of(badge: &str) -> Option<u32> {
    badge.trim().parse::<u32>().ok()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use dbus::{
        arg::{RefArg, Variant},
        blocking::Connection,
        message::MatchRule,
    };
    use std::{
        collections::HashMap,
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
        sync::Arc,
        time::Duration,
    };

    /// A notification to post, and where its answer goes.
    struct Post {
        notification: Notification,
        answer: zgx::task::LatestSender<Result<(), NotifyError>>,
    }
    /// The thread that talks to the notification server: what it is asked
    /// to post, and the descriptor that wakes it.
    struct Poster {
        posts: Mutex<VecDeque<Post>>,
        wake: OwnedFd,
    }
    impl Poster {
        fn post(&self, post: Post) {
            self.posts.lock().unwrap().push_back(post);
            let one = 1u64;
            // SAFETY: 8 bytes, a u64, to the eventfd `self` owns.
            unsafe { libc::write(self.wake.as_raw_fd(), (&one as *const u64).cast(), 8) };
        }
    }
    static POSTER: OnceLock<Option<Arc<Poster>>> = OnceLock::new();

    /// The poster's thread, started the first time: none where there is
    /// no session bus.
    fn poster() -> Option<Arc<Poster>> {
        POSTER
            .get_or_init(|| {
                // SAFETY: takes no pointers; a new descriptor or -1.
                let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
                if fd < 0 {
                    return None;
                }
                // SAFETY: the descriptor just made, owned by nothing else.
                let wake = unsafe { OwnedFd::from_raw_fd(fd) };
                let connection = crate::linux_dbus::session().ok()?;
                let poster = Arc::new(Poster {
                    posts: Mutex::default(),
                    wake,
                });
                let serving = poster.clone();
                std::thread::Builder::new()
                    .name("zgx notifications".into())
                    .spawn(move || serve(connection, &serving))
                    .ok()?;
                Some(poster)
            })
            .clone()
    }

    /// Post what is asked and hear clicks, sleeping between.
    fn serve(connection: Connection, poster: &Poster) {
        // The server's id of each notification shown, both ways: a tag
        // posted again replaces its notification, and a click is told by id.
        let ids: Arc<Mutex<HashMap<u32, String>>> = Arc::default();
        let mut tags: HashMap<String, u32> = HashMap::new();
        let heard = ids.clone();
        let rule = MatchRule::new_signal("org.freedesktop.Notifications", "ActionInvoked");
        let _ = connection.add_match(rule, move |(id, _action): (u32, String), _, _| {
            if let Some(tag) = heard.lock().unwrap().get(&id).cloned() {
                clicked(tag);
            }
            true
        });
        loop {
            let posts: Vec<Post> = poster.posts.lock().unwrap().drain(..).collect();
            for post in posts {
                let replaces = (post.notification.tag.as_ref())
                    .and_then(|tag| tags.get(tag).copied())
                    .unwrap_or(0);
                let answer = notify(&connection, &post.notification, replaces).map(|id| {
                    if let Some(tag) = &post.notification.tag {
                        tags.insert(tag.clone(), id);
                        ids.lock().unwrap().insert(id, tag.clone());
                    }
                });
                post.answer.send(answer);
            }
            // What has arrived, then sleep until the bus or a post wakes it.
            loop {
                match connection.process(Duration::ZERO) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            connection.channel().flush();
            let watch = connection.channel().watch();
            let mut fds = [
                libc::pollfd {
                    fd: watch.fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: poster.wake.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: two pollfds, as many as given.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if ready < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return;
            }
            if fds[1].revents & libc::POLLIN != 0 {
                let mut count = 0u64;
                // SAFETY: 8 bytes into a u64, from the eventfd.
                unsafe { libc::read(poster.wake.as_raw_fd(), (&mut count as *mut u64).cast(), 8) };
            }
        }
    }

    fn notify(
        connection: &Connection,
        notification: &Notification,
        replaces: u32,
    ) -> Result<u32, NotifyError> {
        let name = identity().name;
        let server = connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Duration::from_secs(5),
        );
        // "default": a click on the notification itself, told back.
        let actions = vec!["default", "Open"];
        let hints: HashMap<&str, Variant<Box<dyn RefArg>>> = HashMap::new();
        let (id,): (u32,) = server
            .method_call(
                "org.freedesktop.Notifications",
                "Notify",
                (
                    name.as_str(),
                    replaces,
                    "",
                    notification.title.as_str(),
                    notification.body.as_str(),
                    actions,
                    hints,
                    -1_i32,
                ),
            )
            .map_err(|e| match e.name() {
                Some("org.freedesktop.DBus.Error.ServiceUnknown") => {
                    NotifyError::Unsupported("no notification server runs on this desktop".into())
                }
                _ => NotifyError::Failed(e.to_string()),
            })?;
        Ok(id)
    }

    pub(crate) fn notify_now(notification: Notification) -> Pending<Result<(), NotifyError>> {
        let Some(poster) = poster() else {
            return Box::pin(async {
                Err(NotifyError::Unsupported(
                    "no session bus for notifications".into(),
                ))
            });
        };
        let (answer, mut heard) = zgx::task::latest::<Result<(), NotifyError>>();
        poster.post(Post {
            notification,
            answer,
        });
        Box::pin(async move {
            heard
                .recv()
                .await
                .unwrap_or_else(|| Err(NotifyError::Failed("the notification went nowhere".into())))
        })
    }

    thread_local! {
        /// The bus the badge is told on, kept open: docks drop an entry
        /// whose sender leaves.
        static BADGE: std::cell::RefCell<Option<Connection>> =
            const { std::cell::RefCell::new(None) };
    }

    pub(crate) fn set_badge(badge: Option<&str>, _window: Option<&winit::window::Window>) -> bool {
        let identity = identity();
        let Some(id) = identity.desktop_id.or(identity.application_id) else {
            return false;
        };
        // Docks read a count: text that is no number shows as one.
        let count = badge.map_or(0, |badge| count_of(badge).map_or(1, i64::from));
        BADGE.with_borrow_mut(|bus| {
            if bus.is_none() {
                *bus = Connection::new_session().ok();
            }
            let Some(bus) = bus.as_ref() else {
                return false;
            };
            let mut properties: HashMap<&str, Variant<Box<dyn RefArg>>> = HashMap::new();
            properties.insert("count", Variant(Box::new(count)));
            properties.insert("count-visible", Variant(Box::new(count > 0)));
            let Ok(signal) = dbus::Message::new_signal(
                "/com/zeron/zgx/LauncherEntry",
                "com.canonical.Unity.LauncherEntry",
                "Update",
            ) else {
                return false;
            };
            let signal = signal.append2(format!("application://{id}.desktop"), properties);
            use dbus::channel::Sender;
            bus.send(signal).is_ok()
        })
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use block2::RcBlock;
    use objc2::{
        AllocAnyThread, MainThreadMarker, define_class, msg_send,
        rc::Retained,
        runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject},
    };
    use objc2_foundation::{NSBundle, NSError, NSString, NSUUID};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };

    // SAFETY: an NSObject subclass with no ivars; each method has its
    // protocol's signature, and calls the completion handler it is given.
    define_class!(
        #[unsafe(super(NSObject))]
        #[ivars = ()]
        struct Delegate;
        unsafe impl NSObjectProtocol for Delegate {}
        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            // A click: the notification's identifier is the tag it was
            // posted with.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                done: &block2::DynBlock<dyn Fn()>,
            ) {
                let tag = response.notification().request().identifier().to_string();
                clicked(tag);
                done.call(());
            }
            // Shown as a banner even while the app is in front.
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                done.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
            }
        }
    );
    impl Delegate {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(());
            // SAFETY: NSObject's init, on a freshly allocated object.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// The center's delegate, made once and kept for the process: the
    /// center holds it weakly.
    fn delegate(center: &UNUserNotificationCenter) {
        static SET: OnceLock<()> = OnceLock::new();
        SET.get_or_init(|| {
            let delegate = Delegate::new();
            center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            std::mem::forget(delegate);
        });
    }

    pub(crate) fn notify_now(notification: Notification) -> Pending<Result<(), NotifyError>> {
        // The center raises an exception for a process with no bundle:
        // say so instead.
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            return Box::pin(async {
                Err(NotifyError::Unsupported(
                    "macOS shows an app's notifications only when it runs from an app bundle \
                     (an Info.plist with a CFBundleIdentifier)"
                        .into(),
                ))
            });
        }
        let (tell, mut heard) = zgx::task::latest::<Result<(), NotifyError>>();
        let center = UNUserNotificationCenter::currentNotificationCenter();
        delegate(&center);
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));
        let identifier = match &notification.tag {
            Some(tag) => NSString::from_str(tag),
            None => NSUUID::UUID().UUIDString(),
        };
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        // Asked first: the system asks the person the first time, and
        // answers at once after.
        let asked = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            if !granted.as_bool() {
                // SAFETY: the system passes an NSError or null.
                let why = unsafe { error.as_ref() }
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| {
                        "notifications are off for this app (System Settings › Notifications)"
                            .into()
                    });
                tell.send(Err(NotifyError::Denied(why)));
                return;
            }
            let tell = tell.clone();
            let added = RcBlock::new(move |error: *mut NSError| {
                // SAFETY: as above.
                tell.send(match unsafe { error.as_ref() } {
                    Some(e) => Err(NotifyError::Failed(e.localizedDescription().to_string())),
                    None => Ok(()),
                });
            });
            UNUserNotificationCenter::currentNotificationCenter()
                .addNotificationRequest_withCompletionHandler(&request, Some(&added));
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &asked,
        );
        Box::pin(async move {
            heard
                .recv()
                .await
                .unwrap_or_else(|| Err(NotifyError::Failed("the system did not answer".into())))
        })
    }

    pub(crate) fn set_badge(badge: Option<&str>, _window: Option<&winit::window::Window>) -> bool {
        let Some(main) = MainThreadMarker::new() else {
            return false;
        };
        let app = objc2_app_kit::NSApplication::sharedApplication(main);
        let label = badge.map(NSString::from_str);
        app.dockTile().setBadgeLabel(label.as_deref());
        true
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::{
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
        Win32::{
            Foundation::HWND,
            Graphics::Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection,
                DIB_RGB_COLORS, DeleteObject, HBITMAP,
            },
            System::{
                Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
                Registry::{
                    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
                    RegCloseKey, RegCreateKeyExW, RegSetValueExW,
                },
                WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
            },
            UI::{
                Shell::{ITaskbarList3, SetCurrentProcessExplicitAppUserModelID, TaskbarList},
                WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO},
            },
        },
        core::{HSTRING, IInspectable, PCWSTR},
    };

    /// Settled later with what `work`, run on a thread of its own, returns.
    fn off_thread<T: Send + 'static>(
        work: impl FnOnce() -> Result<T, NotifyError> + Send + 'static,
    ) -> Pending<Result<T, NotifyError>> {
        let (tell, mut heard) = zgx::task::latest::<Result<T, NotifyError>>();
        std::thread::spawn(move || tell.send(work()));
        Box::pin(async move {
            heard
                .recv()
                .await
                .unwrap_or_else(|| Err(NotifyError::Failed("the notification went nowhere".into())))
        })
    }

    /// Text as toast XML takes it.
    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    /// The app's AppUserModelID, set for the process and registered under
    /// the user's classes with its name, as an unpackaged app's toasts need.
    fn register(aumid: &str, name: &str) {
        let id = HSTRING::from(aumid);
        // SAFETY: a valid wide string for the call's length.
        let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&id) };
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{aumid}"));
        let mut opened = HKEY::default();
        // SAFETY: valid strings and an out pointer to a key closed below.
        let made = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &key,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut opened,
                None,
            )
        };
        if made.is_err() {
            return;
        }
        let value: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: `value` is a NUL-terminated UTF-16 string of the length given.
        unsafe {
            let bytes = std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2);
            let _ = RegSetValueExW(
                opened,
                &HSTRING::from("DisplayName"),
                None,
                REG_SZ,
                Some(bytes),
            );
            let _ = RegCloseKey(opened);
        }
    }

    /// Toasts shown, kept while they may be clicked (their `Activated`
    /// fires only while the object lives): the newest few, by tag.
    static SHOWN: Mutex<Vec<(String, ToastNotification)>> = Mutex::new(Vec::new());

    pub(crate) fn notify_now(notification: Notification) -> Pending<Result<(), NotifyError>> {
        let identity = identity();
        let Some(aumid) = identity.application_id.clone() else {
            return Box::pin(async {
                Err(NotifyError::Unsupported(
                    "Windows shows toasts for an app's identity: give it one with \
                     Application::application_id"
                        .into(),
                ))
            });
        };
        let name = if identity.name.is_empty() {
            aumid.clone()
        } else {
            identity.name.clone()
        };
        off_thread(move || {
            // SAFETY: initializes this thread's own apartment, once.
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            static REGISTERED: OnceLock<()> = OnceLock::new();
            REGISTERED.get_or_init(|| register(&aumid, &name));
            let failed = |e: windows::core::Error| NotifyError::Failed(e.message().to_string());
            let xml = XmlDocument::new().map_err(failed)?;
            xml.LoadXml(&HSTRING::from(format!(
                "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
                escape(&notification.title),
                escape(&notification.body)
            )))
            .map_err(failed)?;
            let toast = ToastNotification::CreateToastNotification(&xml).map_err(failed)?;
            // At most 64 characters, as Windows takes a tag.
            let tag: String = (notification.tag.clone().unwrap_or_default())
                .chars()
                .take(64)
                .collect();
            if !tag.is_empty() {
                toast.SetTag(&HSTRING::from(&tag)).map_err(failed)?;
                let told = tag.clone();
                toast
                    .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                        move |_, _| {
                            clicked(told.clone());
                            Ok(())
                        },
                    ))
                    .map_err(failed)?;
            }
            let notifier =
                ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(&aumid))
                    .map_err(failed)?;
            match notifier.Setting() {
                Ok(NotificationSetting::Enabled) | Err(_) => {}
                Ok(_) => {
                    return Err(NotifyError::Denied(
                        "notifications are off for this app (Settings › System › Notifications)"
                            .into(),
                    ));
                }
            }
            notifier.Show(&toast).map_err(failed)?;
            let mut shown = SHOWN.lock().unwrap();
            shown.retain(|(kept, _)| *kept != tag);
            shown.push((tag, toast));
            let excess = shown.len().saturating_sub(32);
            shown.drain(..excess);
            Ok(())
        })
    }

    /// A tiny font for the badge's count: 3 by 5 pixels a glyph, rows from
    /// the top, 4 the leftmost column.
    const GLYPHS: [[u8; 5]; 11] = [
        [7, 5, 5, 5, 7], // 0
        [2, 6, 2, 2, 7], // 1
        [7, 1, 7, 4, 7], // 2
        [7, 1, 7, 1, 7], // 3
        [5, 5, 7, 1, 1], // 4
        [7, 4, 7, 1, 7], // 5
        [7, 4, 7, 5, 7], // 6
        [7, 1, 1, 1, 1], // 7
        [7, 5, 7, 5, 7], // 8
        [7, 5, 7, 1, 7], // 9
        [0, 2, 7, 2, 0], // +
    ];

    /// The badge drawn, 16 by 16 premultiplied BGRA: a red disc and the
    /// count in white (two digits at most, "9+" past them), a plain disc
    /// for text that is no count.
    fn badge_pixels(badge: &str) -> Vec<u32> {
        const SIZE: usize = 16;
        let mut pixels = vec![0u32; SIZE * SIZE];
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (dx, dy) = (x as f32 + 0.5 - 8., y as f32 + 0.5 - 8.);
                let coverage = (8. - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
                let a = (coverage * 255.) as u32;
                // #D93025, premultiplied.
                let (r, g, b) = (0xD9 * a / 255, 0x30 * a / 255, 0x25 * a / 255);
                pixels[y * SIZE + x] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
        let text: Vec<usize> = match count_of(badge) {
            Some(n) if n < 10 => vec![n as usize],
            Some(n) if n < 100 => vec![(n / 10) as usize, (n % 10) as usize],
            Some(_) => vec![9, 10],
            None => vec![],
        };
        // Glyphs at twice their size, a pixel apart, centred.
        let scale = 2;
        let width = text.len() * 3 * scale + text.len().saturating_sub(1);
        let (left, top) = ((SIZE - width.min(SIZE)) / 2, (SIZE - 5 * scale) / 2);
        for (i, glyph) in text.iter().enumerate() {
            let at = left + i * (3 * scale + 1);
            for (row, bits) in GLYPHS[*glyph].iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) == 0 {
                        continue;
                    }
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let (x, y) = (at + col * scale + sx, top + row * scale + sy);
                            if x < SIZE && y < SIZE {
                                pixels[y * SIZE + x] = 0xFFFF_FFFF;
                            }
                        }
                    }
                }
            }
        }
        pixels
    }

    /// The badge as an icon, its alpha its own.
    fn badge_icon(badge: &str) -> Option<HICON> {
        let pixels = badge_pixels(badge);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 16,
                // Negative: rows from the top.
                biHeight: -16,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: a 16 by 16 32-bit DIB; `bits` receives its pixel memory,
        // filled with as many pixels below; both bitmaps are deleted once
        // the icon has copied them.
        unsafe {
            let color: HBITMAP =
                CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
            let mask = [0u8; 32];
            let mask = CreateBitmap(16, 16, 1, 1, Some(mask.as_ptr().cast()));
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            });
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            icon.ok()
        }
    }

    thread_local! {
        /// The taskbar, made the first time a badge is set.
        static TASKBAR: std::cell::RefCell<Option<ITaskbarList3>> =
            const { std::cell::RefCell::new(None) };
    }

    pub(crate) fn set_badge(badge: Option<&str>, window: Option<&winit::window::Window>) -> bool {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Some(window) = window else { return false };
        let Some(RawWindowHandle::Win32(handle)) = window.window_handle().ok().map(|h| h.as_raw())
        else {
            return false;
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        TASKBAR.with_borrow_mut(|taskbar| {
            if taskbar.is_none() {
                // SAFETY: COM is initialized on the UI thread (winit's OLE).
                *taskbar = unsafe {
                    CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
                }
                .ok()
                // SAFETY: the taskbar list just made.
                .filter(|list| unsafe { list.HrInit() }.is_ok());
            }
            let Some(list) = taskbar.as_ref() else {
                return false;
            };
            match badge {
                None => {
                    // The description reset first, as its own call: with the
                    // icon taken off by a null one alone, the taskbar button
                    // went on telling UI Automation the last count.
                    let reset = HSTRING::new();
                    // SAFETY: the window's handle, live while `window` is.
                    unsafe {
                        let _ = list.SetOverlayIcon(hwnd, HICON::default(), &reset);
                        list.SetOverlayIcon(hwnd, HICON::default(), PCWSTR::null())
                    }
                    .is_ok()
                }
                Some(badge) => {
                    let Some(icon) = badge_icon(badge) else {
                        return false;
                    };
                    // What a screen reader says of the taskbar button.
                    let description = HSTRING::from(match count_of(badge) {
                        Some(n) => format!("{n} unread"),
                        None => badge.to_string(),
                    });
                    // SAFETY: as above; the taskbar copies the icon, then it
                    // is destroyed.
                    let set = unsafe { list.SetOverlayIcon(hwnd, icon, &description) }.is_ok();
                    // SAFETY: the icon made above, no longer used.
                    let _ = unsafe { DestroyIcon(icon) };
                    set
                }
            }
        })
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use super::*;

    pub(crate) fn notify_now(notification: Notification) -> Pending<Result<(), NotifyError>> {
        let _ = notification;
        Box::pin(async {
            Err(NotifyError::Unsupported(
                "this system shows no notifications from zgx".into(),
            ))
        })
    }

    pub(crate) fn set_badge(badge: Option<&str>, _window: Option<&winit::window::Window>) -> bool {
        let _ = badge;
        false
    }
}

pub(crate) use imp::{notify_now as notify, set_badge};
