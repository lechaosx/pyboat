use std::{borrow::Cow, mem};

use bytemuck::{Pod, Zeroable};
use rapier2d::prelude::Vector;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::CurrentSurfaceTexture;

use crate::{BoatScene, Demo, VIEW_HEIGHT_M};

const MAX_VERTICES: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

struct Renderer {
    canvas: HtmlCanvasElement,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
}

impl Renderer {
    async fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let (width, height) = canvas_dimensions(&canvas);
        canvas.set_width(width);
        canvas.set_height(height);

        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(instance_descriptor);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(js_error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(js_error)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Pyboat WebGPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(js_error)?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| JsValue::from_str("WebGPU surface has no supported configuration"))?;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Pyboat scene shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("scene.wgsl"))),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pyboat scene pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pyboat scene pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(Vertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pyboat dynamic vertices"),
            size: (MAX_VERTICES * mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            canvas,
            device,
            queue,
            surface,
            config,
            pipeline,
            vertex_buffer,
        })
    }

    fn render(&mut self, scene: BoatScene, camera_m: Vector) -> Result<(), JsValue> {
        self.resize();
        let vertices = scene_vertices(scene, camera_m, self.config.width, self.config.height);
        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
            CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            CurrentSurfaceTexture::Validation => {
                return Err(JsValue::from_str(
                    "WebGPU rejected the current surface texture",
                ));
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Pyboat frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Pyboat scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.11,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(())
    }

    fn resize(&mut self) {
        let (width, height) = canvas_dimensions(&self.canvas);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

#[wasm_bindgen]
pub struct BrowserGame {
    demo: Demo,
    renderer: Renderer,
}

#[wasm_bindgen]
impl BrowserGame {
    pub async fn create(canvas: HtmlCanvasElement) -> Result<BrowserGame, JsValue> {
        console_error_panic_hook::set_once();
        Ok(Self {
            demo: Demo::new(),
            renderer: Renderer::new(canvas).await?,
        })
    }

    pub fn frame(&mut self, elapsed_seconds: f32) -> Result<(), JsValue> {
        self.demo.advance(elapsed_seconds);
        self.renderer
            .render(self.demo.scene(), self.demo.motion().position_m)
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        gamepad_x: f32,
        gamepad_y: f32,
    ) {
        self.demo
            .set_player_input(keyboard_forward, keyboard_turn, gamepad_x, gamepad_y);
    }
}

fn canvas_dimensions(canvas: &HtmlCanvasElement) -> (u32, u32) {
    let scale = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0);
    let width = (f64::from(canvas.client_width()) * scale).round() as u32;
    let height = (f64::from(canvas.client_height()) * scale).round() as u32;
    (width.max(1), height.max(1))
}

fn scene_vertices(scene: BoatScene, camera_m: Vector, width: u32, height: u32) -> Vec<Vertex> {
    let aspect = width as f32 / height as f32;
    let view_width_m = VIEW_HEIGHT_M * aspect;
    let half_width_m = view_width_m / 2.0;
    let half_height_m = VIEW_HEIGHT_M / 2.0;
    let mut vertices = Vec::new();

    let first_x = (camera_m.x - half_width_m).floor() as i32;
    let last_x = (camera_m.x + half_width_m).ceil() as i32;
    for x in first_x..=last_x {
        push_quad(
            &mut vertices,
            Vector::new(x as f32, camera_m.y - half_height_m),
            Vector::new(x as f32, camera_m.y + half_height_m),
            0.012,
            [0.075, 0.18, 0.22],
            camera_m,
            view_width_m,
        );
    }
    let first_y = (camera_m.y - half_height_m).floor() as i32;
    let last_y = (camera_m.y + half_height_m).ceil() as i32;
    for y in first_y..=last_y {
        push_quad(
            &mut vertices,
            Vector::new(camera_m.x - half_width_m, y as f32),
            Vector::new(camera_m.x + half_width_m, y as f32),
            0.012,
            [0.075, 0.18, 0.22],
            camera_m,
            view_width_m,
        );
    }

    for [first, second, third] in [[0, 1, 2], [0, 2, 3], [0, 3, 4]] {
        for point in [
            scene.hull_m[first],
            scene.hull_m[second],
            scene.hull_m[third],
        ] {
            vertices.push(vertex(point, [0.42, 0.82, 0.69], camera_m, view_width_m));
        }
    }
    push_quad(
        &mut vertices,
        scene.sail_m[0],
        scene.sail_m[1],
        0.07,
        [0.96, 0.76, 0.25],
        camera_m,
        view_width_m,
    );
    push_quad(
        &mut vertices,
        scene.rudder_m[0],
        scene.rudder_m[1],
        0.08,
        [0.86, 0.91, 0.91],
        camera_m,
        view_width_m,
    );
    assert!(vertices.len() <= MAX_VERTICES);
    vertices
}

fn push_quad(
    vertices: &mut Vec<Vertex>,
    start_m: Vector,
    end_m: Vector,
    half_width_m: f32,
    color: [f32; 3],
    camera_m: Vector,
    view_width_m: f32,
) {
    let direction = end_m - start_m;
    let normal = Vector::new(-direction.y, direction.x).normalize() * half_width_m;
    let corners = [
        start_m + normal,
        start_m - normal,
        end_m - normal,
        end_m + normal,
    ];
    for index in [0, 1, 2, 0, 2, 3] {
        vertices.push(vertex(corners[index], color, camera_m, view_width_m));
    }
}

fn vertex(point_m: Vector, color: [f32; 3], camera_m: Vector, view_width_m: f32) -> Vertex {
    let relative_m = point_m - camera_m;
    Vertex {
        position: [
            2.0 * relative_m.x / view_width_m,
            -2.0 * relative_m.y / VIEW_HEIGHT_M,
        ],
        color,
    }
}

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
