use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::VecDeque,
    mem,
    rc::Rc,
};

use bytemuck::{Pod, Zeroable};
use bytes::Bytes;
use game::{
    celestial_lighting, scalar_field_time_coordinates, vector_field_time_coordinates,
    world_celestial_conditions,
};
use protocol::{ServerMessage, decode_server_message, encode_client_message};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;
use url::Url;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;
use web_transport_wasm::{ClientBuilder, Session};
use wgpu::CurrentSurfaceTexture;

use crate::{
    BoatScene, DEFAULT_VIEW_HEIGHT_M, MultiplayerClient, SCENE_SHADER, Windsock, cloud_anchor_time,
    sample_production_world_environment, sample_world_geometry, scale_bar,
    viewport_interest_radius_m, windsock_extension, zoom_view_height_m,
};

const MAX_VERTICES: usize = 32768;
const MAXIMUM_PENDING_DATAGRAMS: usize = 15;
const LOW_CLOUD_REFRESH_TICKS: u64 = 108_000;
const UPPER_CLOUD_REFRESH_TICKS: u64 = 216_000;

#[wasm_bindgen]
pub struct PlaygroundWorldGeometrySample {
    raw_x_m: f32,
    raw_y_m: f32,
    canonical_x_m: f32,
    canonical_y_m: f32,
    normalized_x: f32,
    normalized_y: f32,
}

#[wasm_bindgen]
pub struct PlaygroundEnvironmentSample {
    geographic_latitude_degrees: f32,
    season_phase: f32,
    day_phase: f32,
    moon_phase: f32,
    annual_mean_air_temperature_c: f32,
    seasonal_temperature_range_c: f32,
    diurnal_temperature_range_c: f32,
    pressure_hpa: f32,
    pressure_gradient_x_hpa_per_m: f32,
    pressure_gradient_y_hpa_per_m: f32,
    synoptic_pressure_gradient_x_hpa_per_m: f32,
    synoptic_pressure_gradient_y_hpa_per_m: f32,
    surface_air_temperature_anomaly_c: f32,
    air_temperature_500_hpa_c: f32,
    temperature_gradient_x_c_per_m: f32,
    temperature_gradient_y_c_per_m: f32,
    water_vapor_g_kg: f32,
    reference_tidal_range_m: f32,
    tidal_phase_offset: f32,
    surface_air_temperature_c: f32,
    wind_x_m_s: f32,
    wind_y_m_s: f32,
    upper_wind_x_m_s: f32,
    upper_wind_y_m_s: f32,
    relative_humidity: f32,
    lift: f32,
    cloudiness: f32,
    instability: f32,
    upper_cloudiness: f32,
    rain_intensity: f32,
    sun_x: f32,
    sun_y: f32,
    sun_z: f32,
    sun_intensity: f32,
    moon_x: f32,
    moon_y: f32,
    moon_z: f32,
    moon_intensity: f32,
    ambient_intensity: f32,
    ground_temperature_c: f32,
    water_temperature_c: f32,
    seasonal_snow_cover: f32,
    polar_ice_closure: f32,
    solid_ice_cover: f32,
    water_height_m: f32,
    water_depth_m: f32,
    current_x_m_s: f32,
    current_y_m_s: f32,
    wave_orbital_x_m_s: f32,
    wave_orbital_y_m_s: f32,
    significant_wave_height_m: f32,
    wave_surface_displacement_m: f32,
    rogue_crest_excess_m: f32,
}

macro_rules! environment_sample_getters {
    ($($name:ident),+ $(,)?) => {
        $(
            #[wasm_bindgen]
            impl PlaygroundEnvironmentSample {
                #[wasm_bindgen(getter)]
                pub fn $name(&self) -> f32 {
                    self.$name
                }
            }
        )+
    };
}

environment_sample_getters!(
    geographic_latitude_degrees,
    season_phase,
    day_phase,
    moon_phase,
    annual_mean_air_temperature_c,
    seasonal_temperature_range_c,
    diurnal_temperature_range_c,
    pressure_hpa,
    pressure_gradient_x_hpa_per_m,
    pressure_gradient_y_hpa_per_m,
    synoptic_pressure_gradient_x_hpa_per_m,
    synoptic_pressure_gradient_y_hpa_per_m,
    surface_air_temperature_anomaly_c,
    air_temperature_500_hpa_c,
    temperature_gradient_x_c_per_m,
    temperature_gradient_y_c_per_m,
    water_vapor_g_kg,
    reference_tidal_range_m,
    tidal_phase_offset,
    surface_air_temperature_c,
    wind_x_m_s,
    wind_y_m_s,
    upper_wind_x_m_s,
    upper_wind_y_m_s,
    relative_humidity,
    lift,
    cloudiness,
    instability,
    upper_cloudiness,
    rain_intensity,
    sun_x,
    sun_y,
    sun_z,
    sun_intensity,
    moon_x,
    moon_y,
    moon_z,
    moon_intensity,
    ambient_intensity,
    ground_temperature_c,
    water_temperature_c,
    seasonal_snow_cover,
    polar_ice_closure,
    solid_ice_cover,
    water_height_m,
    water_depth_m,
    current_x_m_s,
    current_y_m_s,
    wave_orbital_x_m_s,
    wave_orbital_y_m_s,
    significant_wave_height_m,
    wave_surface_displacement_m,
    rogue_crest_excess_m,
);

#[wasm_bindgen]
pub fn sample_playground_environment(
    width_m: f32,
    height_m: f32,
    match_seed: u64,
    tick: u64,
    center_x_m: f32,
    center_y_m: f32,
) -> Result<PlaygroundEnvironmentSample, JsValue> {
    let geometry = WorldGeometry::new(width_m, height_m).map_err(|error| error.to_string())?;
    let sample = sample_production_world_environment(
        geometry,
        match_seed,
        tick,
        Vector::new(center_x_m, center_y_m),
    );
    let primary = sample.weather.primary;
    let lighting = celestial_lighting(sample.celestial);
    Ok(PlaygroundEnvironmentSample {
        geographic_latitude_degrees: sample.celestial.geographic_latitude_degrees,
        season_phase: sample.celestial.season_phase,
        day_phase: sample.celestial.day_phase,
        moon_phase: sample.celestial.moon_phase,
        annual_mean_air_temperature_c: sample.climate.annual_mean_air_temperature_c,
        seasonal_temperature_range_c: sample.climate.seasonal_temperature_range_c,
        diurnal_temperature_range_c: sample.climate.diurnal_temperature_range_c,
        pressure_hpa: primary.pressure_hpa,
        pressure_gradient_x_hpa_per_m: primary.pressure_gradient_hpa_per_m.x,
        pressure_gradient_y_hpa_per_m: primary.pressure_gradient_hpa_per_m.y,
        synoptic_pressure_gradient_x_hpa_per_m: primary.synoptic_pressure_gradient_hpa_per_m.x,
        synoptic_pressure_gradient_y_hpa_per_m: primary.synoptic_pressure_gradient_hpa_per_m.y,
        surface_air_temperature_anomaly_c: primary.surface_air_temperature_anomaly_c,
        air_temperature_500_hpa_c: primary.air_temperature_500_hpa_c,
        temperature_gradient_x_c_per_m: primary.local_temperature_gradient_c_per_m.x,
        temperature_gradient_y_c_per_m: primary.local_temperature_gradient_c_per_m.y,
        water_vapor_g_kg: primary.water_vapor_g_kg,
        reference_tidal_range_m: sample.tide.reference_range_m,
        tidal_phase_offset: sample.tide.phase_offset,
        surface_air_temperature_c: sample.weather.surface_air_temperature_c,
        wind_x_m_s: sample.weather.wind_velocity_m_s.x,
        wind_y_m_s: sample.weather.wind_velocity_m_s.y,
        upper_wind_x_m_s: sample.weather.upper_wind_velocity_m_s.x,
        upper_wind_y_m_s: sample.weather.upper_wind_velocity_m_s.y,
        relative_humidity: sample.weather.relative_humidity,
        lift: sample.weather.lift,
        cloudiness: sample.weather.cloudiness,
        instability: sample.weather.instability,
        upper_cloudiness: sample.weather.upper_cloudiness,
        rain_intensity: sample.weather.rain_intensity,
        sun_x: lighting.sun_direction[0],
        sun_y: lighting.sun_direction[1],
        sun_z: lighting.sun_direction[2],
        sun_intensity: lighting.sun_intensity,
        moon_x: lighting.moon_direction[0],
        moon_y: lighting.moon_direction[1],
        moon_z: lighting.moon_direction[2],
        moon_intensity: lighting.moon_intensity,
        ambient_intensity: lighting.ambient_intensity,
        ground_temperature_c: sample.surface_climate.ground_temperature_c,
        water_temperature_c: sample.surface_climate.water_temperature_c,
        seasonal_snow_cover: sample.surface_climate.seasonal_snow_cover,
        polar_ice_closure: sample.surface_climate.polar_ice_closure,
        solid_ice_cover: sample.surface_climate.solid_ice_cover,
        water_height_m: sample.water_height_m,
        water_depth_m: sample.environment.water_depth_m,
        current_x_m_s: sample.ocean.current_velocity_m_s.x,
        current_y_m_s: sample.ocean.current_velocity_m_s.y,
        wave_orbital_x_m_s: sample.ocean.wave_orbital_velocity_m_s.x,
        wave_orbital_y_m_s: sample.ocean.wave_orbital_velocity_m_s.y,
        significant_wave_height_m: sample.ocean.significant_wave_height_m,
        wave_surface_displacement_m: sample.ocean.surface_displacement_m,
        rogue_crest_excess_m: sample.ocean.rogue_crest_excess_m,
    })
}

#[wasm_bindgen]
pub fn sample_playground_field_time_coordinates(
    world_width_m: f32,
    world_height_m: f32,
    world_tick: u64,
    pressure_tick: u64,
    current_tick: u64,
) -> Result<Vec<f32>, JsValue> {
    let geometry = WorldGeometry::new(world_width_m, world_height_m)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let scalar = scalar_field_time_coordinates(world_tick);
    let pressure = vector_field_time_coordinates(geometry, pressure_tick);
    let current = vector_field_time_coordinates(geometry, current_tick);
    let mut values = Vec::with_capacity(12);
    values.extend([
        scalar.air_temperature,
        scalar.upper_air_temperature,
        scalar.water_vapor,
        scalar.decorative_water,
    ]);
    values.extend(pressure.pressure);
    values.extend([current.ocean_current, 0.0, 0.0, 0.0]);
    Ok(values)
}

#[wasm_bindgen]
impl PlaygroundWorldGeometrySample {
    #[wasm_bindgen(getter)]
    pub fn raw_x_m(&self) -> f32 {
        self.raw_x_m
    }

    #[wasm_bindgen(getter)]
    pub fn raw_y_m(&self) -> f32 {
        self.raw_y_m
    }

    #[wasm_bindgen(getter)]
    pub fn canonical_x_m(&self) -> f32 {
        self.canonical_x_m
    }

    #[wasm_bindgen(getter)]
    pub fn canonical_y_m(&self) -> f32 {
        self.canonical_y_m
    }

    #[wasm_bindgen(getter)]
    pub fn normalized_x(&self) -> f32 {
        self.normalized_x
    }

    #[wasm_bindgen(getter)]
    pub fn normalized_y(&self) -> f32 {
        self.normalized_y
    }
}

#[wasm_bindgen]
pub fn sample_playground_world_geometry(
    width_m: f32,
    height_m: f32,
    raw_x_m: f32,
    raw_y_m: f32,
) -> Result<PlaygroundWorldGeometrySample, JsValue> {
    let geometry = WorldGeometry::new(width_m, height_m).map_err(|error| error.to_string())?;
    let sample = sample_world_geometry(geometry, [raw_x_m, raw_y_m], 0.0);
    Ok(PlaygroundWorldGeometrySample {
        raw_x_m: sample.raw_center_m[0],
        raw_y_m: sample.raw_center_m[1],
        canonical_x_m: sample.canonical_center_m[0],
        canonical_y_m: sample.canonical_center_m[1],
        normalized_x: sample.normalized_world[0],
        normalized_y: sample.normalized_world[1],
    })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
    is_terrain: f32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3, 2 => Float32];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    camera_and_view_m: [f32; 4],
    scalar_field_time_coordinates: [f32; 4],
    match_seed: [u32; 2],
    low_cloud_epoch: [u32; 2],
    world_size_m: [f32; 2],
    upper_cloud_epoch: [u32; 2],
    cloud_anchor_phase_and_padding: [f32; 4],
    world_phases: [f32; 4],
    world_tick_and_padding: [u32; 4],
    pressure_time_coordinate: [f32; 4],
    ocean_current_time_coordinate_and_padding: [f32; 4],
}

struct Renderer {
    canvas: HtmlCanvasElement,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
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
                label: Some("WebGPU device"),
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
            label: Some("Scene shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SCENE_SHADER)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene uniforms layout"),
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
            label: Some("Scene pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Scene pipeline"),
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
            label: Some("Dynamic vertices"),
            size: (MAX_VERTICES * mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene uniforms"),
            size: mem::size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene uniforms bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            canvas,
            device,
            queue,
            surface,
            config,
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
        })
    }

    fn render(
        &mut self,
        scenes: &[BoatScene],
        match_seed: Option<u64>,
        world_geometry: WorldGeometry,
        tick: u64,
        camera_m: Vector,
        view_height_m: f32,
    ) -> Result<(), JsValue> {
        self.resize();
        let vertices = scene_vertices(
            scenes,
            match_seed.is_some(),
            camera_m,
            self.config.width,
            self.config.height,
            view_height_m,
        );
        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        let view_width_m = view_height_m * self.config.width as f32 / self.config.height as f32;
        let canonical_camera_m = world_geometry.wrap_position_m(camera_m.into());
        let celestial =
            world_celestial_conditions(world_geometry, tick as f64 / 60.0, Vector::ZERO);
        let match_seed = match_seed.unwrap_or(0);
        let low_cloud_time = cloud_anchor_time(tick, LOW_CLOUD_REFRESH_TICKS);
        let upper_cloud_time = cloud_anchor_time(tick, UPPER_CLOUD_REFRESH_TICKS);
        let scalar_time = scalar_field_time_coordinates(tick);
        let vector_time = vector_field_time_coordinates(world_geometry, tick);
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneUniforms {
                camera_and_view_m: [
                    canonical_camera_m[0],
                    canonical_camera_m[1],
                    view_width_m,
                    view_height_m,
                ],
                scalar_field_time_coordinates: [
                    scalar_time.air_temperature,
                    scalar_time.upper_air_temperature,
                    scalar_time.water_vapor,
                    scalar_time.decorative_water,
                ],
                match_seed: [match_seed as u32, (match_seed >> 32) as u32],
                low_cloud_epoch: [
                    low_cloud_time.epoch as u32,
                    (low_cloud_time.epoch >> 32) as u32,
                ],
                world_size_m: [world_geometry.width_m(), world_geometry.height_m()],
                upper_cloud_epoch: [
                    upper_cloud_time.epoch as u32,
                    (upper_cloud_time.epoch >> 32) as u32,
                ],
                cloud_anchor_phase_and_padding: [
                    low_cloud_time.phase,
                    upper_cloud_time.phase,
                    0.0,
                    0.0,
                ],
                world_phases: [
                    celestial.day_phase,
                    celestial.season_phase,
                    celestial.moon_phase,
                    0.0,
                ],
                world_tick_and_padding: [tick as u32, (tick >> 32) as u32, 0, 0],
                pressure_time_coordinate: vector_time.pressure,
                ocean_current_time_coordinate_and_padding: [
                    vector_time.ocean_current,
                    0.0,
                    0.0,
                    0.0,
                ],
            }),
        );

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
                label: Some("Frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene pass"),
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
            pass.set_bind_group(0, &self.bind_group, &[]);
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
    transport: Rc<RefCell<Option<Session>>>,
    pending_datagrams: Rc<RefCell<VecDeque<Vec<u8>>>>,
    pending_reliable: Rc<RefCell<VecDeque<Vec<u8>>>>,
    sending_datagrams: Rc<Cell<bool>>,
    sending_reliable: Rc<Cell<bool>>,
    renderer: Renderer,
    view_height_m: f32,
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
        let transport = Rc::new(RefCell::new(None));
        let pending_datagrams = Rc::new(RefCell::new(VecDeque::new()));
        let pending_reliable = Rc::new(RefCell::new(VecDeque::new()));
        let sending_datagrams = Rc::new(Cell::new(false));
        let sending_reliable = Rc::new(Cell::new(false));

        connect_webtransport(
            Rc::clone(&client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
            Rc::clone(&close_notice),
            Rc::clone(&transport),
            Rc::clone(&pending_datagrams),
            Rc::clone(&pending_reliable),
            Rc::clone(&sending_datagrams),
            Rc::clone(&sending_reliable),
        )?;

        Ok(Self {
            client,
            pending_snapshot,
            connection_status,
            close_notice,
            transport,
            pending_datagrams,
            pending_reliable,
            sending_datagrams,
            sending_reliable,
            renderer,
            view_height_m: DEFAULT_VIEW_HEIGHT_M,
        })
    }

    pub fn frame(&mut self, elapsed_seconds: f32) -> Result<(), JsValue> {
        let viewport_radius_m =
            viewport_interest_radius_m(self.renderer.aspect_ratio(), self.view_height_m);
        let mut client = self.client.borrow_mut();
        let viewport = client.update_viewport(viewport_radius_m);
        let inputs = client.take_inputs(elapsed_seconds);
        drop(client);
        if let Some(viewport) = viewport {
            self.pending_reliable
                .borrow_mut()
                .push_back(encode_client_message(&viewport).map_err(js_error)?);
        }
        for input in inputs {
            let mut pending = self.pending_datagrams.borrow_mut();
            if pending.len() == MAXIMUM_PENDING_DATAGRAMS {
                pending.pop_front();
            }
            pending.push_back(encode_client_message(&input).map_err(js_error)?);
        }
        pump_datagrams(
            &self.transport,
            &self.pending_datagrams,
            &self.sending_datagrams,
        );
        pump_reliable(
            &self.transport,
            &self.pending_reliable,
            &self.sending_reliable,
        );
        if let Some(snapshot) = self.pending_snapshot.borrow_mut().take() {
            self.client.borrow_mut().accept(snapshot);
        }
        let client = self.client.borrow();
        self.renderer.render(
            &client.scenes(),
            client.match_seed(),
            client.world_geometry().unwrap_or_default(),
            client.prediction_tick,
            client.camera_m().unwrap_or(Vector::ZERO),
            self.view_height_m,
        )
    }

    pub fn zoom(&mut self, wheel_delta_y: f32) {
        self.view_height_m = zoom_view_height_m(self.view_height_m, wheel_delta_y);
    }

    pub fn reset_zoom(&mut self) {
        self.view_height_m = DEFAULT_VIEW_HEIGHT_M;
    }

    pub fn scale_bar_width_px(&self) -> f32 {
        scale_bar(
            self.view_height_m,
            self.renderer.canvas.client_height() as f32,
        )
        .width_px
    }

    pub fn scale_bar_label(&self) -> String {
        scale_bar(
            self.view_height_m,
            self.renderer.canvas.client_height() as f32,
        )
        .label
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        world_x: f32,
        world_y: f32,
    ) {
        self.client.borrow_mut().set_player_input(
            keyboard_forward,
            keyboard_turn,
            world_x,
            world_y,
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

    pub fn water_depth_m(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.water_depth_m)
            .unwrap_or(0.0)
    }

    pub fn bottom_clearance_m(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.bottom_clearance_m)
            .unwrap_or(0.0)
    }

    pub fn bottom_contact_fraction(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.bottom_contact_fraction)
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

#[allow(clippy::too_many_arguments)]
fn connect_webtransport(
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
    transport: Rc<RefCell<Option<Session>>>,
    pending_datagrams: Rc<RefCell<VecDeque<Vec<u8>>>>,
    pending_reliable: Rc<RefCell<VecDeque<Vec<u8>>>>,
    sending_datagrams: Rc<Cell<bool>>,
    sending_reliable: Rc<Cell<bool>>,
) -> Result<(), JsValue> {
    let url = Url::parse(&server_url()?).map_err(js_error)?;
    let builder = ClientBuilder::new()
        .with_pooling(false)
        .with_unreliable(true);
    let webtransport_client = match configured_certificate_hash()? {
        Some(hash) => builder.with_server_certificate_hashes(vec![hash]),
        None => builder.with_system_roots(),
    };

    spawn_local(async move {
        let session = match webtransport_client.connect(url).await {
            Ok(session) => session,
            Err(error) => {
                protocol_client.borrow_mut().disconnect();
                close_notice.replace(Some(error.to_string()));
                connection_status.set(ConnectionStatus::Error);
                return;
            }
        };
        transport.replace(Some(session.clone()));
        connection_status.set(ConnectionStatus::Connected);
        receive_datagrams(
            session.clone(),
            Rc::clone(&protocol_client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
        );
        receive_reliable_messages(
            session.clone(),
            Rc::clone(&protocol_client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
        );
        watch_connection_close(session, protocol_client, connection_status, close_notice);
        pump_datagrams(&transport, &pending_datagrams, &sending_datagrams);
        pump_reliable(&transport, &pending_reliable, &sending_reliable);
    });
    Ok(())
}

fn receive_datagrams(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
) {
    spawn_local(async move {
        while let Ok(message) = session.recv_datagram().await {
            if accept_server_message(&message, &protocol_client, &pending_snapshot).is_err() {
                protocol_client.borrow_mut().disconnect();
                connection_status.set(ConnectionStatus::Error);
                break;
            }
        }
    });
}

fn receive_reliable_messages(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
) {
    spawn_local(async move {
        while let Ok(mut stream) = session.accept_uni().await {
            let mut message = Vec::new();
            loop {
                match stream.read(64 * 1024).await {
                    Ok(Some(chunk)) => message.extend_from_slice(&chunk),
                    Ok(None) => break,
                    Err(_) => return,
                }
            }
            if accept_server_message(&message, &protocol_client, &pending_snapshot).is_err() {
                protocol_client.borrow_mut().disconnect();
                connection_status.set(ConnectionStatus::Error);
                break;
            }
        }
    });
}

fn accept_server_message(
    bytes: &[u8],
    protocol_client: &Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: &Rc<RefCell<Option<ServerMessage>>>,
) -> Result<(), protocol::ProtocolError> {
    match decode_server_message(bytes)? {
        message @ ServerMessage::Welcome { .. } => protocol_client.borrow_mut().accept(message),
        message @ ServerMessage::Snapshot { .. } => {
            pending_snapshot.replace(Some(message));
        }
    }
    Ok(())
}

fn watch_connection_close(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
) {
    spawn_local(async move {
        let error = session.closed().await;
        protocol_client.borrow_mut().disconnect();
        close_notice.replace(Some(error.to_string()));
        if connection_status.get() != ConnectionStatus::Error {
            connection_status.set(ConnectionStatus::Closed);
        }
    });
}

fn pump_datagrams(
    transport: &Rc<RefCell<Option<Session>>>,
    pending: &Rc<RefCell<VecDeque<Vec<u8>>>>,
    sending: &Rc<Cell<bool>>,
) {
    if sending.get() {
        return;
    }
    let Some(session) = transport.borrow().clone() else {
        return;
    };
    sending.set(true);
    let pending = Rc::clone(pending);
    let sending = Rc::clone(sending);
    spawn_local(async move {
        loop {
            let Some(message) = pending.borrow_mut().pop_front() else {
                break;
            };
            if session.send_datagram(Bytes::from(message)).await.is_err() {
                pending.borrow_mut().clear();
                break;
            }
        }
        sending.set(false);
    });
}

fn pump_reliable(
    transport: &Rc<RefCell<Option<Session>>>,
    pending: &Rc<RefCell<VecDeque<Vec<u8>>>>,
    sending: &Rc<Cell<bool>>,
) {
    if sending.get() {
        return;
    }
    let Some(session) = transport.borrow().clone() else {
        return;
    };
    sending.set(true);
    let pending = Rc::clone(pending);
    let sending = Rc::clone(sending);
    spawn_local(async move {
        loop {
            let Some(message) = pending.borrow_mut().pop_front() else {
                break;
            };
            let result = async {
                let mut stream = session.open_uni().await?;
                stream.write(&message).await?;
                stream.finish()
            }
            .await;
            if result.is_err() {
                pending.borrow_mut().clear();
                break;
            }
        }
        sending.set(false);
    });
}

fn configured_certificate_hash() -> Result<Option<Vec<u8>>, JsValue> {
    let search = web_sys::window()
        .ok_or_else(|| JsValue::from_str("browser window is unavailable"))?
        .location()
        .search()?;
    let Some(value) = search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|parameter| parameter.split_once('='))
        .find_map(|(key, value)| (key == "webtransportCertificateHash").then_some(value))
    else {
        return Ok(None);
    };
    let bytes = value
        .split(':')
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .map_err(|_| JsValue::from_str("invalid WebTransport certificate hash"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if bytes.len() != 32 {
        return Err(JsValue::from_str(
            "WebTransport certificate hash must contain 32 bytes",
        ));
    }
    Ok(Some(bytes))
}

fn server_url() -> Result<String, JsValue> {
    let location = web_sys::window()
        .ok_or_else(|| JsValue::from_str("browser window is unavailable"))?
        .location();
    Ok(format!("https://{}:8765/game", location.hostname()?))
}

fn canvas_dimensions(canvas: &HtmlCanvasElement) -> (u32, u32) {
    let scale = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0);
    let width = (f64::from(canvas.client_width()) * scale).round() as u32;
    let height = (f64::from(canvas.client_height()) * scale).round() as u32;
    (width.max(1), height.max(1))
}

fn scene_vertices(
    scenes: &[BoatScene],
    render_terrain: bool,
    camera_m: Vector,
    width: u32,
    height: u32,
    view_height_m: f32,
) -> Vec<Vertex> {
    let aspect = width as f32 / height as f32;
    let view_width_m = view_height_m * aspect;
    let mut vertices = Vec::new();

    if render_terrain {
        for position in [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ] {
            vertices.push(Vertex {
                position,
                color: [0.0; 3],
                is_terrain: 1.0,
            });
        }
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
                vertices.push(vertex(
                    point,
                    hull_color,
                    camera_m,
                    view_width_m,
                    view_height_m,
                ));
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
            view_height_m,
        );
        push_quad(
            &mut vertices,
            scene.rudder_m[0],
            scene.rudder_m[1],
            0.08,
            [0.86, 0.91, 0.91],
            camera_m,
            view_width_m,
            view_height_m,
        );
        if let Some(windsock) = scene.windsock {
            push_windsock(
                &mut vertices,
                windsock,
                camera_m,
                view_width_m,
                view_height_m,
            );
        }
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
    view_height_m: f32,
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
        vertices.push(vertex(
            corners[index],
            color,
            camera_m,
            view_width_m,
            view_height_m,
        ));
    }
}

fn push_windsock(
    vertices: &mut Vec<Vertex>,
    windsock: Windsock,
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) {
    let speed_m_s = windsock.apparent_wind_velocity_m_s.length();
    if speed_m_s <= f32::EPSILON {
        return;
    }
    let direction = windsock.apparent_wind_velocity_m_s / speed_m_s;
    let normal = Vector::new(-direction.y, direction.x);
    let extension = windsock_extension(speed_m_s);
    let length_m = 1.2;
    for segment in 0..5 {
        let start = segment as f32 / 5.0;
        if start >= extension {
            break;
        }
        let end = ((segment + 1) as f32 / 5.0).min(extension);
        let start_m = windsock.anchor_m + direction * length_m * start;
        let end_m = windsock.anchor_m + direction * length_m * end;
        let start_half_width_m = 0.1 - 0.075 * start;
        let end_half_width_m = 0.1 - 0.075 * end;
        let corners = [
            start_m + normal * start_half_width_m,
            start_m - normal * start_half_width_m,
            end_m - normal * end_half_width_m,
            end_m + normal * end_half_width_m,
        ];
        let color = if segment % 2 == 0 {
            [0.94, 0.18, 0.12]
        } else {
            [0.96, 0.94, 0.84]
        };
        for index in [0, 1, 2, 0, 2, 3] {
            vertices.push(vertex(
                corners[index],
                color,
                camera_m,
                view_width_m,
                view_height_m,
            ));
        }
    }
}

fn vertex(
    point_m: Vector,
    color: [f32; 3],
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) -> Vertex {
    Vertex {
        position: world_to_clip_position(point_m, camera_m, view_width_m, view_height_m),
        color,
        is_terrain: 0.0,
    }
}

fn world_to_clip_position(
    point_m: Vector,
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) -> [f32; 2] {
    let relative_m = point_m - camera_m;
    [relative_m.x / view_width_m, relative_m.y / view_height_m].map(|component| 2.0 * component)
}

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
