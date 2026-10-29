use std::{env, fs, path::PathBuf};

use browser::{
    ADAPTING_FIELD_TEXELS, AdaptingField, BakedSurface, BakedSurfaceFrame, BakedSurfacePass,
    BakedSurfaceSizes, BakedSurfaceTextures, ENVIRONMENT_FIELD_LAYERS, ENVIRONMENT_FIELD_TEXELS,
    FieldGridHistory, FieldGridTuning, FieldGrids, GraphicsQuality, LANDFORM_HORIZON_LAYER,
    OCCLUDER_FIELD_LAYERS, OCCLUDER_FIELD_TEXELS, TerrainCache, TerrainCacheFrame,
    TerrainCachePass, baked_surface_sizes, exposure_target, field_grids, ocean_wave_mode_uniforms,
    sample_view_cloud_state, seasonal_phase_factors, view_width_m, visual_pixel_footprint_m,
};
use bytemuck::{Pod, Zeroable};
use game::{
    OCEAN_WAVE_MODE_COUNT, celestial_lighting, celestial_orbit,
    cloud_field_time_coordinates_interpolated, procedural_world_environment_interpolated,
    scalar_field_time_coordinates_interpolated, vector_field_time_coordinates_interpolated,
    world_celestial_conditions,
};
use serde::Serialize;
use simulation::{TICKS_PER_SECOND, WorldGeometry};
use wgpu::util::DeviceExt;

mod bench;

const MAXIMUM_CAPTURE_PIXELS: u64 = 4096 * 4096;

/// The scene's binding of the adapting field.
const ADAPTING_FIELD_LAYOUT_ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
    binding: 7,
    visibility: wgpu::ShaderStages::FRAGMENT,
    ty: wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: false },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    },
    count: None,
};

#[derive(Clone, Debug, PartialEq)]
struct ShaderSelection {
    id: String,
    module: Option<&'static str>,
    constants: Vec<(&'static str, f64)>,
}

fn scene_selection() -> ShaderSelection {
    ShaderSelection {
        id: "scene".to_owned(),
        module: None,
        constants: Vec::new(),
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
    world_time_seconds: f64,
    /// Seconds of renderer-local cloud transport accumulated before the capture.
    cloud_motion_seconds: f64,
    diagnostic: ShaderSelection,
    quality: GraphicsQuality,
    legacy_shadow_look: bool,
    cloud_shadows: f32,
    landform_lighting: [f32; 4],
    landscape_visibility: [f32; 4],
    light_contribution_visibility: [u32; 4],
    land_material_color_visibility: [u32; 4],
    solid_material_color_visibility: [u32; 4],
    /// Camera x, camera y, and view height of earlier frames whose terrain
    /// cache fills the requested view's frame reuses.
    prior_views: Vec<[f32; 3]>,
    skip_prior_fills: bool,
    /// Adapts the whole view to the clear-sky light at its centre instead of
    /// its illumination, so lab switches that change the light leave exposure
    /// alone.
    fixed_adaptation: bool,
    bake_memory_bytes: u64,
    /// Peak of the tone curve in diffuse whites: 1 for SDR output, the game's
    /// HDR output peak otherwise. Values above 1 clip in the 8-bit image.
    output_peak: f32,
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
    precipitation_phase_and_css_view_height_px: [f32; 4],
    world_phases_and_pixel_footprint_m: [f32; 4],
    pressure_time_coordinate: [f32; 4],
    ocean_current_time_coordinate_and_padding: [f32; 4],
    seasonal_phase_factors: [f32; 4],
    celestial_orbit_angles: [f32; 4],
    celestial_orbit_light: [f32; 4],
    view_cloud_motion_m: [f32; 4],
    cloud_time_coordinates: [f32; 4],
    exposure_and_output: [f32; 4],
    environment_field_grid_m: [f32; 4],
    environment_field_texels: [u32; 4],
    cloud_weather_grid_m: [f32; 4],
    cloud_weather_texels: [u32; 4],
    cloud_shadow_grids: [f32; 4],
    occluder_field_grid_m: [[f32; 4]; OCCLUDER_FIELD_LAYERS],
    occluder_field_texels: [[u32; 4]; OCCLUDER_FIELD_LAYERS],
    terrain_cache_grid_m: [f32; 4],
    terrain_cache_texels: [u32; 4],
    terrain_cache_level: [u32; 4],
    baked_surface_grid_m: [f32; 4],
    baked_surface_texels: [u32; 4],
    baked_surface_level: [u32; 4],
    ocean_wave_modes: [[f32; 4]; OCEAN_WAVE_MODE_COUNT],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PlaygroundUniforms {
    water_tuning: [f32; 4],
    landform_lighting: [f32; 4],
    cloud_shadows: [f32; 4],
    layer_visibility: [f32; 4],
    landscape_visibility: [f32; 4],
    light_contribution_visibility: [u32; 4],
    land_material_color_visibility: [u32; 4],
    solid_material_color_visibility: [u32; 4],
    water_material_color_visibility: [u32; 4],
    environment_features: [f32; 4],
    material_features: [f32; 4],
    effect_features: [f32; 4],
    occluder_field_inspection: [u32; 4],
}

struct CaptureFields {
    scene_layout: wgpu::BindGroupLayout,
    field_reading_layout: wgpu::BindGroupLayout,
    environment_view: wgpu::TextureView,
    occluder_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    environment_pipeline: wgpu::ComputePipeline,
    occluder_pipeline: wgpu::ComputePipeline,
    landform_horizon_layout: wgpu::BindGroupLayout,
    landform_horizon_input_view: wgpu::TextureView,
    landform_horizon_pipeline: wgpu::ComputePipeline,
    terrain_cache: TerrainCachePass,
    baked_surface: BakedSurfacePass,
    environment_output: wgpu::BindGroup,
    occluder_output: wgpu::BindGroup,
    landform_horizon_output: wgpu::BindGroup,
    _textures: [wgpu::Texture; 2],
}

struct CaptureFieldBindings {
    scene: wgpu::BindGroup,
    field_reading: wgpu::BindGroup,
    landform_horizon_input: wgpu::BindGroup,
    baked_surface_input: wgpu::BindGroup,
}

impl CaptureFields {
    /// `group_1_layout` is the playground uniform layout, or any layout for the
    /// production generators, which leave group 1 empty.
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &wgpu::Device,
        scene_uniform_layout: &wgpu::BindGroupLayout,
        group_1_layout: &wgpu::BindGroupLayout,
        occluder_shader_source: &str,
        occluder_constants: &[(&str, f64)],
        landform_horizon_shader_source: &str,
        landform_horizon_constants: &[(&str, f64)],
        baked_surface_shader_source: &str,
        baked_surface_constants: &[(&str, f64)],
        baked_surface_sizes: BakedSurfaceSizes,
    ) -> Self {
        let uniform_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let field_entry = |binding, visibility, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniforms and field grids layout"),
            entries: &[
                uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT),
                field_entry(1, wgpu::ShaderStages::FRAGMENT, false),
                field_entry(2, wgpu::ShaderStages::FRAGMENT, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                field_entry(4, wgpu::ShaderStages::FRAGMENT, false),
                field_entry(5, wgpu::ShaderStages::FRAGMENT, false),
                field_entry(6, wgpu::ShaderStages::FRAGMENT, false),
                ADAPTING_FIELD_LAYOUT_ENTRY,
            ],
        });
        let field_reading_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene uniforms and environment fields layout"),
                entries: &[
                    uniform_entry(wgpu::ShaderStages::COMPUTE),
                    field_entry(1, wgpu::ShaderStages::COMPUTE, false),
                ],
            });
        let landform_horizon_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene uniforms and landform layer layout"),
                entries: &[
                    uniform_entry(wgpu::ShaderStages::COMPUTE),
                    field_entry(2, wgpu::ShaderStages::COMPUTE, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let field_texture = |label, size, layers, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let environment_texture = field_texture(
            "capture environment fields",
            ENVIRONMENT_FIELD_TEXELS,
            ENVIRONMENT_FIELD_LAYERS,
            wgpu::TextureFormat::Rgba32Float,
        );
        let occluder_texture = field_texture(
            "capture occluder fields",
            OCCLUDER_FIELD_TEXELS,
            OCCLUDER_FIELD_LAYERS as u32,
            wgpu::TextureFormat::Rgba16Float,
        );
        let array_view = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let environment_view = array_view(&environment_texture);
        let occluder_view = array_view(&occluder_texture);
        let occluder_layers_view = |layers: std::ops::Range<usize>| {
            occluder_texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_array_layer: layers.start as u32,
                array_layer_count: Some(layers.len() as u32),
                ..Default::default()
            })
        };
        let occluder_output_view = occluder_layers_view(0..LANDFORM_HORIZON_LAYER);
        let landform_horizon_input_view = occluder_layers_view(0..LANDFORM_HORIZON_LAYER);
        let landform_horizon_output_view =
            occluder_layers_view(LANDFORM_HORIZON_LAYER..OCCLUDER_FIELD_LAYERS);
        let output_layout = |label, format| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                }],
            })
        };
        let environment_output_layout = output_layout(
            "capture environment field output layout",
            wgpu::TextureFormat::Rgba32Float,
        );
        let occluder_output_layout = output_layout(
            "capture occluder field output layout",
            wgpu::TextureFormat::Rgba16Float,
        );
        let compute_pipeline = |label,
                                source: &str,
                                layouts: &[Option<&wgpu::BindGroupLayout>],
                                entry_point,
                                constants: &[(&str, f64)]| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants,
                    ..Default::default()
                },
                cache: None,
            })
        };
        let environment_pipeline = compute_pipeline(
            "capture environment field pipeline",
            browser::environment_field_shader_source(),
            &[Some(scene_uniform_layout), Some(&environment_output_layout)],
            "generate_environment_fields",
            &[],
        );
        let occluder_pipeline = compute_pipeline(
            "capture occluder field pipeline",
            occluder_shader_source,
            &[
                Some(&field_reading_layout),
                Some(group_1_layout),
                Some(&occluder_output_layout),
            ],
            "generate_occluder_fields",
            occluder_constants,
        );
        let landform_horizon_pipeline = compute_pipeline(
            "capture landform horizon pipeline",
            landform_horizon_shader_source,
            &[
                Some(&landform_horizon_layout),
                Some(group_1_layout),
                Some(&occluder_output_layout),
            ],
            "generate_landform_horizons",
            landform_horizon_constants,
        );
        let output_bind_group = |label, layout, view| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                }],
            })
        };
        Self {
            environment_output: output_bind_group(
                "capture environment field output",
                &environment_output_layout,
                &environment_view,
            ),
            occluder_output: output_bind_group(
                "capture occluder field output",
                &occluder_output_layout,
                &occluder_output_view,
            ),
            landform_horizon_output: output_bind_group(
                "capture landform horizon output",
                &occluder_output_layout,
                &landform_horizon_output_view,
            ),
            landform_horizon_layout,
            landform_horizon_input_view,
            landform_horizon_pipeline,
            scene_layout,
            field_reading_layout,
            environment_view,
            occluder_view,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("capture occluder field sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            environment_pipeline,
            occluder_pipeline,
            terrain_cache: TerrainCachePass::new(device, scene_uniform_layout),
            baked_surface: BakedSurfacePass::new(
                device,
                BakedSurfaceTextures::new(device, baked_surface_sizes),
                baked_surface_shader_source,
                baked_surface_constants,
                group_1_layout,
            ),
            _textures: [environment_texture, occluder_texture],
        }
    }

    fn bindings(
        &self,
        device: &wgpu::Device,
        scene_buffer: &wgpu::Buffer,
        adapting_field: &AdaptingField,
    ) -> CaptureFieldBindings {
        let uniforms = wgpu::BindGroupEntry {
            binding: 0,
            resource: scene_buffer.as_entire_binding(),
        };
        let environment = wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&self.environment_view),
        };
        let baked = self.baked_surface.textures();
        CaptureFieldBindings {
            scene: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene uniforms and field grids"),
                layout: &self.scene_layout,
                entries: &[
                    uniforms.clone(),
                    environment.clone(),
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.occluder_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(
                            &self.terrain_cache.noise_view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&baked.height_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&baked.color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&adapting_field.view),
                    },
                ],
            }),
            landform_horizon_input: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene uniforms and landform layer"),
                layout: &self.landform_horizon_layout,
                entries: &[
                    uniforms.clone(),
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &self.landform_horizon_input_view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }),
            field_reading: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene uniforms and environment fields"),
                layout: &self.field_reading_layout,
                entries: &[uniforms, environment],
            }),
            baked_surface_input: self.baked_surface.input_bind_group(
                device,
                scene_buffer,
                &self.terrain_cache.noise_view,
            ),
        }
    }

    /// Bakes the texels `baked_surface` schedules; timestamps, when requested,
    /// go to query indices 10 and 11.
    fn encode_baked_surface(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bindings: &CaptureFieldBindings,
        group_1: &wgpu::BindGroup,
        baked_surface: &BakedSurfaceFrame,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        self.baked_surface.encode(
            queue,
            encoder,
            &bindings.baked_surface_input,
            group_1,
            baked_surface,
            timestamps.map(|query_set| wgpu::ComputePassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(10),
                end_of_pass_write_index: Some(11),
            }),
        );
    }

    /// Evaluates the texels `terrain_cache` schedules; timestamps, when
    /// requested, go to query indices 8 and 9.
    fn encode_terrain_cache(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_uniform_bind_group: &wgpu::BindGroup,
        terrain_cache: &TerrainCacheFrame,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        self.terrain_cache.encode(
            queue,
            encoder,
            scene_uniform_bind_group,
            terrain_cache,
            timestamps.map(|query_set| wgpu::ComputePassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(8),
                end_of_pass_write_index: Some(9),
            }),
        );
    }

    /// Timestamps, when requested, go to query indices 0 to 3.
    #[allow(clippy::too_many_arguments)]
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_uniform_bind_group: &wgpu::BindGroup,
        bindings: &CaptureFieldBindings,
        occluder_group_1: &wgpu::BindGroup,
        grids: &FieldGrids,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        let timestamp_writes = |first| {
            timestamps.map(|query_set| wgpu::ComputePassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(first),
                end_of_pass_write_index: Some(first + 1),
            })
        };
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("capture environment field pass"),
                timestamp_writes: timestamp_writes(0),
            });
            pass.set_pipeline(&self.environment_pipeline);
            pass.set_bind_group(0, scene_uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.environment_output, &[]);
            let [columns, rows] = grids.environment_workgroups();
            pass.dispatch_workgroups(columns, rows, 1);
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("capture occluder field pass"),
            timestamp_writes: timestamp_writes(2),
        });
        pass.set_pipeline(&self.occluder_pipeline);
        pass.set_bind_group(0, &bindings.field_reading, &[]);
        pass.set_bind_group(1, occluder_group_1, &[]);
        pass.set_bind_group(2, &self.occluder_output, &[]);
        let [columns, rows, layers] = grids.occluder_workgroups();
        pass.dispatch_workgroups(columns, rows, layers);
        pass.set_pipeline(&self.landform_horizon_pipeline);
        pass.set_bind_group(0, &bindings.landform_horizon_input, &[]);
        pass.set_bind_group(2, &self.landform_horizon_output, &[]);
        let [columns, rows, layers] = grids.landform_horizon_workgroups();
        pass.dispatch_workgroups(columns, rows, layers);
    }
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
    /// Scene light of the adapting field at the view's centre, for scene
    /// compositions.
    adapting_light: Option<f32>,
    /// Exposure at the view's centre: of its adapting light, or else of its
    /// clear-sky light.
    settled_exposure_multiplier: f32,
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
    world_time_seconds: f64,
    cloud_motion_seconds: f64,
    view_cloud_motion_m: [f32; 4],
    diagnostic: String,
    quality: &'static str,
    legacy_shadow_look: bool,
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
    match parse_options()? {
        Command::Capture(options) => pollster::block_on(render(&options)),
        Command::Bench(options) => pollster::block_on(bench::run(&options)),
    }
}

enum Command {
    Capture(Box<Options>),
    Bench(bench::BenchOptions),
}

fn default_options() -> Options {
    Options {
        output: PathBuf::from("playground-capture.png"),
        width: 960,
        height: 600,
        camera_x_m: 0.0,
        camera_y_m: 0.0,
        world_width_m: 64_000.0,
        world_height_m: 64_000.0,
        view_height_m: 480.0,
        seed: 0x5eed,
        world_time_seconds: 37.0,
        cloud_motion_seconds: 0.0,
        diagnostic: scene_selection(),
        quality: GraphicsQuality::High,
        legacy_shadow_look: false,
        cloud_shadows: 1.0,
        landform_lighting: [1.0, 1.0, 320.0, 0.0],
        landscape_visibility: [1.0; 4],
        light_contribution_visibility: [1; 4],
        land_material_color_visibility: [1; 4],
        solid_material_color_visibility: [1; 4],
        prior_views: Vec::new(),
        skip_prior_fills: false,
        fixed_adaptation: false,
        // A desktop's, without the raise for a large reported device memory.
        bake_memory_bytes: browser::baked_surface_memory_bytes(false, None),
        output_peak: 1.0,
    }
}

fn parse_options() -> Result<Command, String> {
    let mut options = default_options();
    let mut bench = false;
    let mut output_dir = None;
    let mut baseline = None;
    let mut warmup_iterations = 5;
    let mut timed_iterations = 20;
    let mut frame_budget_ms: f64 = 16.7;
    let mut width = None;
    let mut height = None;
    let mut bench_only_option = None;
    let mut capture_only_option = None;
    let mut lab_only_option = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Skip<std::env::Args>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--bench" | "--output-dir" | "--baseline" | "--warmup" | "--iterations"
            | "--frame-budget" => {
                bench_only_option.get_or_insert_with(|| argument.clone());
            }
            "--width" | "--height" | "--help" | "-h" => {}
            _ => {
                capture_only_option.get_or_insert_with(|| argument.clone());
            }
        }
        if matches!(
            argument.as_str(),
            "--legacy-shadow-look"
                | "--cloud-shadows"
                | "--landform-lighting"
                | "--landscape-visibility"
                | "--light-contributions"
                | "--land-material-colors"
                | "--solid-material-colors"
        ) {
            lab_only_option.get_or_insert_with(|| argument.clone());
        }
        match argument.as_str() {
            "--bench" => bench = true,
            "--output-dir" => output_dir = Some(PathBuf::from(value(&mut arguments)?)),
            "--baseline" => baseline = Some(PathBuf::from(value(&mut arguments)?)),
            "--warmup" => warmup_iterations = parse(&value(&mut arguments)?, &argument)?,
            "--iterations" => timed_iterations = parse(&value(&mut arguments)?, &argument)?,
            "--frame-budget" => frame_budget_ms = parse(&value(&mut arguments)?, &argument)?,
            "--output" => options.output = value(&mut arguments)?.into(),
            "--width" => width = Some(parse(&value(&mut arguments)?, &argument)?),
            "--height" => height = Some(parse(&value(&mut arguments)?, &argument)?),
            "--camera-x" => options.camera_x_m = parse(&value(&mut arguments)?, &argument)?,
            "--camera-y" => options.camera_y_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-width" => options.world_width_m = parse(&value(&mut arguments)?, &argument)?,
            "--world-height" => options.world_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--view-height" => options.view_height_m = parse(&value(&mut arguments)?, &argument)?,
            "--seed" => options.seed = parse_seed(&value(&mut arguments)?)?,
            "--world-time" => {
                options.world_time_seconds = parse(&value(&mut arguments)?, &argument)?
            }
            "--cloud-motion" => {
                options.cloud_motion_seconds = parse(&value(&mut arguments)?, &argument)?
            }
            "--diagnostic" => options.diagnostic = parse_diagnostic(&value(&mut arguments)?)?,
            "--quality" => {
                let name = value(&mut arguments)?;
                options.quality = GraphicsQuality::parse(&name)
                    .ok_or_else(|| format!("invalid value for --quality: {name}"))?;
            }
            "--legacy-shadow-look" => options.legacy_shadow_look = true,
            "--cloud-shadows" => {
                options.cloud_shadows = match value(&mut arguments)?.as_str() {
                    "0" => 0.0,
                    "1" => 1.0,
                    _ => return Err(format!("{argument} must be zero or one")),
                }
            }
            "--landform-lighting" => {
                let lighting = parse_f32x4(&value(&mut arguments)?, &argument)?;
                if !lighting[..2]
                    .iter()
                    .all(|lane| *lane == 0.0 || *lane == 1.0)
                {
                    return Err(format!("{argument} relief and shadows must be zero or one"));
                }
                options.landform_lighting = lighting;
            }
            "--landscape-visibility" => {
                options.landscape_visibility = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--light-contributions" => {
                options.light_contribution_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
            }
            "--land-material-colors" => {
                options.land_material_color_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
            }
            "--solid-material-colors" => {
                options.solid_material_color_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
            }
            "--prior-view" => {
                let view = parse_f32x3(&value(&mut arguments)?, &argument)?;
                validate_capture_spatial_inputs(view[0], view[1], view[2])?;
                options.prior_views.push(view);
            }
            "--skip-prior-fills" => options.skip_prior_fills = true,
            "--fixed-adaptation" => options.fixed_adaptation = true,
            "--touch-budget" => {
                options.bake_memory_bytes = browser::baked_surface_memory_bytes(true, None)
            }
            "--output-peak" => {
                options.output_peak = parse(&value(&mut arguments)?, &argument)?;
                if !(options.output_peak.is_finite() && options.output_peak >= 1.0) {
                    return Err("--output-peak must be finite and at least 1".to_owned());
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown option: {argument}")),
        }
    }
    if bench {
        if let Some(option) = capture_only_option {
            return Err(format!("{option} does not apply to --bench"));
        }
        let (width, height) = (width.unwrap_or(1920), height.unwrap_or(1080));
        validate_capture_dimensions(width, height, u32::MAX)?;
        if timed_iterations == 0 {
            return Err("--iterations must be positive".to_owned());
        }
        if !(frame_budget_ms.is_finite() && frame_budget_ms > 0.0) {
            return Err("--frame-budget must be positive and finite".to_owned());
        }
        return Ok(Command::Bench(bench::BenchOptions {
            output_dir: output_dir.ok_or("--bench requires --output-dir")?,
            baseline,
            width,
            height,
            warmup_iterations,
            timed_iterations,
            frame_budget_ms,
        }));
    }
    if let Some(option) = bench_only_option {
        return Err(format!("{option} requires --bench"));
    }
    if let Some(option) = lab_only_option
        && options.diagnostic.id == "scene_production"
    {
        return Err(format!("{option} applies only to lab compositions"));
    }
    options.width = width.unwrap_or(options.width);
    options.height = height.unwrap_or(options.height);
    validate_capture_output_path(&options.output)?;
    validate_capture_dimensions(options.width, options.height, u32::MAX)?;
    validate_time_seconds(options.world_time_seconds, "--world-time")?;
    if !(0.0..=options.world_time_seconds).contains(&options.cloud_motion_seconds) {
        return Err("--cloud-motion must be from 0 through --world-time seconds".to_owned());
    }
    validate_capture_spatial_inputs(
        options.camera_x_m,
        options.camera_y_m,
        options.view_height_m,
    )?;
    if !(8_000.0..=256_000.0).contains(&options.world_width_m)
        || !(8_000.0..=256_000.0).contains(&options.world_height_m)
    {
        return Err("world dimensions must be between 8000 and 256000 metres".to_owned());
    }
    Ok(Command::Capture(Box::new(options)))
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

fn parse_f32x3(value: &str, option: &str) -> Result<[f32; 3], String> {
    let values = value
        .split(',')
        .map(|value| parse(value, option))
        .collect::<Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| format!("{option} requires three comma-separated values"))
}

fn parse_f32x4(value: &str, option: &str) -> Result<[f32; 4], String> {
    let values = value
        .split(',')
        .map(|value| parse(value, option))
        .collect::<Result<Vec<_>, _>>()?;
    let values: [f32; 4] = values
        .try_into()
        .map_err(|_| format!("{option} requires four comma-separated values"))?;
    if values.iter().all(|value| value.is_finite()) {
        Ok(values)
    } else {
        Err(format!("{option} values must be finite"))
    }
}

fn parse_u32x4(value: &str, option: &str) -> Result<[u32; 4], String> {
    let values = value
        .split(',')
        .map(|value| parse(value, option))
        .collect::<Result<Vec<_>, _>>()?;
    let values: [u32; 4] = values
        .try_into()
        .map_err(|_| format!("{option} requires four comma-separated values"))?;
    if values.iter().all(|value| *value <= 1) {
        Ok(values)
    } else {
        Err(format!("{option} values must be zero or one"))
    }
}

fn diagnostic(
    id: &str,
    module: &'static str,
    constants: &[(&'static str, f64)],
) -> ShaderSelection {
    ShaderSelection {
        id: id.to_owned(),
        module: Some(module),
        constants: constants.to_vec(),
    }
}

fn parse_diagnostic(value: &str) -> Result<ShaderSelection, String> {
    let selection = match value {
        "scene" => scene_selection(),
        "scene_direct" | "scene_occluder_coverage" | "scene_production" => ShaderSelection {
            id: value.to_owned(),
            module: None,
            constants: Vec::new(),
        },
        "scene_occluder_coverage_all_branches" => ShaderSelection {
            id: value.to_owned(),
            module: None,
            constants: vec![("EVALUATE_ALL_BRANCHES", 1.0)],
        },
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
        "cloud_cumulus_shadow" => diagnostic(value, "cloud_field", &[("CLOUD_FIELD_OUTPUT", 0.0)]),
        "cloud_deck_shadow" => diagnostic(value, "cloud_field", &[("CLOUD_FIELD_OUTPUT", 1.0)]),
        "cloud_cumulus_along_two_lights" => {
            diagnostic(value, "cloud_field", &[("CLOUD_FIELD_OUTPUT", 2.0)])
        }
        "lighting_cloud_transmittance" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 0.0)],
        ),
        "lighting_ambient_sky_occlusion" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 1.0)],
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
        "lighting_receiver_moon_ambient_datum" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 6.0),
                ("LIGHTING_RECEIVER_HEIGHT_M", 0.0),
            ],
        ),
        "lighting_receiver_moon_ambient_low_cloud" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 6.0),
                ("LIGHTING_RECEIVER_HEIGHT_M", 900.0),
            ],
        ),
        "lighting_receiver_moon_ambient_canopy" => diagnostic(
            value,
            "lighting_solar",
            &[
                ("LIGHTING_SOLAR_OUTPUT", 6.0),
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
        "lighting_canopy_transfer_veil" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 1.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 1.0),
            ],
        ),
        "lighting_canopy_transfer_thin" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 1.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 6.0),
            ],
        ),
        "lighting_canopy_transfer_thick" => diagnostic(
            value,
            "lighting_canopy_transfer",
            &[
                ("CANOPY_TRANSFER_OUTPUT", 1.0),
                ("CANOPY_TRANSFER_OPTICAL_THICKNESS", 30.0),
            ],
        ),
        "lighting_night_vision_unadapted" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 21.0),
                ("LIGHTING_MODEL_EXPOSURE_GAIN", 1.0),
            ],
        ),
        "lighting_night_vision_adapted" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 21.0),
                ("LIGHTING_MODEL_EXPOSURE_GAIN", 1e7),
            ],
        ),
        // Gains of a view adapted to daylight, to about 0.3 cd/m², and to
        // starlight; signals of dark ground under starlight and of white
        // under a quarter Moon, in scene light.
        name if name.starts_with("vision_") => {
            let (output, gain, color, signal, phase) = match name {
                "vision_noise_day" => (22.0, 1.0, [0.0; 3], 1e-9, 0.0),
                "vision_noise_dark" => (22.0, 1e7, [0.0; 3], 1e-9, 0.0),
                "vision_noise_bright" => (22.0, 1e7, [0.0; 3], 1e-7, 0.0),
                "vision_noise_later" => (22.0, 1e7, [0.0; 3], 1e-9, 0.5),
                "vision_noise_period" => (22.0, 1e7, [0.0; 3], 1e-9, 4096.0),
                "vision_noise_before_wrap" => (22.0, 1e7, [0.0; 3], 1e-9, 4095.999),
                "vision_noise_after_wrap" => (22.0, 1e7, [0.0; 3], 1e-9, 0.001),
                "vision_noise_black" => (23.0, 1e7, [0.0; 3], 1e-9, 0.0),
                _ => {
                    let (color, response) = name[7..]
                        .split_once('_')
                        .ok_or_else(|| format!("unknown diagnostic: {name}"))?;
                    let color = match color {
                        "red" => [0.5, 0.0, 0.0],
                        "blue" => [0.0, 0.0, 0.5],
                        "gray" => [0.5; 3],
                        "white" => [1.0; 3],
                        _ => return Err(format!("unknown diagnostic: {name}")),
                    };
                    let gain = match response {
                        "cones" => 1.0,
                        "mesopic" => 1e4,
                        "rods" => 1e7,
                        _ => return Err(format!("unknown diagnostic: {name}")),
                    };
                    (21.0, gain, color, 1e-9, 0.0)
                }
            };
            diagnostic(
                value,
                "lighting_model_regressions",
                &[
                    ("LIGHTING_MODEL_REGRESSION_OUTPUT", output),
                    ("LIGHTING_MODEL_EXPOSURE_GAIN", gain),
                    ("LIGHTING_MODEL_VISION_RED", color[0]),
                    ("LIGHTING_MODEL_VISION_GREEN", color[1]),
                    ("LIGHTING_MODEL_VISION_BLUE", color[2]),
                    ("LIGHTING_MODEL_SIGNAL", signal),
                    ("LIGHTING_MODEL_NOISE_PHASE", phase),
                ],
            )
        }
        "lighting_display_response" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 26.0)],
        ),
        name if name.starts_with("lighting_shadow_darkness_") => {
            let (light_height, cloud_depth) = name["lighting_shadow_darkness_".len()..]
                .split_once('_')
                .and_then(|(z, depth)| Some((z.parse().ok()?, depth.parse().ok()?)))
                .ok_or_else(|| format!("unknown diagnostic: {name}"))?;
            diagnostic(
                value,
                "lighting_model_regressions",
                &[
                    ("LIGHTING_MODEL_REGRESSION_OUTPUT", 24.0),
                    ("LIGHTING_MODEL_LIGHT_HEIGHT", light_height),
                    ("LIGHTING_MODEL_CLOUD_DEPTH", cloud_depth),
                ],
            )
        }
        "lighting_landform_sun_grade_moon_coupling" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 1.0)],
        ),
        "lighting_landform_shadow_disc_radii" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 5.0)],
        ),
        "lighting_cloud_projection" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 6.0)],
        ),
        "lighting_cloud_shadow_receiver" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 7.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
            ],
        ),
        "lighting_world_direction" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 8.0)],
        ),
        "lighting_cloud_projection_isotropy" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 9.0)],
        ),
        "lighting_landform_shadow_tangent_continuity" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 10.0)],
        ),
        "lighting_landform_shadow_penumbra_center" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 11.0)],
        ),
        "lighting_landform_shadow_partial_disc_ray" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 12.0)],
        ),
        "lighting_below_datum_cloud_projection" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 16.0)],
        ),
        "lighting_receiver_ambient_datum" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 2.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 0.0),
            ],
        ),
        "lighting_receiver_ambient_elevated" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 2.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
            ],
        ),
        "lighting_receiver_ambient_share_elevated" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 3.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
            ],
        ),
        "lighting_receiver_moon_clear_sky_share" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 25.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 0.0),
            ],
        ),
        "lighting_ambient_horizon_datum" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 4.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 0.0),
            ],
        ),
        "lighting_ambient_horizon_elevated" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 4.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
            ],
        ),
        "lighting_aerosol_optical_depth" => {
            diagnostic(value, "lighting_aerosol_optical_depth", &[])
        }
        "lighting_canopy_moon_scattering" => {
            diagnostic(value, "lighting_canopy_moon_scattering", &[])
        }
        "temperature_gradient" => diagnostic(value, "temperature_gradient", &[]),
        "surface_air_temperature" => diagnostic(value, "surface_air_temperature", &[]),
        "pressure_gradient" => diagnostic(value, "pressure_gradient", &[]),
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
        "wind_terrain_shelter" => diagnostic(value, "wind_terrain_shelter", &[]),
        "terrain_elevation" => diagnostic(
            value,
            "terrain_geometry",
            &[("TERRAIN_GEOMETRY_OUTPUT", 0.0)],
        ),
        "terrain_slope" => diagnostic(
            value,
            "terrain_geometry",
            &[("TERRAIN_GEOMETRY_OUTPUT", 1.0)],
        ),
        "terrain_normals" => diagnostic(
            value,
            "terrain_geometry",
            &[("TERRAIN_GEOMETRY_OUTPUT", 2.0)],
        ),
        "terrain_broad_landform" => diagnostic(
            value,
            "terrain_geometry",
            &[("TERRAIN_GEOMETRY_OUTPUT", 3.0)],
        ),
        "terrain_island_clusters" => diagnostic(value, "terrain_island_clusters", &[]),
        "terrain_island_detail" => diagnostic(value, "terrain_island_detail", &[]),
        "terrain_coastal_reef" => diagnostic(value, "terrain_coastal_reef", &[]),
        "material_vegetation" => diagnostic(value, "material_vegetation", &[]),
        "material_coastal_character" => diagnostic(value, "material_coastal_character", &[]),
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
        "climate_annual_temperature" => diagnostic(value, "climate_annual_temperature", &[]),
        "climate_spatial_ice" => diagnostic(value, "climate_spatial_ice", &[]),
        "water_tide_height" => diagnostic(value, "water_tide_height", &[]),
        "water_ocean_current" => diagnostic(
            value,
            "water_ocean_forcing",
            &[("OCEAN_FORCING_OUTPUT", 0.0)],
        ),
        "water_significant_wave_height" => diagnostic(
            value,
            "water_ocean_forcing",
            &[("OCEAN_FORCING_OUTPUT", 1.0)],
        ),
        "water_visual_normals" => diagnostic(value, "water_visual_normals", &[]),
        "water_physical_wave_normals" => diagnostic(value, "water_physical_wave_normals", &[]),
        "water_precipitation_contact_normals" => {
            diagnostic(value, "water_precipitation_contact_normals", &[])
        }
        _ => return Err(format!("unknown diagnostic: {value}")),
    };
    Ok(selection)
}

fn print_help() {
    println!(
        "playground-capture [options]\n\n\
         Options:\n\
           --output PATH          image path; .json is reserved for the sidecar (default playground-capture.png)\n\
           --width N --height N   positive output dimensions, at most 16.8 megapixels (default 960x600)\n\
           --camera-x M --camera-y M  finite world-space camera centre\n\
           --world-width M --world-height M  longitude circumference and pole span (default 64000 each)\n\
           --view-height METRES   positive finite vertical world span (default 480)\n\
           --seed N|0xHEX         deterministic visual seed\n\
           --world-time SECONDS   nonnegative deterministic world time (default 37)\n\
           --cloud-motion SECONDS  cloud transport an interactive view accumulates over the\n\
                                  SECONDS before --world-time at 1 s frames (default 0)\n\
           --diagnostic NAME      semantic diagnostic output (default scene, the lab composition;\n\
                                  scene_direct evaluates every field per pixel without grids;\n\
                                  scene_production renders the game's shaders and tuning\n\
                                  and rejects the lab's tuning and visibility options)\n\
           --quality low|medium|high  graphics tier of scene compositions (default high)\n\
           --legacy-shadow-look   scene compositions fade cloud shadows from a light height of\n\
                                  0.22, cap landform shadows at 70%, and fix their reach\n\n\
           --light-contributions V,V,V,V  enable the four light contributions\n\
           --cloud-shadows 0|1    enable cloud shadows\n\
           --landform-lighting V,V,V,V  enable relief and shadows, finite high-light reach, padding\n\
           --land-material-colors V,V,V,V  grass, forest, intertidal, and beach color rows\n\
           --solid-material-colors V,V,V,V  rock, snow, land-ice, and sea-ice color rows\n\
           --landscape-visibility V,V,V,V  finite landscape mechanism visibility\n\
           --prior-view X,Y,HEIGHT  render an earlier frame's terrain cache fills first;\n\
                                  repeat for a pan or zoom sequence ending at the capture\n\
           --skip-prior-fills     count the prior views' cache fills as done without running\n\
                                  them, a test control that reads unwritten texels\n\
           --fixed-adaptation     adapt the whole view to the clear-sky light at its centre,\n\
                                  a test control that holds exposure while switches change light\n\
           --touch-budget         bake the surface within touch devices' memory budget\n\
           --output-peak PEAK     tone-curve peak in diffuse whites, 4 for the game's HDR\n\
                                  output (default 1, SDR); the 8-bit image clips above 1\n\n\
         The PNG gets a JSON sidecar containing world inputs, derived conditions,\n\
         and image metrics.\n\n\
         playground-capture --bench --output-dir DIR [--baseline DIR] [options]\n\n\
         Times the production terrain-cache, surface-bake, environment-field, occluder-field,\n\
         scene, and upscale passes for each built-in scenario in\n\
         playground/src/bench-scenarios.json, once as shipped and once with\n\
         EVALUATE_ALL_BRANCHES forcing normally skipped work, a refill of the active terrain\n\
         cache level, and one frame budget of surface bakes every frame. Shipped frames from\n\
         empty caches until they settle are timed as the cold start. Then times shipped\n\
         frames along each camera path in playground/src/bench-sequences.json, keeping\n\
         every frame's pass durations.\n\n\
         Bench options:\n\
           --output-dir DIR       receives report.json, one PNG per scenario, and contact-sheet.png\n\
           --baseline DIR         earlier bench output to compare timings and images against;\n\
                                  writes a <scenario>-diff.png heat image per scenario\n\
           --width N --height N   render dimensions (default 1920x1080)\n\
           --warmup N             untimed frames per scenario, mode, and sequence, and at\n\
                                  least half a second of them before timed frames; N frames\n\
                                  also make one untimed sweep over every scenario first\n\
                                  (default 5)\n\
           --iterations N         timed frames per scenario and mode (default 20)\n\
           --frame-budget MS      sequence frames longer than this count as over budget\n\
                                  (default 16.7)\n\n\
         Timing uses GPU timestamp queries when the adapter supports them and\n\
         falls back to submit-to-completion wall-clock time."
    );
}

fn split_tick_seconds(seconds: f64) -> (u64, f32) {
    let tick_position = seconds * TICKS_PER_SECOND as f64;
    let tick = tick_position.floor() as u64;
    (tick, (tick_position - tick as f64) as f32)
}

const MINIMUM_TIME_VALUES_PER_TICK: u64 = 256;
const MAXIMUM_PRECISE_TIME_SECONDS: f64 = ((1_u64 << (f64::MANTISSA_DIGITS - 1))
    / (TICKS_PER_SECOND * MINIMUM_TIME_VALUES_PER_TICK))
    .next_power_of_two() as f64;

fn validate_time_seconds(seconds: f64, option: &str) -> Result<(), String> {
    let tick_position = seconds * TICKS_PER_SECOND as f64;
    if seconds.is_finite()
        && (0.0..MAXIMUM_PRECISE_TIME_SECONDS).contains(&seconds)
        && tick_position < u64::MAX as f64
    {
        Ok(())
    } else {
        Err(format!(
            "{option} must be finite, nonnegative, and precise enough to select a 60 Hz simulation tick",
        ))
    }
}

fn validate_capture_output_path(path: &std::path::Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        Err("capture output must not use the reserved .json sidecar extension".to_owned())
    } else {
        Ok(())
    }
}

fn validate_capture_spatial_inputs(
    camera_x_m: f32,
    camera_y_m: f32,
    view_height_m: f32,
) -> Result<(), String> {
    if !camera_x_m.is_finite() || !camera_y_m.is_finite() {
        return Err("capture camera coordinates must be finite".to_owned());
    }
    if !view_height_m.is_finite() || view_height_m <= 0.0 {
        return Err("capture view height must be finite and positive".to_owned());
    }
    Ok(())
}

fn validate_capture_dimensions(
    width: u32,
    height: u32,
    maximum_texture_dimension: u32,
) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("capture dimensions must be positive".to_owned());
    }
    if width > maximum_texture_dimension || height > maximum_texture_dimension {
        return Err(format!(
            "capture dimensions exceed the device limit of {maximum_texture_dimension} pixels",
        ));
    }
    if u64::from(width) * u64::from(height) > MAXIMUM_CAPTURE_PIXELS {
        return Err("capture dimensions exceed the 16,777,216-pixel resource budget".to_owned());
    }
    let bytes_per_row = width
        .checked_mul(4)
        .ok_or_else(|| "capture row size exceeds supported arithmetic".to_owned())?;
    let padded_bytes_per_row = bytes_per_row
        .checked_add(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
        .map(|bytes| {
            bytes / wgpu::COPY_BYTES_PER_ROW_ALIGNMENT * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
        })
        .ok_or_else(|| "capture row size exceeds supported arithmetic".to_owned())?;
    u64::from(padded_bytes_per_row)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "capture buffer size exceeds supported arithmetic".to_owned())?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        parse_diagnostic, parse_f32x4, parse_u32x4, split_tick_seconds,
        validate_capture_dimensions, validate_capture_output_path, validate_capture_spatial_inputs,
        validate_time_seconds,
    };

    #[test]
    fn diagnostics_select_semantic_shader_modules_and_overrides() {
        let pressure = parse_diagnostic("pressure_full_anomaly").unwrap();
        assert_eq!(pressure.module, Some("pressure_anomaly"));
        assert_eq!(pressure.constants, &[("PRESSURE_ANOMALY_OUTPUT", 1.0)]);
        for diagnostic in [
            "lighting_cloud_transmittance",
            "cloud_deck_shadow",
            "pressure_gradient",
            "wind_terrain_shelter",
            "terrain_slope",
            "terrain_normals",
            "terrain_island_clusters",
            "terrain_island_detail",
            "terrain_coastal_reef",
            "material_vegetation",
            "material_coastal_character",
            "climate_annual_temperature",
            "climate_spatial_ice",
            "water_ocean_current",
            "water_visual_normals",
            "water_significant_wave_height",
            "water_precipitation_contact_normals",
        ] {
            assert!(parse_diagnostic(diagnostic).is_ok(), "{diagnostic}");
        }
        assert!(parse_diagnostic("cloud_low_amount").is_err());
        assert!(parse_diagnostic("59").is_err());
    }

    #[test]
    fn capture_visibility_switches_are_binary() {
        assert_eq!(
            parse_u32x4("1,0,1,0", "--light-contributions").unwrap(),
            [1, 0, 1, 0]
        );
        assert!(parse_u32x4("2,0,0,0", "--light-contributions").is_err());
    }

    #[test]
    fn capture_clock_preserves_fractional_tick_progress() {
        let (tick, fraction) = split_tick_seconds(10.5 / 60.0);
        assert_eq!(tick, 10);
        assert!((fraction - 0.5).abs() < 1.0e-5);

        let (tick, fraction) = split_tick_seconds(365.0 * 86_400.0);
        assert_eq!(tick, 365 * 86_400 * 60);
        assert_eq!(fraction, 0.0);
    }

    #[test]
    fn capture_times_must_preserve_fixed_tick_precision() {
        assert!(validate_time_seconds(0.0, "--world-time").is_ok());
        assert!(validate_time_seconds(365.0 * 86_400.0, "--world-time").is_ok());
        assert!(validate_time_seconds((2.0_f64).powi(39) - 1.0, "--world-time").is_ok());
        assert!(validate_time_seconds((2.0_f64).powi(39), "--world-time").is_err());
        assert!(validate_time_seconds(1.0e16, "--world-time").is_err());
        assert!(validate_time_seconds(-1.0, "--world-time").is_err());
        assert!(validate_time_seconds(f64::NAN, "--world-time").is_err());
        assert!(validate_time_seconds(f64::INFINITY, "--world-time").is_err());
        assert!(validate_time_seconds(f64::MAX, "--world-time").is_err());
    }

    #[test]
    fn capture_dimensions_must_fit_the_device_and_readback_arithmetic() {
        assert!(validate_capture_dimensions(4_096, 4_096, 8_192).is_ok());
        assert!(validate_capture_dimensions(4_097, 4_096, 8_192).is_err());
        assert!(validate_capture_dimensions(8_192, 8_192, 8_192).is_err());
        assert!(validate_capture_dimensions(8_193, 1, 8_192).is_err());
        assert!(validate_capture_dimensions(u32::MAX, u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn capture_spatial_and_tuning_inputs_must_be_finite() {
        assert!(validate_capture_spatial_inputs(0.0, 0.0, 1.0).is_ok());
        assert!(validate_capture_spatial_inputs(f32::NAN, 0.0, 1.0).is_err());
        assert!(validate_capture_spatial_inputs(0.0, f32::INFINITY, 1.0).is_err());
        assert!(validate_capture_spatial_inputs(0.0, 0.0, 0.0).is_err());
        assert!(validate_capture_spatial_inputs(0.0, 0.0, f32::INFINITY).is_err());
        assert!(parse_f32x4("1,NaN,3,4", "--landform-lighting").is_err());
    }

    #[test]
    fn capture_output_must_not_collide_with_its_json_sidecar() {
        assert!(validate_capture_output_path(std::path::Path::new("capture.png")).is_ok());
        assert!(validate_capture_output_path(std::path::Path::new("capture.json")).is_err());
        assert!(validate_capture_output_path(std::path::Path::new("capture.JSON")).is_err());
    }
}

async fn hardware_device(
    optional_features: wgpu::Features,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
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
            required_features: adapter.features() & optional_features,
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|error| format!("failed to create capture device: {error}"))?;
    Ok((adapter, device, queue))
}

async fn render(options: &Options) -> Result<(), String> {
    let (_, device, queue) = hardware_device(wgpu::Features::empty()).await?;
    validate_capture_dimensions(
        options.width,
        options.height,
        device.limits().max_texture_dimension_2d,
    )?;
    let grid_scene = options.diagnostic.module.is_none() && options.diagnostic.id != "scene_direct";
    if !grid_scene && !options.prior_views.is_empty() {
        return Err("--prior-view applies only to the scene composition".to_owned());
    }
    let production = options.diagnostic.id == "scene_production";
    let constants = if production {
        options.quality.scene_constants().to_vec()
    } else if options.diagnostic.module.is_none() {
        [
            options.diagnostic.constants.as_slice(),
            options.quality.scene_constants(),
            &[(
                "LEGACY_SHADOW_LOOK",
                f64::from(u8::from(options.legacy_shadow_look)),
            )],
        ]
        .concat()
    } else if options.quality == GraphicsQuality::High && !options.legacy_shadow_look {
        options.diagnostic.constants.clone()
    } else {
        return Err(
            "--quality and --legacy-shadow-look apply only to scene compositions".to_owned(),
        );
    };
    let shader_source = options.diagnostic.module.map_or_else(
        || {
            if production {
                browser::production_scene_shader_source()
            } else if grid_scene {
                browser::playground_scene_shader_source()
            } else {
                browser::playground_direct_scene_shader_source()
            }
        },
        |name| browser::playground_diagnostic_shader_source(name).unwrap(),
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Scene shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    let scene_layout = uniform_layout(&device, "scene uniforms layout");
    let playground_layout = uniform_layout(&device, "playground uniforms layout");
    let fields = grid_scene.then(|| {
        if production {
            return CaptureFields::new(
                &device,
                &scene_layout,
                &playground_layout,
                browser::occluder_field_shader_source(),
                &[],
                browser::landform_horizon_shader_source(),
                &[],
                browser::baked_surface_shader_source(),
                &[],
                baked_surface_sizes((options.width, options.height), options.bake_memory_bytes),
            );
        }
        CaptureFields::new(
            &device,
            &scene_layout,
            &playground_layout,
            browser::playground_occluder_field_shader_source(),
            &[],
            browser::playground_landform_horizon_shader_source(),
            &[
                options.diagnostic.constants.as_slice(),
                &[(
                    "LEGACY_SHADOW_LOOK",
                    f64::from(u8::from(options.legacy_shadow_look)),
                )],
            ]
            .concat(),
            browser::playground_baked_surface_shader_source(),
            &[],
            baked_surface_sizes((options.width, options.height), options.bake_memory_bytes),
        )
    });
    let scene_composition = options.diagnostic.module.is_none();
    let direct_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene uniforms and adapting field layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            ADAPTING_FIELD_LAYOUT_ENTRY,
        ],
    });
    let bind_group_layouts = [
        Some(match &fields {
            Some(fields) => &fields.scene_layout,
            None if scene_composition => &direct_layout,
            None => &scene_layout,
        }),
        Some(&playground_layout),
    ];
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("playground capture pipeline layout"),
        bind_group_layouts: &bind_group_layouts,
        immediate_size: 0,
    });
    let scene_pipeline = |entry_point: &str, target: wgpu::ColorTargetState| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                entry_point: Some(entry_point),
                targets: &[Some(target)],
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let fragment_entry = options.diagnostic.module.map_or_else(
        || {
            if production {
                "fragment"
            } else {
                "fragment_playground_scene"
            }
            .to_owned()
        },
        |name| format!("fragment_{name}"),
    );
    let pipeline = scene_pipeline(&fragment_entry, wgpu::TextureFormat::Rgba8UnormSrgb.into());
    let adaptation = scene_composition.then(|| CaptureAdaptation {
        pipeline: scene_pipeline(
            if production {
                "fragment_illumination"
            } else {
                "fragment_playground_illumination"
            },
            AdaptingField::illumination_target(),
        ),
        field: AdaptingField::new(&device, &scene_layout),
        direct_layout: &direct_layout,
    });
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fullscreen terrain vertices"),
        contents: bytemuck::cast_slice(&fullscreen_vertices()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let playground_uniforms = PlaygroundUniforms {
        water_tuning: [0.5, 0.0, 0.0, 0.0],
        landform_lighting: options.landform_lighting,
        cloud_shadows: [options.cloud_shadows, 0.0, 0.0, 0.0],
        layer_visibility: [1.0; 4],
        landscape_visibility: options.landscape_visibility,
        light_contribution_visibility: options.light_contribution_visibility,
        land_material_color_visibility: options.land_material_color_visibility,
        solid_material_color_visibility: options.solid_material_color_visibility,
        water_material_color_visibility: [1; 4],
        environment_features: [1.0; 4],
        material_features: [1.0; 4],
        effect_features: [1.0; 4],
        occluder_field_inspection: [
            u32::from(options.diagnostic.id.starts_with("scene_occluder_coverage")) * 2,
            options.width,
            options.height,
            0,
        ],
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
    let (rgba, adapting_light) = render_world(
        &device,
        &queue,
        &pipeline,
        &scene_layout,
        &playground_bind_group,
        &vertex_buffer,
        fields.as_ref(),
        adaptation.as_ref(),
        options,
    )?;
    write_png(path, options.width, options.height, &rgba)?;
    let report = capture_report(options, path, &rgba, adapting_light)?;
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
    playground_bind_group: &wgpu::BindGroup,
    vertex_buffer: &wgpu::Buffer,
    fields: Option<&CaptureFields>,
    adaptation: Option<&CaptureAdaptation>,
    options: &Options,
) -> Result<(Vec<u8>, Option<f32>), String> {
    let field_bindings = |fields: &CaptureFields, buffer: &wgpu::Buffer| {
        fields.bindings(
            device,
            buffer,
            &adaptation
                .expect("grid scenes adapt to their illumination")
                .field,
        )
    };
    let mut terrain_cache = TerrainCache::default();
    let mut baked_surface = BakedSurface::new(baked_surface_sizes(
        (options.width, options.height),
        options.bake_memory_bytes,
    ));
    let scene_bind_group = |terrain_cache_frame: &TerrainCacheFrame,
                            baked_surface_frame: &BakedSurfaceFrame,
                            options: &Options| {
        let uniforms = scene_uniforms(options, terrain_cache_frame, baked_surface_frame)?;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = bind_group(device, scene_layout, &buffer, "scene uniforms bind group");
        Ok::<_, String>((buffer, bind_group))
    };
    // Earlier views run one frame each; the requested view then runs until its
    // surface is baked.
    let views = options
        .prior_views
        .iter()
        .map(|&[camera_x_m, camera_y_m, view_height_m]| {
            (
                Options {
                    camera_x_m,
                    camera_y_m,
                    view_height_m,
                    ..options.clone()
                },
                true,
            )
        })
        .chain(std::iter::repeat_with(|| (options.clone(), false)));
    let mut frames = None;
    for (view, prior) in views {
        let terrain_cache_frame = capture_terrain_cache_frame(&view, &mut terrain_cache)?;
        terrain_cache.commit();
        let baked_surface_frame = capture_baked_surface_frame(
            &view,
            visual_pixel_footprint_m(view.view_height_m, view.height),
            &mut baked_surface,
            false,
        )?;
        baked_surface.commit();
        if !prior && (fields.is_none() || !baked_surface_frame.has_fills()) {
            frames = Some((terrain_cache_frame, baked_surface_frame));
            break;
        }
        let Some(fields) = fields.filter(|_| !(prior && options.skip_prior_fills)) else {
            continue;
        };
        let (buffer, bind_group) =
            scene_bind_group(&terrain_cache_frame, &baked_surface_frame, &view)?;
        let bindings = field_bindings(fields, &buffer);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("baking frame encoder"),
        });
        fields.encode_terrain_cache(queue, &mut encoder, &bind_group, &terrain_cache_frame, None);
        fields.encode_baked_surface(
            queue,
            &mut encoder,
            &bindings,
            playground_bind_group,
            &baked_surface_frame,
            None,
        );
        queue.submit([encoder.finish()]);
    }
    let (terrain_cache_frame, baked_surface_frame) = frames.unwrap();
    let (scene_buffer, scene_bind_group) =
        scene_bind_group(&terrain_cache_frame, &baked_surface_frame, options)?;
    let texture = capture_texture(device, options.width, options.height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("playground capture encoder"),
    });
    let fragment_scene_bind_group = match fields {
        Some(fields) => {
            let bindings = field_bindings(fields, &scene_buffer);
            fields.encode_terrain_cache(
                queue,
                &mut encoder,
                &scene_bind_group,
                &terrain_cache_frame,
                None,
            );
            fields.encode(
                &mut encoder,
                &scene_bind_group,
                &bindings,
                playground_bind_group,
                &capture_field_grids(options)?,
                None,
            );
            bindings.scene
        }
        None => match adaptation {
            Some(adaptation) => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene uniforms and adapting field"),
                layout: adaptation.direct_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: scene_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&adaptation.field.view),
                    },
                ],
            }),
            None => scene_bind_group.clone(),
        },
    };
    match adaptation {
        Some(adaptation) if options.fixed_adaptation => {
            let world = WorldGeometry::new(options.world_width_m, options.world_height_m)
                .map_err(|error| error.to_string())?;
            let center = world_celestial_conditions(
                world,
                options.world_time_seconds,
                [options.camera_x_m, options.camera_y_m].into(),
            );
            adaptation
                .field
                .encode_uniform(&mut encoder, celestial_lighting(center).ambient_intensity);
        }
        Some(adaptation) => {
            adaptation
                .field
                .encode(&mut encoder, &scene_bind_group, None, |pass| {
                    pass.set_pipeline(&adaptation.pipeline);
                    pass.set_bind_group(0, &fragment_scene_bind_group, &[]);
                    pass.set_bind_group(1, playground_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.draw(0..6, 0..1);
                });
        }
        None => {}
    }
    let adapting_field_readback = adaptation.map(|adaptation| {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("adapting field readback"),
            size: u64::from(ADAPTING_FIELD_ROW_BYTES * ADAPTING_FIELD_TEXELS),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            adaptation.field.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ADAPTING_FIELD_ROW_BYTES),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: ADAPTING_FIELD_TEXELS,
                height: ADAPTING_FIELD_TEXELS,
                depth_or_array_layers: 1,
            },
        );
        buffer
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
        pass.set_bind_group(0, &fragment_scene_bind_group, &[]);
        pass.set_bind_group(1, playground_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
    let rgba = read_texture(device, queue, encoder, &texture)?;
    let adapting_light = adapting_field_readback
        .map(|buffer| {
            buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|error| format!("adapting field readback failed: {error}"))?;
            let bytes = buffer
                .slice(..)
                .get_mapped_range()
                .map_err(|error| format!("failed to read the adapting field: {error}"))?;
            let stops = |column: u32, row: u32| {
                let offset = (row * ADAPTING_FIELD_ROW_BYTES + 4 * column) as usize;
                f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
            };
            // The view's centre lies between the four central texels.
            let half = ADAPTING_FIELD_TEXELS / 2;
            let mean = (stops(half - 1, half - 1)
                + stops(half, half - 1)
                + stops(half - 1, half)
                + stops(half, half))
                / 4.0;
            Ok::<_, String>(mean.exp2())
        })
        .transpose()?;
    Ok((rgba, adapting_light))
}

/// What a scene composition adapts with: its illumination pipeline, the
/// field, and the layout a direct composition reads the field through.
struct CaptureAdaptation<'a> {
    pipeline: wgpu::RenderPipeline,
    field: AdaptingField,
    direct_layout: &'a wgpu::BindGroupLayout,
}

const ADAPTING_FIELD_ROW_BYTES: u32 = 256;

/// Low-wind then upper-wind cloud offsets that a view held at the camera since
/// `cloud_motion_seconds` before the capture has integrated from view-centre wind.
fn view_cloud_motion_m(options: &Options) -> [f32; 4] {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .expect("parsed options hold a valid world geometry");
    let camera_m = world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let mut offsets_m = [0.0_f64; 4];
    let mut elapsed_seconds = 0.0;
    while elapsed_seconds < options.cloud_motion_seconds {
        let step_seconds = (options.cloud_motion_seconds - elapsed_seconds).min(1.0);
        elapsed_seconds += step_seconds;
        let (tick, tick_fraction) = split_tick_seconds(
            options.world_time_seconds - options.cloud_motion_seconds + elapsed_seconds,
        );
        let state = sample_view_cloud_state(
            world_geometry,
            options.seed,
            tick,
            tick_fraction,
            camera_m.into(),
        );
        let winds = [
            state.low_wind_velocity_m_s.x,
            state.low_wind_velocity_m_s.y,
            state.upper_wind_velocity_m_s.x,
            state.upper_wind_velocity_m_s.y,
        ];
        for (offset_m, wind_m_s) in offsets_m.iter_mut().zip(winds) {
            *offset_m += step_seconds * f64::from(wind_m_s);
        }
    }
    offsets_m.map(|offset_m| offset_m as f32)
}

fn capture_field_grids(options: &Options) -> Result<FieldGrids, String> {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    Ok(field_grids(
        world_geometry,
        options.world_time_seconds,
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]),
        [
            view_width_m(
                options.width as f32 / options.height as f32,
                options.view_height_m,
            ),
            options.view_height_m,
        ],
        view_cloud_motion_m(options),
        FieldGridTuning {
            landform_shadow_reach_m: options.landform_lighting[2],
        },
        &mut FieldGridHistory::default(),
    ))
}

/// `rebake` bakes one frame budget into the read levels again, for timing the
/// bake at its frame budget.
fn capture_baked_surface_frame(
    options: &Options,
    pixel_footprint_m: f32,
    baked_surface: &mut BakedSurface,
    rebake: bool,
) -> Result<BakedSurfaceFrame, String> {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let camera_m = world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let view_size_m = [
        view_width_m(
            options.width as f32 / options.height as f32,
            options.view_height_m,
        ),
        options.view_height_m,
    ];
    Ok(if rebake {
        baked_surface.frame_rebaking_budget(
            world_geometry,
            options.seed,
            camera_m,
            view_size_m,
            pixel_footprint_m,
        )
    } else {
        baked_surface.frame(
            world_geometry,
            options.seed,
            camera_m,
            view_size_m,
            pixel_footprint_m,
        )
    })
}

fn capture_terrain_cache_frame(
    options: &Options,
    terrain_cache: &mut TerrainCache,
) -> Result<TerrainCacheFrame, String> {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    Ok(terrain_cache.frame(
        world_geometry,
        options.seed,
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]),
        [
            view_width_m(
                options.width as f32 / options.height as f32,
                options.view_height_m,
            ),
            options.view_height_m,
        ],
    ))
}

fn scene_uniforms(
    options: &Options,
    terrain_cache: &TerrainCacheFrame,
    baked_surface: &BakedSurfaceFrame,
) -> Result<SceneUniforms, String> {
    let field_grids = capture_field_grids(options)?;
    let (occluder_field_grid_m, occluder_field_texels) = field_grids.occluder_uniforms();
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let canonical_camera_m =
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let precipitation_epoch = (options.world_time_seconds / 1800.0).floor() as u64;
    let precipitation_phase = (options.world_time_seconds.rem_euclid(1800.0) / 1800.0) as f32;
    let (tick, tick_fraction) = split_tick_seconds(options.world_time_seconds);
    let conditions =
        world_celestial_conditions(world_geometry, options.world_time_seconds, [0.0; 2].into());
    let orbit = celestial_orbit(conditions);
    let scalar_time = scalar_field_time_coordinates_interpolated(tick, tick_fraction);
    let vector_time =
        vector_field_time_coordinates_interpolated(world_geometry, tick, tick_fraction);
    let cloud_time = cloud_field_time_coordinates_interpolated(tick, tick_fraction);
    Ok(SceneUniforms {
        camera_and_view_m: [
            canonical_camera_m[0],
            canonical_camera_m[1],
            view_width_m(
                options.width as f32 / options.height as f32,
                options.view_height_m,
            ),
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
        precipitation_phase_and_css_view_height_px: [
            precipitation_phase,
            options.height as f32,
            0.0,
            0.0,
        ],
        world_phases_and_pixel_footprint_m: [
            conditions.day_phase,
            conditions.season_phase,
            conditions.lunar_longitude_phase,
            visual_pixel_footprint_m(options.view_height_m, options.height),
        ],
        pressure_time_coordinate: vector_time.pressure,
        ocean_current_time_coordinate_and_padding: [vector_time.ocean_current, 0.0, 0.0, 0.0],
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
        view_cloud_motion_m: view_cloud_motion_m(options),
        cloud_time_coordinates: [cloud_time.cumulus, cloud_time.upper, 0.0, 0.0],
        exposure_and_output: [1.0, options.output_peak, 0.0, 0.0],
        environment_field_grid_m: field_grids.environment.uniforms,
        environment_field_texels: field_grids.environment.texel_uniforms(),
        cloud_weather_grid_m: field_grids.cloud_weather.uniforms,
        cloud_weather_texels: field_grids.cloud_weather.texel_uniforms(),
        cloud_shadow_grids: field_grids.cloud_shadow_grids,
        occluder_field_grid_m,
        occluder_field_texels,
        terrain_cache_grid_m: terrain_cache.grid_m,
        terrain_cache_texels: terrain_cache.texels,
        terrain_cache_level: terrain_cache.level,
        baked_surface_grid_m: baked_surface.grid_m,
        baked_surface_texels: baked_surface.texels,
        baked_surface_level: baked_surface.level,
        ocean_wave_modes: ocean_wave_mode_uniforms(
            world_geometry,
            options.seed,
            tick,
            tick_fraction,
        ),
    })
}

fn capture_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("procedural world capture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    })
}

/// Submits `encoder` after copying `texture` out and returns its tightly packed RGBA rows.
fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, String> {
    let (width, height) = (texture.width(), texture.height());
    let bytes_per_row = width
        .checked_mul(4)
        .ok_or_else(|| "capture row size exceeds supported arithmetic".to_owned())?;
    let padded_bytes_per_row = bytes_per_row
        .checked_add(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
        .map(|bytes| {
            bytes / wgpu::COPY_BYTES_PER_ROW_ALIGNMENT * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
        })
        .ok_or_else(|| "capture row size exceeds supported arithmetic".to_owned())?;
    let readback_size = u64::from(padded_bytes_per_row)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "capture buffer size exceeds supported arithmetic".to_owned())?;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("playground capture readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
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

fn capture_report(
    options: &Options,
    path: &std::path::Path,
    rgba: &[u8],
    adapting_light: Option<f32>,
) -> Result<CaptureReport, String> {
    let world = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let (world_tick, world_tick_fraction) = split_tick_seconds(options.world_time_seconds);
    let sample = procedural_world_environment_interpolated(
        world,
        options.seed,
        world_tick,
        world_tick_fraction,
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
        world_time_seconds: options.world_time_seconds,
        cloud_motion_seconds: options.cloud_motion_seconds,
        view_cloud_motion_m: view_cloud_motion_m(options),
        diagnostic: options.diagnostic.id.clone(),
        quality: options.quality.name(),
        legacy_shadow_look: options.legacy_shadow_look,
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
            adapting_light,
            settled_exposure_multiplier: exposure_target(
                adapting_light.unwrap_or(celestial.ambient_intensity),
            ),
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
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f32::from(pixel[channel]) / 255.0)
            .sum::<f32>()
            / pixel_count
    };
    let warm_pixel_fraction = rgba
        .as_chunks::<4>()
        .0
        .iter()
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
        .as_chunks::<4>()
        .0
        .iter()
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
