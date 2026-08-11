use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
};

use bytemuck::{Pod, Zeroable};
use pyboat_protocol::{ServerMessage, decode_server_message, encode_client_message};
use rapier2d::prelude::Vector;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, Event, HtmlCanvasElement, MessageEvent, WebSocket};
use wgpu::CurrentSurfaceTexture;

use crate::{BoatScene, MultiplayerClient, VIEW_HEIGHT_M, viewport_interest_radius_m};

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

    fn render(&mut self, scenes: &[BoatScene], camera_m: Vector) -> Result<(), JsValue> {
        self.resize();
        let vertices = scene_vertices(scenes, camera_m, self.config.width, self.config.height);
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

    fn aspect_ratio(&self) -> f32 {
        let (width, height) = canvas_dimensions(&self.canvas);
        width as f32 / height as f32
    }
}

#[wasm_bindgen]
pub struct BrowserGame {
    client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
    socket: WebSocket,
    renderer: Renderer,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

#[derive(Clone, Copy, PartialEq)]
enum ConnectionStatus {
    Connecting,
    Connected,
    Error,
    Closed,
}

#[wasm_bindgen]
impl BrowserGame {
    pub async fn create(canvas: HtmlCanvasElement) -> Result<BrowserGame, JsValue> {
        console_error_panic_hook::set_once();
        let renderer = Renderer::new(canvas).await?;
        let client = Rc::new(RefCell::new(MultiplayerClient::new()));
        let pending_snapshot = Rc::new(RefCell::new(None));
        let connection_status = Rc::new(Cell::new(ConnectionStatus::Connecting));
        let close_notice = Rc::new(RefCell::new(None));
        let socket = WebSocket::new(&server_url()?)?;

        let on_open = {
            let connection_status = Rc::clone(&connection_status);
            Closure::new(move |_: Event| connection_status.set(ConnectionStatus::Connected))
        };
        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let on_message = {
            let client = Rc::clone(&client);
            let pending_snapshot = Rc::clone(&pending_snapshot);
            let connection_status = Rc::clone(&connection_status);
            Closure::new(move |event: MessageEvent| {
                let Some(message) = event.data().as_string() else {
                    client.borrow_mut().disconnect();
                    connection_status.set(ConnectionStatus::Error);
                    return;
                };
                match decode_server_message(&message) {
                    Ok(message @ ServerMessage::Welcome { .. }) => {
                        client.borrow_mut().accept(message);
                    }
                    Ok(message @ ServerMessage::Snapshot { .. }) => {
                        pending_snapshot.replace(Some(message));
                    }
                    Err(_) => {
                        client.borrow_mut().disconnect();
                        connection_status.set(ConnectionStatus::Error);
                    }
                }
            })
        };
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let on_error = {
            let client = Rc::clone(&client);
            let connection_status = Rc::clone(&connection_status);
            Closure::new(move |_: Event| {
                client.borrow_mut().disconnect();
                connection_status.set(ConnectionStatus::Error);
            })
        };
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        let on_close = {
            let client = Rc::clone(&client);
            let connection_status = Rc::clone(&connection_status);
            let close_notice = Rc::clone(&close_notice);
            Closure::new(move |event: CloseEvent| {
                client.borrow_mut().disconnect();
                let reason = event.reason();
                close_notice.replace((!reason.is_empty()).then_some(reason));
                if connection_status.get() != ConnectionStatus::Error {
                    connection_status.set(ConnectionStatus::Closed);
                }
            })
        };
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(Self {
            client,
            pending_snapshot,
            connection_status,
            close_notice,
            socket,
            renderer,
            _on_open: on_open,
            _on_message: on_message,
            _on_error: on_error,
            _on_close: on_close,
        })
    }

    pub fn frame(&mut self, elapsed_seconds: f32) -> Result<(), JsValue> {
        let viewport_radius_m = viewport_interest_radius_m(self.renderer.aspect_ratio());
        let mut client = self.client.borrow_mut();
        let viewport = client.update_viewport(viewport_radius_m);
        let inputs = client.take_inputs(elapsed_seconds);
        drop(client);
        if self.socket.ready_state() == WebSocket::OPEN {
            if let Some(viewport) = viewport {
                self.socket
                    .send_with_str(&encode_client_message(&viewport).map_err(js_error)?)?;
            }
            for input in inputs {
                self.socket
                    .send_with_str(&encode_client_message(&input).map_err(js_error)?)?;
            }
        }
        if let Some(snapshot) = self.pending_snapshot.borrow_mut().take() {
            self.client.borrow_mut().accept(snapshot);
        }
        let client = self.client.borrow();
        self.renderer
            .render(&client.scenes(), client.camera_m().unwrap_or(Vector::ZERO))
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        gamepad_x: f32,
        gamepad_y: f32,
    ) {
        self.client.borrow_mut().set_player_input(
            keyboard_forward,
            keyboard_turn,
            gamepad_x,
            gamepad_y,
        );
    }

    pub fn speed_m_s(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.speed_m_s)
            .unwrap_or(0.0)
    }

    pub fn apparent_wind_speed_m_s(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.apparent_wind_speed_m_s)
            .unwrap_or(0.0)
    }

    pub fn health(&self) -> f32 {
        self.client.borrow().health().unwrap_or(0.0)
    }

    pub fn point_of_sail(&self) -> String {
        self.client
            .borrow()
            .view()
            .map(|view| view.point_of_sail.label().to_owned())
            .unwrap_or_else(|| "waiting".to_owned())
    }

    pub fn status(&self) -> String {
        match self.connection_status.get() {
            ConnectionStatus::Connecting => "Connecting to authoritative server…".to_owned(),
            ConnectionStatus::Connected => self
                .client
                .borrow()
                .player_id()
                .map(|player_id| format!("Authoritative server · {player_id}"))
                .unwrap_or_else(|| "Connected · waiting for identity…".to_owned()),
            ConnectionStatus::Error => "Server connection failed".to_owned(),
            ConnectionStatus::Closed => self
                .close_notice
                .borrow()
                .as_ref()
                .map(|notice| format!("Server connection closed · {notice}"))
                .unwrap_or_else(|| "Server connection closed".to_owned()),
        }
    }
}

fn server_url() -> Result<String, JsValue> {
    let location = web_sys::window()
        .ok_or_else(|| JsValue::from_str("browser window is unavailable"))?
        .location();
    let scheme = if location.protocol()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    Ok(format!("{scheme}://{}:8765", location.hostname()?))
}

fn canvas_dimensions(canvas: &HtmlCanvasElement) -> (u32, u32) {
    let scale = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0);
    let width = (f64::from(canvas.client_width()) * scale).round() as u32;
    let height = (f64::from(canvas.client_height()) * scale).round() as u32;
    (width.max(1), height.max(1))
}

fn scene_vertices(scenes: &[BoatScene], camera_m: Vector, width: u32, height: u32) -> Vec<Vertex> {
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

    for scene in scenes {
        let hull_color = if scene.wreck {
            [0.43, 0.37, 0.29]
        } else if scene.recently_damaged {
            [1.0, 0.43, 0.35]
        } else if scene.is_local {
            [0.42, 0.82, 0.69]
        } else {
            [0.91, 0.42, 0.32]
        };
        for [first, second, third] in [[0, 1, 2], [0, 2, 3], [0, 3, 4]] {
            for point in [
                scene.hull_m[first],
                scene.hull_m[second],
                scene.hull_m[third],
            ] {
                vertices.push(vertex(point, hull_color, camera_m, view_width_m));
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
    }
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
