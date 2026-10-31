//! Every shader module the renderer compiles (`zgx_gpu::shader_modules`)
//! makes a pipeline of each of its entry points, on a new device whose
//! cache is empty: on Windows through FXC, as an app compiles them the first
//! time it draws each. A constant-count search there once unrolled until FXC
//! overflowed its stack, and any canvas stroke crashed its app on first draw
//! with nothing cached; a shader only a rare frame draws shows that here, not
//! in an app. A vertex stage reads one instance buffer of its inputs as
//! declared, a fragment stage follows the vertex stage producing what it
//! takes, and writes as many targets as it declares.
use wgpu::naga;

fn device() -> Option<wgpu::Device> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    #[cfg(windows)]
    {
        descriptor.backends = wgpu::Backends::DX12;
        // FXC, as on a machine without the app's own DXC, optimizing as a
        // release build does: a debug build's device skips FXC's
        // optimizations, where the overflow was.
        descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
        descriptor.flags = wgpu::InstanceFlags::empty();
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let features = adapter.features() & wgpu::Features::DUAL_SOURCE_BLENDING;
    let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        ..Default::default()
    }))
    .ok()?;
    Some(device)
}

/// The vertex format of an input of type `ty`.
fn format(module: &naga::Module, ty: naga::Handle<naga::Type>) -> wgpu::VertexFormat {
    use naga::{ScalarKind as K, TypeInner as T, VectorSize as N};
    match module.types[ty].inner {
        T::Scalar(s) => match s.kind {
            K::Float => wgpu::VertexFormat::Float32,
            K::Uint => wgpu::VertexFormat::Uint32,
            _ => wgpu::VertexFormat::Sint32,
        },
        T::Vector { size, scalar } => match (size, scalar.kind) {
            (N::Bi, K::Float) => wgpu::VertexFormat::Float32x2,
            (N::Tri, K::Float) => wgpu::VertexFormat::Float32x3,
            (N::Quad, K::Float) => wgpu::VertexFormat::Float32x4,
            (N::Bi, K::Uint) => wgpu::VertexFormat::Uint32x2,
            (N::Tri, K::Uint) => wgpu::VertexFormat::Uint32x3,
            (N::Quad, K::Uint) => wgpu::VertexFormat::Uint32x4,
            (N::Bi, _) => wgpu::VertexFormat::Sint32x2,
            (N::Tri, _) => wgpu::VertexFormat::Sint32x3,
            (N::Quad, _) => wgpu::VertexFormat::Sint32x4,
        },
        ref other => panic!("no vertex format for {other:?}"),
    }
}

/// The locations an entry point's arguments (or result) bind, with their
/// types, struct members included.
fn locations(
    module: &naga::Module,
    bound: Vec<(Option<naga::Binding>, naga::Handle<naga::Type>)>,
) -> Vec<(u32, naga::Handle<naga::Type>, bool)> {
    let mut found = Vec::new();
    for (binding, ty) in bound {
        match binding {
            Some(naga::Binding::Location {
                location,
                blend_src,
                ..
            }) => found.push((location, ty, blend_src.is_some())),
            Some(naga::Binding::BuiltIn(_)) => {}
            None => {
                if let naga::TypeInner::Struct { members, .. } = &module.types[ty].inner {
                    found.extend(locations(
                        module,
                        members.iter().map(|m| (m.binding.clone(), m.ty)).collect(),
                    ));
                }
            }
        }
    }
    found
}

#[test]
fn every_shader_module_compiles_each_entry_point() {
    let Some(device) = device() else {
        eprintln!("every_shader: no adapter; nothing checked");
        return;
    };
    let dual = device
        .features()
        .contains(wgpu::Features::DUAL_SOURCE_BLENDING);
    let mut failures = Vec::new();
    let mut made = 0;
    for (name, source) in zgx_gpu::shader_modules() {
        if source.starts_with("enable dual_source_blending") && !dual {
            continue;
        }
        let parsed = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(source.as_str().into()),
        });
        let vertices: Vec<_> = parsed
            .entry_points
            .iter()
            .filter(|e| e.stage == naga::ShaderStage::Vertex)
            .collect();
        for entry in &parsed.entry_points {
            let label = format!("{name} `{}`", entry.name);
            // Named first: a compiler that overflows its stack ends the
            // process with no message of its own.
            eprintln!("every_shader: {label}");
            match entry.stage {
                naga::ShaderStage::Compute => {
                    let _ = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(&label),
                        layout: None,
                        module: &module,
                        entry_point: Some(&entry.name),
                        compilation_options: Default::default(),
                        cache: None,
                    });
                }
                naga::ShaderStage::Fragment => {
                    // The vertex stage making what it takes.
                    let takes = entry.function.arguments.first().map(|a| a.ty);
                    let vertex = vertices
                        .iter()
                        .find(|v| v.function.result.as_ref().map(|r| r.ty) == takes)
                        .or(vertices.first())
                        .unwrap_or_else(|| panic!("{label}: no vertex stage"));
                    let inputs = locations(
                        &parsed,
                        vertex
                            .function
                            .arguments
                            .iter()
                            .map(|a| (a.binding.clone(), a.ty))
                            .collect(),
                    );
                    let mut offset = 0;
                    let attributes: Vec<wgpu::VertexAttribute> = inputs
                        .iter()
                        .map(|&(location, ty, _)| {
                            let format = format(&parsed, ty);
                            let attribute = wgpu::VertexAttribute {
                                format,
                                offset,
                                shader_location: location,
                            };
                            offset += format.size();
                            attribute
                        })
                        .collect();
                    let buffers = [wgpu::VertexBufferLayout {
                        array_stride: offset.max(4),
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    }];
                    let outputs = entry.function.result.as_ref().map_or(Vec::new(), |r| {
                        locations(&parsed, vec![(r.binding.clone(), r.ty)])
                    });
                    let dual_source = outputs.iter().any(|&(_, _, blend_src)| blend_src);
                    let count = outputs
                        .iter()
                        .map(|&(location, ..)| location + 1)
                        .max()
                        .unwrap_or(0);
                    let blend = dual_source.then_some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Src1,
                            dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    });
                    let targets: Vec<_> = (0..count)
                        .map(|_| {
                            Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::Bgra8Unorm,
                                blend,
                                write_mask: wgpu::ColorWrites::ALL,
                            })
                        })
                        .collect();
                    let _ = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(&label),
                        layout: None,
                        vertex: wgpu::VertexState {
                            module: &module,
                            entry_point: Some(&vertex.name),
                            compilation_options: Default::default(),
                            buffers: if attributes.is_empty() { &[] } else { &buffers },
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &module,
                            entry_point: Some(&entry.name),
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
                // Each made with the fragment stages it feeds.
                _ => continue,
            }
            made += 1;
        }
        let internal = pollster::block_on(internal.pop());
        let validation = pollster::block_on(validation.pop());
        if let Some(error) = internal.or(validation) {
            failures.push(format!("{name}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    println!("every_shader: {made} pipelines made");
}
