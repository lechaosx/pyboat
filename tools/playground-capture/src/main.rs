use std::{env, fs, path::PathBuf};

use browser::{
    ENVIRONMENT_FIELD_LAYERS, ENVIRONMENT_FIELD_TEXELS, EnvironmentFieldGrid,
    environment_field_grid, exposure_target, ocean_wave_mode_uniforms, seasonal_phase_factors,
    view_width_m, visual_pixel_footprint_m,
};
use bytemuck::{Pod, Zeroable};
use game::{
    OCEAN_WAVE_MODE_COUNT, celestial_lighting, celestial_orbit,
    cloud_field_time_coordinates_interpolated, procedural_world_environment_for_render_clocks,
    scalar_field_time_coordinates_interpolated, vector_field_time_coordinates_interpolated,
    world_celestial_conditions,
};
use serde::Serialize;
use simulation::{TICKS_PER_SECOND, WorldGeometry};
use wgpu::util::DeviceExt;

mod bench;

const MAXIMUM_CAPTURE_PIXELS: u64 = 4096 * 4096;
const CAPTURE_SHADOW_CACHE_ATLAS_BYTES: u64 = 64 * 1024 * 1024;

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
    cloud_time_seconds: f64,
    world_time_seconds: f64,
    diagnostic: ShaderSelection,
    light_balance: [f32; 4],
    cloud_rendering: [f32; 4],
    cloud_shadows: [f32; 4],
    landform_lighting: [f32; 4],
    landscape_visibility: [f32; 4],
    light_contribution_visibility: [u32; 4],
    layer_material_color_visibility: [u32; 4],
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
    ocean_wave_modes: [[f32; 4]; OCEAN_WAVE_MODE_COUNT],
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

struct CaptureCloudShadowCache {
    sample_layout: wgpu::BindGroupLayout,
    discovery_pipeline: wgpu::ComputePipeline,
    fill_pipeline: wgpu::RenderPipeline,
    entries: wgpu::Buffer,
    header: wgpu::Buffer,
    atlas_view: wgpu::TextureView,
    discovery_bind_group: wgpu::BindGroup,
    fill_bind_group: wgpu::BindGroup,
    sample_bind_group: wgpu::BindGroup,
    _tiles: wgpu::Buffer,
    _atlas: wgpu::Texture,
    _debug: wgpu::Buffer,
}

impl CaptureCloudShadowCache {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &wgpu::Device,
        scene_layout: &wgpu::BindGroupLayout,
        playground_layout: &wgpu::BindGroupLayout,
        shader_source: &str,
        constants: &[(&str, f64)],
        layout: browser::CloudShadowCacheLayout,
        viewport_width: u32,
        viewport_height: u32,
        debug_mode: u32,
    ) -> Self {
        let buffer_entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capture cache sampling layout"),
            entries: &[
                buffer_entry(
                    0,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Uniform,
                ),
                buffer_entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buffer_entry(
                    4,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                buffer_entry(
                    5,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
                buffer_entry(
                    6,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Uniform,
                ),
            ],
        });
        let discovery_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capture cache discovery layout"),
            entries: &[
                buffer_entry(
                    0,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Uniform,
                ),
                buffer_entry(
                    1,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: false },
                ),
                buffer_entry(
                    2,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: false },
                ),
                buffer_entry(
                    3,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BufferBindingType::Storage { read_only: false },
                ),
            ],
        });
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capture cache fill layout"),
            entries: &[
                buffer_entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BufferBindingType::Uniform,
                ),
                buffer_entry(
                    4,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BufferBindingType::Storage { read_only: true },
                ),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("capture cache shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let compilation_options = wgpu::PipelineCompilationOptions {
            constants,
            ..Default::default()
        };
        let discovery_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("capture cache discovery pipeline layout"),
                bind_group_layouts: &[
                    Some(scene_layout),
                    Some(playground_layout),
                    Some(&discovery_layout),
                ],
                immediate_size: 0,
            });
        let discovery_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("capture cache discovery pipeline"),
            layout: Some(&discovery_pipeline_layout),
            module: &shader,
            entry_point: Some("discover_cloud_shadow_cache_tiles"),
            compilation_options: compilation_options.clone(),
            cache: None,
        });
        let fill_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("capture cache fill pipeline layout"),
            bind_group_layouts: &[
                Some(scene_layout),
                Some(playground_layout),
                Some(&fill_layout),
            ],
            immediate_size: 0,
        });
        let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("capture cache fill pipeline"),
            layout: Some(&fill_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("cloud_shadow_cache_vertex"),
                buffers: &[],
                compilation_options: compilation_options.clone(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("cloud_shadow_cache_fragment"),
                targets: &[Some(wgpu::TextureFormat::Rg16Float.into())],
                compilation_options,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = CloudShadowCacheUniforms {
            viewport_size_px: [viewport_width, viewport_height],
            atlas_size_px: [layout.atlas_width, layout.atlas_height],
            table_mask: layout.table_entries - 1,
            maximum_tiles: layout.maximum_tiles,
            atlas_columns: layout.atlas_columns,
            tile_texels: browser::CLOUD_SHADOW_CACHE_TILE_TEXELS,
            padding: [0; 4],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("capture cache uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let header = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("capture cache draw arguments"),
            contents: bytemuck::cast_slice(&[6u32, 0, 0, 0]),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
        });
        let entries = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture cache page table"),
            size: u64::from(layout.table_entries) * 8,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tiles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture cache tiles"),
            size: u64::from(layout.maximum_tiles) * 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture cache atlas"),
            size: wgpu::Extent3d {
                width: layout.atlas_width,
                height: layout.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("capture cache sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let debug = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("capture cache debug uniforms"),
            contents: bytemuck::cast_slice(&[debug_mode, 0, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let discovery_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture cache discovery bind group"),
            layout: &discovery_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
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
        let fill_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture cache fill bind group"),
            layout: &fill_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: tiles.as_entire_binding(),
                },
            ],
        });
        let sample_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture cache sample bind group"),
            layout: &sample_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: entries.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: tiles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: header.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: debug.as_entire_binding(),
                },
            ],
        });
        Self {
            sample_layout,
            discovery_pipeline,
            fill_pipeline,
            entries,
            header,
            atlas_view,
            discovery_bind_group,
            fill_bind_group,
            sample_bind_group,
            _tiles: tiles,
            _atlas: atlas,
            _debug: debug,
        }
    }

    /// Timestamps, when requested, go to query indices 0 to 3: discovery begin
    /// and end, then fill begin and end.
    #[allow(clippy::too_many_arguments)]
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_bind_group: &wgpu::BindGroup,
        playground_bind_group: &wgpu::BindGroup,
        viewport_width: u32,
        viewport_height: u32,
        atlas_load: wgpu::LoadOp<wgpu::Color>,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        encoder.clear_buffer(&self.entries, 0, None);
        encoder.clear_buffer(&self.header, 4, Some(4));
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("capture cache discovery pass"),
                timestamp_writes: timestamps.map(|query_set| wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
            });
            pass.set_pipeline(&self.discovery_pipeline);
            pass.set_bind_group(0, scene_bind_group, &[]);
            pass.set_bind_group(1, playground_bind_group, &[]);
            pass.set_bind_group(2, &self.discovery_bind_group, &[]);
            pass.dispatch_workgroups(
                viewport_width.div_ceil(128),
                viewport_height.div_ceil(128),
                1,
            );
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("capture cache fill pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.atlas_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: atlas_load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(2),
                end_of_pass_write_index: Some(3),
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.fill_pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, playground_bind_group, &[]);
        pass.set_bind_group(2, &self.fill_bind_group, &[]);
        pass.draw_indirect(&self.header, 0);
    }
}

struct CaptureEnvironmentFields {
    scene_layout: wgpu::BindGroupLayout,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    pipeline: wgpu::ComputePipeline,
    output_bind_group: wgpu::BindGroup,
}

impl CaptureEnvironmentFields {
    fn new(device: &wgpu::Device, scene_uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniforms and environment fields layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture environment fields"),
            size: wgpu::Extent3d {
                width: ENVIRONMENT_FIELD_TEXELS,
                height: ENVIRONMENT_FIELD_TEXELS,
                depth_or_array_layers: ENVIRONMENT_FIELD_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capture environment field output layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba32Float,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("capture environment field shader"),
            source: wgpu::ShaderSource::Wgsl(browser::environment_field_shader_source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("capture environment field pipeline layout"),
            bind_group_layouts: &[Some(scene_uniform_layout), Some(&output_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("capture environment field pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("generate_environment_fields"),
            compilation_options: Default::default(),
            cache: None,
        });
        let output_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture environment field output"),
            layout: &output_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Self {
            scene_layout,
            _texture: texture,
            view,
            pipeline,
            output_bind_group,
        }
    }

    fn scene_bind_group(
        &self,
        device: &wgpu::Device,
        scene_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene uniforms and environment fields"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
            ],
        })
    }

    /// Timestamps, when requested, go to query indices 4 and 5.
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_uniform_bind_group: &wgpu::BindGroup,
        grid: &EnvironmentFieldGrid,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("capture environment field pass"),
            timestamp_writes: timestamps.map(|query_set| wgpu::ComputePassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(4),
                end_of_pass_write_index: Some(5),
            }),
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_uniform_bind_group, &[]);
        pass.set_bind_group(1, &self.output_bind_group, &[]);
        let [columns, rows] = grid.workgroups();
        pass.dispatch_workgroups(columns, rows, 1);
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
    cloud_time_seconds: f64,
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
    match parse_options()? {
        Command::Capture(options) => pollster::block_on(render(&options)),
        Command::Bench(options) => pollster::block_on(bench::run(&options)),
    }
}

enum Command {
    Capture(Options),
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
        cloud_time_seconds: 37.0,
        world_time_seconds: 0.0,
        diagnostic: scene_selection(),
        light_balance: [1.0; 4],
        cloud_rendering: [1.0, 1.0, 1.0, 1.0],
        cloud_shadows: [1.0, 1.0, 0.85, 1.0],
        landform_lighting: [1.0, 0.7, 320.0, 0.0],
        landscape_visibility: [1.0; 4],
        light_contribution_visibility: [1; 4],
        layer_material_color_visibility: [1; 4],
    }
}

fn parse_options() -> Result<Command, String> {
    let mut options = default_options();
    let mut bench = false;
    let mut output_dir = None;
    let mut baseline = None;
    let mut warmup_iterations = 5;
    let mut timed_iterations = 20;
    let mut width = None;
    let mut height = None;
    let mut bench_only_option = None;
    let mut capture_only_option = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = |arguments: &mut std::iter::Skip<std::env::Args>| {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))
        };
        match argument.as_str() {
            "--bench" | "--output-dir" | "--baseline" | "--warmup" | "--iterations" => {
                bench_only_option.get_or_insert_with(|| argument.clone());
            }
            "--width" | "--height" | "--help" | "-h" => {}
            _ => {
                capture_only_option.get_or_insert_with(|| argument.clone());
            }
        }
        match argument.as_str() {
            "--bench" => bench = true,
            "--output-dir" => output_dir = Some(PathBuf::from(value(&mut arguments)?)),
            "--baseline" => baseline = Some(PathBuf::from(value(&mut arguments)?)),
            "--warmup" => warmup_iterations = parse(&value(&mut arguments)?, &argument)?,
            "--iterations" => timed_iterations = parse(&value(&mut arguments)?, &argument)?,
            "--output" => options.output = value(&mut arguments)?.into(),
            "--width" => width = Some(parse(&value(&mut arguments)?, &argument)?),
            "--height" => height = Some(parse(&value(&mut arguments)?, &argument)?),
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
            "--cloud-rendering" => {
                options.cloud_rendering = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--cloud-shadows" => {
                options.cloud_shadows = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--landform-lighting" => {
                options.landform_lighting = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--landscape-visibility" => {
                options.landscape_visibility = parse_f32x4(&value(&mut arguments)?, &argument)?
            }
            "--light-contributions" => {
                options.light_contribution_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
            }
            "--layer-material-colors" => {
                options.layer_material_color_visibility =
                    parse_u32x4(&value(&mut arguments)?, &argument)?
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
        return Ok(Command::Bench(bench::BenchOptions {
            output_dir: output_dir.ok_or("--bench requires --output-dir")?,
            baseline,
            width,
            height,
            warmup_iterations,
            timed_iterations,
        }));
    }
    if let Some(option) = bench_only_option {
        return Err(format!("{option} requires --bench"));
    }
    options.width = width.unwrap_or(options.width);
    options.height = height.unwrap_or(options.height);
    validate_capture_output_path(&options.output)?;
    validate_capture_dimensions(options.width, options.height, u32::MAX)?;
    validate_time_seconds(options.cloud_time_seconds, "--cloud-time")?;
    validate_time_seconds(options.world_time_seconds, "--world-time")?;
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
    Ok(Command::Capture(options))
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
        "scene_cached" | "scene_cached_activity" | "scene_cached_hits" => ShaderSelection {
            id: value.to_owned(),
            module: None,
            constants: Vec::new(),
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
        "lighting_combined_transmittance" => diagnostic(
            value,
            "lighting_cloud_shadow",
            &[("CLOUD_SHADOW_OUTPUT", 2.0)],
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
                ("LIGHTING_MODEL_EXPOSURE_GAIN", 16.0),
            ],
        ),
        name if name.starts_with("vision_") => {
            let (output, gain, color, signal, phase) = match name {
                "vision_noise_day" => (22.0, 1.0, [0.0; 3], 0.0001, 0.0),
                "vision_noise_dark" => (22.0, 16.0, [0.0; 3], 0.0001, 0.0),
                "vision_noise_bright" => (22.0, 16.0, [0.0; 3], 0.01, 0.0),
                "vision_noise_later" => (22.0, 16.0, [0.0; 3], 0.0001, 0.5),
                "vision_noise_period" => (22.0, 16.0, [0.0; 3], 0.0001, 4096.0),
                "vision_noise_before_wrap" => (22.0, 16.0, [0.0; 3], 0.0001, 4095.999),
                "vision_noise_after_wrap" => (22.0, 16.0, [0.0; 3], 0.0001, 0.001),
                "vision_noise_black" => (23.0, 16.0, [0.0; 3], 0.0001, 0.0),
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
                        "mesopic" => 4.0,
                        "rods" => 16.0,
                        _ => return Err(format!("unknown diagnostic: {name}")),
                    };
                    (21.0, gain, color, 0.0001, 0.0)
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
        "lighting_cloud_shadow_disc_radius" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 13.0)],
        ),
        "lighting_low_cloud_canopy_shadow_disc_radius" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 14.0)],
        ),
        "lighting_projected_low_cloud_receiver" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 15.0)],
        ),
        "lighting_below_datum_cloud_projection" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 16.0)],
        ),
        "lighting_cloud_disc_transverse_spread" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 17.0)],
        ),
        "lighting_projected_low_cloud_direct_color" => diagnostic(
            value,
            "lighting_model_regressions",
            &[("LIGHTING_MODEL_REGRESSION_OUTPUT", 18.0)],
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
        "lighting_receiver_atmospheric_transmittance_elevated" => diagnostic(
            value,
            "lighting_model_regressions",
            &[
                ("LIGHTING_MODEL_REGRESSION_OUTPUT", 19.0),
                ("LIGHTING_MODEL_SURFACE_HEIGHT_M", 600.0),
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
        "cloud_shadow_cache_tiles"
        | "cloud_shadow_cache_marker_integrity"
        | "cloud_shadow_cache_fallback_integrity"
        | "cloud_shadow_cache_tide_initialization" => ShaderSelection {
            id: value.to_owned(),
            module: Some("cloud_shadow_cache"),
            constants: Vec::new(),
        },
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
           --cloud-time SECONDS   nonnegative deterministic cloud animation time (default 37)\n\
           --world-time SECONDS   nonnegative deterministic world time (default 0)\n\
           --diagnostic NAME      semantic diagnostic output (default scene)\n\n\
           --light-balance V,V,V,V  finite Sun, Moon, clear-sky, and canopy scales\n\
           --light-contributions V,V,V,V  enable the four light contributions\n\
           --cloud-rendering V,V,V,V  finite lower-cloud depth, opacity, storm, parallax\n\
           --cloud-shadows V,V,V,V  finite low/canopy strength, extinction, contrast\n\
           --landform-lighting V,V,V,V  finite relief, shadow, reach, padding\n\
           --layer-material-colors V,V,V,V  lower-cloud material color rows\n\
           --landscape-visibility V,V,V,V  finite landscape mechanism visibility\n\n\
         The PNG gets a JSON sidecar containing world inputs, derived conditions,\n\
         and image metrics.\n\n\
         playground-capture --bench --output-dir DIR [--baseline DIR] [options]\n\n\
         Times the production scene, environment-field and shadow-cache passes included,\n\
         for each built-in scenario in playground/src/bench-scenarios.json, once as shipped\n\
         and once with EVALUATE_ALL_BRANCHES forcing normally skipped work.\n\n\
         Bench options:\n\
           --output-dir DIR       receives report.json, one PNG per scenario, and contact-sheet.png\n\
           --baseline DIR         earlier bench output to compare timings and images against;\n\
                                  writes a <scenario>-diff.png heat image per scenario\n\
           --width N --height N   render dimensions (default 1920x1080)\n\
           --warmup N             untimed frames per scenario and mode, also used for one\n\
                                  untimed sweep over every scenario first (default 5)\n\
           --iterations N         timed frames per scenario and mode (default 20)\n\n\
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
            "lighting_combined_transmittance",
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
        assert!(validate_time_seconds(0.0, "--cloud-time").is_ok());
        assert!(validate_time_seconds(365.0 * 86_400.0, "--cloud-time").is_ok());
        assert!(validate_time_seconds((2.0_f64).powi(39) - 1.0, "--cloud-time").is_ok());
        assert!(validate_time_seconds((2.0_f64).powi(39), "--cloud-time").is_err());
        assert!(validate_time_seconds(1.0e16, "--world-time").is_err());
        assert!(validate_time_seconds(-1.0, "--cloud-time").is_err());
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
        assert!(parse_f32x4("1,NaN,3,4", "--cloud-shadows").is_err());
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
    let cached_scene = matches!(
        options.diagnostic.id.as_str(),
        "scene_cached" | "scene_cached_activity" | "scene_cached_hits"
    );
    let fragment_module = options
        .diagnostic
        .module
        .filter(|name| *name != "cloud_shadow_cache");
    let shader_source = fragment_module.map_or_else(
        || {
            if cached_scene {
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
    let environment_fields = fragment_module
        .is_none()
        .then(|| CaptureEnvironmentFields::new(&device, &scene_layout));
    let cloud_shadow_cache = cached_scene.then(|| {
        CaptureCloudShadowCache::new(
            &device,
            &scene_layout,
            &playground_layout,
            browser::playground_cloud_shadow_cache_shader_source(),
            &[],
            browser::cloud_shadow_cache_layout_with_budget(
                options.width,
                options.height,
                device.limits().max_texture_dimension_2d,
                CAPTURE_SHADOW_CACHE_ATLAS_BYTES,
            ),
            options.width,
            options.height,
            match options.diagnostic.id.as_str() {
                "scene_cached_activity" => 1,
                "scene_cached_hits" => 4,
                _ => 0,
            },
        )
    });
    let mut bind_group_layouts = vec![
        Some(
            environment_fields
                .as_ref()
                .map_or(&scene_layout, |fields| &fields.scene_layout),
        ),
        Some(&playground_layout),
    ];
    if let Some(cache) = &cloud_shadow_cache {
        bind_group_layouts.push(Some(&cache.sample_layout));
    }
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("playground capture pipeline layout"),
        bind_group_layouts: &bind_group_layouts,
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
                constants: &options.diagnostic.constants,
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
        cloud_rendering: options.cloud_rendering,
        terrain_tuning: [1.0, 3.0, 0.08, 0.0],
        water_tuning: [0.5, 1.0, 0.0, 0.0],
        landform_lighting: options.landform_lighting,
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
        layer_material_color_visibility: options.layer_material_color_visibility,
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
        cloud_shadow_cache.as_ref(),
        environment_fields.as_ref(),
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
    cloud_shadow_cache: Option<&CaptureCloudShadowCache>,
    environment_fields: Option<&CaptureEnvironmentFields>,
    options: &Options,
) -> Result<Vec<u8>, String> {
    let uniforms = scene_uniforms(options)?;
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
            match options.diagnostic.id.as_str() {
                "cloud_shadow_cache_marker_integrity" => {
                    Some("test_cloud_shadow_cache_marker_integrity")
                }
                "cloud_shadow_cache_fallback_integrity" => {
                    Some("test_cloud_shadow_cache_fallback_integrity")
                }
                "cloud_shadow_cache_tide_initialization" => {
                    Some("test_cloud_shadow_cache_tide_initialization")
                }
                _ => None,
            },
        );
    }
    let texture = capture_texture(device, options.width, options.height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("playground capture encoder"),
    });
    if let Some(cache) = cloud_shadow_cache {
        cache.encode(
            &mut encoder,
            &scene_bind_group,
            playground_bind_group,
            options.width,
            options.height,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            None,
        );
    }
    let fragment_scene_bind_group = match environment_fields {
        Some(fields) => {
            fields.encode(
                &mut encoder,
                &scene_bind_group,
                &capture_environment_field_grid(options)?,
                None,
            );
            fields.scene_bind_group(device, &scene_buffer)
        }
        None => scene_bind_group,
    };
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
        if let Some(cache) = cloud_shadow_cache {
            pass.set_bind_group(2, &cache.sample_bind_group, &[]);
        }
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
    read_texture(device, queue, encoder, &texture)
}

fn capture_environment_field_grid(options: &Options) -> Result<EnvironmentFieldGrid, String> {
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    Ok(environment_field_grid(
        options.world_width_m,
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

fn scene_uniforms(options: &Options) -> Result<SceneUniforms, String> {
    let environment_field_grid = capture_environment_field_grid(options)?;
    let world_geometry = WorldGeometry::new(options.world_width_m, options.world_height_m)
        .map_err(|error| error.to_string())?;
    let canonical_camera_m =
        world_geometry.wrap_position_m([options.camera_x_m, options.camera_y_m]);
    let precipitation_epoch = (options.cloud_time_seconds / 1800.0).floor() as u64;
    let precipitation_phase = (options.cloud_time_seconds.rem_euclid(1800.0) / 1800.0) as f32;
    let (tick, tick_fraction) = split_tick_seconds(options.world_time_seconds);
    let conditions =
        world_celestial_conditions(world_geometry, options.world_time_seconds, [0.0; 2].into());
    let orbit = celestial_orbit(conditions);
    let (cloud_tick, cloud_tick_fraction) = split_tick_seconds(options.cloud_time_seconds);
    let scalar_time = scalar_field_time_coordinates_interpolated(cloud_tick, cloud_tick_fraction);
    let pressure_time =
        vector_field_time_coordinates_interpolated(world_geometry, cloud_tick, cloud_tick_fraction);
    let current_time =
        vector_field_time_coordinates_interpolated(world_geometry, tick, tick_fraction);
    let cloud_time = cloud_field_time_coordinates_interpolated(cloud_tick, cloud_tick_fraction);
    let local_celestial = world_celestial_conditions(
        world_geometry,
        options.world_time_seconds,
        canonical_camera_m.into(),
    );
    let exposure = exposure_target(celestial_lighting(local_celestial).ambient_intensity);
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
        precipitation_phase_and_padding: [precipitation_phase, 0.0, 0.0, 0.0],
        world_phases_and_pixel_footprint_m: [
            conditions.day_phase,
            conditions.season_phase,
            conditions.lunar_longitude_phase,
            visual_pixel_footprint_m(options.view_height_m, options.height),
        ],
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
        view_cloud_motion_m: [0.0; 4],
        cloud_time_coordinates: [cloud_time.low, cloud_time.upper, 0.0, 0.0],
        exposure_and_output: [exposure, 1.0, 0.0, 0.0],
        environment_field_grid_m: environment_field_grid.uniforms,
        environment_field_texels: environment_field_grid.texel_uniforms(),
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
        view_formats: &[],
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
    test_entry_point: Option<&str>,
) -> Result<Vec<u8>, String> {
    let layout = browser::cloud_shadow_cache_layout_with_budget(
        viewport_width,
        viewport_height,
        device.limits().max_texture_dimension_2d,
        CAPTURE_SHADOW_CACHE_ATLAS_BYTES,
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
        size: u64::from(layout.table_entries) * 8,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
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
    let mut shader_source = browser::playground_cloud_shadow_cache_shader_source().to_owned();
    if let Some(test_entry_point) = test_entry_point {
        shader_source.push_str(match test_entry_point {
            "test_cloud_shadow_cache_marker_integrity" => {
                r#"
@compute @workgroup_size(1)
fn test_cloud_shadow_cache_marker_integrity() {
    let key = vec2<f32>(0.0);
    mark_cloud_shadow_cache_tile_key(key, 0u, 0u);
    mark_cloud_shadow_cache_tile_key(key, 0u, 1u);
    mark_cloud_shadow_cache_tile_key(key, 0u, 2u);
}
"#
            }
            "test_cloud_shadow_cache_fallback_integrity" => {
                r#"
@compute @workgroup_size(1)
fn test_cloud_shadow_cache_fallback_integrity() {
    for (var key_x = 0u; key_x <= cloud_shadow_cache.maximum_tiles; key_x++) {
        let key = vec2<f32>(f32(key_x), 0.0);
        mark_cloud_shadow_cache_tile_key(key, 0u, 0u);
        mark_cloud_shadow_cache_tile_key(key, 0u, 1u);
    }
}
"#
            }
            "test_cloud_shadow_cache_tide_initialization" => {
                r#"
@compute @workgroup_size(1)
fn test_cloud_shadow_cache_tide_initialization() {
    initialize_cloud_shadow_cache_discovery(scene.camera_and_view_m.xy);
    if abs(tide_height_m()) > 0.05 {
        let key = vec2<f32>(0.0);
        mark_cloud_shadow_cache_tile_key(key, 0u, 0u);
        mark_cloud_shadow_cache_tile_key(key, 0u, 1u);
    }
}
"#
            }
            _ => unreachable!(),
        });
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("capture cloud shadow cache shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
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
        entry_point: Some(test_entry_point.unwrap_or("discover_cloud_shadow_cache_tiles")),
        compilation_options: Default::default(),
        cache: None,
    });
    let entries_offset = 16;
    let readback_size = entries_offset + u64::from(layout.table_entries) * 8;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture cloud shadow cache readback"),
        size: readback_size,
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
        if test_entry_point.is_some() {
            pass.dispatch_workgroups(1, 1, 1);
        } else {
            pass.dispatch_workgroups(
                viewport_width.div_ceil(128),
                viewport_height.div_ceil(128),
                1,
            );
        }
    }
    encoder.copy_buffer_to_buffer(&header, 0, &readback, 0, 16);
    encoder.copy_buffer_to_buffer(
        &entries,
        0,
        &readback,
        entries_offset,
        u64::from(layout.table_entries) * 8,
    );
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
        .map_err(|error| format!("failed to read cache discovery buffers: {error}"))?;
    let tile_count = u32::from_ne_bytes(data[4..8].try_into().unwrap());
    let entries_start = usize::try_from(entries_offset).unwrap();
    let mut allocated_slots = data[entries_start..]
        .as_chunks::<8>()
        .0
        .iter()
        .filter_map(|entry| {
            let slot = u32::from_ne_bytes(entry[4..].try_into().unwrap());
            (slot & 0x8000_0000 != 0)
                .then_some(slot & 0x7fff_ffff)
                .filter(|slot| *slot < layout.maximum_tiles)
        })
        .collect::<Vec<_>>();
    allocated_slots.sort_unstable();
    let valid = allocated_slots.iter().copied().eq(0..tile_count);
    let pixel = if tile_count > 0 && valid {
        [255, 255, 255, 255]
    } else {
        [0, 0, 0, 255]
    };
    let pixel_count = usize::try_from(u64::from(viewport_width) * u64::from(viewport_height))
        .map_err(|_| "capture pixel count exceeds supported arithmetic".to_owned())?;
    Ok((0..pixel_count).flat_map(|_| pixel).collect())
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
    let (world_tick, world_tick_fraction) = split_tick_seconds(options.world_time_seconds);
    let (cloud_tick, cloud_tick_fraction) = split_tick_seconds(options.cloud_time_seconds);
    let sample = procedural_world_environment_for_render_clocks(
        world,
        options.seed,
        world_tick,
        world_tick_fraction,
        cloud_tick,
        cloud_tick_fraction,
        cloud_tick,
        cloud_tick_fraction,
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
            settled_exposure_multiplier: exposure_target(celestial.ambient_intensity),
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
