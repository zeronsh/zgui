//! System notifications and the app icon's badge (`Platform::notify`,
//! `Platform::set_badge`), each where its platform keeps them:
//! - macOS: the Notification Center (`UNUserNotificationCenter`), which
//!   takes an app run from a bundle (an `Info.plist` with a
//!   `CFBundleIdentifier`) and asks the person once whether it may; and the
//!   Dock tile's badge.
//! - Linux: the desktop's notification server (`org.freedesktop.
//!   Notifications` on the session bus), and the badge docks read from a
//!   launcher entry (`com.canonical.Unity.LauncherEntry`), matched to the
//!   app's desktop file (`Application::desktop_id`).
//! - Windows: a toast (`ToastNotificationManager`), shown for the app's
//!   identity (`Application::application_id`, registered as its AppUserModelID).
//!
//! Anything else says why not (`NotifyError::Unsupported`).
use std::sync::OnceLock;
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

#[cfg_attr(target_os = "macos", allow(dead_code))]
/// Settled later with what `work`, run on a thread of its own, returns:
/// the system's answer may take a moment (a bus call, a permission).
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

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use dbus::arg::{RefArg, Variant};
    use std::{collections::HashMap, sync::Mutex, time::Duration};

    /// The server's id of each tagged notification shown: one posted with
    /// the same tag replaces it.
    static TAGGED: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);

    pub(crate) fn notify(notification: Notification) -> Pending<Result<(), NotifyError>> {
        let name = identity().name;
        off_thread(move || {
            let connection = dbus::blocking::Connection::new_session().map_err(|e| {
                NotifyError::Unsupported(format!("no session bus for notifications: {e}"))
            })?;
            let server = connection.with_proxy(
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                Duration::from_secs(5),
            );
            let replaces = notification
                .tag
                .as_ref()
                .and_then(|tag| TAGGED.lock().unwrap().as_ref()?.get(tag).copied())
                .unwrap_or(0);
            let actions: Vec<&str> = Vec::new();
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
                    Some("org.freedesktop.DBus.Error.ServiceUnknown") => NotifyError::Unsupported(
                        "no notification server runs on this desktop".into(),
                    ),
                    _ => NotifyError::Failed(e.to_string()),
                })?;
            if let Some(tag) = notification.tag {
                TAGGED
                    .lock()
                    .unwrap()
                    .get_or_insert_default()
                    .insert(tag, id);
            }
            Ok(())
        })
    }

    thread_local! {
        /// The bus the badge is told on, kept open: docks drop an entry
        /// whose sender leaves.
        static BADGE: std::cell::RefCell<Option<dbus::blocking::Connection>> =
            const { std::cell::RefCell::new(None) };
    }

    pub(crate) fn set_badge(badge: Option<&str>) -> bool {
        let identity = identity();
        let Some(id) = identity.desktop_id.or(identity.application_id) else {
            return false;
        };
        // Docks read a count: text that is no number shows as one.
        let count = badge.map_or(0, |badge| badge.trim().parse::<i64>().unwrap_or(1));
        BADGE.with_borrow_mut(|bus| {
            if bus.is_none() {
                *bus = dbus::blocking::Connection::new_session().ok();
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
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError, NSString, NSUUID};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNUserNotificationCenter,
    };

    pub(crate) fn notify(notification: Notification) -> Pending<Result<(), NotifyError>> {
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

    pub(crate) fn set_badge(badge: Option<&str>) -> bool {
        let Some(main) = objc2::MainThreadMarker::new() else {
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
        UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
        Win32::System::{
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
                RegCreateKeyExW, RegSetValueExW,
            },
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
        },
        Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID,
        core::{HSTRING, PCWSTR},
    };

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

    pub(crate) fn notify(notification: Notification) -> Pending<Result<(), NotifyError>> {
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
            if let Some(tag) = &notification.tag {
                // At most 64 characters, as Windows takes a tag.
                let tag: String = tag.chars().take(64).collect();
                toast.SetTag(&HSTRING::from(tag)).map_err(failed)?;
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
            notifier.Show(&toast).map_err(failed)
        })
    }

    pub(crate) fn set_badge(badge: Option<&str>) -> bool {
        let _ = badge;
        false
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use super::*;

    pub(crate) fn notify(notification: Notification) -> Pending<Result<(), NotifyError>> {
        let _ = notification;
        let _ = off_thread::<()>;
        Box::pin(async {
            Err(NotifyError::Unsupported(
                "this system shows no notifications from zgx".into(),
            ))
        })
    }

    pub(crate) fn set_badge(badge: Option<&str>) -> bool {
        let _ = badge;
        false
    }
}

pub(crate) use imp::{notify, set_badge};
