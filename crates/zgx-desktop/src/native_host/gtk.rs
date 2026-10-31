//! WebKitGTK and the GTK 3, GDK, GLib, cairo, JavaScriptCore and libsoup
//! calls its pages need, opened at run time (`api`): a build with the
//! `webkitgtk` feature needs none of them to build, and runs without them
//! (pages are then placeholders). Declared as the 4.1 API's headers declare
//! them; the event structs are GDK 3's, whose layout is its ABI.
#![allow(non_camel_case_types)]
use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void};

pub type gboolean = c_int;
pub type GType = usize;
pub type GCallback = unsafe extern "C" fn();
pub type Ptr = *mut c_void;

/// The libraries, by soname, in the order symbols are looked up.
const LIBRARIES: [&str; 9] = [
    "libwebkit2gtk-4.1.so.0",
    "libjavascriptcoregtk-4.1.so.0",
    "libgtk-3.so.0",
    "libgdk-3.so.0",
    "libgobject-2.0.so.0",
    "libglib-2.0.so.0",
    "libgio-2.0.so.0",
    "libcairo.so.2",
    "libsoup-3.0.so.0",
];

macro_rules! api {
    ($( fn $name:ident($($arg:ty),*) $(-> $ret:ty)?; )*) => {
        /// The calls, each looked up in the first library that has it.
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
            /// `g_object_new(type, first_property, …, NULL)`.
            pub g_object_new: unsafe extern "C" fn(GType, *const c_char, ...) -> Ptr,
            /// `g_signal_emit_by_name(instance, signal, …)`.
            pub g_signal_emit_by_name: unsafe extern "C" fn(Ptr, *const c_char, ...),
            _libraries: Vec<Library>,
        }
        impl Api {
            fn load() -> Result<Self, String> {
                let libraries = LIBRARIES
                    .iter()
                    // SAFETY: system libraries whose initializers expect
                    // nothing of the process.
                    .map(|name| unsafe { Library::new(name) }.map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                fn find<T: Copy>(libraries: &[Library], name: &str) -> Result<T, String> {
                    let symbol = format!("{name}\0");
                    libraries
                        .iter()
                        // SAFETY: each field's type is the function's
                        // declared C signature.
                        .find_map(|library| unsafe { library.get::<T>(symbol.as_bytes()) }.ok().map(|s| *s))
                        .ok_or_else(|| format!("{name} not found"))
                }
                Ok(Self {
                    $( $name: find(&libraries, stringify!($name))?, )*
                    g_object_new: find(&libraries, "g_object_new")?,
                    g_signal_emit_by_name: find(&libraries, "g_signal_emit_by_name")?,
                    _libraries: libraries,
                })
            }
        }
    };
}

api! {
    // GLib and GObject.
    fn g_idle_add_full(c_int, unsafe extern "C" fn(Ptr) -> gboolean, Ptr, Option<unsafe extern "C" fn(Ptr)>) -> c_uint;
    fn g_timeout_add_full(c_int, c_uint, unsafe extern "C" fn(Ptr) -> gboolean, Ptr, Option<unsafe extern "C" fn(Ptr)>) -> c_uint;
    fn g_free(Ptr);
    fn g_strdup(*const c_char) -> *mut c_char;
    fn g_list_free(*mut GList);
    fn g_bytes_new(*const c_void, usize) -> Ptr;
    fn g_bytes_get_data(Ptr, *mut usize) -> *const c_void;
    fn g_bytes_unref(Ptr);
    fn g_error_free(*mut GError);
    fn g_error_new_literal(u32, c_int, *const c_char) -> *mut GError;
    fn g_quark_from_static_string(*const c_char) -> u32;
    fn g_object_ref(Ptr) -> Ptr;
    fn g_object_unref(Ptr);
    fn g_signal_connect_data(Ptr, *const c_char, GCallback, Ptr, Option<unsafe extern "C" fn(Ptr, Ptr)>, c_int) -> u64;
    fn g_signal_handlers_disconnect_matched(Ptr, c_int, c_uint, u32, Ptr, Ptr, Ptr) -> c_uint;
    fn g_type_query(GType, *mut GTypeQuery);
    fn g_type_register_static(GType, *const c_char, *const GTypeInfo, c_int) -> GType;
    fn g_memory_input_stream_new_from_bytes(Ptr) -> Ptr;
    fn g_input_stream_read_bytes(Ptr, usize, Ptr, *mut *mut GError) -> Ptr;
    // GTK and GDK.
    fn gtk_init_check(*mut c_int, *mut *mut *mut c_char) -> gboolean;
    fn gtk_main();
    fn gtk_offscreen_window_new() -> Ptr;
    fn gtk_window_resize(Ptr, c_int, c_int);
    fn gtk_widget_set_size_request(Ptr, c_int, c_int);
    fn gtk_container_add(Ptr, Ptr);
    fn gtk_widget_show_all(Ptr);
    fn gtk_widget_destroy(Ptr);
    fn gtk_widget_show(Ptr);
    fn gtk_widget_hide(Ptr);
    fn gtk_widget_draw(Ptr, Ptr);
    fn gtk_widget_grab_focus(Ptr);
    fn gtk_widget_get_window(Ptr) -> Ptr;
    fn gtk_widget_send_focus_change(Ptr, *mut GdkEventFocus) -> gboolean;
    fn gtk_widget_get_allocated_width(Ptr) -> c_int;
    fn gtk_widget_get_allocated_height(Ptr) -> c_int;
    fn gtk_main_do_event(Ptr);
    fn gdk_error_trap_push();
    fn gdk_event_new(c_int) -> Ptr;
    fn gdk_event_free(Ptr);
    fn gdk_event_set_device(Ptr, Ptr);
    fn gdk_event_set_source_device(Ptr, Ptr);
    fn gdk_display_get_default() -> Ptr;
    fn gdk_display_get_name(Ptr) -> *const c_char;
    fn gtk_clipboard_get_for_display(Ptr, Ptr) -> Ptr;
    fn gtk_clipboard_set_text(Ptr, *const c_char, c_int);
    fn gtk_clipboard_request_text(Ptr, unsafe extern "C" fn(Ptr, *const c_char, Ptr), Ptr);
    fn gdk_display_get_default_seat(Ptr) -> Ptr;
    fn gdk_seat_get_pointer(Ptr) -> Ptr;
    fn gdk_seat_get_keyboard(Ptr) -> Ptr;
    fn gdk_cursor_new_from_name(Ptr, *const c_char) -> Ptr;
    fn gdk_window_get_cursor(Ptr) -> Ptr;
    fn gdk_unicode_to_keyval(u32) -> c_uint;
    // cairo.
    fn cairo_image_surface_create(c_int, c_int, c_int) -> Ptr;
    fn cairo_image_surface_get_data(Ptr) -> *mut u8;
    fn cairo_image_surface_get_stride(Ptr) -> c_int;
    fn cairo_surface_flush(Ptr);
    fn cairo_surface_destroy(Ptr);
    fn cairo_create(Ptr) -> Ptr;
    fn cairo_destroy(Ptr);
    fn cairo_rectangle(Ptr, f64, f64, f64, f64);
    fn cairo_clip(Ptr);
    fn cairo_set_operator(Ptr, c_int);
    fn cairo_paint(Ptr);
    // JavaScriptCore and libsoup.
    fn jsc_value_to_json(Ptr, c_uint) -> *mut c_char;
    fn webkit_javascript_result_get_js_value(Ptr) -> Ptr;
    fn jsc_value_to_string(Ptr) -> *mut c_char;
    fn jsc_value_is_object(Ptr) -> gboolean;
    fn jsc_value_is_undefined(Ptr) -> gboolean;
    fn jsc_value_to_boolean(Ptr) -> gboolean;
    fn jsc_value_object_get_property(Ptr, *const c_char) -> Ptr;
    fn soup_message_headers_new(c_int) -> Ptr;
    fn soup_message_headers_append(Ptr, *const c_char, *const c_char);
    fn soup_message_headers_foreach(Ptr, unsafe extern "C" fn(*const c_char, *const c_char, Ptr), Ptr);
    // WebKit.
    fn webkit_web_view_get_type() -> GType;
    fn webkit_website_data_manager_get_type() -> GType;
    fn webkit_web_context_get_default() -> Ptr;
    fn webkit_web_context_new_ephemeral() -> Ptr;
    fn webkit_web_context_new_with_website_data_manager(Ptr) -> Ptr;
    fn webkit_web_context_register_uri_scheme(Ptr, *const c_char, unsafe extern "C" fn(Ptr, Ptr), Ptr, Option<unsafe extern "C" fn(Ptr)>);
    fn webkit_web_context_get_security_manager(Ptr) -> Ptr;
    fn webkit_security_manager_register_uri_scheme_as_secure(Ptr, *const c_char);
    fn webkit_security_manager_register_uri_scheme_as_cors_enabled(Ptr, *const c_char);
    fn webkit_user_content_manager_new() -> Ptr;
    fn webkit_user_content_manager_register_script_message_handler(Ptr, *const c_char) -> gboolean;
    fn webkit_user_content_manager_add_script(Ptr, Ptr);
    fn webkit_user_script_new(*const c_char, c_int, c_int, *const *const c_char, *const *const c_char) -> Ptr;
    fn webkit_user_script_unref(Ptr);
    fn webkit_web_view_get_settings(Ptr) -> Ptr;
    fn webkit_settings_set_enable_developer_extras(Ptr, gboolean);
    fn webkit_settings_set_user_agent(Ptr, *const c_char);
    fn webkit_web_view_load_uri(Ptr, *const c_char);
    fn webkit_web_view_load_html(Ptr, *const c_char, *const c_char);
    fn webkit_web_view_reload(Ptr);
    fn webkit_web_view_stop_loading(Ptr);
    fn webkit_web_view_go_back(Ptr);
    fn webkit_web_view_go_forward(Ptr);
    fn webkit_web_view_get_back_forward_list(Ptr) -> Ptr;
    fn webkit_back_forward_list_get_nth_item(Ptr, c_int) -> Ptr;
    fn webkit_back_forward_list_get_back_list_with_limit(Ptr, c_uint) -> *mut GList;
    fn webkit_back_forward_list_get_forward_list_with_limit(Ptr, c_uint) -> *mut GList;
    fn webkit_back_forward_list_item_get_uri(Ptr) -> *const c_char;
    fn webkit_back_forward_list_item_get_title(Ptr) -> *const c_char;
    fn webkit_web_view_go_to_back_forward_list_item(Ptr, Ptr);
    fn webkit_web_view_get_uri(Ptr) -> *const c_char;
    fn webkit_web_view_get_title(Ptr) -> *const c_char;
    fn webkit_web_view_is_loading(Ptr) -> gboolean;
    fn webkit_web_view_get_estimated_load_progress(Ptr) -> f64;
    fn webkit_web_view_can_go_back(Ptr) -> gboolean;
    fn webkit_web_view_can_go_forward(Ptr) -> gboolean;
    fn webkit_web_view_evaluate_javascript(Ptr, *const c_char, isize, *const c_char, *const c_char, Ptr, Option<unsafe extern "C" fn(Ptr, Ptr, Ptr)>, Ptr);
    fn webkit_web_view_evaluate_javascript_finish(Ptr, Ptr, *mut *mut GError) -> Ptr;
    fn webkit_web_view_set_zoom_level(Ptr, f64);
    fn webkit_web_view_get_inspector(Ptr) -> Ptr;
    fn webkit_web_inspector_show(Ptr);
    fn webkit_web_view_set_background_color(Ptr, *const GdkRGBA);
    fn webkit_web_view_set_input_method_context(Ptr, Ptr);
    fn webkit_input_method_context_get_type() -> GType;
    fn webkit_option_menu_get_n_items(Ptr) -> c_uint;
    fn webkit_option_menu_get_item(Ptr, c_uint) -> Ptr;
    fn webkit_option_menu_item_get_label(Ptr) -> *const c_char;
    fn webkit_option_menu_item_is_group_label(Ptr) -> gboolean;
    fn webkit_option_menu_item_is_enabled(Ptr) -> gboolean;
    fn webkit_option_menu_item_is_selected(Ptr) -> gboolean;
    fn webkit_option_menu_activate_item(Ptr, c_uint);
    fn webkit_option_menu_close(Ptr);
    fn webkit_policy_decision_use(Ptr);
    fn webkit_policy_decision_ignore(Ptr);
    fn webkit_navigation_policy_decision_get_navigation_action(Ptr) -> Ptr;
    fn webkit_navigation_action_get_request(Ptr) -> Ptr;
    fn webkit_navigation_action_get_navigation_type(Ptr) -> c_int;
    fn webkit_uri_request_get_uri(Ptr) -> *const c_char;
    fn webkit_uri_scheme_request_get_uri(Ptr) -> *const c_char;
    fn webkit_uri_scheme_request_get_http_method(Ptr) -> *const c_char;
    fn webkit_uri_scheme_request_get_http_headers(Ptr) -> Ptr;
    fn webkit_uri_scheme_request_get_http_body(Ptr) -> Ptr;
    fn webkit_uri_scheme_request_get_web_view(Ptr) -> Ptr;
    fn webkit_uri_scheme_request_finish_with_response(Ptr, Ptr);
    fn webkit_uri_scheme_request_finish_error(Ptr, *mut GError);
    fn webkit_uri_scheme_response_new(Ptr, i64) -> Ptr;
    fn webkit_uri_scheme_response_set_status(Ptr, c_uint, *const c_char);
    fn webkit_uri_scheme_response_set_content_type(Ptr, *const c_char);
    fn webkit_uri_scheme_response_set_http_headers(Ptr, Ptr);
}

/// The calls, if this system has WebKitGTK 4.1 (loaded once, on first ask).
pub fn api() -> Option<&'static Api> {
    static API: std::sync::OnceLock<Option<Api>> = std::sync::OnceLock::new();
    API.get_or_init(|| match Api::load() {
        Ok(api) => Some(api),
        Err(error) => {
            eprintln!("zgx: web views need WebKitGTK 4.1 (libwebkit2gtk-4.1-0): {error}");
            None
        }
    })
    .as_ref()
}

#[repr(C)]
pub struct GList {
    pub data: Ptr,
    pub next: *mut GList,
    pub prev: *mut GList,
}
#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}
#[repr(C)]
pub struct GTypeQuery {
    pub type_: GType,
    pub type_name: *const c_char,
    pub class_size: c_uint,
    pub instance_size: c_uint,
}
#[repr(C)]
pub struct GTypeInfo {
    pub class_size: u16,
    pub base_init: Ptr,
    pub base_finalize: Ptr,
    pub class_init: Option<unsafe extern "C" fn(Ptr, Ptr)>,
    pub class_finalize: Ptr,
    pub class_data: Ptr,
    pub instance_size: u16,
    pub n_preallocs: u16,
    pub instance_init: Ptr,
    pub value_table: Ptr,
}
#[repr(C)]
pub struct GdkRGBA {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}
#[repr(C)]
pub struct GdkRectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

// GDK 3's events (gdkevents.h), filled in what `gdk_event_new` allocates.
#[repr(C)]
pub struct GdkEventKey {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub time: u32,
    pub state: c_uint,
    pub keyval: c_uint,
    pub length: c_int,
    pub string: *mut c_char,
    pub hardware_keycode: u16,
    pub group: u8,
    // `is_modifier`, a bit field: left as `gdk_event_new` zeroed it.
}
#[repr(C)]
pub struct GdkEventButton {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub time: u32,
    pub x: f64,
    pub y: f64,
    pub axes: *mut f64,
    pub state: c_uint,
    pub button: c_uint,
    pub device: Ptr,
    pub x_root: f64,
    pub y_root: f64,
}
#[repr(C)]
pub struct GdkEventMotion {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub time: u32,
    pub x: f64,
    pub y: f64,
    pub axes: *mut f64,
    pub state: c_uint,
    pub is_hint: i16,
    pub device: Ptr,
    pub x_root: f64,
    pub y_root: f64,
}
#[repr(C)]
pub struct GdkEventScroll {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub time: u32,
    pub x: f64,
    pub y: f64,
    pub state: c_uint,
    pub direction: c_int,
    pub device: Ptr,
    pub x_root: f64,
    pub y_root: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    // `is_stop`, a bit field: left as `gdk_event_new` zeroed it.
}
#[repr(C)]
pub struct GdkEventCrossing {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub subwindow: Ptr,
    pub time: u32,
    pub x: f64,
    pub y: f64,
    pub x_root: f64,
    pub y_root: f64,
    pub mode: c_int,
    pub detail: c_int,
    pub focus: gboolean,
    pub state: c_uint,
}
#[repr(C)]
pub struct GdkEventFocus {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub in_: i16,
}
#[repr(C)]
pub struct GdkEventExpose {
    pub type_: c_int,
    pub window: Ptr,
    pub send_event: i8,
    pub area: GdkRectangle,
    pub region: Ptr,
    pub count: c_int,
}

pub const GDK_MOTION_NOTIFY: c_int = 3;
pub const GDK_BUTTON_PRESS: c_int = 4;
pub const GDK_BUTTON_RELEASE: c_int = 7;
pub const GDK_KEY_PRESS: c_int = 8;
pub const GDK_KEY_RELEASE: c_int = 9;
pub const GDK_ENTER_NOTIFY: c_int = 10;
pub const GDK_LEAVE_NOTIFY: c_int = 11;
pub const GDK_FOCUS_CHANGE: c_int = 12;
pub const GDK_SCROLL: c_int = 31;
pub const GDK_SCROLL_SMOOTH: c_int = 4;
pub const GDK_SHIFT_MASK: c_uint = 1 << 0;
pub const GDK_CONTROL_MASK: c_uint = 1 << 2;
pub const GDK_MOD1_MASK: c_uint = 1 << 3;
pub const GDK_BUTTON1_MASK: c_uint = 1 << 8;
pub const GDK_SUPER_MASK: c_uint = 1 << 26;
pub const GDK_META_MASK: c_uint = 1 << 28;
pub const CAIRO_FORMAT_ARGB32: c_int = 0;
pub const CAIRO_OPERATOR_CLEAR: c_int = 0;
pub const CAIRO_OPERATOR_OVER: c_int = 2;
pub const G_CONNECT_AFTER: c_int = 1;
/// `GDK_SELECTION_CLIPBOARD`, an atom GDK predefines (69).
pub const GDK_SELECTION_CLIPBOARD: Ptr = 69 as Ptr;
pub const G_PRIORITY_DEFAULT: c_int = 0;
/// `G_SIGNAL_MATCH_DATA`: every handler connected with some data.
pub const G_SIGNAL_MATCH_DATA: c_int = 1 << 4;

/// A C string GLib owns (or none), as Rust's.
///
/// # Safety
/// `text` is null or a NUL-terminated string alive for this call.
pub unsafe fn string(text: *const c_char) -> String {
    if text.is_null() {
        String::new()
    } else {
        // SAFETY: as the caller promises.
        unsafe { std::ffi::CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }
}
/// `text` without interior NULs, NUL-terminated for C.
pub fn c(text: &str) -> std::ffi::CString {
    std::ffi::CString::new(text.replace('\0', "")).expect("no NUL left")
}

/// Connect `handler` (a C function of the signal's signature, cast) to
/// `signal` of `instance` with `data`, after the default handler if `after`.
///
/// # Safety
/// `instance` is a live GObject with `signal`, whose signature `handler` has.
pub unsafe fn connect(
    api: &Api,
    instance: Ptr,
    signal: &str,
    handler: GCallback,
    data: Ptr,
    after: bool,
) {
    let signal = c(signal);
    // SAFETY: as the caller promises; GObject copies the name.
    unsafe {
        (api.g_signal_connect_data)(
            instance,
            signal.as_ptr(),
            handler,
            data,
            None,
            if after { G_CONNECT_AFTER } else { 0 },
        );
    }
}
