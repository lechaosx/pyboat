use std::{env, fs, path::PathBuf};

use bytemuck::{Pod, Zeroable};
use game::{
    AtmospherePrimary, CelestialConditions, ClimateConditions, TidalConditions, celestial_lighting,
    climate_surface, current_air_temperature_c, derive_weather, surface_weather_transport,
    tide_height_m,
};
use rapier2d::prelude::Vector;
use serde::{Deserialize, Serialize};
use simulation::WorldGeometry;
use wgpu::util::DeviceExt;

const SHADER: &str = include_str!("../../../crates/browser/src/scene.wgsl");
const SCENARIOS: &str = include_str!("../../../playground/scenarios.json");

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    slug: String,
    label: String,
    description: String,
    pressure_hpa: f32,
    pressure_gradient: f32,
    pressure_gradient_direction: f32,
    synoptic_pressure_gradient: f32,
    synoptic_pressure_gradient_direction: f32,
    annual_mean_air_temperature: f32,
    current_air_temperature_anomaly: f32,
    seasonal_temperature_range: f32,
    diurnal_temperature_range: f32,
    current_upper_air_temperature: f32,
    temperature_gradient: f32,
    temperature_gradient_direction: f32,
    current_water_vapor: f32,
    aerosol_optical_depth: f32,
    geographic_latitude: f32,
    season_phase: f32,
    day_phase: f32,
    moon_phase: f32,
    tidal_range: f32,
    tidal_phase_offset: f32,
}

#[derive(Clone)]
struct Options {
    scenario: String,
    all: bool,
    output: Option<PathBuf>,
    output_dir: PathBuf,
    width: u32,
    height: u32,
    camera_x_m: f32,
    camera_y_m: f32,
    world_width_m: f32,
    world_height_m: f32,
    view_height_m: f32,
    seed: u64,
    time_seconds: f32,
    debug_view: u32,
}

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
    pressure: [f32; 4],
    synoptic_pressure_and_temperature: [f32; 4],
    moisture_and_temperature_gradient: [f32; 4],
    celestial_conditions: [f32; 4],
    world_surface_conditions: [f32; 4],
    match_seed: [u32; 2],
    low_cloud_epoch: [u32; 2],
    aerosol_and_padding: [f32; 2],
    world_size_m: [f32; 2],
    upper_cloud_epoch: [u32; 2],
    cloud_anchor_phase: [f32; 2],
    surface_weather_transport_m: [f32; 4],
    world_phases: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PlaygroundUniforms {
    cloud_detail: [f32; 4],
    cloud_wisps: [f32; 4],
    cloud_rendering: [f32; 4],
    terrain_tuning: [f32; 4],
    water_tuning: [f32; 4],
    landform_lighting: [f32; 4],
    layer_visibility: [f32; 4],
    landscape_visibility: [f32; 4],
    atmosphere_model: [f32; 4],
    climate_visibility: [f32; 4],
    diagnostics: [u32; 4],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DerivedOutput {
    current_air_temperature_c: f32,
    sun_direction: [f32; 3],
    sun_intensity: f32,
    moon_direction: [f32; 3],
    moon_intensity: f32,
    clear_sky_ambient_intensity: f32,
    ground_temperature_c: f32,
    water_temperature_c: f32,
    seasonal_snow_cover_at_600_m: f32,
    polar_ice_closure: f32,
    solid_ice_cover_at_600_m: f32,
    tide_height_m: f32,
    surface_wind_m_s: [f32; 2],
    surface_wind_speed_m_s: f32,
    upper_wind_m_s: [f32; 2],
    upper_wind_speed_m_s: f32,
    wind_shear_m_s: [f32; 2],
    wind_shear_speed_m_s: f32,
    relative_humidity: f32,
    instability: f32,
    lift: f32,
    low_cloudiness: f32,
    upper_cloudiness: f32,
    rain_intensity: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageMetrics {
    mean_luminance: f32,
    mean_red: f32,
    mean_green: f32,
    mean_blue: f32,
    luminance_contrast: f32,
    luminance_p10: f32,
    luminance_p50: f32,
    luminance_p90: f32,
    dark_pixel_fraction: f32,
    bright_pixel_fraction: f32,
    warm_pixel_fraction: f32,
    center_vertical_rgb_step: f32,
    center_horizontal_rgb_step: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureReport<'a> {
    scenario: &'a Scenario,
    image: String,
    width: u32,
    height: u32,
    camera_m: [f32; 2],
    world_size_m: [f32; 2],
    view_height_m: f32,
    seed: u64,
    time_seconds: f32,
    debug_view: u32,
    derived: DerivedOutput,
    image_metrics: ImageMetrics,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(SCENARIOS).map_err(|error| format!("invalid scenarios: {error}"))?;
    if env::args().any(|argument| argument == "--list") {
        for scenario in scenarios {
            println!(
                "{}\t{}\t{}",
                scenario.slug, scenario.label, scenario.description
            );
        }
        return Ok(());
    }
    let options = parse_options()?;
    let selected: Vec<&Scenario> = if options.all {
        scenarios.iter().collect()
    } else {
        vec![
            scenarios
                .iter()
                .find(|scenario| scenario.slug == options.scenario)
                .ok_or_else(|| format!("unknown scenario: {}", options.scenario))?,
        ]
    };
    pollster::block_on(render(selected, &options))
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        scenario: "fair-cumulus".to_owned(),
        all: false,
        output: None,
        output_dir: PathBuf::from("playground-captures"),
        width: 960,
        height: 600,
        camera_x_m: 0.0,
        camera_y_m: 0.0,
        world_width_m: 64_000.0,
        world_height_m: 64_000.0,
        view_height_m: 480.0,
        seed: 0x5eed,
        time_seconds: 37.0,
        debug_view: 0,
    };
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Skip<std::env::Args>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--all" => options.all = true,
            "--scenario" => options.scenario = value(&mut arguments)?,
            "--output" => options.output = Some(value(&mut arguments)?.into()),
            "--output-dir" => options.output_dir = value(&mut arguments)?.into(),
            "--width" => options.width = parse(&value(&mut arguments)?, &argument)?,
            "--height" => options.height = parse(&value(&mut arguments)?, &argument)?,
            "--camera-x" => options.camera_x_m = parse(&value(&mut arguments)?, &argument)?,
            "--camera-y" => options.camera_y_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-width" => options.world_width_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-height" => options.world_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--view-height" => options.view_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--seed" => options.seed = parse_seed(&value(&mut arguments)?)?,
            "--time" => options.time_seconds = parse(&value(&mut arguments)?, &argument)?,
            "--debug" => options.debug_view = parse_debug_view(&value(&mut arguments)?)?,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown option: {argument}")),
        }
    }
    if options.output.is_some() && options.all {
        return Err("--output cannot be combined with --all; use --output-dir".to_owned());
    }
    if options.width == 0 || options.height == 0 {
        return Err("capture dimensions must be positive".to_owned());
    }
    if !(8_000.0..=256_000.0).contains(&options.world_width_m)
        || !(8_000.0..=256_000.0).contains(&options.world_height_m)
    {
        return Err("world dimensions must be between 8000 and 256000 metres".to_owned());
    }
    Ok(options)
}

fn parse<T: std::str::FromStr>(value: &str, option: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value for {option}: {value}"))
}

fn parse_seed(value: &str) -> Result<u64, String> {
    value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| u64::from_str_radix(hex, 16))
        .map_err(|_| format!("invalid seed: {value}"))
}

fn parse_debug_view(value: &str) -> Result<u32, String> {
    match value {
        "final" => Ok(0),
        "low-density" => Ok(3),
        "upper-depth" => Ok(6),
        "low-shadow" => Ok(9),
        "upper-shadow" => Ok(10),
        "surface-wind" => Ok(16),
        "upper-wind" => Ok(17),
        "shear" => Ok(18),
        "snow" => Ok(40),
        "ice" => Ok(41),
        "autumn" => Ok(42),
        "tide" => Ok(48),
        _ => parse(value, "--debug"),
    }
}

fn print_help() {
    println!(
        "playground-capture [--scenario SLUG | --all] [options]\n\n\
         Options:\n\
           --list                 list scenario slugs and descriptions\n\
           --output PATH          write one scenario to PATH\n\
           --output-dir PATH      directory for --all (default playground-captures)\n\
           --width N --height N   output dimensions (default 960x600)\n\
           --camera-x M --camera-y M  world-space camera centre\n\
           --world-width M --world-height M  periodic world size (default 64000 each)\n\
           --view-height METRES   vertical world span (default 480)\n\
           --seed N|0xHEX         deterministic visual seed\n\
           --time SECONDS         deterministic animation time\n\
           --debug NAME|N         final, low-density, upper-depth, low-shadow,\n\
                                  upper-shadow, surface-wind, upper-wind, shear,\n\
                                  snow, ice, autumn, or tide\n\n\
         Each PNG gets a JSON sidecar containing primary inputs, derived weather,\n\
         surface climate, tide, and image metrics."
    );
}

async fn render(scenarios: Vec<&Scenario>, options: &Options) -> Result<(), String> {
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
        .map_err(|error| format!("no headless Vulkan adapter available: {error}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Playground capture device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|error| format!("failed to create capture device: {error}"))?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Scene shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let scene_layout = uniform_layout(&device, "scene uniforms layout");
    let playground_layout = uniform_layout(&device, "playground uniforms layout");
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("playground capture pipeline layout"),
        bind_group_layouts: &[Some(&scene_layout), Some(&playground_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("playground capture pipeline"),
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
            entry_point: Some("fragment_playground"),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fullscreen terrain vertices"),
        contents: bytemuck::cast_slice(&fullscreen_vertices()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let playground_uniforms = PlaygroundUniforms {
        cloud_detail: [0.35, 0.25, 1.15, 0.06],
        cloud_wisps: [0.25, 0.0, 0.0, 0.0],
        cloud_rendering: [1.0; 4],
        terrain_tuning: [1.0, 3.0, 0.08, 0.0],
        water_tuning: [0.5, 1.0, 0.0, 0.0],
        landform_lighting: [1.0, 0.7, 320.0, 0.0],
        layer_visibility: [1.0; 4],
        landscape_visibility: [1.0; 4],
        atmosphere_model: [1.0; 4],
        climate_visibility: [1.0; 4],
        diagnostics: [options.debug_view, 1, 0, 0],
    };
    let playground_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("playground uniforms"),
        contents: bytemuck::bytes_of(&playground_uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let playground_bind_group = bind_group(
        &device,
        &playground_layout,
        &playground_buffer,
        "playground uniforms bind group",
    );

    for scenario in scenarios {
        let path = options
            .output
            .clone()
            .unwrap_or_else(|| options.output_dir.join(format!("{}.png", scenario.slug)));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        let rgba = render_scenario(
            &device,
            &queue,
            &pipeline,
            &scene_layout,
            &playground_bind_group,
            &vertex_buffer,
            scenario,
            options,
        )?;
        write_png(&path, options.width, options.height, &rgba)?;
        let report = capture_report(scenario, options, &path, &rgba);
        let report_json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to serialize report: {error}"))?;
        let sidecar_path = path.with_extension("json");
        fs::write(&sidecar_path, &report_json)
            .map_err(|error| format!("failed to write {}: {error}", sidecar_path.display()))?;
        println!("{}", serde_json::to_string(&report).unwrap());
    }
    Ok(())
}

fn uniform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

#[allow(clippy::too_many_arguments)]
fn render_scenario(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    scene_layout: &wgpu::BindGroupLayout,
    playground_bind_group: &wgpu::BindGroup,
    vertex_buffer: &wgpu::Buffer,
    scenario: &Scenario,
    options: &Options,
) -> Result<Vec<u8>, String> {
    let pressure_gradient = directed_vector(
        scenario.pressure_gradient / 1000.0,
        scenario.pressure_gradient_direction,
    );
    let synoptic_gradient = directed_vector(
        scenario.synoptic_pressure_gradient / 1000.0,
        scenario.synoptic_pressure_gradient_direction,
    );
    let temperature_gradient = directed_vector(
        scenario.temperature_gradient / 1000.0,
        scenario.temperature_gradient_direction,
    );
    let conditions = CelestialConditions {
        geographic_latitude_degrees: scenario.geographic_latitude,
        season_phase: scenario.season_phase,
        day_phase: scenario.day_phase,
        moon_phase: scenario.moon_phase,
    };
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let canonical_camera_m =
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let low_cloud_epoch = (options.time_seconds / 1800.0).floor() as u64;
    let upper_cloud_epoch = (options.time_seconds / 3600.0).floor() as u64;
    let tick = (options.time_seconds / simulation::DT_SECONDS) as u64;
    let surface_weather_transport = surface_weather_transport(world_geometry, options.seed, tick);
    let uniforms = SceneUniforms {
        camera_and_view_m: [
            canonical_camera_m[0],
            canonical_camera_m[1],
            options.view_height_m * options.width as f32 / options.height as f32,
            options.view_height_m,
        ],
        pressure: [
            scenario.pressure_hpa,
            pressure_gradient.x,
            pressure_gradient.y,
            scenario.seasonal_temperature_range,
        ],
        synoptic_pressure_and_temperature: [
            synoptic_gradient.x,
            synoptic_gradient.y,
            scenario.annual_mean_air_temperature,
            scenario.current_upper_air_temperature,
        ],
        moisture_and_temperature_gradient: [
            scenario.current_water_vapor,
            temperature_gradient.x,
            temperature_gradient.y,
            scenario.diurnal_temperature_range,
        ],
        celestial_conditions: [
            conditions.geographic_latitude_degrees,
            conditions.season_phase,
            conditions.day_phase,
            conditions.moon_phase,
        ],
        world_surface_conditions: [
            scenario.tidal_range,
            scenario.tidal_phase_offset,
            scenario.current_air_temperature_anomaly,
            0.0,
        ],
        match_seed: [options.seed as u32, (options.seed >> 32) as u32],
        low_cloud_epoch: [low_cloud_epoch as u32, (low_cloud_epoch >> 32) as u32],
        aerosol_and_padding: [scenario.aerosol_optical_depth, 0.0],
        world_size_m: [options.world_width_m, options.world_height_m],
        upper_cloud_epoch: [upper_cloud_epoch as u32, (upper_cloud_epoch >> 32) as u32],
        cloud_anchor_phase: [
            options.time_seconds.rem_euclid(1800.0) / 1800.0,
            options.time_seconds.rem_euclid(3600.0) / 3600.0,
        ],
        surface_weather_transport_m: [
            surface_weather_transport.air_temperature_offset_m.x,
            surface_weather_transport.air_temperature_offset_m.y,
            surface_weather_transport.water_vapor_offset_m.x,
            surface_weather_transport.water_vapor_offset_m.y,
        ],
        world_phases: [
            conditions.day_phase,
            conditions.season_phase,
            conditions.moon_phase,
            0.0,
        ],
    };
    let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let scene_bind_group = bind_group(
        device,
        scene_layout,
        &scene_buffer,
        "scene uniforms bind group",
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&scenario.slug),
        size: wgpu::Extent3d {
            width: options.width,
            height: options.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let bytes_per_row = options.width * 4;
    let padded_bytes_per_row = bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("playground capture readback"),
        size: u64::from(padded_bytes_per_row * options.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("playground capture encoder"),
    });
    {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("playground capture pass"),
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
        pass.set_bind_group(0, &scene_bind_group, &[]);
        pass.set_bind_group(1, playground_bind_group, &[]);
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
                rows_per_image: Some(options.height),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("capture GPU poll failed: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("capture readback stopped: {error}"))?
        .map_err(|error| format!("capture readback failed: {error}"))?;
    let data = slice
        .get_mapped_range()
        .map_err(|error| format!("failed to read capture: {error}"))?;
    Ok(data
        .chunks_exact(padded_bytes_per_row as usize)
        .flat_map(|row| &row[..bytes_per_row as usize])
        .copied()
        .collect())
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

fn directed_vector(magnitude: f32, direction_degrees: f32) -> Vector {
    let direction = direction_degrees.to_radians();
    magnitude * Vector::new(direction.cos(), direction.sin())
}

fn capture_report<'a>(
    scenario: &'a Scenario,
    options: &Options,
    path: &std::path::Path,
    rgba: &[u8],
) -> CaptureReport<'a> {
    let pressure_gradient = directed_vector(
        scenario.pressure_gradient / 1000.0,
        scenario.pressure_gradient_direction,
    );
    let synoptic_gradient = directed_vector(
        scenario.synoptic_pressure_gradient / 1000.0,
        scenario.synoptic_pressure_gradient_direction,
    );
    let temperature_gradient = directed_vector(
        scenario.temperature_gradient / 1000.0,
        scenario.temperature_gradient_direction,
    );
    let conditions = CelestialConditions {
        geographic_latitude_degrees: scenario.geographic_latitude,
        season_phase: scenario.season_phase,
        day_phase: scenario.day_phase,
        moon_phase: scenario.moon_phase,
    };
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: scenario.annual_mean_air_temperature,
        seasonal_temperature_range_c: scenario.seasonal_temperature_range,
        diurnal_temperature_range_c: scenario.diurnal_temperature_range,
    };
    let current_air_temperature =
        current_air_temperature_c(climate, conditions) + scenario.current_air_temperature_anomaly;
    let celestial = celestial_lighting(conditions);
    let tide_height = tide_height_m(
        TidalConditions {
            reference_range_m: scenario.tidal_range,
            phase_offset: scenario.tidal_phase_offset,
        },
        conditions,
    );
    let weather = derive_weather(
        AtmospherePrimary {
            pressure_hpa: scenario.pressure_hpa,
            pressure_gradient_hpa_per_m: pressure_gradient,
            synoptic_pressure_gradient_hpa_per_m: synoptic_gradient,
            surface_air_temperature_anomaly_c: scenario.current_air_temperature_anomaly,
            air_temperature_500_hpa_c: scenario.current_upper_air_temperature,
            temperature_gradient_c_per_m: temperature_gradient,
            water_vapor_g_kg: scenario.current_water_vapor,
        },
        climate,
        conditions,
    );
    let surface = climate_surface(
        climate,
        conditions,
        scenario.current_water_vapor,
        scenario.current_air_temperature_anomaly,
        weather.rain_intensity,
        600.0,
    );
    CaptureReport {
        scenario,
        image: path.display().to_string(),
        width: options.width,
        height: options.height,
        camera_m: [options.camera_x_m, options.camera_y_m],
        world_size_m: [options.world_width_m, options.world_height_m],
        view_height_m: options.view_height_m,
        seed: options.seed,
        time_seconds: options.time_seconds,
        debug_view: options.debug_view,
        derived: DerivedOutput {
            current_air_temperature_c: current_air_temperature,
            sun_direction: celestial.sun_direction,
            sun_intensity: celestial.sun_intensity,
            moon_direction: celestial.moon_direction,
            moon_intensity: celestial.moon_intensity,
            clear_sky_ambient_intensity: celestial.ambient_intensity,
            ground_temperature_c: surface.ground_temperature_c,
            water_temperature_c: surface.water_temperature_c,
            seasonal_snow_cover_at_600_m: surface.seasonal_snow_cover,
            polar_ice_closure: surface.polar_ice_closure,
            solid_ice_cover_at_600_m: surface.solid_ice_cover,
            tide_height_m: tide_height,
            surface_wind_m_s: weather.wind_velocity_m_s.into(),
            surface_wind_speed_m_s: weather.wind_velocity_m_s.length(),
            upper_wind_m_s: weather.upper_wind_velocity_m_s.into(),
            upper_wind_speed_m_s: weather.upper_wind_velocity_m_s.length(),
            wind_shear_m_s: weather.wind_shear_m_s.into(),
            wind_shear_speed_m_s: weather.wind_shear_m_s.length(),
            relative_humidity: weather.relative_humidity,
            instability: weather.instability,
            lift: weather.lift,
            low_cloudiness: weather.cloudiness,
            upper_cloudiness: weather.upper_cloudiness,
            rain_intensity: weather.rain_intensity,
        },
        image_metrics: image_metrics(rgba, options.width, options.height),
    }
}

fn image_metrics(rgba: &[u8], width: u32, height: u32) -> ImageMetrics {
    let pixel_count = rgba.len() as f32 / 4.0;
    let channel_mean = |channel: usize| {
        rgba.chunks_exact(4)
            .map(|pixel| f32::from(pixel[channel]) / 255.0)
            .sum::<f32>()
            / pixel_count
    };
    let warm_pixel_fraction = rgba
        .chunks_exact(4)
        .filter(|pixel| {
            let red = f32::from(pixel[0]);
            let green = f32::from(pixel[1]);
            let blue = f32::from(pixel[2]);
            red > 1.05 * green && red > 1.20 * blue && red > 24.0
        })
        .count() as f32
        / pixel_count;
    let pixel_rgb_step = |first: usize, second: usize| {
        (0..3)
            .map(|channel| {
                (f32::from(rgba[4 * first + channel]) - f32::from(rgba[4 * second + channel])).abs()
                    / 255.0
            })
            .sum::<f32>()
            / 3.0
    };
    let center_x = width as usize / 2;
    let center_y = height as usize / 2;
    let center_vertical_rgb_step = (0..height as usize)
        .map(|y| {
            pixel_rgb_step(
                y * width as usize + center_x.saturating_sub(1),
                y * width as usize + center_x,
            )
        })
        .sum::<f32>()
        / height as f32;
    let center_horizontal_rgb_step = (0..width as usize)
        .map(|x| {
            pixel_rgb_step(
                center_y.saturating_sub(1) * width as usize + x,
                center_y * width as usize + x,
            )
        })
        .sum::<f32>()
        / width as f32;
    let mut luminances: Vec<f32> = rgba
        .chunks_exact(4)
        .map(|pixel| {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
                / 255.0
        })
        .collect();
    let mean = luminances.iter().sum::<f32>() / luminances.len() as f32;
    let variance = luminances
        .iter()
        .map(|luminance| (luminance - mean).powi(2))
        .sum::<f32>()
        / luminances.len() as f32;
    luminances.sort_by(f32::total_cmp);
    let percentile = |amount: f32| luminances[(luminances.len() as f32 * amount) as usize];
    ImageMetrics {
        mean_luminance: mean,
        mean_red: channel_mean(0),
        mean_green: channel_mean(1),
        mean_blue: channel_mean(2),
        luminance_contrast: variance.sqrt(),
        luminance_p10: percentile(0.10),
        luminance_p50: percentile(0.50),
        luminance_p90: percentile(0.90),
        dark_pixel_fraction: luminances.iter().filter(|value| **value < 0.30).count() as f32
            / luminances.len() as f32,
        bright_pixel_fraction: luminances.iter().filter(|value| **value > 0.55).count() as f32
            / luminances.len() as f32,
        warm_pixel_fraction,
        center_vertical_rgb_step,
        center_horizontal_rgb_step,
    }
}

fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|error| format!("failed to write PNG header: {error}"))?
        .write_image_data(rgba)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}
