//! Per-frame vertex uploads (and the backdrop filters' records after them)
//! without GPU copies on unified-memory adapters.
//!
//! A copy (a Metal blit encoder) every frame costs an encoder switch and keeps
//! the driver's blit machinery (~110 MB on Apple GPUs) resident while anything
//! animates. With `MAPPABLE_PRIMARY_BUFFERS` the CPU writes vertex buffers
//! directly; each buffer is remapped once the GPU has finished reading it.
//! On macOS the buffers are shared memory the CPU writes in place, never
//! mapped through wgpu: mapping and unmapping one each frame cost more than
//! writing it. Direct3D 12 does the same with its upload heap, which may stay
//! mapped while the GPU reads it: each buffer is mapped once, for good (a
//! frame's Map, Unmap and wgpu's transitions out of and back into mapping
//! cost NVIDIA's driver and wgpu some 40 K cycles).
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Buffers kept for reuse; more are only allocated while all are in flight.
const RING: usize = 4;
const MIN_BYTES: u64 = 4096;

struct Slot {
    buffer: wgpu::Buffer,
    capacity: u64,
    /// The CPU may write the buffer: mapped, or (written in place) the GPU
    /// has finished the submission that read it.
    mapped: Arc<AtomicBool>,
    /// Written for the coming submission; released once it completes.
    remap: bool,
    /// Written in place with a kept frame's quads first (`write_kept`):
    /// which (`KeptQuads::generation`), as of which patch, and how many
    /// bytes of them. A slot made again (grown, replaced, trimmed) holds
    /// none.
    holds: Option<(u64, u64, usize)>,
    /// macOS: the buffer's memory, written in place.
    #[cfg(target_os = "macos")]
    contents: std::ptr::NonNull<u8>,
    /// macOS: the buffer itself, for frames encoded with Metal directly.
    #[cfg(target_os = "macos")]
    metal: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    /// Windows: the buffer's upload-heap memory, mapped for good and written
    /// in place; None off Direct3D 12, mapped through wgpu each frame.
    #[cfg(windows)]
    contents: Option<std::ptr::NonNull<u8>>,
}
impl Slot {
    /// The buffer's memory when written in place, mapped for good.
    fn in_place(&self) -> Option<std::ptr::NonNull<u8>> {
        #[cfg(target_os = "macos")]
        return Some(self.contents);
        #[cfg(windows)]
        return self.contents;
        #[cfg(not(any(target_os = "macos", windows)))]
        None
    }
}

pub(crate) struct MappedRing {
    device: wgpu::Device,
    queue: wgpu::Queue,
    slots: Vec<Slot>,
    /// The slot `write` filled last.
    last: usize,
}
/// Kept frames told apart (`KeptQuads::generation`); 0 is none.
static GENERATIONS: AtomicU64 = AtomicU64::new(1);
/// Patches a kept frame remembers the writes of (`KeptQuads`).
const CHANGES: usize = 8;
/// Draws in a row finding a kept frame as long after which its writes are
/// remembered again (`KeptQuads::drawn`).
const STEADY: u8 = 2;
/// What a kept frame's quads (lib.rs `Kept`: a window drawn whole, then
/// patched) have had written by its recent patches, so that a buffer
/// still holding them takes only what changed since (`write_kept`).
#[derive(Default)]
pub(crate) struct KeptQuads {
    /// Which kept frame: unique, or 0 for none.
    generation: u64,
    /// Patches since it was made.
    version: u64,
    /// The quads each of the `CHANGES` patches before this one wrote,
    /// oldest first.
    changes: std::collections::VecDeque<(u64, Vec<(u32, u32)>)>,
    /// And this one's (version `version`).
    writing: Vec<(u32, u32)>,
    /// Whether writes are remembered: not while its length changes from
    /// one frame drawn from it to the next (a splice mounting rows), when
    /// every buffer takes it whole anyway (`drawn`).
    tracking: bool,
    /// Its bytes when last drawn from, and how many draws in a row since
    /// found it as long.
    drawn_bytes: usize,
    steady: u8,
}
impl KeptQuads {
    pub(crate) fn new() -> Self {
        Self {
            generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
            tracking: true,
            ..Default::default()
        }
    }
    /// A frame draws from it, `bytes` of its quads: writes stop being
    /// remembered when that differs from the last, and are remembered again,
    /// as a new kept frame no buffer holds, after `STEADY` draws in a row
    /// find it as long (a list mounting a row now and then keeps it off).
    pub(crate) fn drawn(&mut self, bytes: usize) {
        self.steady = match bytes == self.drawn_bytes {
            true => self.steady.saturating_add(1),
            false => 0,
        };
        self.drawn_bytes = bytes;
        if self.steady == 0 {
            self.tracking = false;
        } else if !self.tracking && self.steady >= STEADY {
            self.generation = GENERATIONS.fetch_add(1, Ordering::Relaxed);
            self.version = 0;
            self.changes.clear();
            self.writing.clear();
            self.tracking = true;
        }
    }
    /// A patch begins: what it writes counts as one version (nothing to
    /// count while writes go unremembered).
    pub(crate) fn patch(&mut self) {
        if !self.tracking {
            return;
        }
        let mut spare = match self.changes.len() == CHANGES {
            true => self
                .changes
                .pop_front()
                .map(|(_, wrote)| wrote)
                .unwrap_or_default(),
            false => Vec::new(),
        };
        spare.clear();
        let wrote = std::mem::replace(&mut self.writing, spare);
        if self.version > 0 {
            self.changes.push_back((self.version, wrote));
        }
        self.version += 1;
    }
    /// This patch wrote quads `start..end` (one run with the last, when
    /// they meet: neighbouring items' writes copy at once).
    #[inline(always)]
    pub(crate) fn wrote(&mut self, start: u32, end: u32) {
        if !self.tracking {
            return;
        }
        match self.writing.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => self.writing.push((start, end)),
        }
    }
    /// What a buffer written with its quads now holds of them, while its
    /// writes are remembered: which kept frame, as of which patch
    /// (d3d11_frame.rs's ring keeps it as `MappedRing` does).
    #[cfg(d3d11)]
    pub(crate) fn held(&self) -> Option<(u64, u64)> {
        (self.generation != 0 && self.tracking).then_some((self.generation, self.version))
    }
    /// The runs of quads written after `version`, if it is one it knows
    /// (no older than the patch before the first it remembers).
    pub(crate) fn since(&self, version: u64) -> Option<impl Iterator<Item = (u32, u32)> + '_> {
        // (The open patch's writes are remembered too.)
        let first = self
            .changes
            .front()
            .map_or(self.version, |(first, _)| *first);
        let known = first.saturating_sub(1);
        (known..=self.version).contains(&version).then(|| {
            self.changes
                .iter()
                .filter(move |(at, _)| *at > version)
                .flat_map(|(_, wrote)| wrote.iter().copied())
                .chain(
                    self.writing
                        .iter()
                        .copied()
                        .filter(move |_| self.version > version),
                )
        })
    }
}
impl MappedRing {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            slots: Vec::new(),
            last: 0,
        }
    }
    /// Copy `parts`, one after another, into a CPU-written vertex buffer
    /// and return it, ready for the GPU. The second value reports a new
    /// allocation.
    pub(crate) fn write_parts(&mut self, parts: &[&[u8]]) -> (wgpu::Buffer, bool) {
        let len = parts.iter().map(|part| part.len() as u64).sum::<u64>();
        let mut index = self.ready(len);
        if index.is_none() {
            // Deliver callbacks for frames the GPU has already finished.
            let _ = self.device.poll(wgpu::PollType::Poll);
            index = self.ready(len);
        }
        if index.is_none() && self.slots.len() >= RING {
            // Every buffer is in flight: wait for the GPU rather than grow.
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            index = self.ready(len);
        }
        let allocated = index.is_none();
        let index = match index {
            Some(index) => index,
            None => {
                // Replace a too-small idle buffer instead of accumulating them.
                if let Some(small) = self
                    .slots
                    .iter()
                    .position(|slot| slot.mapped.load(Ordering::Acquire) && slot.capacity < len)
                {
                    self.slots.swap_remove(small);
                }
                let capacity = len.next_power_of_two().max(MIN_BYTES);
                self.slots.push(self.slot(capacity));
                self.slots.len() - 1
            }
        };
        let slot = &mut self.slots[index];
        if let Some(contents) = slot.in_place() {
            let mut at = 0;
            for part in parts {
                // SAFETY: `contents` is the buffer's mapped memory, alive with
                // the buffer (on macOS, with `slot.metal`), `capacity` bytes,
                // at least `len`, so every part fits from `at`; Rust holds no
                // other reference into it. The slot is ready: no submission
                // still reads it (`submitted`, or `completed` after a direct
                // frame was waited for).
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        part.as_ptr(),
                        contents.as_ptr().add(at),
                        part.len(),
                    )
                };
                at += part.len();
            }
        } else {
            if len > 0 {
                let mut range = slot.buffer.slice(..len).get_mapped_range_mut();
                let mut at = 0;
                for part in parts {
                    range.slice(at..at + part.len()).copy_from_slice(part);
                    at += part.len();
                }
            }
            slot.buffer.unmap();
        }
        slot.mapped.store(false, Ordering::Release);
        slot.remap = true;
        slot.holds = None;
        self.last = index;
        (slot.buffer.clone(), allocated)
    }
    /// `write_parts`, where `parts[0]` begins with `kept_bytes` of kept
    /// frame `kept`'s quads (`unit` bytes each): into a ready buffer still
    /// holding them as of a patch `kept` remembers, only the quads written
    /// since and what follows them; otherwise all of it, the buffer then
    /// holding them. The buffer's bytes are those `write_parts` writes.
    pub(crate) fn write_kept(
        &mut self,
        parts: &[&[u8]],
        kept: &KeptQuads,
        kept_bytes: usize,
        unit: usize,
    ) -> (wgpu::Buffer, bool) {
        let len = parts.iter().map(|part| part.len() as u64).sum::<u64>();
        let holding = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.capacity >= len
                    && slot.in_place().is_some()
                    && slot.mapped.load(Ordering::Acquire)
                    && slot.holds.is_some_and(|(generation, version, bytes)| {
                        kept.generation != 0
                            && kept.tracking
                            && generation == kept.generation
                            && kept.since(version).is_some()
                            && bytes == kept_bytes
                    })
            })
            .max_by_key(|(_, slot)| slot.holds.map(|(_, version, _)| version))
            .map(|(index, _)| index);
        let (Some(index), Some(quads)) = (holding, parts.first()) else {
            let written = self.write_parts(parts);
            let slot = &mut self.slots[self.last];
            if kept.generation != 0 && kept.tracking && slot.in_place().is_some() {
                slot.holds = Some((kept.generation, kept.version, kept_bytes));
            }
            return written;
        };
        let slot = &mut self.slots[index];
        let contents = slot.in_place().expect("written in place");
        let since = slot.holds.map_or(0, |(_, version, _)| version);
        let copy = |from: usize, bytes: &[u8]| {
            // SAFETY: As `write_parts`: `contents` is the buffer's memory,
            // `capacity` bytes, at least `len`, and every write lies within
            // the parts' `len` bytes; the slot is ready (no submission
            // still reads it).
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    contents.as_ptr().add(from),
                    bytes.len(),
                )
            };
        };
        for (start, end) in kept.since(since).expect("a version it knows") {
            let (from, to) = (start as usize * unit, end as usize * unit);
            copy(from, &quads[from..to.min(kept_bytes)]);
        }
        copy(kept_bytes, &quads[kept_bytes..]);
        let mut at = quads.len();
        for part in &parts[1..] {
            copy(at, part);
            at += part.len();
        }
        slot.mapped.store(false, Ordering::Release);
        slot.remap = true;
        slot.holds = Some((kept.generation, kept.version, kept_bytes));
        self.last = index;
        (slot.buffer.clone(), false)
    }
    /// The Metal buffer `write` filled last.
    #[cfg(target_os = "macos")]
    pub(crate) fn last_metal(&self) -> &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLBuffer> {
        &self.slots[self.last].metal
    }
    /// The GPU has finished every frame written so far (one encoded with
    /// Metal directly, waited for): their buffers are writable again.
    #[cfg(target_os = "macos")]
    pub(crate) fn completed(&mut self) {
        for slot in self.slots.iter_mut().filter(|slot| slot.remap) {
            slot.remap = false;
            slot.mapped.store(true, Ordering::Release);
        }
    }
    #[cfg(target_os = "macos")]
    fn slot(&self, capacity: u64) -> Slot {
        use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};
        // SAFETY: The raw device only makes a buffer; nothing destroys it, and
        // the guard is dropped before wgpu is called again.
        let hal = unsafe { self.device.as_hal::<wgpu::hal::api::Metal>() }.expect("a Metal device");
        let raw = hal
            .raw_device()
            .newBufferWithLength_options(
                capacity as usize,
                MTLResourceOptions::StorageModeShared
                    | MTLResourceOptions::CPUCacheModeWriteCombined,
            )
            .expect("a shared vertex buffer");
        drop(hal);
        let contents = raw.contents().cast::<u8>();
        let metal = raw.clone();
        // SAFETY: Metal made this buffer on this device, `capacity` bytes long
        // (never zero: at least `MIN_BYTES`), usable as any buffer. wgpu takes
        // it as initialized: only bytes `write` filled are drawn from.
        let buffer = unsafe {
            let raw = wgpu::hal::metal::Device::buffer_from_raw(raw, capacity);
            self.device.create_buffer_from_hal::<wgpu::hal::api::Metal>(
                raw,
                &wgpu::BufferDescriptor {
                    label: Some("frame vertices"),
                    size: capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                },
            )
        };
        Slot {
            buffer,
            capacity,
            mapped: Arc::new(AtomicBool::new(true)),
            remap: false,
            holds: None,
            contents,
            metal,
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn slot(&self, capacity: u64) -> Slot {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mapped frame vertices"),
            size: capacity,
            usage: wgpu::BufferUsages::MAP_WRITE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: true,
        });
        #[cfg(windows)]
        let contents = map_for_good(&self.device, &buffer, capacity);
        // Direct3D counts a resource's maps: unmapped through wgpu (which
        // takes the whole buffer as written), it stays mapped by the above.
        #[cfg(windows)]
        if contents.is_some() {
            buffer.unmap();
        }
        Slot {
            buffer,
            capacity,
            mapped: Arc::new(AtomicBool::new(true)),
            remap: false,
            holds: None,
            #[cfg(windows)]
            contents,
        }
    }
    /// Call after every submission: buffers written for it become writable
    /// again once the GPU completes it.
    pub(crate) fn submitted(&mut self) {
        let mut written = Vec::new();
        for slot in self.slots.iter_mut().filter(|slot| slot.remap) {
            slot.remap = false;
            let mapped = slot.mapped.clone();
            if slot.in_place().is_some() {
                written.push(mapped);
            } else {
                slot.buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Write, move |result| {
                        if result.is_ok() {
                            mapped.store(true, Ordering::Release);
                        }
                    });
            }
        }
        if !written.is_empty() {
            self.queue.on_submitted_work_done(move || {
                for ready in written {
                    ready.store(true, Ordering::Release);
                }
            });
        }
    }
    /// Drop buffers not in flight; in-flight ones are reused as usual.
    pub(crate) fn trim(&mut self) {
        self.slots
            .retain(|slot| !slot.mapped.load(Ordering::Acquire));
    }
    /// Buffers held.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
    pub(crate) fn bytes(&self) -> usize {
        self.slots.iter().map(|slot| slot.capacity as usize).sum()
    }
    fn ready(&self, len: u64) -> Option<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.capacity >= len && slot.mapped.load(Ordering::Acquire))
            .min_by_key(|(_, slot)| slot.capacity)
            .map(|(index, _)| index)
    }
}

/// Windows: map `buffer` (`capacity` bytes, mapped by wgpu at creation) once
/// more through Direct3D 12, for as long as it lives; None off Direct3D 12.
#[cfg(windows)]
fn map_for_good(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    capacity: u64,
) -> Option<std::ptr::NonNull<u8>> {
    use wgpu::hal::Device as _;
    // SAFETY: The raw device and buffer only map the buffer; nothing destroys
    // either, and the guards are dropped before wgpu is called again.
    let hal_device = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }?;
    let hal_buffer = unsafe { buffer.as_hal::<wgpu::hal::api::Dx12>() }?;
    // SAFETY: A MAP_WRITE buffer lives in the upload heap, which may stay
    // mapped while the GPU reads it; mapped whole, and never unmapped: the
    // mapping ends with the buffer.
    unsafe { hal_device.map_buffer(&hal_buffer, 0..capacity) }
        .ok()
        .map(|mapping| mapping.ptr)
}

/// A small buffer filled at creation. Mapped adapters write it directly;
/// otherwise wgpu stages the contents with a copy.
pub(crate) fn init_buffer(
    device: &wgpu::Device,
    mapped: bool,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    if !mapped {
        use wgpu::util::DeviceExt;
        return device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage,
        });
    }
    let size = (contents.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: true,
    });
    let mut padded = contents.to_vec();
    padded.resize(size as usize, 0);
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(&padded);
    buffer.unmap();
    buffer
}

/// macOS with unified memory: a sampled texture in shared memory, written by
/// the CPU in place (`write_texels`). No upload pass, so no command buffer of
/// its own: the driver keeps memory for as many command buffers as were ever
/// alive at once, for good. Made through the HAL, wgpu takes it as
/// initialized and never clears it first either.
#[cfg(target_os = "macos")]
pub(crate) fn shared_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    use objc2_metal::{
        MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
        MTLTextureUsage,
    };
    // SAFETY: The raw device only makes a texture; nothing destroys it, and
    // the guard is dropped before wgpu is called again.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.expect("a Metal device");
    // SAFETY: A plain 2D texture descriptor of this size.
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    let metal = hal
        .raw_device()
        .newTextureWithDescriptor(&descriptor)
        .expect("a shared texture");
    drop(hal);
    // SAFETY: Metal made this texture on this device, in `FORMAT`, this size,
    // one mip level and layer, readable by shaders as the descriptor says;
    // wgpu takes ownership of it. It takes it as initialized too: texels not
    // yet written are only ever read as pixels.
    unsafe {
        let raw = wgpu::hal::metal::Device::texture_from_raw(
            metal,
            crate::FORMAT,
            MTLTextureType::Type2D,
            1,
            1,
            wgpu::hal::CopyExtent {
                width,
                height,
                depth: 1,
            },
        );
        device.create_texture_from_hal::<wgpu::hal::api::Metal>(
            raw,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        )
    }
}

/// Write premultiplied RGBA texels (reordered in place to the texture's
/// BGRA) to `(x, y, width, height)` of a `shared_texture`. No submission may
/// still read that region: the caller writes only cells no submitted frame
/// samples.
#[cfg(target_os = "macos")]
pub(crate) fn write_texels(
    texture: &wgpu::Texture,
    (x, y, width, height): (u32, u32, u32, u32),
    rgba: &mut [u8],
) {
    use objc2_metal::{MTLOrigin, MTLRegion, MTLSize, MTLTexture};
    // Metal copies on the CPU, unchecked: these keep it in both buffers.
    assert_eq!(rgba.len() as u64, u64::from(width) * u64::from(height) * 4);
    assert!(
        u64::from(x) + u64::from(width) <= u64::from(texture.width())
            && u64::from(y) + u64::from(height) <= u64::from(texture.height()),
        "texels within the texture"
    );
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    // SAFETY: The raw texture is only written through; nothing destroys it.
    let hal = unsafe { texture.as_hal::<wgpu::hal::api::Metal>() }.expect("a Metal texture");
    let region = MTLRegion {
        origin: MTLOrigin {
            x: x as usize,
            y: y as usize,
            z: 0,
        },
        size: MTLSize {
            width: width as usize,
            height: height as usize,
            depth: 1,
        },
    };
    // SAFETY: A region within a shared texture's bounds (asserted above), of
    // a BGRA8 texture, from `rgba` holding `height` rows of `width * 4`
    // bytes (asserted); the caller writes only cells no submitted frame
    // samples.
    unsafe {
        hal.raw_handle()
            .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                region,
                0,
                std::ptr::NonNull::from(&mut *rgba).cast(),
                (width * 4) as usize,
            )
    };
}

/// Texture writes queued while building a frame: glyph cells, images and
/// rasterized canvases. A render pass per target texture draws them from a
/// CPU-mapped buffer, so uploads never open a transfer encoder.
/// Staging capacity kept between frames: steady glyph uploads fit it.
const STAGING_KEEP: usize = 256 * 1024;
#[derive(Default)]
pub(crate) struct TextureUploads {
    /// Per target: its texture and `[x, y, width, height]`, `[offset, 0, 0, 0]` pairs.
    targets: Vec<(wgpu::Texture, Vec<[u32; 4]>)>,
    pixels: Vec<u8>,
    pipeline: Option<wgpu::RenderPipeline>,
}
impl TextureUploads {
    /// Queue premultiplied RGBA8 texels for `(x, y, width, height)` of `target`.
    /// The queue keeps `target` alive until it is written.
    pub(crate) fn push(
        &mut self,
        target: &wgpu::Texture,
        (x, y, width, height): (u32, u32, u32, u32),
        rgba: &[u8],
    ) {
        debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
        let offset = (self.pixels.len() / 4) as u32;
        self.pixels.extend_from_slice(rgba);
        let rects = match self
            .targets
            .iter_mut()
            .find(|(texture, _)| texture == target)
        {
            Some((_, rects)) => rects,
            None => {
                self.targets.push((target.clone(), Vec::new()));
                &mut self.targets.last_mut().expect("just pushed").1
            }
        };
        rects.push([x, y, width, height]);
        rects.push([offset, 0, 0, 0]);
    }
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
    /// Record every queued write; returns the number of regions written.
    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> usize {
        if self.targets.is_empty() {
            return 0;
        }
        let format = self.targets[0].0.format();
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("texture upload"),
                source: wgpu::ShaderSource::Wgsl(include_str!("texture_upload.wgsl").into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("texture upload"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: crate::QUAD_STRIP,
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let storage = wgpu::BufferUsages::STORAGE;
        let pixels = init_buffer(device, true, "texture upload pixels", &self.pixels, storage);
        let mut written = 0;
        for (texture, rects) in self.targets.drain(..) {
            debug_assert_eq!(texture.format(), format, "one upload pipeline format");
            let count = rects.len() / 2;
            written += count;
            // The shader maps texel rects to clip space with one size, so the
            // header carries both dimensions.
            let mut header = Vec::with_capacity(rects.len() + 1);
            header.push([texture.width(), texture.height(), 0, 0]);
            header.extend_from_slice(&rects);
            let rects = init_buffer(
                device,
                true,
                "texture upload rects",
                bytemuck::cast_slice(&header),
                storage,
            );
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("texture upload"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: rects.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: pixels.as_entire_binding(),
                    },
                ],
            });
            let view = texture.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("texture upload"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..4, 0..count as u32);
        }
        // A burst (a first frame's icons and glyphs) need not stay allocated.
        if self.pixels.capacity() > STAGING_KEEP {
            self.pixels = Vec::new();
        } else {
            self.pixels.clear();
        }
        written
    }
}
