//! Frame pacing as the user sees it, for any app: every frame the compositor
//! puts on the screen (Desktop Duplication, no elevation needed), and
//! whether a screen rectangle's pixels changed in it. The times between
//! changes are a window's frame times as shown. Windows only; the Windows
//! counterpart of scripts/framepace.swift, run by scripts/windows_framepace.py.
//!
//!     cargo build --release -p zgx-gpu --example framepace
//!     framepace <x> <y> <width> <height> <seconds>
//!
//! Prints `HZ <refresh rate>`, then `FRAME <ms>` for each composed frame
//! whose pixels in the rectangle changed (the compositor's present time,
//! from the start), then `COMPOSED <frames composed>`.

#[cfg(windows)]
mod pace {
    use windows::{
        Win32::{
            Foundation::HMODULE,
            Graphics::{
                Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
                Direct3D11::{
                    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ,
                    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                    D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                    ID3D11Texture2D,
                },
                Dxgi::{
                    Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
                    IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIResource,
                },
            },
            System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
        core::Interface,
    };

    fn qpc() -> i64 {
        let mut now = 0;
        // SAFETY: writes one integer into a local.
        let _ = unsafe { QueryPerformanceCounter(&mut now) };
        now
    }

    pub fn main() -> windows::core::Result<()> {
        let args: Vec<i64> = std::env::args()
            .skip(1)
            .map(|a| a.parse().expect("a number"))
            .collect();
        let [x, y, w, h, seconds] = args[..] else {
            panic!("framepace <x> <y> <width> <height> <seconds>")
        };
        // SAFETY: DXGI and Direct3D 11 calls on objects made here; the
        // mapped staging texture is read within its size and row pitch.
        unsafe {
            let mut frequency = 0;
            QueryPerformanceFrequency(&mut frequency)?;
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            // The output the rectangle's corner is on, and its adapter.
            let (adapter, output, origin) = (|| {
                for a in 0.. {
                    let adapter: IDXGIAdapter1 = factory.EnumAdapters1(a).ok()?;
                    for o in 0.. {
                        let Ok(output) = adapter.EnumOutputs(o) else {
                            break;
                        };
                        let r = output.GetDesc().ok()?.DesktopCoordinates;
                        if (r.left as i64..r.right as i64).contains(&x)
                            && (r.top as i64..r.bottom as i64).contains(&y)
                        {
                            let origin = (r.left as i64, r.top as i64);
                            return Some((adapter, output.cast::<IDXGIOutput1>().ok()?, origin));
                        }
                    }
                }
                None
            })()
            .expect("an output showing the rectangle");
            let mut mode = Default::default();
            let hz = if windows::Win32::Graphics::Gdi::EnumDisplaySettingsW(
                windows::core::PCWSTR(output.GetDesc()?.DeviceName.as_ptr()),
                windows::Win32::Graphics::Gdi::ENUM_CURRENT_SETTINGS,
                &mut mode,
            )
            .as_bool()
            {
                mode.dmDisplayFrequency
            } else {
                0
            };
            println!("HZ {hz}");
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let (device, context) = (device.unwrap(), context.unwrap());
            let duplication = output.DuplicateOutput(&device)?;
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: w as u32,
                Height: h as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging: Option<ID3D11Texture2D> = None;
            device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            let staging = staging.unwrap();
            let region = D3D11_BOX {
                left: (x - origin.0) as u32,
                top: (y - origin.1) as u32,
                front: 0,
                right: (x - origin.0 + w) as u32,
                bottom: (y - origin.1 + h) as u32,
                back: 1,
            };
            let start = qpc();
            let end = start + seconds * frequency;
            let mut previous: Option<u64> = None;
            let mut composed = 0u64;
            while qpc() < end {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<IDXGIResource> = None;
                match duplication.AcquireNextFrame(100, &mut info, &mut resource) {
                    Ok(()) => {}
                    Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => continue,
                    Err(e) => return Err(e),
                }
                // A frame whose image changed (not the pointer alone): hash
                // the rectangle's pixels.
                if info.LastPresentTime != 0 {
                    composed += 1;
                    let texture: ID3D11Texture2D = resource.unwrap().cast()?;
                    context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, 0, Some(&region));
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
                    for row in 0..h as usize {
                        let line = std::slice::from_raw_parts(
                            (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                            w as usize * 4,
                        );
                        for &b in line {
                            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
                        }
                    }
                    context.Unmap(&staging, 0);
                    if previous.is_some_and(|p| p != hash) {
                        let at = (info.LastPresentTime - start) as f64 * 1000. / frequency as f64;
                        println!("FRAME {at:.3}");
                    }
                    previous = Some(hash);
                }
                duplication.ReleaseFrame()?;
            }
            println!("COMPOSED {composed}");
        }
        Ok(())
    }
}

fn main() {
    #[cfg(windows)]
    pace::main().expect("frame pacing");
    #[cfg(not(windows))]
    eprintln!("framepace reads Windows' Desktop Duplication: Windows only");
}
