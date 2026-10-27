//! Windows: the shaders FXC compiles are kept on disk and read back by the
//! next device that needs them (`vendor/wgpu-hal/ZGX_PATCH.md`), so a
//! launch after the first compiles none. With the repository's wgpu-hal
//! (`zgx_wgpu_hal_patched`, see build.rs).
#![cfg(all(windows, zgx_wgpu_hal_patched))]
use std::path::{Path, PathBuf};

fn device() -> Option<wgpu::Device> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    // FXC, as on a machine without the app's own DXC.
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let (device, _queue) = pollster::block_on(adapter.request_device(&Default::default())).ok()?;
    Some(device)
}

/// Make a pipeline from `source` on a new device (whose own cache is
/// empty): the error it gave, if any.
fn pipeline(source: &str) -> Option<String> {
    let device = device()?;
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    // D3D12 refusing a pipeline's bytecode is an internal error.
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let _pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Bgra8Unorm.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let internal = pollster::block_on(internal.pop());
    let validation = pollster::block_on(validation.pop());
    internal.or(validation).map(|error| error.to_string())
}

fn kept(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| Some(e.ok()?.path())).collect())
        .unwrap_or_default();
    files.retain(|path| path.extension().is_some_and(|ext| ext == "fxc"));
    files.sort();
    files
}

#[test]
fn compiled_shaders_are_kept_and_read_back_by_the_next_device() {
    let dir = std::env::temp_dir().join(format!("zgx-shader-cache-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    wgpu::hal::dx12::set_shader_cache_dir(Some(dir.clone()));
    if device().is_none() {
        return;
    }
    // wgpu's own shaders, compiled for every device.
    let own = kept(&dir);
    // A source no earlier run compiled.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        % 1_000_000;
    let source = format!(
        "@vertex fn vs() -> @builtin(position) vec4<f32> {{ return vec4<f32>(0.{nonce}, 0.0, 0.0, 1.0); }}
         @fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(1.0); }}"
    );
    let before = zgx_gpu::shader_cache_counts().expect("counted on Windows");
    assert_eq!(pipeline(&source), None);
    let after = zgx_gpu::shader_cache_counts().unwrap();
    assert!(
        after.compiled >= before.compiled + 2,
        "the new shaders counted as compiled: {before:?} then {after:?}"
    );
    assert_eq!(after.unwritten, 0, "every compiled shader kept");
    let mut files = kept(&dir);
    files.retain(|file| !own.contains(file));
    assert_eq!(
        files.len(),
        2,
        "the vertex and fragment shaders kept: {files:?}"
    );
    // The next device reads them back: with their bytecode spoilt, its
    // pipeline fails, where one it compiled itself would not.
    let originals: Vec<_> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();
    for (file, bytes) in files.iter().zip(&originals) {
        let code = bytes
            .windows(4)
            .position(|w| w == b"DXBC")
            .expect("the bytecode");
        let mut spoilt = bytes[..code + 4].to_vec();
        spoilt.resize(bytes.len(), 0xAB);
        std::fs::write(file, spoilt).unwrap();
    }
    assert!(
        pipeline(&source).is_some(),
        "the next device compiled its own shaders instead of reading them"
    );
    for (file, bytes) in files.iter().zip(&originals) {
        std::fs::write(file, bytes).unwrap();
    }
    let before = zgx_gpu::shader_cache_counts().unwrap();
    assert_eq!(pipeline(&source), None, "read back intact, they work");
    let after = zgx_gpu::shader_cache_counts().unwrap();
    assert_eq!(kept(&dir).len(), own.len() + 2, "nothing compiled again");
    assert_eq!(
        after.compiled, before.compiled,
        "counted as nothing compiled"
    );
    assert!(
        after.read >= before.read + 2,
        "the shaders counted as read back: {before:?} then {after:?}"
    );
    wgpu::hal::dx12::set_shader_cache_dir(None);
    let _ = std::fs::remove_dir_all(&dir);
}
