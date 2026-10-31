//! Frames drawn with Direct3D 11 (Windows, the `d3d11` feature, with
//! `ZGX_D3D11=1`): experimental, off by default. Measured against Direct3D
//! 12 in docs/results/d3d11-proof-windows-2026-10-27: about half the
//! memory, a third less CPU and a fifth faster startup, for Direct3D 11's
//! own GPU floor of some 10 µs a frame (docs/windows.md, Direct3D 11).
//!
//! A Direct3D 12 device and its queue are a floor under every zgx window on
//! Windows: memory, the driver's threads at rest, startup (docs/windows.md).
//! Here none is opened. wgpu runs on its no-op backend, a shell keeping the
//! renderer's handles (textures, buffers, bind groups) without a driver, so
//! every CPU path of `GpuRenderer` runs as it does on Direct3D 12. Where
//! `render_flat` holds the frame as plain data (where `metal_frame` takes it
//! on macOS), it is recorded here with Direct3D 11; the glyph atlas, the
//! window's swap chain (dxgi_present.rs) and readbacks are Direct3D 11's.
//!
//! Only what the Windows suite's scenes draw is here, and isolated layers:
//! quads, text and clip regions, kept frames, scroll copies and catch-up,
//! and layers drawn into textures of their own and composited. A frame
//! needing more fails, naming what it lacks: there is no Direct3D 12 to
//! fall back to.
//!
//! The shaders are draw.wgsl and copy_offset.wgsl, translated by naga to
//! HLSL (draw.hlsl, copy_offset.hlsl) and compiled ahead by FXC, as wgpu's
//! Direct3D 12 backend compiles them, to the DXBC in `dxbc/`:
//! `translated_hlsl_matches` keeps all of them current
//! (`ZGX_BLESS=1 cargo test -p zgx-gpu --features d3d11 --lib translated_hlsl`).
use crate::{GpuError, Scissor, ScrollCopy, Shading};
use std::cell::Cell;
use windows::{
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0,
                D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
            },
            Direct3D11::*,
            Dxgi::{
                Common::{
                    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32_TYPELESS,
                    DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
                IDXGIAdapter, IDXGIFactory6,
            },
        },
    },
    core::{Interface, s},
};

/// `ZGX_D3D11=1`: windows and offscreen renderers draw with Direct3D 11.
pub(crate) static ENABLED: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| zgx::env::var_os("ZGX_D3D11").is_some_and(|value| value != "0"));
/// `ZGX_D3D11_THREADED=1`: the driver's own threading left on. Off by
/// default, as GPUI's renderer runs: on NVIDIA it moves a frame's cost to a
/// thread of the driver's rather than saving it (docs/windows.md).
static THREADED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    zgx::env::var_os("ZGX_D3D11_THREADED").is_some_and(|value| value != "0")
});

/// draw.wgsl's and copy_offset.wgsl's entry points, compiled ahead.
const DRAW_VS: &[u8] = include_bytes!("dxbc/draw_vs.dxbc");
const DRAW_FS: &[u8] = include_bytes!("dxbc/draw_fs.dxbc");
const BASIC_VS: &[u8] = include_bytes!("dxbc/draw_vs_basic.dxbc");
const BASIC_FS: &[u8] = include_bytes!("dxbc/draw_fs_basic.dxbc");
const COPY_VS: &[u8] = include_bytes!("dxbc/copy_offset_vs.dxbc");
const COPY_FS: &[u8] = include_bytes!("dxbc/copy_offset_fs.dxbc");

/// Registers the HLSL is written with (`translate`): the sampled texture
/// (the atlas, or the shown buffer for copies) in t0, the clip regions in
/// t1, the viewport in b0, the sampler in s0.
const TEXTURE: u32 = 0;
const REGIONS: u32 = 1;

fn failed(what: &str) -> impl Fn(windows::core::Error) -> GpuError + '_ {
    move |error| GpuError(format!("Direct3D 11 {what}: {error}"))
}

/// What a draw binds: shaders, their input layout and blending.
#[derive(Clone, Copy, PartialEq)]
enum Bound {
    Full,
    FullClear,
    Basic,
    Opaque,
    Copy,
}

struct Pipeline {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    blend: ID3D11BlendState,
}

/// The device, its immediate context and every pipeline: shared by the
/// renderers of a `GpuContext`, as its wgpu device is.
pub(crate) struct Device {
    pub(crate) device: ID3D11Device,
    pub(crate) context: ID3D11DeviceContext,
    /// By `Bound`, in its order.
    pipelines: [Pipeline; 5],
    rasterizer: ID3D11RasterizerState,
    sampler: ID3D11SamplerState,
    /// Bound where a frame has no clip regions: one zeroed `Region`.
    no_regions: ID3D11ShaderResourceView,
    /// The query `wait_idle` ends and waits for.
    idle: ID3D11Query,
}

/// A `Device` made on another thread (`GpuContext::prepare`), handed over.
pub(crate) struct Made(pub(crate) Device);
// SAFETY: A Direct3D 11 device is free-threaded. The immediate context is
// not, and is used by one thread at a time: made on the thread making the
// context, then handed to the one drawing, which alone uses it after.
unsafe impl Send for Made {}

impl Device {
    /// A device on the adapter DXGI lists first (its own order, as the
    /// Direct3D 12 path takes it), at feature level 11_0, with every
    /// pipeline made.
    pub(crate) fn new() -> Result<Self, GpuError> {
        let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        if !*THREADED {
            flags |= D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS;
        }
        let adapter = first_adapter();
        let driver = match adapter {
            Some(_) => D3D_DRIVER_TYPE_UNKNOWN,
            None => D3D_DRIVER_TYPE_HARDWARE,
        };
        let (mut device, mut context) = (None, None);
        // SAFETY: Plain device creation; the out pointers live across it.
        unsafe {
            D3D11CreateDevice(
                adapter.as_ref(),
                driver,
                HMODULE::default(),
                flags,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(failed("device"))?;
        let (device, context): (ID3D11Device, ID3D11DeviceContext) = device
            .zip(context)
            .ok_or_else(|| GpuError("Direct3D 11 made no device".into()))?;
        zgx::profile::mark("Direct3D 11 device");
        let quads = input_layout(8);
        let copies = input_layout(2);
        let vertex = |code: &[u8]| {
            let mut shader = None;
            // SAFETY: DXBC compiled by FXC (`translated_hlsl_matches`).
            unsafe { device.CreateVertexShader(code, None, Some(&mut shader)) }
                .map_err(failed("vertex shader"))?;
            shader.ok_or_else(|| GpuError("Direct3D 11 made no vertex shader".into()))
        };
        let pixel = |code: &[u8]| {
            let mut shader = None;
            // SAFETY: As `vertex`.
            unsafe { device.CreatePixelShader(code, None, Some(&mut shader)) }
                .map_err(failed("pixel shader"))?;
            shader.ok_or_else(|| GpuError("Direct3D 11 made no pixel shader".into()))
        };
        let layout = |elements: &[D3D11_INPUT_ELEMENT_DESC], code: &[u8]| {
            let mut layout = None;
            // SAFETY: Elements naga's `LOC` semantics name, and the vertex
            // shader they feed.
            unsafe { device.CreateInputLayout(elements, code, Some(&mut layout)) }
                .map_err(failed("input layout"))?;
            layout.ok_or_else(|| GpuError("Direct3D 11 made no input layout".into()))
        };
        let blend = |over: bool| {
            let mut desc = D3D11_BLEND_DESC::default();
            // Premultiplied alpha, as wgpu's PREMULTIPLIED_ALPHA_BLENDING;
            // otherwise the colour replaces the target's.
            desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: over.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: if over {
                    D3D11_BLEND_INV_SRC_ALPHA
                } else {
                    D3D11_BLEND_ZERO
                },
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: if over {
                    D3D11_BLEND_INV_SRC_ALPHA
                } else {
                    D3D11_BLEND_ZERO
                },
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut state = None;
            // SAFETY: A plain blend description.
            unsafe { device.CreateBlendState(&desc, Some(&mut state)) }
                .map_err(failed("blend state"))?;
            state.ok_or_else(|| GpuError("Direct3D 11 made no blend state".into()))
        };
        let (full_vs, basic_vs) = (vertex(DRAW_VS)?, vertex(BASIC_VS)?);
        let (full_fs, basic_fs) = (pixel(DRAW_FS)?, pixel(BASIC_FS)?);
        let (full_layout, basic_layout) = (layout(&quads, DRAW_VS)?, layout(&quads, BASIC_VS)?);
        let (over, replace) = (blend(true)?, blend(false)?);
        let pipeline = |vertex: &ID3D11VertexShader,
                        pixel: &ID3D11PixelShader,
                        layout: &ID3D11InputLayout,
                        blend: &ID3D11BlendState| Pipeline {
            vertex: vertex.clone(),
            pixel: pixel.clone(),
            layout: layout.clone(),
            blend: blend.clone(),
        };
        let pipelines = [
            pipeline(&full_vs, &full_fs, &full_layout, &over),
            pipeline(&full_vs, &full_fs, &full_layout, &replace),
            pipeline(&basic_vs, &basic_fs, &basic_layout, &over),
            pipeline(&basic_vs, &basic_fs, &basic_layout, &replace),
            Pipeline {
                vertex: vertex(COPY_VS)?,
                pixel: pixel(COPY_FS)?,
                layout: layout(&copies, COPY_VS)?,
                blend: replace.clone(),
            },
        ];
        // As wgpu's quad pipelines: no culling, depth clipped, scissored.
        let rasterizer = {
            let desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                FrontCounterClockwise: false.into(),
                DepthBias: 0,
                DepthBiasClamp: 0.,
                SlopeScaledDepthBias: 0.,
                DepthClipEnable: true.into(),
                ScissorEnable: true.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
            };
            let mut state = None;
            // SAFETY: A plain rasterizer description.
            unsafe { device.CreateRasterizerState(&desc, Some(&mut state)) }
                .map_err(failed("rasterizer state"))?;
            state.ok_or_else(|| GpuError("Direct3D 11 made no rasterizer state".into()))?
        };
        // Linear and clamped, as `GpuRenderer::texture_bind`'s.
        let sampler = {
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MipLODBias: 0.,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                BorderColor: [0.; 4],
                MinLOD: 0.,
                MaxLOD: 32.,
            };
            let mut state = None;
            // SAFETY: A plain sampler description.
            unsafe { device.CreateSamplerState(&desc, Some(&mut state)) }
                .map_err(failed("sampler"))?;
            state.ok_or_else(|| GpuError("Direct3D 11 made no sampler".into()))?
        };
        let no_regions = raw_buffer(
            &device,
            size_of::<crate::Region>(),
            Some(&[0; size_of::<crate::Region>()]),
        )?
        .1;
        let idle = event_query(&device)?;
        zgx::profile::mark("Direct3D 11 pipelines");
        Ok(Self {
            device,
            context,
            pipelines,
            rasterizer,
            sampler,
            no_regions,
            idle,
        })
    }

    /// Wait until the GPU has run everything given it so far.
    pub(crate) fn wait_idle(&self) {
        // SAFETY: The device's own query, ended on its own context.
        unsafe {
            self.context.End(&self.idle);
            self.context.Flush();
        }
        while !query_done(&self.context, &self.idle, true) {
            std::thread::yield_now();
        }
    }

    /// A `width`×`height` BGRA texture the shaders sample, and a target to
    /// draw into where `target`; cleared to transparent, as wgpu's are.
    pub(crate) fn surface(
        &self,
        width: u32,
        height: u32,
        target: bool,
    ) -> Result<Surface, GpuError> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0
                | if target {
                    D3D11_BIND_RENDER_TARGET.0
                } else {
                    0
                }) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: A plain 2D texture description.
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(failed("texture"))?;
        let texture: ID3D11Texture2D =
            texture.ok_or_else(|| GpuError("Direct3D 11 made no texture".into()))?;
        let surface = Surface::of(&self.device, texture, target, true)?;
        if let Some(rtv) = &surface.rtv {
            // SAFETY: The surface's own view.
            unsafe { self.context.ClearRenderTargetView(rtv, &[0.; 4]) };
        } else {
            // A sampled texture's cells are written before they are read,
            // borders included; start it transparent all the same.
            let zeros = vec![0u8; (width * height * 4) as usize];
            self.write(&surface, (0, 0, width, height), &zeros);
        }
        Ok(surface)
    }

    /// Write BGRA texels to `(x, y, width, height)` of `surface`.
    pub(crate) fn write(&self, surface: &Surface, (x, y, width, height): Scissor, bgra: &[u8]) {
        let area = D3D11_BOX {
            left: x,
            top: y,
            front: 0,
            right: x + width,
            bottom: y + height,
            back: 1,
        };
        // SAFETY: `bgra` holds `width`×`height` texels, rows of `width * 4`
        // bytes, inside the texture (the caller's cell or rectangle); the
        // runtime copies them before returning.
        unsafe {
            self.context.UpdateSubresource(
                &surface.texture,
                0,
                Some(&area),
                bgra.as_ptr().cast(),
                width * 4,
                0,
            )
        };
    }

    /// `surface`'s pixels, RGBA, a row at a time. Waits for the GPU.
    pub(crate) fn read(&self, surface: &Surface) -> Result<Vec<u8>, GpuError> {
        let (width, height) = (surface.width, surface.height);
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
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
        let mut staging = None;
        // SAFETY: A staging texture the CPU reads, the surface's size and
        // format, copied whole from it and mapped while `mapped` is read.
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .map_err(failed("readback texture"))?;
            let staging: ID3D11Texture2D =
                staging.ok_or_else(|| GpuError("Direct3D 11 made no readback texture".into()))?;
            self.context.CopyResource(&staging, &surface.texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(failed("readback map"))?;
            let mut pixels = Vec::with_capacity((width * height * 4) as usize);
            for row in 0..height as usize {
                let start = mapped
                    .pData
                    .cast::<u8>()
                    .add(row * mapped.RowPitch as usize);
                pixels.extend_from_slice(std::slice::from_raw_parts(start, width as usize * 4));
            }
            self.context.Unmap(&staging, 0);
            // Retained textures are BGRA (see `FORMAT`); callers read RGBA.
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            Ok(pixels)
        }
    }

    /// Scroll `copies` within `target`, through `spare` (the target's size):
    /// as wgpu's path copies them before its pass, when no shown surface
    /// draws them.
    pub(crate) fn scroll(&self, target: &Surface, spare: &Surface, copies: &[ScrollCopy]) {
        for &((x, y, width, height), (sx, sy)) in copies {
            let (from_x, from_y) = ((x as i32 + sx) as u32, (y as i32 + sy) as u32);
            let area = |x: u32, y: u32| D3D11_BOX {
                left: x,
                top: y,
                front: 0,
                right: x + width,
                bottom: y + height,
                back: 1,
            };
            // SAFETY: Both rectangles lie inside both textures, which have
            // the same size and format (`scroll_copies` keeps them in).
            unsafe {
                self.context.CopySubresourceRegion(
                    &spare.texture,
                    0,
                    x,
                    y,
                    0,
                    &target.texture,
                    0,
                    Some(&area(from_x, from_y)),
                );
                self.context.CopySubresourceRegion(
                    &target.texture,
                    0,
                    x,
                    y,
                    0,
                    &spare.texture,
                    0,
                    Some(&area(x, y)),
                );
            }
        }
    }

    /// Open a frame drawing into `target`, its pixels kept, from `vertices`
    /// (`Ring::write`), with `viewport` (logical width and height, scale)
    /// and the frame's clip `regions` in `state`.
    pub(crate) fn begin<'a>(
        &'a self,
        state: &'a State,
        target: &Surface,
        vertices: &ID3D11Buffer,
    ) -> Frame<'a> {
        let context = &self.context;
        let rtv = target.rtv.clone().expect("a render target");
        let regions = state
            .regions
            .as_ref()
            .map_or(&self.no_regions, |(_, view, _)| view);
        // SAFETY: Live objects of this device, bound to its context; the
        // vertex buffer holds 128-byte quads (`crate::Quad`).
        unsafe {
            context.OMSetRenderTargets(Some(&[Some(rtv)]), None);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.,
                TopLeftY: 0.,
                Width: target.width as f32,
                Height: target.height as f32,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            context.RSSetState(&self.rasterizer);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            context.IASetVertexBuffers(
                0,
                1,
                Some(&Some(vertices.clone())),
                Some(&(size_of::<crate::Quad>() as u32)),
                Some(&0),
            );
            context.VSSetConstantBuffers(0, Some(&[Some(state.viewport.clone())]));
            context.PSSetConstantBuffers(0, Some(&[Some(state.viewport.clone())]));
            context.PSSetShaderResources(REGIONS, Some(&[Some(regions.clone())]));
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
        }
        Frame {
            device: self,
            current: None,
            scissor: None,
        }
    }
}

/// The adapter DXGI lists first for low power, as the Direct3D 12 path
/// takes it (`crate::POWER`, `preferred_dx12_adapter`).
fn first_adapter() -> Option<IDXGIAdapter> {
    // SAFETY: Plain DXGI calls on a factory made here.
    unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
        factory
            .EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER)
            .ok()
    }
}

/// `count` of the quads' 128-byte attributes, naga's `LOC0`.., one quad an
/// instance.
fn input_layout(count: u32) -> Vec<D3D11_INPUT_ELEMENT_DESC> {
    (0..count)
        .map(|index| D3D11_INPUT_ELEMENT_DESC {
            SemanticName: s!("LOC"),
            SemanticIndex: index,
            Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 16 * index,
            InputSlotClass: D3D11_INPUT_PER_INSTANCE_DATA,
            InstanceDataStepRate: 1,
        })
        .collect()
}

/// A buffer of `bytes` the shaders read as a `ByteAddressBuffer` (naga's
/// storage buffers), with `data` if given, and its view.
fn raw_buffer(
    device: &ID3D11Device,
    bytes: usize,
    data: Option<&[u8]>,
) -> Result<(ID3D11Buffer, ID3D11ShaderResourceView), GpuError> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: bytes as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS.0 as u32,
        StructureByteStride: 0,
    };
    let init = data.map(|data| D3D11_SUBRESOURCE_DATA {
        pSysMem: data.as_ptr().cast(),
        SysMemPitch: 0,
        SysMemSlicePitch: 0,
    });
    let mut buffer = None;
    // SAFETY: `data`, where given, holds `bytes` bytes, copied during the call.
    unsafe {
        device.CreateBuffer(
            &desc,
            init.as_ref().map(|init| init as *const _),
            Some(&mut buffer),
        )
    }
    .map_err(failed("regions buffer"))?;
    let buffer: ID3D11Buffer =
        buffer.ok_or_else(|| GpuError("Direct3D 11 made no regions buffer".into()))?;
    let view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R32_TYPELESS,
        ViewDimension: windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_BUFFEREX,
        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
            BufferEx: D3D11_BUFFEREX_SRV {
                FirstElement: 0,
                NumElements: (bytes / 4) as u32,
                Flags: D3D11_BUFFEREX_SRV_FLAG_RAW.0 as u32,
            },
        },
    };
    let mut view = None;
    // SAFETY: A raw view of the whole buffer, made to allow one.
    unsafe { device.CreateShaderResourceView(&buffer, Some(&view_desc), Some(&mut view)) }
        .map_err(failed("regions view"))?;
    let view = view.ok_or_else(|| GpuError("Direct3D 11 made no regions view".into()))?;
    Ok((buffer, view))
}

fn event_query(device: &ID3D11Device) -> Result<ID3D11Query, GpuError> {
    let desc = D3D11_QUERY_DESC {
        Query: D3D11_QUERY_EVENT,
        MiscFlags: 0,
    };
    let mut query = None;
    // SAFETY: A plain event query.
    unsafe { device.CreateQuery(&desc, Some(&mut query)) }.map_err(failed("query"))?;
    query.ok_or_else(|| GpuError("Direct3D 11 made no query".into()))
}

/// Whether the GPU has passed `query`'s end; `flush` lets the call submit
/// what is queued first.
fn query_done(context: &ID3D11DeviceContext, query: &ID3D11Query, flush: bool) -> bool {
    let flags = if flush {
        0
    } else {
        D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32
    };
    // Through the vtable: `GetData`'s wrapper takes S_FALSE (not yet) for
    // success too.
    // SAFETY: An event query of the context's device; its data (a BOOL) is
    // not asked for.
    let result = unsafe {
        (Interface::vtable(context).GetData)(
            Interface::as_raw(context),
            Interface::as_raw(query),
            std::ptr::null_mut(),
            0,
            flags,
        )
    };
    result.0 == 0
}

/// A texture of the device's: what shaders sample, and what a frame draws
/// into where it has a render target view.
pub(crate) struct Surface {
    pub(crate) texture: ID3D11Texture2D,
    pub(crate) rtv: Option<ID3D11RenderTargetView>,
    pub(crate) srv: Option<ID3D11ShaderResourceView>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Surface {
    /// `texture`'s views: a render target where `target`, sampled where
    /// `sampled`.
    pub(crate) fn of(
        device: &ID3D11Device,
        texture: ID3D11Texture2D,
        target: bool,
        sampled: bool,
    ) -> Result<Self, GpuError> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: Reads the texture's description.
        unsafe { texture.GetDesc(&mut desc) };
        let rtv = target
            .then(|| {
                let mut view = None;
                // SAFETY: A view of the whole texture, made with the binding.
                unsafe { device.CreateRenderTargetView(&texture, None, Some(&mut view)) }
                    .map_err(failed("render target view"))?;
                view.ok_or_else(|| GpuError("Direct3D 11 made no render target view".into()))
            })
            .transpose()?;
        let srv = sampled
            .then(|| {
                let mut view = None;
                // SAFETY: As above.
                unsafe { device.CreateShaderResourceView(&texture, None, Some(&mut view)) }
                    .map_err(failed("shader resource view"))?;
                view.ok_or_else(|| GpuError("Direct3D 11 made no shader resource view".into()))
            })
            .transpose()?;
        Ok(Self {
            texture,
            rtv,
            srv,
            width: desc.Width,
            height: desc.Height,
        })
    }
}

/// A renderer's own Direct3D 11 state: its vertex buffers, clip regions,
/// viewport, glyph atlas and (offscreen) retained target.
pub(crate) struct State {
    pub(crate) device: std::rc::Rc<Device>,
    pub(crate) ring: Ring,
    /// The frame's clip regions, the buffer's size.
    regions: Option<(ID3D11Buffer, ID3D11ShaderResourceView, usize)>,
    /// What `regions` holds: most frames clip with the last one's, and
    /// write nothing.
    last_regions: Vec<u8>,
    /// `crate::GpuRenderer::viewport_values` as last written.
    viewport: ID3D11Buffer,
    viewport_values: Cell<[f32; 4]>,
    /// The glyph atlas, the shell's `atlas` (`write_texels` makes it at its
    /// size).
    pub(crate) atlas: Option<Surface>,
    /// Offscreen: the retained target, and the spare scroll copies go
    /// through.
    pub(crate) target: Option<Surface>,
    pub(crate) spare: Option<Surface>,
    /// Isolated layers' contents (`crate::GpuRenderer::paint_layer`), each
    /// under the shell's texture standing for it.
    pub(crate) layers: rustc_hash::FxHashMap<wgpu::Texture, Surface>,
}

impl State {
    pub(crate) fn new(device: std::rc::Rc<Device>) -> Result<Self, GpuError> {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let mut viewport = None;
        // SAFETY: A plain constant buffer description.
        unsafe { device.device.CreateBuffer(&desc, None, Some(&mut viewport)) }
            .map_err(failed("viewport buffer"))?;
        Ok(Self {
            viewport: viewport
                .ok_or_else(|| GpuError("Direct3D 11 made no viewport buffer".into()))?,
            viewport_values: Cell::new([f32::NAN; 4]),
            device,
            ring: Ring::default(),
            regions: None,
            last_regions: Vec::new(),
            atlas: None,
            target: None,
            spare: None,
            layers: Default::default(),
        })
    }

    /// The frame's viewport and clip regions (`crate::Region`s as bytes).
    pub(crate) fn set_frame(&mut self, viewport: [f32; 4], regions: &[u8]) -> Result<(), GpuError> {
        let context = &self.device.context;
        if self.viewport_values.get().map(f32::to_bits) != viewport.map(f32::to_bits) {
            self.viewport_values.set(viewport);
            // SAFETY: Sixteen bytes into the 16-byte constant buffer.
            unsafe {
                context.UpdateSubresource(&self.viewport, 0, None, viewport.as_ptr().cast(), 0, 0)
            };
        }
        // No quad reads them (`mask.x` 0), or they are the last frame's:
        // whatever is bound stays.
        if regions.is_empty() || self.last_regions == regions {
            return Ok(());
        }
        self.last_regions.clear();
        self.last_regions.extend_from_slice(regions);
        let fits = self
            .regions
            .as_ref()
            .is_some_and(|(_, _, capacity)| *capacity >= regions.len());
        if !fits {
            let capacity = regions.len().next_power_of_two().max(4096);
            let (buffer, view) = raw_buffer(&self.device.device, capacity, None)?;
            self.regions = Some((buffer, view, capacity));
        }
        let (buffer, _, _) = self.regions.as_ref().expect("made above");
        let area = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: regions.len() as u32,
            bottom: 1,
            back: 1,
        };
        // SAFETY: `regions.len()` bytes into a buffer at least as large.
        unsafe { context.UpdateSubresource(buffer, 0, Some(&area), regions.as_ptr().cast(), 0, 0) };
        Ok(())
    }
}

/// Vertex buffers the CPU writes in place (dynamic, mapped without
/// overwriting what a frame in flight reads), reused once the GPU has
/// passed the frame that read them: `upload::MappedRing`'s, as Direct3D 11
/// has them. A kept frame's quads stay in a buffer between frames, so a
/// frame drawn from it writes only what changed (`write_kept`).
#[derive(Default)]
pub(crate) struct Ring {
    slots: Vec<Slot>,
    /// The slot written last.
    last: usize,
}

struct Slot {
    buffer: ID3D11Buffer,
    capacity: usize,
    /// Ended after the frame that read the slot last.
    query: ID3D11Query,
    /// Read by a frame the GPU may not have finished (its query ended).
    in_flight: bool,
    /// Never mapped yet: the first map discards.
    fresh: bool,
    /// As `upload::Slot::holds`.
    holds: Option<(u64, u64, usize)>,
}

const RING: usize = 4;
const MIN_BYTES: usize = 4096;

impl Ring {
    /// A slot of at least `len` bytes no frame in flight reads, if any;
    /// `flush` submits what is queued first, for a caller that waits.
    fn ready(&mut self, context: &ID3D11DeviceContext, len: usize, flush: bool) -> Option<usize> {
        for slot in &mut self.slots {
            if slot.in_flight && query_done(context, &slot.query, flush) {
                slot.in_flight = false;
            }
        }
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.capacity >= len && !slot.in_flight)
            .min_by_key(|(_, slot)| slot.capacity)
            .map(|(index, _)| index)
    }

    /// Copy `parts`, one after another, into a vertex buffer and return
    /// it; the second value reports a new allocation.
    pub(crate) fn write_parts(
        &mut self,
        device: &Device,
        parts: &[&[u8]],
    ) -> Result<(ID3D11Buffer, bool), GpuError> {
        let len = parts.iter().map(|part| part.len()).sum::<usize>();
        let mut index = self.ready(&device.context, len, false);
        // Every buffer is in flight, or too small: wait for the GPU rather
        // than grow, until one large enough is free or none is in flight (a
        // small one is then replaced below). Offscreen, nothing presents to
        // submit the frames: the wait does.
        if self.slots.len() >= RING {
            while index.is_none() && self.slots.iter().any(|slot| slot.in_flight) {
                std::thread::yield_now();
                index = self.ready(&device.context, len, true);
            }
        }
        let allocated = index.is_none();
        let index = match index {
            Some(index) => index,
            None => {
                // Replace a too-small idle buffer instead of accumulating them.
                if let Some(small) =
                    (self.slots.iter()).position(|slot| !slot.in_flight && slot.capacity < len)
                {
                    self.slots.swap_remove(small);
                }
                let capacity = len.next_power_of_two().max(MIN_BYTES);
                self.slots.push(Slot::new(device, capacity)?);
                self.slots.len() - 1
            }
        };
        let slot = &mut self.slots[index];
        slot.write(&device.context, |contents| {
            let mut at = 0;
            for part in parts {
                contents[at..at + part.len()].copy_from_slice(part);
                at += part.len();
            }
        })?;
        slot.holds = None;
        self.last = index;
        Ok((slot.buffer.clone(), allocated))
    }

    /// `upload::MappedRing::write_kept`, for this ring.
    pub(crate) fn write_kept(
        &mut self,
        device: &Device,
        parts: &[&[u8]],
        kept: &crate::upload::KeptQuads,
        kept_bytes: usize,
        unit: usize,
    ) -> Result<(ID3D11Buffer, bool), GpuError> {
        let len = parts.iter().map(|part| part.len()).sum::<usize>();
        self.ready(&device.context, len, false);
        let held = kept.held();
        let holding = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.capacity >= len
                    && !slot.in_flight
                    && !slot.fresh
                    && slot.holds.zip(held).is_some_and(
                        |((generation, version, bytes), (now, _))| {
                            generation == now
                                && kept.since(version).is_some()
                                && bytes == kept_bytes
                        },
                    )
            })
            .max_by_key(|(_, slot)| slot.holds.map(|(_, version, _)| version))
            .map(|(index, _)| index);
        let (Some(index), Some(quads)) = (holding, parts.first()) else {
            let written = self.write_parts(device, parts)?;
            if let Some((generation, version)) = held {
                self.slots[self.last].holds = Some((generation, version, kept_bytes));
            }
            return Ok(written);
        };
        let slot = &mut self.slots[index];
        let since = slot.holds.map_or(0, |(_, version, _)| version);
        slot.write(&device.context, |contents| {
            for (start, end) in kept.since(since).expect("a version it knows") {
                let (from, to) = (start as usize * unit, (end as usize * unit).min(kept_bytes));
                contents[from..to].copy_from_slice(&quads[from..to]);
            }
            contents[kept_bytes..quads.len()].copy_from_slice(&quads[kept_bytes..]);
            let mut at = quads.len();
            for part in &parts[1..] {
                contents[at..at + part.len()].copy_from_slice(part);
                at += part.len();
            }
        })?;
        slot.holds = held.map(|(generation, version)| (generation, version, kept_bytes));
        self.last = index;
        Ok((slot.buffer.clone(), false))
    }

    /// After a frame drawn from the buffer written last: it is read until
    /// the GPU passes the frame.
    pub(crate) fn submitted(&mut self, context: &ID3D11DeviceContext) {
        if let Some(slot) = self.slots.get_mut(self.last) {
            // SAFETY: The slot's own query, ended on the device's context.
            unsafe { context.End(&slot.query) };
            slot.in_flight = true;
        }
    }

    /// Drop buffers not in flight; in-flight ones are reused as usual.
    pub(crate) fn trim(&mut self) {
        self.slots.retain(|slot| slot.in_flight);
    }

    /// Buffers held.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
    pub(crate) fn bytes(&self) -> usize {
        self.slots.iter().map(|slot| slot.capacity).sum()
    }
}

impl Slot {
    fn new(device: &Device, capacity: usize) -> Result<Self, GpuError> {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: capacity as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let mut buffer = None;
        // SAFETY: A plain dynamic vertex buffer description.
        unsafe { device.device.CreateBuffer(&desc, None, Some(&mut buffer)) }
            .map_err(failed("vertex buffer"))?;
        Ok(Self {
            buffer: buffer.ok_or_else(|| GpuError("Direct3D 11 made no vertex buffer".into()))?,
            capacity,
            query: event_query(&device.device)?,
            in_flight: false,
            fresh: true,
            holds: None,
        })
    }

    /// Write the buffer in place through `fill`: no frame in flight reads
    /// it (`Ring::ready`), so nothing is overwritten that the GPU may read,
    /// and what it held stays (a kept frame's quads).
    fn write(
        &mut self,
        context: &ID3D11DeviceContext,
        fill: impl FnOnce(&mut [u8]),
    ) -> Result<(), GpuError> {
        let kind = match std::mem::replace(&mut self.fresh, false) {
            true => D3D11_MAP_WRITE_DISCARD,
            false => D3D11_MAP_WRITE_NO_OVERWRITE,
        };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: A dynamic buffer the CPU writes, `capacity` bytes, mapped
        // while `fill` writes it and unmapped after.
        unsafe {
            context
                .Map(&self.buffer, 0, kind, 0, Some(&mut mapped))
                .map_err(failed("vertex map"))?;
            fill(std::slice::from_raw_parts_mut(
                mapped.pData.cast::<u8>(),
                self.capacity,
            ));
            context.Unmap(&self.buffer, 0);
        }
        Ok(())
    }
}

/// One frame's draws into its target, as `metal_frame::Frame` records them.
pub(crate) struct Frame<'a> {
    device: &'a Device,
    current: Option<Bound>,
    /// The scissor last set: setting it again costs the driver as much as
    /// a change (most batches share their region's).
    scissor: Option<Scissor>,
}

impl Frame<'_> {
    fn bind(&mut self, bound: Bound) {
        if self.current == Some(bound) {
            return;
        }
        self.current = Some(bound);
        let pipeline = &self.device.pipelines[bound as usize];
        let context = &self.device.context;
        // SAFETY: The device's own shaders, layout and blend state.
        unsafe {
            context.IASetInputLayout(&pipeline.layout);
            context.VSSetShader(&pipeline.vertex, None);
            context.PSSetShader(&pipeline.pixel, None);
            context.OMSetBlendState(&pipeline.blend, None, u32::MAX);
        }
    }
    /// The texture fragments sample: the atlas, or for catch-up copies the
    /// shown buffer (whose size the copy's vertex stage reads).
    pub(crate) fn texture(&mut self, view: &ID3D11ShaderResourceView) {
        // SAFETY: A live view, in the register both modules read it from.
        unsafe {
            self.device
                .context
                .VSSetShaderResources(TEXTURE, Some(&[Some(view.clone())]));
            self.device
                .context
                .PSSetShaderResources(TEXTURE, Some(&[Some(view.clone())]));
        }
    }
    pub(crate) fn scissor(&mut self, rect: Scissor) {
        if self.scissor == Some(rect) {
            return;
        }
        self.scissor = Some(rect);
        let (x, y, width, height) = rect;
        // SAFETY: One rectangle.
        unsafe {
            self.device.context.RSSetScissorRects(Some(&[RECT {
                left: x as i32,
                top: y as i32,
                right: (x + width) as i32,
                bottom: (y + height) as i32,
            }]))
        };
    }
    /// Draw `instances` quads of `shading`; `clear` for the pipeline that
    /// replaces rather than blends (a region's background).
    pub(crate) fn quads(&mut self, shading: Shading, clear: bool, instances: std::ops::Range<u32>) {
        let bound = match (shading, clear) {
            (Shading::Basic | Shading::Opaque, true) => Bound::Opaque,
            (_, true) => Bound::FullClear,
            (Shading::Basic, false) => Bound::Basic,
            (Shading::Opaque, false) => Bound::Opaque,
            // The proof draws nothing else (`GpuRenderer::d3d11_lacks`).
            (_, false) => Bound::Full,
        };
        self.bind(bound);
        self.draw(instances);
    }
    /// Draw `instances` shifted copies from the texture bound with `texture`.
    pub(crate) fn copies(&mut self, instances: std::ops::Range<u32>) {
        self.bind(Bound::Copy);
        self.draw(instances);
    }
    fn draw(&mut self, instances: std::ops::Range<u32>) {
        // SAFETY: Four-vertex strips over instances the vertex buffer holds:
        // the renderer draws ranges of the quads it wrote to it.
        unsafe {
            self.device.context.DrawInstanced(
                4,
                instances.end - instances.start,
                0,
                instances.start,
            )
        };
    }
    /// The frame recorded: the immediate context submits it as it goes, and
    /// presenting flushes it. Then `Ring::submitted`.
    pub(crate) fn end(self) {
        // SAFETY: Unbinds the target, which a resize or readback may next
        // release or copy, and the texture sampled, which may be a layer
        // the next frame draws into.
        unsafe {
            self.device.context.OMSetRenderTargets(None, None);
            self.device
                .context
                .PSSetShaderResources(TEXTURE, Some(&[None]));
            self.device
                .context
                .VSSetShaderResources(TEXTURE, Some(&[None]));
        }
    }
}

/// A WGSL module as HLSL for shader model 5.0 (Direct3D 11's feature level
/// 11_0), with zgx's registers: `TEXTURE`, `REGIONS`, the viewport in b0
/// and the sampler in s0. naga writes samplers through Direct3D 12's
/// sampler heap, which shader model 5.0 has none of: its declarations are
/// replaced with the plain sampler the module binds.
#[cfg(test)]
fn translate(source: &str, copies: bool) -> Result<(String, Vec<(String, String)>), String> {
    use naga::back::hlsl;
    use wgpu::naga;
    let module = naga::front::wgsl::parse_str(source).map_err(|e| e.to_string())?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|e| e.to_string())?;
    let target = |register| hlsl::BindTarget {
        space: 0,
        register,
        ..Default::default()
    };
    let binding = |group, binding| naga::ResourceBinding { group, binding };
    let mut binding_map = hlsl::BindingMap::new();
    if copies {
        binding_map.insert(binding(0, 0), target(TEXTURE));
    } else {
        binding_map.insert(binding(0, 1), target(TEXTURE));
        // The sampler's index in the heap's table (replaced below).
        binding_map.insert(binding(0, 2), target(0));
        binding_map.insert(binding(1, 0), target(0));
        binding_map.insert(binding(1, 1), target(REGIONS));
    }
    let mut sampler_buffer_binding_map = hlsl::SamplerIndexBufferBindingMap::new();
    sampler_buffer_binding_map.insert(hlsl::SamplerIndexBufferKey { group: 0 }, target(15));
    // As wgpu's Direct3D 12 backend writes them (wgpu-hal dx12 device.rs).
    let options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V5_0,
        binding_map,
        sampler_buffer_binding_map,
        restrict_indexing: true,
        force_loop_bounding: true,
        ..Default::default()
    };
    let mut hlsl = String::new();
    let reflection = hlsl::Writer::new(&mut hlsl, &options, &hlsl::PipelineOptions::default())
        .write(&module, &info, None)
        .map_err(|e| e.to_string())?;
    let names = module
        .entry_points
        .iter()
        .zip(reflection.entry_point_names)
        .map(|(entry, name)| Ok((entry.name.clone(), name.map_err(|e| e.to_string())?)))
        .collect::<Result<Vec<_>, String>>()?;
    if copies {
        return Ok((hlsl, names));
    }
    // The sampler heap, its comparison twin and group 0's index table go;
    // the sampler read from them is bound itself.
    let mut out = String::new();
    let mut replaced = 0;
    for line in hlsl.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("SamplerState nagaSamplerHeap[")
            || trimmed.starts_with("SamplerComparisonState nagaComparisonSamplerHeap[")
            || trimmed.starts_with("StructuredBuffer<uint> nagaGroup0SamplerIndexArray")
        {
            replaced += 1;
            continue;
        }
        if trimmed.starts_with("static const SamplerState tex_sampler = nagaSamplerHeap[") {
            replaced += 1;
            out.push_str("SamplerState tex_sampler : register(s0);\n");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if replaced != 4 || out.contains("nagaSamplerHeap") || out.contains("SamplerIndexArray") {
        return Err(format!(
            "naga's sampler heap is not written as expected ({replaced} of 4 lines replaced)"
        ));
    }
    Ok((out, names))
}

/// FXC's DXBC for `entry` (an HLSL name) of `hlsl`, as wgpu's Direct3D 12
/// backend compiles release shaders (`D3DCOMPILE_ENABLE_STRICTNESS`).
#[cfg(test)]
fn compile(hlsl: &str, entry: &str, target: &str) -> Result<Vec<u8>, String> {
    use windows::{
        Win32::Graphics::Direct3D::{Fxc::*, ID3DBlob},
        core::PCSTR,
    };
    let entry = std::ffi::CString::new(entry).unwrap();
    let target = std::ffi::CString::new(target).unwrap();
    let (mut code, mut errors): (Option<ID3DBlob>, Option<ID3DBlob>) = (None, None);
    // SAFETY: `hlsl`'s bytes and the names live across the call; the blobs
    // are read while they live.
    unsafe {
        let result = D3DCompile(
            hlsl.as_ptr().cast(),
            hlsl.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            D3DCOMPILE_ENABLE_STRICTNESS,
            0,
            &mut code,
            Some(&mut errors),
        );
        let text = |blob: &ID3DBlob| {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                blob.GetBufferPointer().cast::<u8>(),
                blob.GetBufferSize(),
            ))
            .into_owned()
        };
        if let Err(error) = result {
            return Err(errors.as_ref().map_or_else(|| error.to_string(), text));
        }
        let code = code.ok_or("FXC wrote no code")?;
        Ok(
            std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize())
                .to_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_hlsl_matches() {
        let bless = std::env::var_os("ZGX_BLESS").is_some_and(|v| v != "0");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let stale = |file: &str| {
            format!(
                "src/{file} is not its WGSL's translation: \
                 `ZGX_BLESS=1 cargo test -p zgx-gpu --features d3d11 --lib translated_hlsl`"
            )
        };
        for (wgsl, copies, name, entries) in [
            (
                include_str!("draw.wgsl"),
                false,
                "draw",
                &[
                    ("vs", "vs_5_0", DRAW_VS),
                    ("fs", "ps_5_0", DRAW_FS),
                    ("vs_basic", "vs_5_0", BASIC_VS),
                    ("fs_basic", "ps_5_0", BASIC_FS),
                ][..],
            ),
            (
                include_str!("copy_offset.wgsl"),
                true,
                "copy_offset",
                &[("vs", "vs_5_0", COPY_VS), ("fs", "ps_5_0", COPY_FS)][..],
            ),
        ] {
            let (hlsl, names) = translate(wgsl, copies).unwrap();
            let file = format!("{name}.hlsl");
            if bless {
                std::fs::write(dir.join(&file), &hlsl).unwrap();
            } else {
                let kept = std::fs::read_to_string(dir.join(&file)).unwrap_or_default();
                assert!(kept == hlsl, "{}", stale(&file));
            }
            for &(entry, target, kept) in entries {
                let (_, hlsl_name) = names
                    .iter()
                    .find(|(wgsl_name, _)| wgsl_name == entry)
                    .unwrap_or_else(|| panic!("no entry point `{entry}` in {name}.wgsl"));
                let code = compile(&hlsl, hlsl_name, target)
                    .unwrap_or_else(|e| panic!("FXC failed on {name}.hlsl `{entry}`: {e}"));
                let file = format!("dxbc/{name}_{entry}.dxbc");
                if bless {
                    std::fs::write(dir.join(&file), &code).unwrap();
                } else {
                    assert!(kept == &code[..], "{}", stale(&file));
                }
            }
        }
    }
}
