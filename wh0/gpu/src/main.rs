//! WH0 probe P3a: list every wgpu adapter (all backends), and on each try the smallest drawing: clear a 64x64 texture to
//! (255,128,0), copy it back, compare the first pixel. Prints one `adapter ...` line per adapter and a `draw ...` verdict.
//! Also tries the two ways to ask for the software adapter (force_fallback_adapter, and the backend filter DX12).

fn main() {
    for (label, backends) in [("all", wgpu::Backends::all()), ("dx12", wgpu::Backends::DX12), ("vulkan", wgpu::Backends::VULKAN), ("gl", wgpu::Backends::GL)] {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends, ..Default::default() });
        let list = instance.enumerate_adapters(backends);
        println!("backends={label} adapters={}", list.len());
        for a in &list {
            let i = a.get_info();
            println!("adapter backends={label} name={:?} backend={:?} type={:?} driver={:?}", i.name, i.backend, i.device_type, i.driver);
            println!("draw backends={label} name={:?} -> {}", i.name, draw(a));
        }
        for fallback in [false, true] {
            let r = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: fallback,
                compatible_surface: None,
            }));
            match r {
                Ok(a) => println!("request backends={label} force_fallback={fallback} -> {:?} ({:?}) draw={}", a.get_info().name, a.get_info().device_type, draw(&a)),
                Err(e) => println!("request backends={label} force_fallback={fallback} -> error {e}"),
            }
        }
    }
}

fn draw(a: &wgpu::Adapter) -> String {
    let err = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let (device, queue) = match pollster::block_on(a.request_device(&wgpu::DeviceDescriptor::default())) {
        Ok(x) => x,
        Err(e) => return format!("request_device failed: {e}"),
    };
    let sink = err.clone();
    device.on_uncaptured_error(Box::new(move |e| sink.lock().unwrap().push(e.to_string())));
    let size = wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 4 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    let view = tex.create_view(&Default::default());
    drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.5, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    }));
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(64) } },
        size,
    );
    queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::Wait);
    let data = slice.get_mapped_range();
    let px = [data[0], data[1], data[2], data[3]];
    let errors = err.lock().unwrap().join(" | ");
    format!("pixel={px:?} errors=[{errors}] {}", if px == [255, 128, 0, 255] || px == [255, 127, 0, 255] { "DRAWS" } else { "NO-DRAW" })
}
