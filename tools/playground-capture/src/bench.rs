use std::{
    cell::RefCell,
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use browser::{
    BAKED_SURFACE_LEVELS, BakedSurface, BakedSurfaceFrame, BakedSurfaceSizes, FieldGrids,
    GraphicsQuality, MINIMUM_RENDER_SCALE, TerrainCache, TerrainCacheFrame, UpscaleTargets,
    Upscaler, baked_surface_sizes, scene_render_size, visual_pixel_footprint_m,
};

use super::{
    CaptureFieldBindings, CaptureFields, Options, PlaygroundUniforms, Vertex, bind_group,
    capture_baked_surface_frame, capture_field_grids, capture_terrain_cache_frame, capture_texture,
    default_options, fullscreen_vertices, hardware_device, parse_seed, read_texture,
    scene_uniforms, uniform_layout, validate_capture_dimensions, validate_capture_spatial_inputs,
    validate_time_seconds, write_png,
};

const SCENARIOS: &str = include_str!("../../../playground/src/bench-scenarios.json");
/// A pixel counts as visibly changed when one 8-bit channel moves by more than this.
const VISIBLE_DIFFERENCE_LEVELS: u8 = 2;
const DIFF_IMAGE_GAIN: u8 = 16;
const CONTACT_SHEET_COLUMNS: usize = 3;
const CONTACT_SHEET_TILE_WIDTH: u32 = 480;
/// Environment field, occluder field, scene, upscale, terrain cache, and baked
/// surface begin/end pairs.
const TIMESTAMP_COUNT: u32 = 12;
/// More frames than a cold start takes to settle at the bench's sizes.
const MAXIMUM_COLD_START_FRAMES: u32 = 64;

pub struct BenchOptions {
    pub output_dir: PathBuf,
    pub baseline: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub warmup_iterations: u32,
    pub timed_iterations: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    id: String,
    exercises: String,
    seed: String,
    camera_m: [f32; 2],
    view_height_m: f32,
    world_time_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    render_scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quality: Option<String>,
}

impl Scenario {
    fn graphics_quality(&self) -> Result<GraphicsQuality, String> {
        self.quality
            .as_deref()
            .map_or(Ok(GraphicsQuality::High), |name| {
                GraphicsQuality::parse(name)
                    .ok_or_else(|| format!("{}: invalid quality {name}", self.id))
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Timing {
    median_ms: f64,
    min_ms: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModeReport {
    #[serde(default)]
    fields: Option<Timing>,
    #[serde(default)]
    occluders: Option<Timing>,
    scene: Option<Timing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upscale: Option<Timing>,
    #[serde(default)]
    terrain_cache: Option<Timing>,
    #[serde(default)]
    baked_surface: Option<Timing>,
    total: Timing,
    total_ns_per_pixel: f64,
    #[serde(default)]
    discarded_frames: u32,
}

/// Shipped frames from an empty terrain cache and bake until both settle, as
/// after a teleport.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ColdStart {
    frames: u32,
    first_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageDifference {
    /// Mean of each pixel's largest channel difference, in 8-bit levels.
    mean_absolute: f64,
    /// 99th percentile of each pixel's largest channel difference.
    p99: u8,
    max: u8,
    visible_fraction: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselineComparison {
    total_ratio: f64,
    scene_ratio: Option<f64>,
    all_branches_total_ratio: f64,
    image_difference: ImageDifference,
    diff_image: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioReport {
    #[serde(flatten)]
    scenario: Scenario,
    image: String,
    shipped: ModeReport,
    all_branches: ModeReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cold_start: Option<ColdStart>,
    all_branches_image_difference: ImageDifference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline: Option<BaselineComparison>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchReport {
    adapter: String,
    timing: String,
    width: u32,
    height: u32,
    warmup_iterations: u32,
    timed_iterations: u32,
    contact_sheet: String,
    scenarios: Vec<ScenarioReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    baseline_notes: Vec<String>,
}

struct ModeRenderer {
    /// Indexed by `GraphicsQuality`.
    pipelines: [wgpu::RenderPipeline; 3],
    /// One per bake size, as the game sizes its bake for the scene.
    fields: Vec<(BakedSurfaceSizes, CaptureFields)>,
    /// The scenario the cache and surface were last filled for; another one
    /// starts from empty ones, like a teleport.
    surfaces: RefCell<(String, TerrainCache, BakedSurface)>,
    /// Refills the active cache level and bakes one frame budget of the read
    /// surface level every frame.
    refill: bool,
}

impl ModeRenderer {
    fn new(
        device: &wgpu::Device,
        scene_layout: &wgpu::BindGroupLayout,
        unused_layout: &wgpu::BindGroupLayout,
        evaluate_all_branches: bool,
        baked_surface_sizes: &[BakedSurfaceSizes],
    ) -> Result<Self, String> {
        // WebGPU rejects constants a module does not declare, and compaction
        // drops the override from modules that never consult it.
        let declares_override = |source: &str| source.contains("override EVALUATE_ALL_BRANCHES");
        let constants = |source: &str| -> &[(&str, f64)] {
            if evaluate_all_branches && declares_override(source) {
                &[("EVALUATE_ALL_BRANCHES", 1.0)]
            } else {
                &[]
            }
        };
        let occluder_source = browser::occluder_field_shader_source();
        let baked_surface_source = browser::baked_surface_shader_source();
        let scene_source = browser::production_scene_shader_source();
        if evaluate_all_branches && !declares_override(scene_source) {
            return Err(
                "the production scene shader no longer declares EVALUATE_ALL_BRANCHES".to_owned(),
            );
        }
        if evaluate_all_branches && !declares_override(baked_surface_source) {
            return Err(
                "the baked surface shader no longer declares EVALUATE_ALL_BRANCHES".to_owned(),
            );
        }
        let fields: Vec<_> = baked_surface_sizes
            .iter()
            .map(|&sizes| {
                (
                    sizes,
                    CaptureFields::new(
                        device,
                        scene_layout,
                        unused_layout,
                        occluder_source,
                        constants(occluder_source),
                        baked_surface_source,
                        constants(baked_surface_source),
                        sizes,
                    ),
                )
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bench production scene shader"),
            source: wgpu::ShaderSource::Wgsl(scene_source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bench production scene pipeline layout"),
            bind_group_layouts: &[Some(&fields[0].1.scene_layout), Some(unused_layout)],
            immediate_size: 0,
        });
        let pipeline = |quality: GraphicsQuality| {
            let scene_constants = [constants(scene_source), quality.scene_constants()].concat();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bench production scene pipeline"),
                layout: Some(&layout),
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
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &scene_constants,
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
        Ok(Self {
            pipelines: [
                GraphicsQuality::Low,
                GraphicsQuality::Medium,
                GraphicsQuality::High,
            ]
            .map(pipeline),
            surfaces: RefCell::new((
                String::new(),
                TerrainCache::default(),
                BakedSurface::new(fields[0].0),
            )),
            fields,
            refill: evaluate_all_branches,
        })
    }

    fn fields(&self, sizes: BakedSurfaceSizes) -> &CaptureFields {
        &self
            .fields
            .iter()
            .find(|(fields_sizes, _)| *fields_sizes == sizes)
            .expect("every scenario's bake size has fields")
            .1
    }

    fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bindings: &FrameBindings<'_>,
        target: &wgpu::Texture,
        timestamps: Option<&wgpu::QuerySet>,
    ) -> Result<(), String> {
        let mut surfaces = self.surfaces.borrow_mut();
        let (scenario_id, terrain_cache, baked_surface) = &mut *surfaces;
        if scenario_id != bindings.scenario_id {
            // Fill the new scenario's cache and surface completely first, so
            // warm-up and timed frames see them settled whatever their count.
            *scenario_id = bindings.scenario_id.to_owned();
            *terrain_cache = TerrainCache::default();
            *baked_surface = BakedSurface::new(bindings.baked_surface_sizes);
            loop {
                let cache_frame = capture_terrain_cache_frame(bindings.options, terrain_cache)?;
                let surface_frame = capture_baked_surface_frame(
                    bindings.options,
                    bindings.pixel_footprint_m,
                    baked_surface,
                    false,
                )?;
                if !cache_frame.has_fills() && !surface_frame.has_fills() {
                    break;
                }
                let mut fill_encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("bench surface settle encoder"),
                    });
                let fields = self.fields(bindings.baked_surface_sizes);
                fields.encode_terrain_cache(
                    queue,
                    &mut fill_encoder,
                    bindings.scene,
                    &cache_frame,
                    None,
                );
                fields.encode_baked_surface(
                    queue,
                    &mut fill_encoder,
                    bindings.fields,
                    bindings.unused,
                    &surface_frame,
                    None,
                );
                queue.submit([fill_encoder.finish()]);
                terrain_cache.commit();
                baked_surface.commit();
            }
        }
        if self.refill {
            terrain_cache.invalidate();
        }
        let terrain_cache_frame = capture_terrain_cache_frame(bindings.options, terrain_cache)?;
        terrain_cache.commit();
        let baked_surface_frame = capture_baked_surface_frame(
            bindings.options,
            bindings.pixel_footprint_m,
            baked_surface,
            self.refill,
        )?;
        baked_surface.commit();
        // Scene uniforms are written once per scenario; the active cache level
        // is complete within every frame and the surface settled, so their rows
        // match the scenario's.
        if (
            terrain_cache_frame.grid_m,
            terrain_cache_frame.texels,
            terrain_cache_frame.level,
        ) != bindings.terrain_cache_rows
        {
            return Err("terrain cache rows changed at a fixed camera".to_owned());
        }
        if baked_surface_rows(&baked_surface_frame) != bindings.baked_surface_rows {
            return Err("baked surface rows changed at a fixed camera".to_owned());
        }
        self.encode_frame(
            queue,
            encoder,
            bindings,
            &terrain_cache_frame,
            &baked_surface_frame,
            target,
            timestamps,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_frame(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bindings: &FrameBindings<'_>,
        terrain_cache_frame: &TerrainCacheFrame,
        baked_surface_frame: &BakedSurfaceFrame,
        target: &wgpu::Texture,
        timestamps: Option<&wgpu::QuerySet>,
    ) {
        let fields = self.fields(bindings.baked_surface_sizes);
        fields.encode_terrain_cache(
            queue,
            encoder,
            bindings.scene,
            terrain_cache_frame,
            timestamps,
        );
        fields.encode_baked_surface(
            queue,
            encoder,
            bindings.fields,
            bindings.unused,
            baked_surface_frame,
            timestamps,
        );
        fields.encode(
            encoder,
            bindings.scene,
            bindings.fields,
            bindings.unused,
            bindings.field_grids,
            timestamps,
        );
        let output_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bench scene pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: bindings
                    .upscale
                    .map_or(&output_view, |upscale| &upscale.scene_view),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps.map(|query_set| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(4),
                end_of_pass_write_index: Some(5),
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines[bindings.quality as usize]);
        pass.set_bind_group(0, &bindings.fields.scene, &[]);
        pass.set_bind_group(1, bindings.unused, &[]);
        pass.set_vertex_buffer(0, bindings.vertices.slice(..));
        pass.draw(0..6, 0..1);
        drop(pass);
        if let Some(upscale) = bindings.upscale {
            bindings.upscaler.encode(
                encoder,
                upscale,
                &target.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(wgpu::TextureFormat::Rgba8Unorm),
                    ..Default::default()
                }),
                timestamps.map(|query_set| (query_set, 6, 7)),
            );
        }
    }
}

type SurfaceRows = ([f32; 4], [u32; 4], [u32; 4]);

fn baked_surface_rows(frame: &BakedSurfaceFrame) -> SurfaceRows {
    (frame.grid_m, frame.texels, frame.level)
}

#[derive(Clone, Copy)]
struct FrameBindings<'a> {
    scenario_id: &'a str,
    quality: GraphicsQuality,
    options: &'a Options,
    pixel_footprint_m: f32,
    baked_surface_sizes: BakedSurfaceSizes,
    terrain_cache_rows: SurfaceRows,
    baked_surface_rows: SurfaceRows,
    scene: &'a wgpu::BindGroup,
    fields: &'a CaptureFieldBindings,
    field_grids: &'a FieldGrids,
    unused: &'a wgpu::BindGroup,
    vertices: &'a wgpu::Buffer,
    upscaler: &'a Upscaler,
    upscale: Option<&'a UpscaleTargets>,
}

struct ScenarioBindings {
    id: String,
    quality: GraphicsQuality,
    options: Options,
    pixel_footprint_m: f32,
    baked_surface_sizes: BakedSurfaceSizes,
    terrain_cache_rows: SurfaceRows,
    baked_surface_rows: SurfaceRows,
    scene: wgpu::BindGroup,
    /// Shipped, then forced.
    fields: [CaptureFieldBindings; 2],
    field_grids: FieldGrids,
    upscale: Option<UpscaleTargets>,
}

impl ScenarioBindings {
    fn frame<'a>(
        &'a self,
        mode: usize,
        unused: &'a wgpu::BindGroup,
        vertices: &'a wgpu::Buffer,
        upscaler: &'a Upscaler,
    ) -> FrameBindings<'a> {
        FrameBindings {
            scenario_id: &self.id,
            quality: self.quality,
            options: &self.options,
            pixel_footprint_m: self.pixel_footprint_m,
            baked_surface_sizes: self.baked_surface_sizes,
            terrain_cache_rows: self.terrain_cache_rows,
            baked_surface_rows: self.baked_surface_rows,
            scene: &self.scene,
            fields: &self.fields[mode],
            field_grids: &self.field_grids,
            unused,
            vertices,
            upscaler,
            upscale: self.upscale.as_ref(),
        }
    }
}

struct TimestampReadback {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f64,
}

pub async fn run(options: &BenchOptions) -> Result<(), String> {
    let scenarios = builtin_scenarios()?;
    if options
        .baseline
        .as_ref()
        .is_some_and(|baseline| same_directory(baseline, &options.output_dir))
    {
        return Err("--baseline must differ from --output-dir".to_owned());
    }
    let baseline = options
        .baseline
        .as_ref()
        .map(|directory| read_report(directory).map(|report| (directory, report)))
        .transpose()?;
    if let Some((_, report)) = &baseline
        && (report.width, report.height) != (options.width, options.height)
    {
        return Err(format!(
            "baseline was rendered at {}x{}; rerun with --width {} --height {}",
            report.width, report.height, report.width, report.height
        ));
    }
    fs::create_dir_all(&options.output_dir)
        .map_err(|error| format!("failed to create {}: {error}", options.output_dir.display()))?;

    let (adapter, device, queue) = hardware_device(wgpu::Features::TIMESTAMP_QUERY).await?;
    validate_capture_dimensions(
        options.width,
        options.height,
        device.limits().max_texture_dimension_2d,
    )?;
    let timestamps = device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
        .then(|| TimestampReadback {
            query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("bench pass timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: TIMESTAMP_COUNT,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bench timestamp resolve"),
                size: u64::from(TIMESTAMP_COUNT) * 8,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bench timestamp readback"),
                size: u64::from(TIMESTAMP_COUNT) * 8,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period_ns: f64::from(queue.get_timestamp_period()),
        });
    let scene_layout = uniform_layout(&device, "bench scene uniforms layout");
    let unused_layout = uniform_layout(&device, "bench unused group layout");
    let unused_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bench unused group buffer"),
        size: size_of::<PlaygroundUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let unused = bind_group(
        &device,
        &unused_layout,
        &unused_buffer,
        "bench unused group",
    );
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("bench fullscreen vertices"),
        contents: bytemuck::cast_slice(&fullscreen_vertices()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    eprintln!("compiling production pipelines");
    let scenario_bake_sizes = |scenario: &Scenario| {
        baked_surface_sizes(
            scene_render_size(
                options.width,
                options.height,
                scenario.render_scale.unwrap_or(1.0),
            ),
            default_options().bake_memory_bytes,
        )
    };
    let mut bake_sizes: Vec<BakedSurfaceSizes> = Vec::new();
    for sizes in scenarios.iter().map(scenario_bake_sizes) {
        if !bake_sizes.contains(&sizes) {
            bake_sizes.push(sizes);
        }
    }
    let renderers = [
        ModeRenderer::new(&device, &scene_layout, &unused_layout, false, &bake_sizes)?,
        ModeRenderer::new(&device, &scene_layout, &unused_layout, true, &bake_sizes)?,
    ];
    let upscaler = Upscaler::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let timing = if timestamps.is_some() {
        "timestamp-query"
    } else {
        "submit-to-completion wall clock"
    };
    let adapter_name = adapter.get_info().name;
    let mut notes = Vec::new();
    if let Some((_, report)) = &baseline {
        if report.timing != timing {
            return Err(format!(
                "baseline timing used {}, this device offers {timing}",
                report.timing
            ));
        }
        if report.adapter != adapter_name {
            notes.push(format!(
                "baseline adapter {} differs from {adapter_name}; timing ratios compare different GPUs",
                report.adapter
            ));
        }
        for previous in &report.scenarios {
            if !scenarios
                .iter()
                .any(|scenario| scenario.id == previous.scenario.id)
            {
                notes.push(format!(
                    "baseline scenario {} is not in this run",
                    previous.scenario.id
                ));
            }
        }
        for scenario in &scenarios {
            if !report
                .scenarios
                .iter()
                .any(|previous| previous.scenario.id == scenario.id)
            {
                notes.push(format!("scenario {} has no baseline", scenario.id));
            }
        }
    }
    let scene_bind_groups = scenarios
        .iter()
        .map(|scenario| {
            let scenario_options = scenario_options(scenario, options.width, options.height)?;
            let output_size = (options.width, options.height);
            let scene_size = scene_render_size(
                options.width,
                options.height,
                scenario.render_scale.unwrap_or(1.0),
            );
            let pixel_footprint_m = visual_pixel_footprint_m(scenario.view_height_m, scene_size.1);
            let terrain_cache_frame =
                capture_terrain_cache_frame(&scenario_options, &mut TerrainCache::default())?;
            let baked_surface_sizes = scenario_bake_sizes(scenario);
            let mut baked_surface = BakedSurface::new(baked_surface_sizes);
            let baked_surface_frame = loop {
                let frame = capture_baked_surface_frame(
                    &scenario_options,
                    pixel_footprint_m,
                    &mut baked_surface,
                    false,
                )?;
                baked_surface.commit();
                if !frame.has_fills() {
                    break frame;
                }
            };
            let mut uniforms = scene_uniforms(
                &scenario_options,
                &terrain_cache_frame,
                &baked_surface_frame,
            )?;
            uniforms.world_phases_and_pixel_footprint_m[3] = pixel_footprint_m;
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bench scene uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            Ok(ScenarioBindings {
                id: scenario.id.clone(),
                quality: scenario.graphics_quality()?,
                pixel_footprint_m,
                baked_surface_sizes,
                terrain_cache_rows: (
                    terrain_cache_frame.grid_m,
                    terrain_cache_frame.texels,
                    terrain_cache_frame.level,
                ),
                baked_surface_rows: baked_surface_rows(&baked_surface_frame),
                scene: bind_group(&device, &scene_layout, &buffer, "bench scene"),
                fields: renderers.each_ref().map(|renderer| {
                    renderer
                        .fields(baked_surface_sizes)
                        .bindings(&device, &buffer)
                }),
                field_grids: capture_field_grids(&scenario_options)?,
                options: scenario_options,
                upscale: (scene_size != output_size).then(|| {
                    upscaler.targets(
                        &device,
                        scene_size,
                        output_size,
                        wgpu::TextureFormat::Rgba8UnormSrgb,
                    )
                }),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    // GPU clocks settle lower under sustained load; one untimed sweep lets every
    // scenario, including the first, be timed in that settled state.
    eprintln!("warming up");
    for scene in &scene_bind_groups {
        for (mode, renderer) in renderers.iter().enumerate() {
            time_mode(
                &device,
                &queue,
                renderer,
                &scene.frame(mode, &unused, &vertices, &upscaler),
                timestamps.as_ref(),
                options,
                0,
            )?;
        }
    }
    let pixels = f64::from(options.width) * f64::from(options.height);
    let mut reports = Vec::new();
    let mut thumbnails = Vec::new();
    for (scenario, scene) in scenarios.into_iter().zip(&scene_bind_groups) {
        eprintln!("{}", scenario.id);
        let shipped_bindings = scene.frame(0, &unused, &vertices, &upscaler);
        let shipped = mode_report(
            time_mode(
                &device,
                &queue,
                &renderers[0],
                &shipped_bindings,
                timestamps.as_ref(),
                options,
                options.timed_iterations,
            )?,
            pixels,
        )?;
        let shipped_rgba =
            render_image(&device, &queue, &renderers[0], &shipped_bindings, options)?;
        let cold_start = time_cold_start(
            &device,
            &queue,
            &renderers[0],
            &scene_layout,
            &shipped_bindings,
            timestamps.as_ref(),
            options,
        )?;
        let all_branches_bindings = scene.frame(1, &unused, &vertices, &upscaler);
        let all_branches = mode_report(
            time_mode(
                &device,
                &queue,
                &renderers[1],
                &all_branches_bindings,
                timestamps.as_ref(),
                options,
                options.timed_iterations,
            )?,
            pixels,
        )?;
        let all_branches_rgba = render_image(
            &device,
            &queue,
            &renderers[1],
            &all_branches_bindings,
            options,
        )?;
        let image = format!("{}.png", scenario.id);
        write_png(
            &options.output_dir.join(&image),
            options.width,
            options.height,
            &shipped_rgba,
        )?;
        thumbnails.push(thumbnail(&shipped_rgba, options.width, options.height));
        let baseline = match &baseline {
            Some((directory, report)) => report
                .scenarios
                .iter()
                .find(|previous| previous.scenario.id == scenario.id)
                .map(|previous| {
                    compare_with_baseline(
                        directory,
                        previous,
                        &scenario.id,
                        &shipped,
                        &all_branches,
                        &shipped_rgba,
                        options,
                    )
                })
                .transpose()?,
            None => None,
        };
        reports.push(ScenarioReport {
            image,
            shipped,
            all_branches,
            cold_start,
            all_branches_image_difference: image_difference(&pixel_differences(
                &shipped_rgba,
                &all_branches_rgba,
            )),
            baseline,
            scenario,
        });
    }
    let contact_sheet = "contact-sheet.png".to_owned();
    write_contact_sheet(&options.output_dir.join(&contact_sheet), &thumbnails)?;
    let report = BenchReport {
        adapter: adapter_name,
        timing: timing.to_owned(),
        width: options.width,
        height: options.height,
        warmup_iterations: options.warmup_iterations,
        timed_iterations: options.timed_iterations,
        contact_sheet,
        scenarios: reports,
        baseline_notes: notes,
    };
    let report_path = options.output_dir.join("report.json");
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to serialize report: {error}"))?,
    )
    .map_err(|error| format!("failed to write {}: {error}", report_path.display()))?;
    print_summary(&report);
    Ok(())
}

fn builtin_scenarios() -> Result<Vec<Scenario>, String> {
    serde_json::from_str(SCENARIOS).map_err(|error| format!("invalid bench scenarios: {error}"))
}

fn scenario_options(scenario: &Scenario, width: u32, height: u32) -> Result<Options, String> {
    let [camera_x_m, camera_y_m] = scenario.camera_m;
    validate_capture_spatial_inputs(camera_x_m, camera_y_m, scenario.view_height_m)?;
    if scenario
        .render_scale
        .is_some_and(|scale| !(MINIMUM_RENDER_SCALE..=1.0).contains(&scale))
    {
        return Err(format!(
            "renderScale must be from {MINIMUM_RENDER_SCALE} through 1"
        ));
    }
    validate_time_seconds(scenario.world_time_seconds, "worldTimeSeconds")?;
    scenario.graphics_quality()?;
    Ok(Options {
        width,
        height,
        camera_x_m,
        camera_y_m,
        view_height_m: scenario.view_height_m,
        seed: parse_seed(&scenario.seed)?,
        world_time_seconds: scenario.world_time_seconds,
        ..default_options()
    })
}

/// Pass durations in milliseconds: environment fields, occluder fields, scene,
/// upscale, first begin to last end, terrain cache, and baked surface.
type FrameTimes = [Option<f64>; 7];

struct TimedFrames {
    frames: Vec<FrameTimes>,
    /// Frames whose timestamps ran backwards, which some drivers report.
    discarded: u32,
}

fn time_mode(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &ModeRenderer,
    bindings: &FrameBindings<'_>,
    timestamps: Option<&TimestampReadback>,
    options: &BenchOptions,
    timed_frames: u32,
) -> Result<TimedFrames, String> {
    let target = capture_texture(device, options.width, options.height);
    let mut timed = TimedFrames {
        frames: Vec::new(),
        discarded: 0,
    };
    for iteration in 0..options.warmup_iterations + timed_frames {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench frame encoder"),
        });
        renderer.encode(
            device,
            queue,
            &mut encoder,
            bindings,
            &target,
            timestamps.map(|timestamps| &timestamps.query_set),
        )?;
        let frame = submit_timed(
            device,
            queue,
            encoder,
            timestamps,
            bindings.upscale.is_some(),
        )?;
        if iteration < options.warmup_iterations {
            continue;
        }
        let complete = if timestamps.is_some() {
            frame[..3].iter().all(Option::is_some)
                && frame[3].is_some() == bindings.upscale.is_some()
                && frame[4..].iter().all(Option::is_some)
        } else {
            frame[4].is_some()
        };
        if complete {
            timed.frames.push(frame);
        } else {
            timed.discarded += 1;
        }
    }
    Ok(timed)
}

/// Times frames from an empty terrain cache and bake, writing scene uniforms
/// each frame since the read rows change while they fill.
fn time_cold_start(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &ModeRenderer,
    scene_layout: &wgpu::BindGroupLayout,
    bindings: &FrameBindings<'_>,
    timestamps: Option<&TimestampReadback>,
    options: &BenchOptions,
) -> Result<Option<ColdStart>, String> {
    let target = capture_texture(device, options.width, options.height);
    let mut terrain_cache = TerrainCache::default();
    let mut baked_surface = BakedSurface::new(bindings.baked_surface_sizes);
    let mut totals = Vec::new();
    for _ in 0..MAXIMUM_COLD_START_FRAMES {
        let cache_frame = capture_terrain_cache_frame(bindings.options, &mut terrain_cache)?;
        terrain_cache.commit();
        let surface_frame = capture_baked_surface_frame(
            bindings.options,
            bindings.pixel_footprint_m,
            &mut baked_surface,
            false,
        )?;
        baked_surface.commit();
        if !cache_frame.has_fills() && !surface_frame.has_fills() {
            break;
        }
        let mut uniforms = scene_uniforms(bindings.options, &cache_frame, &surface_frame)?;
        uniforms.world_phases_and_pixel_footprint_m[3] = bindings.pixel_footprint_m;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bench cold start scene uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let scene = bind_group(device, scene_layout, &buffer, "bench cold start scene");
        let fields = renderer
            .fields(bindings.baked_surface_sizes)
            .bindings(device, &buffer);
        let frame_bindings = FrameBindings {
            scene: &scene,
            fields: &fields,
            ..*bindings
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench cold start encoder"),
        });
        renderer.encode_frame(
            queue,
            &mut encoder,
            &frame_bindings,
            &cache_frame,
            &surface_frame,
            &target,
            timestamps.map(|timestamps| &timestamps.query_set),
        );
        let frame = submit_timed(
            device,
            queue,
            encoder,
            timestamps,
            bindings.upscale.is_some(),
        )?;
        totals.extend(frame[4]);
    }
    Ok(totals.first().map(|&first_ms| ColdStart {
        frames: totals.len() as u32,
        first_ms,
        max_ms: totals.iter().copied().fold(0.0, f64::max),
    }))
}

/// Submits a frame and returns its pass durations.
fn submit_timed(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    timestamps: Option<&TimestampReadback>,
    upscaled: bool,
) -> Result<FrameTimes, String> {
    if let Some(timestamps) = timestamps {
        encoder.resolve_query_set(
            &timestamps.query_set,
            0..TIMESTAMP_COUNT,
            &timestamps.resolve,
            0,
        );
        encoder.copy_buffer_to_buffer(
            &timestamps.resolve,
            0,
            &timestamps.readback,
            0,
            u64::from(TIMESTAMP_COUNT) * 8,
        );
    }
    let started = Instant::now();
    queue.submit([encoder.finish()]);
    let mapped = timestamps.map(|timestamps| {
        let (sender, receiver) = std::sync::mpsc::channel();
        timestamps
            .readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        receiver
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("bench GPU poll failed: {error}"))?;
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let frame = match (timestamps, mapped) {
        (Some(timestamps), Some(mapped)) => {
            mapped
                .recv()
                .map_err(|error| format!("timestamp readback stopped: {error}"))?
                .map_err(|error| format!("timestamp readback failed: {error}"))?;
            let ticks: Vec<u64> = timestamps
                .readback
                .slice(..)
                .get_mapped_range()
                .map_err(|error| format!("failed to read timestamps: {error}"))?
                .as_chunks::<8>()
                .0
                .iter()
                .map(|bytes| u64::from_ne_bytes(*bytes))
                .collect();
            timestamps.readback.unmap();
            let milliseconds = |begin: usize, end: usize| {
                ticks[end]
                    .checked_sub(ticks[begin])
                    .map(|ticks| ticks as f64 * timestamps.period_ns / 1e6)
            };
            let last = if upscaled { 7 } else { 5 };
            [
                milliseconds(0, 1),
                milliseconds(2, 3),
                milliseconds(4, 5),
                milliseconds(6, 7).filter(|_| upscaled),
                milliseconds(8, last),
                milliseconds(8, 9),
                milliseconds(10, 11),
            ]
        }
        _ => [None, None, None, None, Some(wall_ms), None, None],
    };
    Ok(frame)
}

fn render_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &ModeRenderer,
    bindings: &FrameBindings<'_>,
    options: &BenchOptions,
) -> Result<Vec<u8>, String> {
    let texture = capture_texture(device, options.width, options.height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bench image encoder"),
    });
    renderer.encode(device, queue, &mut encoder, bindings, &texture, None)?;
    read_texture(device, queue, encoder, &texture)
}

fn mode_report(timed: TimedFrames, pixels: f64) -> Result<ModeReport, String> {
    let pass = |index: usize| {
        let mut samples: Vec<f64> = timed
            .frames
            .iter()
            .filter_map(|frame| frame[index])
            .collect();
        (!samples.is_empty()).then(|| Timing {
            median_ms: median(&mut samples),
            min_ms: samples.iter().copied().fold(f64::INFINITY, f64::min),
        })
    };
    let total = pass(4).ok_or("every timed frame had invalid timestamps")?;
    Ok(ModeReport {
        fields: pass(0),
        occluders: pass(1),
        scene: pass(2),
        upscale: pass(3),
        terrain_cache: pass(5),
        baked_surface: pass(6),
        total,
        total_ns_per_pixel: total.median_ms * 1e6 / pixels,
        discarded_frames: timed.discarded,
    })
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        0.5 * (samples[middle - 1] + samples[middle])
    } else {
        samples[middle]
    }
}

/// Each pixel's largest absolute RGB channel difference.
fn pixel_differences(first: &[u8], second: &[u8]) -> Vec<u8> {
    first
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.as_chunks::<4>().0)
        .map(|(first, second)| {
            (0..3)
                .map(|channel| first[channel].abs_diff(second[channel]))
                .max()
                .unwrap()
        })
        .collect()
}

fn image_difference(differences: &[u8]) -> ImageDifference {
    let mut histogram = [0usize; 256];
    for difference in differences {
        histogram[usize::from(*difference)] += 1;
    }
    let percentile_rank = (differences.len() as f64 * 0.99).ceil() as usize;
    let mut cumulative = 0;
    let p99 = histogram
        .iter()
        .position(|count| {
            cumulative += count;
            cumulative >= percentile_rank
        })
        .unwrap_or(0) as u8;
    ImageDifference {
        mean_absolute: differences
            .iter()
            .map(|value| f64::from(*value))
            .sum::<f64>()
            / differences.len().max(1) as f64,
        p99,
        max: differences.iter().copied().max().unwrap_or(0),
        visible_fraction: differences
            .iter()
            .filter(|value| **value > VISIBLE_DIFFERENCE_LEVELS)
            .count() as f64
            / differences.len().max(1) as f64,
    }
}

fn compare_with_baseline(
    directory: &Path,
    previous: &ScenarioReport,
    id: &str,
    shipped: &ModeReport,
    all_branches: &ModeReport,
    shipped_rgba: &[u8],
    options: &BenchOptions,
) -> Result<BaselineComparison, String> {
    let previous_rgba = read_png(&directory.join(&previous.image))?;
    let differences = pixel_differences(shipped_rgba, &previous_rgba);
    let diff_image = format!("{id}-diff.png");
    let heat: Vec<u8> = differences
        .iter()
        .flat_map(|difference| {
            let level = difference.saturating_mul(DIFF_IMAGE_GAIN);
            [level, level, level, 255]
        })
        .collect();
    write_png(
        &options.output_dir.join(&diff_image),
        options.width,
        options.height,
        &heat,
    )?;
    Ok(BaselineComparison {
        total_ratio: shipped.total.median_ms / previous.shipped.total.median_ms,
        scene_ratio: shipped
            .scene
            .zip(previous.shipped.scene)
            .map(|(current, previous)| current.median_ms / previous.median_ms),
        all_branches_total_ratio: all_branches.total.median_ms
            / previous.all_branches.total.median_ms,
        image_difference: image_difference(&differences),
        diff_image,
    })
}

fn read_report(directory: &Path) -> Result<BenchReport, String> {
    let path = directory.join("report.json");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn read_png(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?;
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader
        .next_frame(&mut rgba)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{} is not an 8-bit RGBA bench image",
            path.display()
        ));
    }
    rgba.truncate(info.buffer_size());
    Ok(rgba)
}

fn same_directory(first: &Path, second: &Path) -> bool {
    match (fs::canonicalize(first), fs::canonicalize(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => first == second,
    }
}

struct Thumbnail {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn thumbnail(rgba: &[u8], width: u32, height: u32) -> Thumbnail {
    let factor = width.div_ceil(CONTACT_SHEET_TILE_WIDTH).max(1);
    let (tile_width, tile_height) = ((width / factor).max(1), (height / factor).max(1));
    let mut tile = Vec::with_capacity((tile_width * tile_height * 4) as usize);
    for y in 0..tile_height {
        for x in 0..tile_width {
            let mut sum = [0u32; 4];
            for source_y in y * factor..((y + 1) * factor).min(height) {
                for source_x in x * factor..((x + 1) * factor).min(width) {
                    let index = 4 * (source_y * width + source_x) as usize;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(rgba[index + channel]);
                    }
                }
            }
            tile.extend(sum.map(|total| (total / (factor * factor)) as u8));
        }
    }
    Thumbnail {
        width: tile_width,
        height: tile_height,
        rgba: tile,
    }
}

/// Lays thumbnails out in scenario order, left to right, with black gutters.
fn write_contact_sheet(path: &Path, thumbnails: &[Thumbnail]) -> Result<(), String> {
    const GUTTER: u32 = 4;
    let Some(first) = thumbnails.first() else {
        return Ok(());
    };
    let columns = CONTACT_SHEET_COLUMNS.min(thumbnails.len()) as u32;
    let rows = thumbnails.len().div_ceil(CONTACT_SHEET_COLUMNS) as u32;
    let width = columns * (first.width + GUTTER) - GUTTER;
    let height = rows * (first.height + GUTTER) - GUTTER;
    let mut sheet = vec![0; (width * height * 4) as usize];
    for (index, thumbnail) in thumbnails.iter().enumerate() {
        let origin_x = (index as u32 % columns) * (first.width + GUTTER);
        let origin_y = (index as u32 / columns) * (first.height + GUTTER);
        for y in 0..thumbnail.height {
            let source = (y * thumbnail.width * 4) as usize;
            let target = (((origin_y + y) * width + origin_x) * 4) as usize;
            let row = (thumbnail.width * 4) as usize;
            sheet[target..target + row].copy_from_slice(&thumbnail.rgba[source..source + row]);
        }
    }
    for pixel in sheet.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    write_png(path, width, height, &sheet)
}

fn print_summary(report: &BenchReport) {
    let milliseconds = |timing: Option<Timing>| {
        timing.map_or_else(
            || "-".to_owned(),
            |timing| format!("{:.2}", timing.median_ms),
        )
    };
    println!(
        "{} · {}x{} · {} · median of {} frames",
        report.adapter, report.width, report.height, report.timing, report.timed_iterations
    );
    println!(
        "forced: EVALUATE_ALL_BRANCHES, baking one frame budget; Δmax: largest 8-bit difference from the shipped image"
    );
    let sizes = baked_surface_sizes(
        (report.width, report.height),
        default_options().bake_memory_bytes,
    );
    println!(
        "baked surface: {} levels of {}x{} texels, {:.0} MiB; cold: frames from empty caches until they settle",
        BAKED_SURFACE_LEVELS,
        sizes.texels[0],
        sizes.texels[1],
        sizes.bytes() as f64 / f64::from(1 << 20),
    );
    println!(
        "{:<28} {:>9} {:>7} {:>7} {:>9} {:>7} {:>7} {:>9} {:>8} {:>10} {:>11} {:>12} {:>15} {:>10} {:>12} {:>9}",
        "scenario",
        "total ms",
        "ns/px",
        "fields",
        "occluders",
        "cache",
        "bake",
        "scene",
        "upscale",
        "forced ms",
        "forced bake",
        "forced Δmax",
        "cold max (n)",
        "vs base",
        "forced base",
        "base p99"
    );
    for scenario in &report.scenarios {
        let shipped = &scenario.shipped;
        let (ratio, forced_ratio, baseline_p99) = scenario.baseline.as_ref().map_or_else(
            || ("-".to_owned(), "-".to_owned(), "-".to_owned()),
            |baseline| {
                (
                    format!("{:.3}x", baseline.total_ratio),
                    format!("{:.3}x", baseline.all_branches_total_ratio),
                    baseline.image_difference.p99.to_string(),
                )
            },
        );
        let cold_start = scenario.cold_start.map_or_else(
            || "-".to_owned(),
            |cold| format!("{:.2} ({})", cold.max_ms, cold.frames),
        );
        println!(
            "{:<28} {:>9.2} {:>7.1} {:>7} {:>9} {:>7} {:>7} {:>9} {:>8} {:>10.2} {:>11} {:>12} {:>15} {:>10} {:>12} {:>9}",
            scenario.scenario.id,
            shipped.total.median_ms,
            shipped.total_ns_per_pixel,
            milliseconds(shipped.fields),
            milliseconds(shipped.occluders),
            milliseconds(shipped.terrain_cache),
            milliseconds(shipped.baked_surface),
            milliseconds(shipped.scene),
            milliseconds(shipped.upscale),
            scenario.all_branches.total.median_ms,
            milliseconds(scenario.all_branches.baked_surface),
            scenario.all_branches_image_difference.max,
            cold_start,
            ratio,
            forced_ratio,
            baseline_p99,
        );
        let discarded = shipped.discarded_frames + scenario.all_branches.discarded_frames;
        if discarded > 0 {
            println!(
                "  {discarded} frames with reordered timestamps discarded ({} shipped, {} forced)",
                shipped.discarded_frames, scenario.all_branches.discarded_frames
            );
        }
    }
    for note in &report.baseline_notes {
        println!("note: {note}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ImageDifference, builtin_scenarios, image_difference, median, pixel_differences,
        scenario_options,
    };

    #[test]
    fn median_uses_the_middle_sample_or_the_mean_of_the_middle_pair() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&mut [7.0]), 7.0);
    }

    #[test]
    fn pixel_difference_is_the_largest_color_channel_change() {
        let first = [10, 20, 30, 255, 0, 0, 0, 255];
        let second = [12, 15, 30, 0, 0, 0, 0, 0];

        assert_eq!(pixel_differences(&first, &second), [5, 0]);
    }

    #[test]
    fn image_difference_summarizes_per_pixel_changes() {
        let mut differences = vec![0u8; 100];
        differences[0] = 9;
        differences[1] = 3;
        differences[2] = 2;

        assert_eq!(
            image_difference(&differences),
            ImageDifference {
                mean_absolute: 0.14,
                p99: 3,
                max: 9,
                visible_fraction: 0.02,
            }
        );
        assert_eq!(
            image_difference(&[0; 16]),
            ImageDifference {
                mean_absolute: 0.0,
                p99: 0,
                max: 0,
                visible_fraction: 0.0,
            }
        );
    }

    #[test]
    fn builtin_scenarios_have_unique_ids_and_valid_world_inputs() {
        let scenarios = builtin_scenarios().unwrap();
        assert!(!scenarios.is_empty());
        for (index, scenario) in scenarios.iter().enumerate() {
            assert!(
                scenarios[..index]
                    .iter()
                    .all(|previous| previous.id != scenario.id),
                "{}",
                scenario.id
            );
            assert!(
                scenario_options(scenario, 1920, 1080).is_ok(),
                "{}",
                scenario.id
            );
        }
    }
}
