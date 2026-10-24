//! zgui patch: buffer fills and copies without a blit encoder.
//!
//! On Apple silicon the first blit encoder a process uses leaves the driver
//! holding roughly 110 MB for about eight seconds, and part of it for good
//! (measured with `footprint`: 297 MB graphics memory at launch and 65 MB at
//! rest for a small window, against 185 MB and 41 MB without blits). wgpu-core
//! issues two tiny buffer blits while creating every device. Here those
//! operations run as vertex-only render passes instead: rasterization is off
//! and each vertex writes one 32-bit word of the destination.
//!
//! Each pass renders to a 1x1 texture it neither loads nor stores. Two passes
//! without attachments (or with memoryless ones) in one command buffer make
//! the driver keep a second set of six 512 KB regions for good: 3 MB, against
//! none for passes with a real attachment.

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLColorWriteMask, MTLCommandBuffer, MTLCommandEncoder, MTLCompileOptions,
    MTLDevice, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage,
};

const SOURCE: &str = "
#include <metal_stdlib>
using namespace metal;
vertex void zgui_fill(device uint* dst [[buffer(0)]], uint i [[vertex_id]]) { dst[i] = 0u; }
vertex void zgui_copy(device uint* dst [[buffer(0)]], const device uint* src [[buffer(1)]],
                      uint i [[vertex_id]]) { dst[i] = src[i]; }
";

pub(super) struct BufferWrites {
    fill: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    copy: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    target: Retained<ProtocolObject<dyn MTLTexture>>,
}

impl BufferWrites {
    /// `None` if the pipelines cannot be built; callers then use blits.
    pub(super) fn new(device: &ProtocolObject<dyn MTLDevice>) -> Option<Self> {
        let library = device
            .newLibraryWithSource_options_error(
                &NSString::from_str(SOURCE),
                Some(&MTLCompileOptions::new()),
            )
            .ok()?;
        let pipeline = |name: &str| {
            let function = library.newFunctionWithName(&NSString::from_str(name))?;
            let descriptor = MTLRenderPipelineDescriptor::new();
            descriptor.setVertexFunction(Some(&function));
            descriptor.setRasterizationEnabled(false);
            // SAFETY: attachment 0 is below Metal's eight colour attachments.
            let color = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
            color.setPixelFormat(MTLPixelFormat::R8Unorm);
            color.setWriteMask(MTLColorWriteMask::empty());
            device
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
                .ok()
        };
        // SAFETY: a 1x1 descriptor in a renderable format, no mipmaps.
        let target = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::R8Unorm,
                1,
                1,
                false,
            )
        };
        target.setUsage(MTLTextureUsage::RenderTarget);
        target.setStorageMode(MTLStorageMode::Private);
        Some(Self {
            fill: pipeline("zgui_fill")?,
            copy: pipeline("zgui_copy")?,
            target: device.newTextureWithDescriptor(&target)?,
        })
    }

    /// Zero `range` of `dst`, or copy `size` bytes from `src` into it, as a
    /// render pass on `cmd_buf`. Offsets and sizes must be multiples of four.
    pub(super) fn encode(
        &self,
        cmd_buf: &ProtocolObject<dyn MTLCommandBuffer>,
        dst: (&ProtocolObject<dyn MTLBuffer>, u64),
        src: Option<(&ProtocolObject<dyn MTLBuffer>, u64)>,
        size: u64,
    ) {
        let descriptor = MTLRenderPassDescriptor::new();
        // SAFETY: as in `new`.
        let color = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
        color.setTexture(Some(&self.target));
        color.setLoadAction(MTLLoadAction::DontCare);
        color.setStoreAction(MTLStoreAction::DontCare);
        let encoder = cmd_buf
            .renderCommandEncoderWithDescriptor(&descriptor)
            .unwrap();
        encoder.setLabel(Some(&NSString::from_str("zgui buffer write")));
        // SAFETY: vertex `i` writes word `i` of `dst` from its offset (and
        // reads word `i` of `src`), for `i < size / 4`: inside both buffers,
        // as wgpu-core checks `clear_buffer` and `copy_buffer_to_buffer`
        // ranges against their sizes before the HAL sees them, and 4-byte
        // aligned (`write_buffer_words` checks). The buffers are distinct
        // for a copy (the caller checks), and Metal tracks hazards between
        // this pass and the work around it.
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(dst.0), dst.1 as usize, 0);
            if let Some((src, offset)) = src {
                encoder.setRenderPipelineState(&self.copy);
                encoder.setVertexBuffer_offset_atIndex(Some(src), offset as usize, 1);
            } else {
                encoder.setRenderPipelineState(&self.fill);
            }
            encoder.drawPrimitives_vertexStart_vertexCount(
                MTLPrimitiveType::Point,
                0,
                (size / 4) as usize,
            );
        }
        encoder.endEncoding();
    }
}
