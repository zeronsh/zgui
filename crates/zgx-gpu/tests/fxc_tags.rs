//! Scratch (not for merge): each released version's polyline module
//! (`fxc_tags/<tag>/`, its draw.wgsl and polyline.wgsl as tagged) made into
//! the strokes' pipeline through FXC optimizing as a release build does,
//! each in a process of its own: which published versions overflow FXC.
//! `WGPU_DX12_COMPILER` chooses another compiler (`dxc`: a dxcompiler.dll
//! beside the test).
const TAGS: [(&str, &str, &str); 7] = [
    (
        "v0.1.0",
        include_str!("fxc_tags/v0.1.0/draw.wgsl"),
        include_str!("fxc_tags/v0.1.0/polyline.wgsl"),
    ),
    (
        "v0.1.1",
        include_str!("fxc_tags/v0.1.1/draw.wgsl"),
        include_str!("fxc_tags/v0.1.1/polyline.wgsl"),
    ),
    (
        "v0.1.2",
        include_str!("fxc_tags/v0.1.2/draw.wgsl"),
        include_str!("fxc_tags/v0.1.2/polyline.wgsl"),
    ),
    (
        "v0.1.3",
        include_str!("fxc_tags/v0.1.3/draw.wgsl"),
        include_str!("fxc_tags/v0.1.3/polyline.wgsl"),
    ),
    (
        "v0.1.4",
        include_str!("fxc_tags/v0.1.4/draw.wgsl"),
        include_str!("fxc_tags/v0.1.4/polyline.wgsl"),
    ),
    (
        "v0.1.5",
        include_str!("fxc_tags/v0.1.5/draw.wgsl"),
        include_str!("fxc_tags/v0.1.5/polyline.wgsl"),
    ),
    (
        "v0.1.6",
        include_str!("fxc_tags/v0.1.6/draw.wgsl"),
        include_str!("fxc_tags/v0.1.6/polyline.wgsl"),
    ),
];

fn source(tag: &str) -> String {
    if tag == "this branch" {
        return zgx_gpu::shader_modules()
            .into_iter()
            .find(|(n, _)| *n == "polyline")
            .unwrap()
            .1;
    }
    let (_, draw, polyline) = TAGS.iter().find(|(t, ..)| *t == tag).unwrap();
    format!("{draw}\n{polyline}")
}

fn compile(source: &str, entries: &[&str]) -> Result<(), String> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc.with_env();
    descriptor.flags = wgpu::InstanceFlags::empty();
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("no adapter: {e}"))?;
    let (device, _q) = pollster::block_on(adapter.request_device(&Default::default()))
        .map_err(|e| e.to_string())?;
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    for entry in entries {
        pipeline(&device, &module, entry);
    }
    let a = pollster::block_on(internal.pop());
    let b = pollster::block_on(validation.pop());
    a.or(b).map_or(Ok(()), |e| Err(e.to_string()))
}

fn pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule, entry: &str) {
    let basic = entry.starts_with("fs_basic");
    let target = Some(wgpu::TextureFormat::Bgra8Unorm.into());
    let targets = if entry.ends_with("_both") {
        vec![target.clone(), target]
    } else {
        vec![target]
    };
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    let _ = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(if basic { "vs_basic" } else { "vs" }),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 128,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &all,
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &targets,
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
}

fn main() {
    if let Ok(tag) = std::env::var("FXC_TAG") {
        // As every_shader makes them, or the strokes' alone; on a thread of
        // test threads' 8 MiB, or the main thread's own.
        let entries: &[&str] = match std::env::var("FXC_ORDER").as_deref() {
            Ok("every_shader") => &["fs_basic", "fs_basic_both", "fs_polyline"],
            _ => &["fs_polyline"],
        };
        let source = source(&tag);
        let result = match std::env::var("FXC_THREAD").as_deref() {
            Ok("main") => compile(&source, entries),
            _ => std::thread::Builder::new()
                .stack_size(8 << 20)
                .spawn(move || compile(&source, entries))
                .unwrap()
                .join()
                .unwrap(),
        };
        match result {
            Ok(()) => println!("COMPILED"),
            Err(e) => println!("ERROR {e}"),
        }
        return;
    }
    let compiler = std::env::var("WGPU_DX12_COMPILER").unwrap_or_else(|_| "fxc".into());
    for (order, thread) in [
        ("alone", "main"),
        ("alone", "8 MiB"),
        ("every_shader", "8 MiB"),
    ] {
        for tag in TAGS.iter().map(|(t, ..)| *t).chain(["this branch"]) {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .env("FXC_TAG", tag)
                .env("FXC_ORDER", order)
                .env("FXC_THREAD", thread)
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            println!(
                "fxc_tags {compiler} {order} {thread}: {tag:12} exit {:?} {}",
                out.status.code().map(|c| format!("{:#x}", c as u32)),
                text.lines()
                    .last()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect::<String>()
            );
        }
    }
}
