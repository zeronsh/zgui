// Adapted from rfd 0.17.2 / ashpd parent exporter. MIT; see LICENSE.
use std::fmt;

use wayland_backend::sys::client::Backend;
use wayland_client::{
    Proxy, QueueHandle,
    protocol::{wl_registry, wl_surface::WlSurface},
};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
};

use super::WindowIdentifierType;

// Supported versions.
const ZXDG_EXPORTER_V2: u32 = 1;

#[derive(Debug)]
pub struct WaylandWindowIdentifier {
    exported: ZxdgExportedV2,
    type_: WindowIdentifierType,
}

impl WaylandWindowIdentifier {
    /// The toplevel `surface_ptr` exported (`zxdg_exporter_v2`) on a queue of
    /// this call's own on `display_ptr`.
    ///
    /// # Safety
    ///
    /// `display_ptr` is a connected `wl_display` and `surface_ptr` a live
    /// `wl_surface` proxy on it, both until this returns. Nothing made here
    /// holds the display after: the backend made on it goes when this
    /// returns (the identifier's proxy keeps only a weak reference, so its
    /// `destroy` on drop sends nothing), and libwayland discards what the
    /// server sends its objects after.
    pub unsafe fn from_raw(
        surface_ptr: *mut std::ffi::c_void,
        display_ptr: *mut std::ffi::c_void,
    ) -> Option<Self> {
        if surface_ptr.is_null() || display_ptr.is_null() {
            return None;
        }

        // SAFETY: a connected display, until this returns (the caller's
        // word), and the backend and its clones go before then.
        let backend = unsafe { Backend::from_foreign_display(display_ptr as *mut _) };
        let conn = wayland_client::Connection::from_backend(backend);
        // SAFETY: a live wl_surface until this returns (the caller's word);
        // the id and the proxy made of it are dropped by then.
        let obj_id = unsafe {
            wayland_backend::sys::client::ObjectId::from_ptr(
                WlSurface::interface(),
                surface_ptr as *mut _,
            )
        }
        .ok()?;

        let surface = WlSurface::from_id(&conn, obj_id).ok()?;

        wayland_export_handle(conn, &surface)
    }
}

impl fmt::Display for WaylandWindowIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.type_.fmt(f)
    }
}

impl Drop for WaylandWindowIdentifier {
    fn drop(&mut self) {
        self.exported.destroy();
    }
}

#[derive(Default, Debug)]
struct State {
    handle: String,
    exporter: Option<ZxdgExporterV2>,
}

impl wayland_client::Dispatch<ZxdgExportedV2, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgExportedV2,
        event: <ZxdgExportedV2 as Proxy>::Event,
        _data: &(),
        _connhandle: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            state.handle = handle;
        }
    }
}

impl wayland_client::Dispatch<ZxdgExporterV2, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZxdgExporterV2,
        _event: <ZxdgExporterV2 as Proxy>::Event,
        _data: &(),
        _connhandle: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &wayland_client::Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface.as_str() == "zxdg_exporter_v2"
        {
            let exporter = registry.bind::<ZxdgExporterV2, (), State>(
                name,
                version.min(ZXDG_EXPORTER_V2),
                qhandle,
                (),
            );
            state.exporter = Some(exporter);
        }
    }
}

fn wayland_export_handle(
    conn: wayland_client::Connection,
    surface: &WlSurface,
) -> Option<WaylandWindowIdentifier> {
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();
    let mut state = State::default();
    display.get_registry(&qhandle, ());
    event_queue.roundtrip(&mut state).ok()?;

    let exported = match state.exporter.take() {
        Some(exporter) => {
            let exp = exporter.export_toplevel(surface, &qhandle, ());
            event_queue.roundtrip(&mut state).ok()?;
            exporter.destroy();

            Some(exp)
        }
        None => None,
    };

    if let Some(exported) = exported {
        Some(WaylandWindowIdentifier {
            exported,
            type_: WindowIdentifierType::Wayland(state.handle),
        })
    } else {
        None
    }
}
