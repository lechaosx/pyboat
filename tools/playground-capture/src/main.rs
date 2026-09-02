use std::{env, fs, path::PathBuf};

use browser::{seasonal_phase_factors, visual_pixel_footprint_m};
use bytemuck::{Pod, Zeroable};
use game::{
    celestial_lighting, celestial_orbit, cloud_field_time_coordinates,
    procedural_world_environment, scalar_field_time_coordinates, vector_field_time_coordinates,
    world_celestial_conditions,
};
use serde::Serialize;
use simulation::WorldGeometry;
use wgpu::util::DeviceExt;

#[derive(Clone, Debug, PartialEq)]
struct ShaderSelection {
    id: String,
    module: Option<&'static str>,
    constants: &'static [(&'static str, f64)],
}

fn scene_selection() -> ShaderSelection {
    ShaderSelection {
        id: "scene".to_owned(),
        module: None,
        constants: &[],
    }
}

#[derive(Clone)]
struct Options {
    output: PathBuf,
    width: u32,
    height: u32,
    camera_x_m: f32,
    camera_y_m: f32,
    world_width_m: f32,
    world_height_m: f32,
    view_height_m: f32,
    seed: u64,
    cloud_time_seconds: f32,
    world_time_seconds: f64,
    diagnostic: ShaderSelection,
    light_balance: [f32; 4],
    cloud_shadows: [f32; 4],
    landscape_visibility: [f32; 4],
    light_contribution_visibility: [u32; 4],
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
    scalar_field_time_coordinates: [f32; 4],
    match_seed: [u32; 2],
    precipitation_epoch: [u32; 2],
    world_size_m: [f32; 2],
    inverse_world_size_m: [f32; 2],
    precipitation_phase_and_padding: [f32; 4],
    world_phases_and_pixel_footprint_m: [f32; 4],
    world_tick_and_padding: [u32; 4],
    pressure_time_coordinate: [f32; 4],
    ocean_current_time_coordinate_and_padding: [f32; 4],
    seasonal_phase_factors: [f32; 4],
    celestial_orbit_angles: [f32; 4],
    celestial_orbit_light: [f32; 4],
    view_cloud_motion_m: [f32; 4],
    cloud_time_coordinates: [f32; 4],
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
    cloud_lighting: [f32; 4],
    cloud_shadows: [f32; 4],
    water_surface: [f32; 4],
    ice_surface: [f32; 4],
    ice_structure: [f32; 4],
    land_material_normals: [f32; 4],
    material_coverage: [f32; 4],
    light_balance: [f32; 4],
    layer_visibility: [f32; 4],
    landscape_visibility: [f32; 4],
    wind_model: [f32; 4],
    climate_visibility: [f32; 4],
    light_contribution_visibility: [u32; 4],
    land_material_color_visibility: [u32; 4],
    solid_material_color_visibility: [u32; 4],
    water_material_color_visibility: [u32; 4],
    layer_material_color_visibility: [u32; 4],
    environment_features: [f32; 4],
    material_features: [f32; 4],
    effect_features: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CloudShadowCacheUniforms {
    viewport_size_px: [u32; 2],
    atlas_size_px: [u32; 2],
    table_mask: u32,
    maximum_tiles: u32,
    atlas_columns: u32,
    tile_texels: u32,
    padding: [u32; 4],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DerivedOutput {
    current_air_temperature_c: f32,
    sun_direction: [f32; 3],
    sun_intensity: f32,
    moon_direction: [f32; 3],
    moon_intensity: f32,
    moon_light_color: [f32; 3],
    solar_eclipse_fraction: f32,
    lunar_penumbra_fraction: f32,
    lunar_umbra_fraction: f32,
    clear_sky_ambient_intensity: f32,
    ground_temperature_c: f32,
    water_temperature_c: f32,
    seasonal_snow_cover: f32,
    polar_ice_closure: f32,
    solid_ice_cover: f32,
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
struct CaptureReport {
    image: String,
    width: u32,
    height: u32,
    camera_m: [f32; 2],
    world_size_m: [f32; 2],
    view_height_m: f32,
    seed: u64,
    cloud_time_seconds: f32,
    world_time_seconds: f64,
    diagnostic: String,
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
    let options = parse_options()?;
    pollster::block_on(render(&options))
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        output: PathBuf::from("playground-capture.png"),
        width: 960,
        height: 600,
        camera_x_m: 0.0,
        camera_y_m: 0.0,
        world_width_m: 64_000.0,
        world_height_m: 64_000.0,
        view_height_m: 480.0,
        seed: 0x5eed,
        cloud_time_seconds: 37.0,
        world_time_seconds: 0.0,
        diagnostic: scene_selection(),
        light_balance: [1.0; 4],
        cloud_shadows: [1.0, 1.0, 0.85, 1.0],
        landscape_visibility: [1.0; 4],
        light_contribution_visibility: [1; 4],
    };
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Skip<std::env::Args>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--output" => options.output = value(&mut arguments)?.into(),
            "--width" => options.width = parse(&value(&mut arguments)?, &argument)?,
            "--height" => options.height = parse(&value(&mut arguments)?, &argument)?,
            "--camera-x" => options.camera_x_m = parse(&value(&mut arguments)?, &argument)?,
            "--camera-y" => options.camera_y_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-width" => options.world_width_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-height" => options.world_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--view-height" => options.view_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--seed" => options.seed = parse_seed(&value(&mut arguments)?)?,
            "--cloud-time" => {
                options.cloud_time_seconds = parse(&value(&mut arguments)?, &argument)?
            }
            "--world-time" => {
                options.world_time_seconds = parse(&value(&mut arguments)?, &argument)?
            }
            "--diagnostic" => options.diagnostic = parse_diagnostic(&value(&mut arguments)?)?,
            "--light-balance" => {
                options.light_balance = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--cloud-shadows" => {
                options.cloud_shadows = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--landscape-visibility" => {
                options.landscape_visibility = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--light-contributions" => {
                options.light_contribution_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown option: {argument}")),
        }
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

fn parse_f32x4(value: &str, option: &str) -> Result<[f32; 4], String> {
    let values = value
        .split(',')
        .map(|value| parse(value, option))
        .collect::<Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| format!("{option} requires four comma-separated values"))
}

fn parse_u32x4(value: &str, option: &str) -> Result<[u32; 4], String> {
    let values = value
        .split(',')
        .map(|value| parse(value, option))
        .collect::<Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| format!("{option} requires four comma-separated values"))
}

fn diagnostic(
    id: &str,
    module: &'static str,
    constants: &'static [(&'static str, f64)],
) -> ShaderSelection {
    ShaderSelection {
        id: id.to_owned(),
        module: Some(module),
        constants,
    }
}

fn parse_diagnostic(value: &str) -> Result<ShaderSelection, String> {
    let selection = match value {
        "scene" => scene_selection(),
        "atmosphere_water_vapor" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 0.0)],
        ),
        "weather_relative_humidity" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 1.0)],
        ),
        "weather_instability" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 2.0)],
        ),
        "weather_lift" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 3.0)],
        ),
        "weather_low_coverage" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 4.0)],
        ),
        "weather_regional_condensation" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 7.0)],
        ),
        "weather_broad_pressure_lift" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 8.0)],
        ),
        "weather_storm_potential" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 9.0)],
        ),
        "weather_sky_fraction" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 10.0)],
        ),
        "weather_canopy_optical_thickness" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 5.0)],
        ),
        "weather_rain" => diagnostic(
            value,
            "atmosphere_conditions",
            &[("ATMOSPHERE_CONDITION_OUTPUT", 6.0)],
        ),
        "cloud_low_latent_texture" => diagnostic(value, "cloud_low_latent_texture", &[]),
        "cloud_low_optical_depth" => {
            diagnostic(value, "cloud_low_sample", &[("CLOUD_LOW_OUTPUT", 0.0)])
        }
        "cloud_low_normals" => diagnostic(value, "cloud_low_sample", &[("CLOUD_LOW_OUTPUT", 1.0)]),
        "cloud_weather_canopy_optical_depth" => {
            diagnostic(value, "cloud_weather_canopy_optical_depth", &[])
        }
        "lighting_projected_low_shadow" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 0.0)],
        ),
        "lighting_projected_upper_shadow" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 1.0)],
        ),
        "lighting_ambient_sky_occlusion" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 3.0)],
        ),
        "lighting_solar_elevation" => {
            diagnostic(value, "lighting_solar", &[("LIGHTING_SOLAR_OUTPUT", 0.0)])
        }
        "lighting_solar_eclipse_coverage" => {
            diagnostic(value, "lighting_solar", &[("LIGHTING_SOLAR_OUTPUT", 1.0)])
        }
        "lighting_low_cloud_sun_visibility" => {
            diagnostic(value, "lighting_solar", &[("LIGHTING_SOLAR_OUTPUT", 2.0)])
        }
        "lighting_canopy_sun_visibility" => {
            diagnostic(value, "lighting_solar", &[("LIGHTING_SOLAR_OUTPUT", 3.0)])
        }
        "lighting_surface_sun_amount" => {
            diagnostic(value, "lighting_solar", &[("LIGHTING_SOLAR_OUTPUT", 4.0)])
        }
        "lighting_receiver_moon_datum" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 5.0),
                ("LIGHTING_RECEIVER_HEIGHT_M", 0.0),
            ],
        ),
        "lighting_receiver_moon_low_cloud" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 5.0),
                ("LIGHTING_RECEIVER_HEIGHT_M", 900.0),
            ],
        ),
        "lighting_receiver_moon_canopy" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 5.0),
                ("LIGHTING_RECEIVER_HEIGHT_M", 3_600.0),
            ],
        ),
        "lighting_canopy_transfer_clear" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 0.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 0.0),
            ],
        ),
        "lighting_canopy_transfer_zero" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 1.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 0.0),
            ],
        ),
        "lighting_canopy_transfer_thick" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 1.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 6.0),
            ],
        ),
        "lighting_low_cloud_canopy_transmittance" => {
            diagnostic(value, "lighting_low_cloud_canopy_transmittance", &[])
        }
        "lighting_low_cloud_sun_lighting" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 0.0)],
        ),
        "lighting_low_cloud_sun_lighting_elevated" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 0.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
            ],
        ),
        "lighting_landform_sun_grade_moon_coupling" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 1.0)],
        ),
        "cloud_shadow_cache_tiles" => ShaderSelection {
            id: value.to_owned(),
            module: Some("cloud_shadow_cache"),
            constants: &[],
        },
        "lighting_aerosol_optical_depth" => {
            diagnostic(value, "lighting_aerosol_optical_depth", &[])
        }
        "lighting_canopy_moon_scattering" => {
            diagnostic(value, "lighting_canopy_moon_scattering", &[])
        }
        "temperature_gradient" => diagnostic(value, "temperature_gradient", &[]),
        "surface_air_temperature" => diagnostic(value, "surface_air_temperature", &[]),
        "pressure_broad_anomaly" => diagnostic(
            value,
            "pressure_anomaly",
            &[("PRESSURE_ANOMALY_OUTPUT", 0.0)],
        ),
        "pressure_full_anomaly" => diagnostic(
            value,
            "pressure_anomaly",
            &[("PRESSURE_ANOMALY_OUTPUT", 1.0)],
        ),
        "pressure_synoptic_anomaly" => diagnostic(
            value,
            "pressure_anomaly",
            &[("PRESSURE_ANOMALY_OUTPUT", 2.0)],
        ),
        "pressure_local_anomaly" => diagnostic(
            value,
            "pressure_anomaly",
            &[("PRESSURE_ANOMALY_OUTPUT", 3.0)],
        ),
        "wind_surface" => diagnostic(
            value,
            "wind_surface_motion",
            &[("WIND_SURFACE_OUTPUT", 0.0)],
        ),
        "wind_shear" => diagnostic(
            value,
            "wind_surface_motion",
            &[("WIND_SURFACE_OUTPUT", 1.0)],
        ),
        "wind_surface_gust" => diagnostic(
            value,
            "wind_surface_motion",
            &[("WIND_SURFACE_OUTPUT", 2.0)],
        ),
        "wind_upper" => diagnostic(value, "wind_flow", &[("WIND_FLOW_OUTPUT", 0.0)]),
        "wind_resolved_surface" => diagnostic(value, "wind_flow", &[("WIND_FLOW_OUTPUT", 1.0)]),
        "wind_broad_synoptic" => diagnostic(value, "wind_flow", &[("WIND_FLOW_OUTPUT", 2.0)]),
        "terrain_elevation" => diagnostic(
            value,
            "terrain_geometry",
            &[("TERRAIN_GEOMETRY_OUTPUT", 0.0)],
        ),
        "climate_solid_ice" => {
            diagnostic(value, "climate_surface", &[("CLIMATE_SURFACE_OUTPUT", 0.0)])
        }
        "climate_autumn_vegetation" => {
            diagnostic(value, "climate_surface", &[("CLIMATE_SURFACE_OUTPUT", 1.0)])
        }
        "climate_spatial_snow" => {
            diagnostic(value, "climate_surface", &[("CLIMATE_SURFACE_OUTPUT", 2.0)])
        }
        "climate_composition" => {
            diagnostic(value, "climate_surface", &[("CLIMATE_SURFACE_OUTPUT", 3.0)])
        }
        "water_tide_height" => diagnostic(value, "water_tide_height", &[]),
        _ => return Err(format!("unknown diagnostic: {value}")),
    };
    Ok(selection)
}

fn print_help() {
    println!(
        "playground-capture [options]\n\n\
         Options:\n\
           --output PATH          output path (default playground-capture.png)\n\
           --width N --height N   output dimensions (default 960x600)\n\
           --camera-x M --camera-y M  world-space camera centre\n\
           --world-width M --world-height M  longitude circumference and pole span (default 64000 each)\n\
           --view-height METRES   vertical world span (default 480)\n\
           --seed N|0xHEX         deterministic visual seed\n\
           --cloud-time SECONDS   deterministic cloud animation time (default 37)\n\
           --world-time SECONDS   deterministic world time (default 0)\n\
           --diagnostic NAME      semantic diagnostic output (default scene)\n\n\
           --light-balance V,V,V,V  Sun, Moon, clear-sky, and canopy light scales\n\
           --light-contributions V,V,V,V  enable the four light contributions\n\
           --cloud-shadows V,V,V,V  low/canopy strength, extinction, contrast\n\
           --landscape-visibility V,V,V,V  landscape mechanism visibility\n\n\
         The PNG gets a JSON sidecar containing world inputs, derived conditions,\n\
         and image metrics."
    );
}

#[cfg(test)]
mod tests {
    use super::parse_diagnostic;

    #[test]
    fn diagnostics_select_semantic_shader_modules_and_overrides() {
        let pressure = parse_diagnostic("pressure_full_anomaly").unwrap();
        assert_eq!(pressure.module, Some("pressure_anomaly"));
        assert_eq!(pressure.constants, &[("PRESSURE_ANOMALY_OUTPUT", 1.0)]);
        assert!(parse_diagnostic("cloud_low_amount").is_err());
        assert!(parse_diagnostic("59").is_err());
    }
}

async fn render(options: &Options) -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            ..Default::default()
        })
        .await
        .map_err(|error| format!("no headless Vulkan adapter available: {error}"))?;
    let adapter_info = adapter.get_info();
    if !capture_adapter_is_hardware(adapter_info.device_type) {
        return Err(format!(
            "hardware Vulkan GPU required; refusing adapter {} ({:?})",
            adapter_info.name, adapter_info.device_type
        ));
    }
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
    let fragment_module = options
        .diagnostic
        .module
        .filter(|name| *name != "cloud_shadow_cache");
    let shader_source = fragment_module
        .map_or_else(browser::playground_direct_scene_shader_source, |name| {
            browser::playground_diagnostic_shader_source(name).unwrap()
        });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Scene shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    let scene_layout = uniform_layout(&device, "scene uniforms layout");
    let playground_layout = uniform_layout(&device, "playground uniforms layout");
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("playground capture pipeline layout"),
        bind_group_layouts: &[Some(&scene_layout), Some(&playground_layout)],
        immediate_size: 0,
    });
    let fragment_entry = fragment_module.map_or_else(
        || "fragment_playground_scene".to_owned(),
        |name| format!("fragment_{name}"),
    );
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
            entry_point: Some(&fragment_entry),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: options.diagnostic.constants,
                ..Default::default()
            },
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
        cloud_wisps: [0.25, 0.0, 1.0, 1.0],
        cloud_rendering: [1.0, 1.0, 1.0, 0.0],
        terrain_tuning: [1.0, 3.0, 0.08, 0.0],
        water_tuning: [0.5, 1.0, 0.0, 0.0],
        landform_lighting: [1.0, 0.7, 320.0, 0.0],
        cloud_lighting: [0.18, 0.14, 0.0, 0.0],
        cloud_shadows: options.cloud_shadows,
        water_surface: [2.5, 0.2, 1.0, 1.0],
        ice_surface: [0.18, 0.32, 1.0, 1.0],
        ice_structure: [192.0, 1.0, 1.0, 1.0],
        land_material_normals: [1.0; 4],
        material_coverage: [1.0; 4],
        light_balance: options.light_balance,
        layer_visibility: [1.0; 4],
        landscape_visibility: options.landscape_visibility,
        wind_model: [1.0; 4],
        climate_visibility: [1.0, 1.0, 0.0, 1.0],
        light_contribution_visibility: options.light_contribution_visibility,
        land_material_color_visibility: [1; 4],
        solid_material_color_visibility: [1; 4],
        water_material_color_visibility: [1; 4],
        layer_material_color_visibility: [1; 4],
        environment_features: [1.0; 4],
        material_features: [1.0; 4],
        effect_features: [1.0; 4],
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

    let path = &options.output;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let rgba = render_world(
        &device,
        &queue,
        &pipeline,
        &scene_layout,
        &playground_layout,
        &playground_bind_group,
        &vertex_buffer,
        options,
    )?;
    write_png(path, options.width, options.height, &rgba)?;
    let report = capture_report(options, path, &rgba)?;
    let report_json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to serialize report: {error}"))?;
    let sidecar_path = path.with_extension("json");
    fs::write(&sidecar_path, &report_json)
        .map_err(|error| format!("failed to write {}: {error}", sidecar_path.display()))?;
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}

fn capture_adapter_is_hardware(device_type: wgpu::DeviceType) -> bool {
    matches!(
        device_type,
        wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::VirtualGpu
    )
}

fn uniform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
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
fn render_world(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    scene_layout: &wgpu::BindGroupLayout,
    playground_layout: &wgpu::BindGroupLayout,
    playground_bind_group: &wgpu::BindGroup,
    vertex_buffer: &wgpu::Buffer,
    options: &Options,
) -> Result<Vec<u8>, String> {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let canonical_camera_m =
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let precipitation_epoch = (options.cloud_time_seconds / 1800.0).floor() as u64;
    let precipitation_phase = options.cloud_time_seconds.rem_euclid(1800.0) / 1800.0;
    let tick = (options.world_time_seconds / f64::from(simulation::DT_SECONDS)) as u64;
    let conditions =
        world_celestial_conditions(world_geometry, options.world_time_seconds, [0.0; 2].into());
    let orbit = celestial_orbit(conditions);
    let cloud_tick = (options.cloud_time_seconds / simulation::DT_SECONDS) as u64;
    let scalar_time = scalar_field_time_coordinates(cloud_tick);
    let pressure_time = vector_field_time_coordinates(world_geometry, cloud_tick);
    let current_time = vector_field_time_coordinates(world_geometry, tick);
    let cloud_time = cloud_field_time_coordinates(cloud_tick);
    let view_weather = procedural_world_environment(
        world_geometry,
        options.seed,
        cloud_tick,
        canonical_camera_m.into(),
    )
    .weather;
    let low_wind = view_weather.wind_velocity_m_s - view_weather.surface_gust_velocity_m_s;
    let uniforms = SceneUniforms {
        camera_and_view_m: [
            canonical_camera_m[0],
            canonical_camera_m[1],
            options.view_height_m * options.width as f32 / options.height as f32,
            options.view_height_m,
        ],
        scalar_field_time_coordinates: [
            scalar_time.air_temperature,
            scalar_time.upper_air_temperature,
            scalar_time.water_vapor,
            scalar_time.decorative_water,
        ],
        match_seed: [options.seed as u32, (options.seed >> 32) as u32],
        precipitation_epoch: [
            precipitation_epoch as u32,
            (precipitation_epoch >> 32) as u32,
        ],
        world_size_m: [options.world_width_m, options.world_height_m],
        inverse_world_size_m: [
            options.world_width_m.recip(),
            options.world_height_m.recip(),
        ],
        precipitation_phase_and_padding: [precipitation_phase, 0.0, 0.0, 0.0],
        world_phases_and_pixel_footprint_m: [
            conditions.day_phase,
            conditions.season_phase,
            conditions.lunar_longitude_phase,
            visual_pixel_footprint_m(options.view_height_m, options.height),
        ],
        world_tick_and_padding: [tick as u32, (tick >> 32) as u32, 0, 0],
        pressure_time_coordinate: pressure_time.pressure,
        ocean_current_time_coordinate_and_padding: [current_time.ocean_current, 0.0, 0.0, 0.0],
        seasonal_phase_factors: seasonal_phase_factors(conditions.season_phase),
        celestial_orbit_angles: [
            orbit.solar_declination_rad as f32,
            orbit.solar_right_ascension_rad as f32,
            orbit.lunar_declination_rad as f32,
            orbit.lunar_right_ascension_rad as f32,
        ],
        celestial_orbit_light: [
            orbit.illuminated_fraction as f32,
            orbit.lunar_penumbra_fraction,
            orbit.lunar_umbra_fraction,
            0.0,
        ],
        view_cloud_motion_m: [
            low_wind.x * options.cloud_time_seconds,
            low_wind.y * options.cloud_time_seconds,
            view_weather.upper_wind_velocity_m_s.x * options.cloud_time_seconds,
            view_weather.upper_wind_velocity_m_s.y * options.cloud_time_seconds,
        ],
        cloud_time_coordinates: [cloud_time.low, cloud_time.upper, 0.0, 0.0],
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
    if options.diagnostic.module == Some("cloud_shadow_cache") {
        return discover_cloud_shadow_cache_tiles(
            device,
            queue,
            scene_layout,
            playground_layout,
            &scene_bind_group,
            playground_bind_group,
            options.width,
            options.height,
        );
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("procedural world capture"),
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

#[allow(clippy::too_many_arguments)]
fn discover_cloud_shadow_cache_tiles(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene_layout: &wgpu::BindGroupLayout,
    playground_layout: &wgpu::BindGroupLayout,
    scene_bind_group: &wgpu::BindGroup,
    playground_bind_group: &wgpu::BindGroup,
    viewport_width: u32,
    viewport_height: u32,
) -> Result<Vec<u8>, String> {
    let layout = browser::cloud_shadow_cache_layout(
        viewport_width,
        viewport_height,
        device.limits().max_texture_dimension_2d,
    );
    let cache_uniforms = CloudShadowCacheUniforms {
        viewport_size_px: [viewport_width, viewport_height],
        atlas_size_px: [layout.atlas_width, layout.atlas_height],
        table_mask: layout.table_entries - 1,
        maximum_tiles: layout.maximum_tiles,
        atlas_columns: layout.atlas_columns,
        tile_texels: browser::CLOUD_SHADOW_CACHE_TILE_TEXELS,
        padding: [0; 4],
    };
    let cache_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("capture cloud shadow cache uniforms"),
        contents: bytemuck::bytes_of(&cache_uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let header = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("capture cloud shadow cache header"),
        contents: bytemuck::cast_slice(&[6u32, 0, 0, 0]),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    });
    let entries = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture cloud shadow cache entries"),
        size: u64::from(layout.table_entries) * 20,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let tiles = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture cloud shadow cache tiles"),
        size: u64::from(layout.maximum_tiles) * 16,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let buffer_layout_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let cache_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("capture cloud shadow cache discovery layout"),
        entries: &[
            buffer_layout_entry(0, wgpu::BufferBindingType::Uniform),
            buffer_layout_entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer_layout_entry(2, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer_layout_entry(3, wgpu::BufferBindingType::Storage { read_only: false }),
        ],
    });
    let cache_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("capture cloud shadow cache discovery bind group"),
        layout: &cache_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: cache_uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: header.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: entries.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: tiles.as_entire_binding(),
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("capture cloud shadow cache shader"),
        source: wgpu::ShaderSource::Wgsl(
            browser::playground_cloud_shadow_cache_shader_source().into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("capture cloud shadow cache discovery pipeline layout"),
        bind_group_layouts: &[
            Some(scene_layout),
            Some(playground_layout),
            Some(&cache_layout),
        ],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("capture cloud shadow cache discovery pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("discover_cloud_shadow_cache_tiles"),
        compilation_options: Default::default(),
        cache: None,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture cloud shadow cache header readback"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("capture cloud shadow cache discovery encoder"),
    });
    encoder.clear_buffer(&entries, 0, None);
    encoder.clear_buffer(&header, 4, Some(4));
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("capture cloud shadow cache discovery pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, playground_bind_group, &[]);
        pass.set_bind_group(2, &cache_bind_group, &[]);
        pass.dispatch_workgroups(
            viewport_width.div_ceil(128),
            viewport_height.div_ceil(128),
            1,
        );
    }
    encoder.copy_buffer_to_buffer(&header, 0, &readback, 0, 16);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("cache discovery GPU poll failed: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("cache discovery readback stopped: {error}"))?
        .map_err(|error| format!("cache discovery readback failed: {error}"))?;
    let data = slice
        .get_mapped_range()
        .map_err(|error| format!("failed to read cache discovery header: {error}"))?;
    let tile_count = u32::from_ne_bytes(data[4..8].try_into().unwrap());
    let pixel = if tile_count == 0 {
        [0, 0, 0, 255]
    } else {
        [255, 255, 255, 255]
    };
    Ok((0..viewport_width * viewport_height)
        .flat_map(|_| pixel)
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

fn capture_report(
    options: &Options,
    path: &std::path::Path,
    rgba: &[u8],
) -> Result<CaptureReport, String> {
    let world = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let tick = (options.world_time_seconds / f64::from(simulation::DT_SECONDS)) as u64;
    let sample = procedural_world_environment(
        world,
        options.seed,
        tick,
        [options.camera_x_m, options.camera_y_m].into(),
    );
    let celestial = celestial_lighting(sample.celestial);
    Ok(CaptureReport {
        image: path.display().to_string(),
        width: options.width,
        height: options.height,
        camera_m: [options.camera_x_m, options.camera_y_m],
        world_size_m: [options.world_width_m, options.world_height_m],
        view_height_m: options.view_height_m,
        seed: options.seed,
        cloud_time_seconds: options.cloud_time_seconds,
        world_time_seconds: options.world_time_seconds,
        diagnostic: options.diagnostic.id.clone(),
        derived: DerivedOutput {
            current_air_temperature_c: sample.weather.surface_air_temperature_c,
            sun_direction: celestial.sun_direction,
            sun_intensity: celestial.sun_intensity,
            moon_direction: celestial.moon_direction,
            moon_intensity: celestial.moon_intensity,
            moon_light_color: celestial.moon_color,
            solar_eclipse_fraction: celestial.solar_eclipse_fraction,
            lunar_penumbra_fraction: celestial.lunar_penumbra_fraction,
            lunar_umbra_fraction: celestial.lunar_umbra_fraction,
            clear_sky_ambient_intensity: celestial.ambient_intensity,
            ground_temperature_c: sample.surface_climate.ground_temperature_c,
            water_temperature_c: sample.surface_climate.water_temperature_c,
            seasonal_snow_cover: sample.surface_climate.seasonal_snow_cover,
            polar_ice_closure: sample.surface_climate.polar_ice_closure,
            solid_ice_cover: sample.surface_climate.solid_ice_cover,
            tide_height_m: sample.water_height_m,
            surface_wind_m_s: sample.weather.wind_velocity_m_s.into(),
            surface_wind_speed_m_s: sample.weather.wind_velocity_m_s.length(),
            upper_wind_m_s: sample.weather.upper_wind_velocity_m_s.into(),
            upper_wind_speed_m_s: sample.weather.upper_wind_velocity_m_s.length(),
            wind_shear_m_s: sample.weather.wind_shear_m_s.into(),
            wind_shear_speed_m_s: sample.weather.wind_shear_m_s.length(),
            relative_humidity: sample.weather.relative_humidity,
            instability: sample.weather.instability,
            lift: sample.weather.lift,
            low_cloudiness: sample.weather.cloudiness,
            upper_cloudiness: sample.weather.upper_cloudiness,
            rain_intensity: sample.weather.rain_intensity,
        },
        image_metrics: image_metrics(rgba, options.width, options.height),
    })
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
