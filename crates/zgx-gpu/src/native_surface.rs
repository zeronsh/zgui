//! CoreVideo→Metal import. CPU code never reads pixel memory.
use crate::GpuError;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetWidthOfPlane,
};
use objc2_metal::{MTLPixelFormat, MTLTextureType};
use std::{
    collections::{HashMap, HashSet},
    ptr::NonNull,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use zgx::native_surface::{NativeSurface, SurfaceFormat};
struct Entry {
    _source: Rc<NativeSurface>,
    _planes: Vec<CFRetained<CVMetalTexture>>,
    texture: wgpu::Texture,
    bytes: usize,
}
/// Resources retired from a scene remain alive until submitted GPU work completes.
pub(crate) struct SurfaceCache {
    cache: CFRetained<CVMetalTextureCache>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    entries: HashMap<u64, Entry>,
    retired: Vec<(u64, Entry)>,
    submitted: std::cell::Cell<u64>,
    completed: Arc<AtomicU64>,
    conversion: Option<wgpu::ComputePipeline>,
}
impl SurfaceCache {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, GpuError> {
        // SAFETY: The raw device is only handed to CoreVideo, which retains
        // it; nothing destroys it.
        let metal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }
            .ok_or_else(|| GpuError("CoreVideo surfaces require a Metal device".into()))?;
        let mut pointer = std::ptr::null_mut();
        // SAFETY: No attribute dictionaries (their values are untyped); the
        // output points to a writable local.
        let status = unsafe {
            CVMetalTextureCache::create(
                None,
                None,
                metal.raw_device(),
                None,
                NonNull::from(&mut pointer),
            )
        };
        if status != 0 {
            return Err(GpuError(format!(
                "CVMetalTextureCacheCreate failed: {status}"
            )));
        }
        let pointer = NonNull::new(pointer)
            .ok_or_else(|| GpuError("CoreVideo returned no texture cache".into()))?;
        // SAFETY: A successful Create returns a +1 reference, adopted here.
        let cache = unsafe { CFRetained::from_raw(pointer) };
        Ok(Self {
            cache,
            device: device.clone(),
            queue: queue.clone(),
            entries: HashMap::new(),
            retired: Vec::new(),
            submitted: std::cell::Cell::new(0),
            completed: Arc::new(AtomicU64::new(0)),
            conversion: None,
        })
    }
    pub fn submitted(&self) {
        self.submitted.set(
            self.submitted
                .get()
                .checked_add(1)
                .expect("surface submission identity exhausted"),
        );
        let epoch = self.submitted.get();
        let completed = self.completed.clone();
        self.queue.on_submitted_work_done(move || {
            completed.fetch_max(epoch, Ordering::Release);
        });
    }
    pub fn retain(&mut self, live: &HashSet<u64>) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        let dead: Vec<_> = self
            .entries
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        for id in dead {
            self.retired
                .push((self.submitted.get(), self.entries.remove(&id).unwrap()));
        }
        let completed = self.completed.load(Ordering::Acquire);
        self.retired.retain(|(epoch, _)| *epoch > completed);
        self.cache.flush(0);
    }
    pub fn contains(&self, id: u64) -> bool {
        self.entries.contains_key(&id)
    }
    pub fn texture(&self, id: u64) -> &wgpu::Texture {
        &self.entries[&id].texture
    }
    pub fn bytes(&self, id: u64) -> usize {
        self.entries[&id].bytes
    }
    pub fn import(&mut self, source: Rc<NativeSurface>) -> Result<(), GpuError> {
        if self.contains(source.id()) {
            return Ok(());
        }
        let (width, height) = (source.width(), source.height());
        if width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            return Err(GpuError("surface exceeds Metal texture limit".into()));
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| {
                n.checked_mul(if source.format() == SurfaceFormat::Bgra {
                    4
                } else {
                    6
                })
            })
            .ok_or_else(|| GpuError("surface size overflow".into()))?;
        let retained_bytes = |this: &Self| {
            this.entries
                .values()
                .map(|e| e.bytes)
                .chain(this.retired.iter().map(|(_, e)| e.bytes))
                .sum::<usize>()
        };
        if retained_bytes(self).saturating_add(bytes) > 64 * 1024 * 1024 && !self.retired.is_empty()
        {
            // Backpressure only at the byte limit: let already-submitted reads
            // complete instead of failing an otherwise valid stream replacement.
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| GpuError(format!("surface completion failed: {e}")))?;
            let completed = self.completed.load(Ordering::Acquire);
            self.retired.retain(|(epoch, _)| *epoch > completed);
        }
        if retained_bytes(self).saturating_add(bytes) > 64 * 1024 * 1024 {
            return Err(GpuError(
                "active and in-flight CoreVideo surfaces exceed 64 MiB".into(),
            ));
        }
        if self.entries.len() + self.retired.len() >= 1024 {
            return Err(GpuError(
                "native surface cache exceeds 1024 retained frames".into(),
            ));
        }
        let mut planes = Vec::new();
        let texture = if source.format() == SurfaceFormat::Bgra {
            let (texture, plane) = self.plane(
                &source,
                0,
                width,
                height,
                MTLPixelFormat::BGRA8Unorm,
                wgpu::TextureFormat::Bgra8Unorm,
            )?;
            planes.push(plane);
            texture
        } else {
            let buffer = source.pixel_buffer();
            let (yw, yh) = (
                CVPixelBufferGetWidthOfPlane(buffer, 0),
                CVPixelBufferGetHeightOfPlane(buffer, 0),
            );
            let (cw, ch) = (
                CVPixelBufferGetWidthOfPlane(buffer, 1),
                CVPixelBufferGetHeightOfPlane(buffer, 1),
            );
            if (yw, yh) != (width as usize, height as usize)
                || (cw, ch) != (width.div_ceil(2) as usize, height.div_ceil(2) as usize)
            {
                return Err(GpuError("NV12 plane dimensions do not match frame".into()));
            }
            let (y, yp) = self.plane(
                &source,
                0,
                width,
                height,
                MTLPixelFormat::R8Unorm,
                wgpu::TextureFormat::R8Unorm,
            )?;
            let (uv, uvp) = self.plane(
                &source,
                1,
                cw as u32,
                ch as u32,
                MTLPixelFormat::RG8Unorm,
                wgpu::TextureFormat::Rg8Unorm,
            )?;
            planes.extend([yp, uvp]);
            let output = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("NV12 converted frame"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            if self.conversion.is_none() {
                let module = self
                    .device
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("full-range NV12 conversion"),
                        source: wgpu::ShaderSource::Wgsl(
                            include_str!("native_surface.wgsl").into(),
                        ),
                    });
                self.conversion = Some(self.device.create_compute_pipeline(
                    &wgpu::ComputePipelineDescriptor {
                        label: Some("full-range NV12 conversion"),
                        layout: None,
                        module: &module,
                        entry_point: Some("convert"),
                        compilation_options: Default::default(),
                        cache: None,
                    },
                ));
            }
            let pipeline = self.conversion.as_ref().unwrap();
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("NV12 planes"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &y.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &uv.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &output.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("NV12 conversion"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("NV12 conversion"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
            }
            self.queue.submit([encoder.finish()]);
            self.submitted();
            output
        };
        self.entries.insert(
            source.id(),
            Entry {
                _source: source,
                _planes: planes,
                texture,
                bytes,
            },
        );
        Ok(())
    }
    fn plane(
        &self,
        source: &NativeSurface,
        plane: usize,
        width: u32,
        height: u32,
        format: MTLPixelFormat,
        wgpu_format: wgpu::TextureFormat,
    ) -> Result<(wgpu::Texture, CFRetained<CVMetalTexture>), GpuError> {
        let mut pointer = std::ptr::null_mut();
        // SAFETY: A live pixel buffer, this plane's own dimensions (the
        // buffer's, or NV12's checked in `import`) and a format of its
        // pixels; no attribute dictionary, and the output is a writable local.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.cache,
                source.pixel_buffer(),
                None,
                format,
                width as usize,
                height as usize,
                plane,
                NonNull::from(&mut pointer),
            )
        };
        if status != 0 {
            return Err(GpuError(format!(
                "CoreVideo Metal plane import failed: {status}"
            )));
        }
        let pointer =
            NonNull::new(pointer).ok_or_else(|| GpuError("CoreVideo returned no plane".into()))?;
        // SAFETY: A successful Create returns a +1 reference, adopted here.
        let retained = unsafe { CFRetained::from_raw(pointer) };
        let metal = CVMetalTextureGetTexture(&retained)
            .ok_or_else(|| GpuError("CoreVideo plane has no Metal texture".into()))?;
        // SAFETY: CoreVideo created this initialized texture from exactly this
        // device, format and dimensions. Both the CV wrapper and source frame
        // remain retained through completion of every submitted access.
        let raw = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                metal,
                wgpu_format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
            )
        };
        // SAFETY: The texture is of this device (the cache was made on its
        // raw device), as the descriptor says: CoreVideo's textures are
        // shader-readable. `NativeSurface::from_pixel_buffer`'s caller
        // promised its pixels written and left alone; the entry keeps the
        // CoreVideo wrapper and the frame until submitted reads complete.
        let texture = unsafe {
            self.device
                .create_texture_from_hal::<wgpu::hal::api::Metal>(
                    raw,
                    &wgpu::TextureDescriptor {
                        label: Some("CoreVideo plane"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu_format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                )
        };
        Ok((texture, retained))
    }
}
impl Drop for SurfaceCache {
    fn drop(&mut self) {
        // Teardown only: CoreVideo wrappers must outlive GPU accesses, including
        // when a window closes before completion callbacks have been polled.
        if !self.entries.is_empty() || !self.retired.is_empty() {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }
    }
}
