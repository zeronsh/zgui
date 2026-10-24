//! The vendored Metal backend fills and copies buffers with vertex-only render
//! passes instead of blit encoders (vendor/wgpu-hal ZGX_PATCH.md). These must
//! write exactly the requested words and nothing around them.
fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, buffer: &wgpu::Buffer) -> Vec<u32> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, buffer.size());
    queue.submit([encoder.finish()]);
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    bytemuck::cast_slice(&staging.slice(..).get_mapped_range()).to_vec()
}

fn filled(device: &wgpu::Device, words: &[u32]) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (words.len() * 4) as u64,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(words));
    buffer.unmap();
    buffer
}

#[test]
fn buffer_clears_and_copies_touch_exactly_their_ranges() {
    let (device, queue) = device();
    let pattern: Vec<u32> = (1..=1024).collect();
    let target = filled(&device, &pattern);
    let source = filled(
        &device,
        &pattern.iter().map(|w| w * 1000).collect::<Vec<_>>(),
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.clear_buffer(&target, 40, Some(400));
    // Copies after and before the clear in one encoder keep their order.
    encoder.copy_buffer_to_buffer(&source, 8, &target, 2000, 64);
    encoder.copy_buffer_to_buffer(&source, 3000, &target, 100, 12);
    queue.submit([encoder.finish()]);
    let words = read(&device, &queue, &target);
    let mut expected = pattern.clone();
    for word in &mut expected[10..110] {
        *word = 0;
    }
    for i in 0..16 {
        expected[500 + i] = (3 + i as u32) * 1000;
    }
    for i in 0..3 {
        expected[25 + i] = (751 + i as u32) * 1000;
    }
    assert_eq!(words, expected);
}

#[test]
fn large_clears_zero_every_word() {
    let (device, queue) = device();
    let pattern = vec![0xdead_beef_u32; 1 << 20];
    let target = filled(&device, &pattern);
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.clear_buffer(&target, 0, None);
    queue.submit([encoder.finish()]);
    assert!(read(&device, &queue, &target).iter().all(|&w| w == 0));
}
