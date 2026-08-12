use std::{fs, path::Path};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const WIDTH: u32 = 1440;
const HEIGHT: u32 = 900;
const VIEW_HEIGHT_M: f32 = 480.0;
const MATCH_SEED: u64 = 0x5eed;
const CLOUD_SEED: u64 = 0x7079_626f_6174;
const SHADER: &str = include_str!("../../../crates/browser/src/scene.wgsl");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
    is_terrain: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    camera_and_view_m: [f32; 4],
    weather: [f32; 4],
    weather_structure: [f32; 4],
    cloud_motion: [f32; 4],
    match_seed: [u32; 2],
    cloud_visual_seed: [u32; 2],
    cloud_offsets_m: [f32; 4],
    time_seconds: f32,
    padding: [u32; 3],
}

struct Showcase {
    name: &'static str,
    weather: [f32; 4],
    structure: [f32; 4],
}

const SHOWCASES: [Showcase; 5] = [
    Showcase {
        name: "fair-cumulus",
        weather: [6.0, 1.0, 0.48, 0.0],
        structure: [0.48, 0.82, 0.04, 0.0],
    },
    Showcase {
        name: "marine-stratus",
        weather: [7.0, 2.0, 0.72, 0.0],
        structure: [0.90, 0.12, 0.05, 0.0],
    },
    Showcase {
        name: "high-cirrus",
        weather: [8.0, 3.0, 0.0, 0.0],
        structure: [0.28, 0.35, 0.96, 0.0],
    },
    Showcase {
        name: "rain-squall",
        weather: [10.0, 3.0, 0.64, 0.62],
        structure: [0.92, 0.62, 0.18, 0.0],
    },
    Showcase {
        name: "convective-storm",
        weather: [12.0, 4.0, 0.76, 0.90],
        structure: [0.98, 0.96, 0.28, 0.0],
    },
];

fn main() {
    pollster::block_on(render_showcases());
}

async fn render_showcases() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: true,
            ..Default::default()
        })
        .await
        .expect("no Vulkan adapter available");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("weather showcase device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("failed to create device");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Pyboat scene shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene uniforms layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("weather showcase pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("weather showcase pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3, 2 => Float32],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let vertices = fullscreen_vertices();
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fullscreen terrain vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let output_dir = Path::new("weather-showcase");
    fs::create_dir_all(output_dir).expect("failed to create output directory");

    for showcase in SHOWCASES {
        render_showcase(
            &device,
            &queue,
            &layout,
            &pipeline,
            &vertex_buffer,
            &showcase,
            output_dir,
        );
    }
}

fn render_showcase(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    pipeline: &wgpu::RenderPipeline,
    vertex_buffer: &wgpu::Buffer,
    showcase: &Showcase,
    output_dir: &Path,
) {
    let view_width_m = VIEW_HEIGHT_M * WIDTH as f32 / HEIGHT as f32;
    let uniforms = SceneUniforms {
        camera_and_view_m: [0.0, 0.0, view_width_m, VIEW_HEIGHT_M],
        weather: showcase.weather,
        weather_structure: showcase.structure,
        cloud_motion: [
            1.08 * showcase.weather[0],
            1.08 * showcase.weather[1],
            1.18 * showcase.weather[0],
            1.18 * showcase.weather[1],
        ],
        match_seed: [MATCH_SEED as u32, (MATCH_SEED >> 32) as u32],
        cloud_visual_seed: [CLOUD_SEED as u32, (CLOUD_SEED >> 32) as u32],
        cloud_offsets_m: [0.0; 4],
        time_seconds: 37.0,
        padding: [0; 3],
    };
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene uniforms bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(showcase.name),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let bytes_per_row = WIDTH * 4;
    let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weather showcase readback"),
        size: u64::from(padded_bytes_per_row * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("weather showcase encoder"),
    });
    {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("weather showcase pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap()
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().expect("failed to map GPU output");
    let data = slice.get_mapped_range().expect("failed to read GPU output");
    let path = output_dir.join(format!("{}.png", showcase.name));
    let rgba: Vec<_> = data
        .chunks_exact(padded_bytes_per_row as usize)
        .flat_map(|row| &row[..bytes_per_row as usize])
        .copied()
        .collect();
    write_png(&path, &rgba);
    println!("wrote {}", path.display());
}

fn fullscreen_vertices() -> [Vertex; 6] {
    [
        [-1.0, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
    ]
    .map(|position| Vertex {
        position,
        color: [0.0; 3],
        is_terrain: 1.0,
    })
}

fn write_png(path: &Path, rgba: &[u8]) {
    let file = fs::File::create(path).expect("failed to create PNG");
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .expect("failed to write PNG");
}
