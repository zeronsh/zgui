//! What DXGI reports of a window's presents, for `ZGUI_PRESENT_LOG`: the
//! refresh each shown frame went up on. While a window animates at the
//! display's rate, every present should show one refresh after the last;
//! `+2 refreshes` for one present is a refresh that repeated a frame.
use windows::Win32::Graphics::Dxgi::DXGI_FRAME_STATISTICS;

#[derive(Default)]
pub(crate) struct PresentStats {
    /// The last shown present: its count and the refresh it showed on.
    shown: Option<(u32, u64)>,
}

impl PresentStats {
    /// After a present: log the frames shown since the last call.
    pub(crate) fn presented(&mut self, surface: &wgpu::Surface) {
        if !crate::present_log::enabled() {
            return;
        }
        // SAFETY: the hal surface is only read (its swap chain taken, not
        // changed), on the thread that presents to it.
        let Some(hal) = (unsafe { surface.as_hal::<wgpu::hal::api::Dx12>() }) else {
            return;
        };
        let Some(chain) = hal.swap_chain() else {
            return;
        };
        let mut stats = DXGI_FRAME_STATISTICS::default();
        // Fails until a present has shown, and after a mode change.
        // SAFETY: `chain` is a reference of our own to the surface's swap
        // chain (`swap_chain` adds one); writes `stats`, a local.
        if unsafe { chain.GetFrameStatistics(&mut stats) }.is_err() {
            self.shown = None;
            return;
        }
        // SAFETY: as above; reads a count.
        let queued = unsafe { chain.GetLastPresentCount() }
            .map_or(0, |last| last.saturating_sub(stats.PresentCount));
        // A composed (windowed) swap chain reports no refresh counts of its
        // own; DWM's count of the display's refreshes stands in.
        let refresh = dwm_refresh();
        if let Some((present, last)) = self.shown
            && stats.PresentCount != present
        {
            crate::present_log::log(|| {
                format!(
                    "dxgi shown present={} refresh={refresh} +{} presents +{} refreshes queued={queued}",
                    stats.PresentCount,
                    stats.PresentCount.wrapping_sub(present),
                    refresh.wrapping_sub(last),
                )
            });
        }
        self.shown = Some((stats.PresentCount, refresh));
    }
}

/// The display refresh nearest now: refreshes since the performance counter started,
/// on the grid of the compositor's last reported refresh. (DWM's own count
/// is of compositions, which some displays make only on demand.)
fn dwm_refresh() -> u64 {
    use windows::Win32::{
        Graphics::Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo},
        System::Performance::QueryPerformanceCounter,
    };
    let mut info = DWM_TIMING_INFO {
        cbSize: size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    let mut now = 0;
    // SAFETY: fills `info`, whose size it is given (a null window asks for
    // the compositor's own timing), and reads the counter into a local.
    unsafe {
        let _ = DwmGetCompositionTimingInfo(Default::default(), &mut info);
        let _ = QueryPerformanceCounter(&mut now);
    }
    // The nearest refresh: presents follow the compositor's ticks closely,
    // and rounding down there counted some refreshes twice and some not.
    let period = (info.qpcRefreshPeriod as i64).max(1);
    ((now - (info.qpcVBlank as i64).rem_euclid(period) + period / 2) / period) as u64
}

/// The performance counter's ticks a second.
fn frequency() -> i64 {
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    let mut frequency = 0;
    // SAFETY: writes one integer into a local.
    let _ = unsafe { QueryPerformanceFrequency(&mut frequency) };
    frequency.max(1)
}

/// Nanoseconds on the performance counter (`QueryPerformanceCounter`), the
/// clock DXGI and DWM stamp refreshes with.
pub(crate) fn now_ns() -> u64 {
    use windows::Win32::System::Performance::QueryPerformanceCounter;
    let mut count = 0;
    // SAFETY: writes one integer into a local.
    let _ = unsafe { QueryPerformanceCounter(&mut count) };
    (count as u128 * 1_000_000_000 / frequency() as u128) as u64
}
