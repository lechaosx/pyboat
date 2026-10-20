use std::{
    cell::RefCell,
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    time::{Duration, Instant},
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
const SEQUENCES: &str = include_str!("../../../playground/src/bench-sequences.json");
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
/// GPU clocks drop while the bench reads back and encodes images, then take a
/// few hundred milliseconds of frames, past a brief overshoot, to return to
/// the level sustained rendering holds. Light frames need far more warm-up
/// frames than heavy ones to span it. On a Radeon 780M, shipped medians after
/// half a second differed from those after 1.5 seconds by -1.6% on average,
/// -11% at worst, and by more than 5% in 10 of 36 scenarios, about the
/// run-to-run noise there.
const MINIMUM_TIMED_WARMUP: Duration = Duration::from_millis(500);

pub struct BenchOptions {
    pub output_dir: PathBuf,
    pub baseline: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub warmup_iterations: u32,
    pub timed_iterations: u32,
    pub frame_budget_ms: f64,
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Keyframe {
    frame: u32,
    camera_m: [f32; 2],
    view_height_m: f32,
}

/// A camera path through keyframes. Between two keyframes the camera moves
/// linearly and the view height changes geometrically, so a zoom scales the view
/// by the same factor every frame.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Sequence {
    id: String,
    exercises: String,
    seed: String,
    world_time_seconds: f64,
    keyframes: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    render_scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quality: Option<String>,
}

impl Sequence {
    /// Timed frames, which follow frame 0, the settled first view.
    fn frames(&self) -> u32 {
        self.keyframes.last().map_or(0, |keyframe| keyframe.frame)
    }

    fn view(&self, frame: u32) -> Scenario {
        let next = self
            .keyframes
            .iter()
            .position(|keyframe| keyframe.frame >= frame)
            .unwrap_or(self.keyframes.len() - 1);
        let to = &self.keyframes[next];
        let (camera_m, view_height_m) = if to.frame <= frame {
            (to.camera_m, to.view_height_m)
        } else {
            let from = &self.keyframes[next - 1];
            let progress = (frame - from.frame) as f32 / (to.frame - from.frame) as f32;
            (
                [0, 1].map(|axis| {
                    from.camera_m[axis] + (to.camera_m[axis] - from.camera_m[axis]) * progress
                }),
                from.view_height_m * (to.view_height_m / from.view_height_m).powf(progress),
            )
        };
        Scenario {
            id: self.id.clone(),
            exercises: self.exercises.clone(),
            seed: self.seed.clone(),
            camera_m,
            view_height_m,
            world_time_seconds: self.world_time_seconds,
            render_scale: self.render_scale,
            quality: self.quality.clone(),
        }
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

/// One pass's durations through a sequence, in frame order.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SequenceTiming {
    mean_ms: f64,
    median_ms: f64,
    /// Nearest rank, so an observed duration.
    p95_ms: f64,
    max_ms: f64,
    samples_ms: Vec<f64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SequenceBaselineComparison {
    mean_ratio: f64,
    p95_ratio: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SequenceReport {
    #[serde(flatten)]
    sequence: Sequence,
    fields: Option<SequenceTiming>,
    occluders: Option<SequenceTiming>,
    scene: Option<SequenceTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upscale: Option<SequenceTiming>,
    terrain_cache: Option<SequenceTiming>,
    baked_surface: Option<SequenceTiming>,
    total: SequenceTiming,
    frames_over_budget: u32,
    /// Pixel footprint over the read level's texel spacing, below one while a
    /// coarser level stands in.
    texels_per_pixel_min: f32,
    texels_per_pixel_last: f32,
    discarded_frames: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline: Option<SequenceBaselineComparison>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    minimum_warmup_ms: Option<f64>,
    contact_sheet: String,
    scenarios: Vec<ScenarioReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frame_budget_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sequences: Vec<SequenceReport>,
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
    let sequences = builtin_sequences()?;
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
    let first_sequence_views: Vec<Scenario> =
        sequences.iter().map(|sequence| sequence.view(0)).collect();
    let mut bake_sizes: Vec<BakedSurfaceSizes> = Vec::new();
    for sizes in scenarios
        .iter()
        .chain(&first_sequence_views)
        .map(scenario_bake_sizes)
    {
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
        notes = baseline_notes(report, &adapter_name, &scenarios, &sequences);
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
    let mut sequence_reports = Vec::new();
    for sequence in sequences {
        eprintln!("{}", sequence.id);
        let (timed, texels_per_pixel) = time_sequence(
            &device,
            &queue,
            &renderers[0],
            &scene_layout,
            &unused,
            &vertices,
            &upscaler,
            &sequence,
            timestamps.as_ref(),
            options,
        )?;
        let previous = baseline.as_ref().and_then(|(_, report)| {
            report
                .sequences
                .iter()
                .find(|previous| previous.sequence.id == sequence.id)
        });
        sequence_reports.push(sequence_report(
            sequence,
            timed,
            &texels_per_pixel,
            options.frame_budget_ms,
            previous,
        )?);
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
        minimum_warmup_ms: Some(minimum_warmup_ms()),
        contact_sheet,
        scenarios: reports,
        frame_budget_ms: Some(options.frame_budget_ms),
        sequences: sequence_reports,
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

fn minimum_warmup_ms() -> f64 {
    MINIMUM_TIMED_WARMUP.as_secs_f64() * 1000.0
}

fn baseline_notes(
    report: &BenchReport,
    adapter_name: &str,
    scenarios: &[Scenario],
    sequences: &[Sequence],
) -> Vec<String> {
    let mut notes = Vec::new();
    match report.minimum_warmup_ms {
        None => notes.push(
            "baseline predates the timed warm-up minimum; light-scenario ratios are biased"
                .to_owned(),
        ),
        Some(previous) if previous != minimum_warmup_ms() => notes.push(format!(
            "baseline used a {previous} ms timed warm-up minimum, this run {} ms; light-scenario ratios are biased",
            minimum_warmup_ms()
        )),
        Some(_) => {}
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
    for scenario in scenarios {
        if !report
            .scenarios
            .iter()
            .any(|previous| previous.scenario.id == scenario.id)
        {
            notes.push(format!("scenario {} has no baseline", scenario.id));
        }
    }
    for previous in &report.sequences {
        if !sequences
            .iter()
            .any(|sequence| sequence.id == previous.sequence.id)
        {
            notes.push(format!(
                "baseline sequence {} is not in this run",
                previous.sequence.id
            ));
        }
    }
    for sequence in sequences {
        if !report
            .sequences
            .iter()
            .any(|previous| previous.sequence.id == sequence.id)
        {
            notes.push(format!("sequence {} has no baseline", sequence.id));
        }
    }
    notes
}

fn builtin_scenarios() -> Result<Vec<Scenario>, String> {
    serde_json::from_str(SCENARIOS).map_err(|error| format!("invalid bench scenarios: {error}"))
}

fn builtin_sequences() -> Result<Vec<Sequence>, String> {
    parse_sequences(SEQUENCES)
}

fn parse_sequences(json: &str) -> Result<Vec<Sequence>, String> {
    let sequences: Vec<Sequence> =
        serde_json::from_str(json).map_err(|error| format!("invalid bench sequences: {error}"))?;
    for sequence in &sequences {
        let keyframes = &sequence.keyframes;
        if keyframes.len() < 2
            || keyframes[0].frame != 0
            || keyframes
                .windows(2)
                .any(|pair| pair[1].frame <= pair[0].frame)
        {
            return Err(format!(
                "{}: keyframes must start at frame 0 and their frames increase",
                sequence.id
            ));
        }
    }
    Ok(sequences)
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
    // Started after the first frame, which also fills a new scenario's caches.
    let mut warmup_started = None;
    let mut warmup_frames = 0;
    while timed.frames.len() + (timed.discarded as usize) < timed_frames as usize
        || warmup_frames < options.warmup_iterations
    {
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
        let warmup_started = *warmup_started.get_or_insert_with(Instant::now);
        if warmup_frames < options.warmup_iterations
            || (timed_frames > 0 && warmup_started.elapsed() < MINIMUM_TIMED_WARMUP)
        {
            warmup_frames += 1;
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

/// Times shipped frames along `sequence` through one terrain cache and bake,
/// starting once both have settled at its first view and the GPU has warmed
/// up there. Also returns each timed frame's baked texels per pixel.
#[allow(clippy::too_many_arguments)]
fn time_sequence(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &ModeRenderer,
    scene_layout: &wgpu::BindGroupLayout,
    unused: &wgpu::BindGroup,
    vertices: &wgpu::Buffer,
    upscaler: &Upscaler,
    sequence: &Sequence,
    timestamps: Option<&TimestampReadback>,
    options: &BenchOptions,
) -> Result<(TimedFrames, Vec<f32>), String> {
    let first_view = sequence.view(0);
    let quality = first_view.graphics_quality()?;
    let output_size = (options.width, options.height);
    let scene_size = scene_render_size(
        options.width,
        options.height,
        sequence.render_scale.unwrap_or(1.0),
    );
    let surface_sizes = baked_surface_sizes(scene_size, default_options().bake_memory_bytes);
    let upscale = (scene_size != output_size).then(|| {
        upscaler.targets(
            device,
            scene_size,
            output_size,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    });
    let target = capture_texture(device, options.width, options.height);
    let mut terrain_cache = TerrainCache::default();
    let mut baked_surface = BakedSurface::new(surface_sizes);
    let mut frame = |view: &Scenario| -> Result<(FrameTimes, bool, f32), String> {
        let view_options = scenario_options(view, options.width, options.height)?;
        let pixel_footprint_m = visual_pixel_footprint_m(view.view_height_m, scene_size.1);
        let cache_frame = capture_terrain_cache_frame(&view_options, &mut terrain_cache)?;
        terrain_cache.commit();
        let surface_frame = capture_baked_surface_frame(
            &view_options,
            pixel_footprint_m,
            &mut baked_surface,
            false,
        )?;
        baked_surface.commit();
        let mut uniforms = scene_uniforms(&view_options, &cache_frame, &surface_frame)?;
        uniforms.world_phases_and_pixel_footprint_m[3] = pixel_footprint_m;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bench sequence scene uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let scene = bind_group(device, scene_layout, &buffer, "bench sequence scene");
        let fields = renderer.fields(surface_sizes).bindings(device, &buffer);
        let field_grids = capture_field_grids(&view_options)?;
        let bindings = FrameBindings {
            scenario_id: &view.id,
            quality,
            options: &view_options,
            pixel_footprint_m,
            baked_surface_sizes: surface_sizes,
            terrain_cache_rows: (cache_frame.grid_m, cache_frame.texels, cache_frame.level),
            baked_surface_rows: baked_surface_rows(&surface_frame),
            scene: &scene,
            fields: &fields,
            field_grids: &field_grids,
            unused,
            vertices,
            upscaler,
            upscale: upscale.as_ref(),
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench sequence encoder"),
        });
        renderer.encode_frame(
            queue,
            &mut encoder,
            &bindings,
            &cache_frame,
            &surface_frame,
            &target,
            timestamps.map(|timestamps| &timestamps.query_set),
        );
        let times = submit_timed(device, queue, encoder, timestamps, upscale.is_some())?;
        Ok((
            times,
            cache_frame.has_fills() || surface_frame.has_fills(),
            pixel_footprint_m / surface_frame.grid_m[3],
        ))
    };
    let mut warmup_started = None;
    let mut warmup_frames = 0;
    loop {
        let (_, has_fills, _) = frame(&first_view)?;
        warmup_frames += 1;
        if has_fills {
            continue;
        }
        let warmup_started = *warmup_started.get_or_insert_with(Instant::now);
        if warmup_frames >= options.warmup_iterations
            && warmup_started.elapsed() >= MINIMUM_TIMED_WARMUP
        {
            break;
        }
    }
    let mut timed = TimedFrames {
        frames: Vec::new(),
        discarded: 0,
    };
    let mut texels_per_pixel = Vec::new();
    for index in 1..=sequence.frames() {
        let (times, _, frame_texels_per_pixel) = frame(&sequence.view(index))?;
        texels_per_pixel.push(frame_texels_per_pixel);
        let complete = if timestamps.is_some() {
            times[..3].iter().all(Option::is_some)
                && times[3].is_some() == upscale.is_some()
                && times[4..].iter().all(Option::is_some)
        } else {
            times[4].is_some()
        };
        if complete {
            timed.frames.push(times);
        } else {
            timed.discarded += 1;
        }
    }
    Ok((timed, texels_per_pixel))
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

fn sequence_report(
    sequence: Sequence,
    timed: TimedFrames,
    texels_per_pixel: &[f32],
    frame_budget_ms: f64,
    previous: Option<&SequenceReport>,
) -> Result<SequenceReport, String> {
    let pass = |index: usize| {
        let samples_ms: Vec<f64> = timed
            .frames
            .iter()
            .filter_map(|frame| frame[index])
            .collect();
        (!samples_ms.is_empty()).then(|| {
            let mut sorted = samples_ms.clone();
            let median_ms = median(&mut sorted);
            SequenceTiming {
                mean_ms: samples_ms.iter().sum::<f64>() / samples_ms.len() as f64,
                median_ms,
                p95_ms: sorted[(95 * sorted.len()).div_ceil(100) - 1],
                max_ms: sorted[sorted.len() - 1],
                samples_ms,
            }
        })
    };
    let total = pass(4).ok_or("every sequence frame had invalid timestamps")?;
    Ok(SequenceReport {
        sequence,
        fields: pass(0),
        occluders: pass(1),
        scene: pass(2),
        upscale: pass(3),
        terrain_cache: pass(5),
        baked_surface: pass(6),
        frames_over_budget: total
            .samples_ms
            .iter()
            .filter(|&&ms| ms > frame_budget_ms)
            .count() as u32,
        texels_per_pixel_min: texels_per_pixel
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min),
        texels_per_pixel_last: *texels_per_pixel
            .last()
            .expect("a sequence has timed frames"),
        discarded_frames: timed.discarded,
        baseline: previous.map(|previous| SequenceBaselineComparison {
            mean_ratio: total.mean_ms / previous.total.mean_ms,
            p95_ratio: total.p95_ms / previous.total.p95_ms,
        }),
        total,
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
    if let Some(budget_ms) = report.frame_budget_ms
        && !report.sequences.is_empty()
    {
        println!(
            "sequences: shipped frames from caches settled at the first view; over: frames above {budget_ms} ms; texels/px: baked texels per pixel"
        );
        println!(
            "{:<26} {:>6} {:>7} {:>7} {:>7} {:>7} {:>5} {:>9} {:>8} {:>18} {:>9} {:>9}",
            "sequence",
            "frames",
            "mean",
            "median",
            "p95",
            "max",
            "over",
            "cache max",
            "bake max",
            "texels/px min,last",
            "vs base",
            "base p95"
        );
    }
    let maximum = |timing: Option<&SequenceTiming>| {
        timing.map_or_else(|| "-".to_owned(), |timing| format!("{:.2}", timing.max_ms))
    };
    for sequence in &report.sequences {
        let total = &sequence.total;
        let (mean_ratio, p95_ratio) = sequence.baseline.as_ref().map_or_else(
            || ("-".to_owned(), "-".to_owned()),
            |baseline| {
                (
                    format!("{:.3}x", baseline.mean_ratio),
                    format!("{:.3}x", baseline.p95_ratio),
                )
            },
        );
        println!(
            "{:<26} {:>6} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>5} {:>9} {:>8} {:>18} {:>9} {:>9}",
            sequence.sequence.id,
            total.samples_ms.len(),
            total.mean_ms,
            total.median_ms,
            total.p95_ms,
            total.max_ms,
            sequence.frames_over_budget,
            maximum(sequence.terrain_cache.as_ref()),
            maximum(sequence.baked_surface.as_ref()),
            format!(
                "{:.2},{:.2}",
                sequence.texels_per_pixel_min, sequence.texels_per_pixel_last
            ),
            mean_ratio,
            p95_ratio,
        );
        if sequence.discarded_frames > 0 {
            println!(
                "  {} frames with reordered timestamps discarded",
                sequence.discarded_frames
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
        BenchReport, ImageDifference, Sequence, TimedFrames, baseline_notes, builtin_scenarios,
        builtin_sequences, image_difference, median, parse_sequences, pixel_differences,
        scenario_options, sequence_report,
    };

    #[test]
    fn median_uses_the_middle_sample_or_the_mean_of_the_middle_pair() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&mut [7.0]), 7.0);
    }

    fn sequence(keyframes: serde_json::Value) -> Sequence {
        serde_json::from_value(serde_json::json!({
            "id": "test",
            "exercises": "test",
            "seed": "0x5eed",
            "worldTimeSeconds": 37,
            "keyframes": keyframes,
        }))
        .unwrap()
    }

    #[test]
    fn sequence_views_move_linearly_and_zoom_geometrically_between_keyframes() {
        let sequence = sequence(serde_json::json!([
            { "frame": 0, "cameraM": [0, 0], "viewHeightM": 100 },
            { "frame": 10, "cameraM": [100, -50], "viewHeightM": 400 },
            { "frame": 11, "cameraM": [-300, 70], "viewHeightM": 30 },
            { "frame": 20, "cameraM": [-300, 70], "viewHeightM": 30 },
        ]));
        let view = |frame| {
            let view = sequence.view(frame);
            (view.camera_m, view.view_height_m)
        };

        assert_eq!(sequence.frames(), 20);
        assert_eq!(view(0), ([0.0, 0.0], 100.0));
        assert_eq!(view(5), ([50.0, -25.0], 200.0));
        assert_eq!(view(10), ([100.0, -50.0], 400.0));
        assert_eq!(view(11), ([-300.0, 70.0], 30.0));
        assert_eq!(view(15), ([-300.0, 70.0], 30.0));
        assert_eq!(view(20), ([-300.0, 70.0], 30.0));
    }

    #[test]
    fn sequences_need_increasing_keyframes_from_frame_zero() {
        let parse = |keyframes: serde_json::Value| {
            parse_sequences(&serde_json::json!([sequence(keyframes)]).to_string())
        };
        let keyframe =
            |frame| serde_json::json!({ "frame": frame, "cameraM": [0, 0], "viewHeightM": 240 });

        assert!(parse(serde_json::json!([keyframe(0), keyframe(1)])).is_ok());
        assert!(parse(serde_json::json!([keyframe(0)])).is_err());
        assert!(parse(serde_json::json!([keyframe(1), keyframe(2)])).is_err());
        assert!(parse(serde_json::json!([keyframe(0), keyframe(2), keyframe(2)])).is_err());
        assert!(parse(serde_json::json!([keyframe(0), keyframe(3), keyframe(2)])).is_err());
    }

    #[test]
    fn builtin_sequences_have_unique_ids_and_valid_views() {
        let sequences = builtin_sequences().unwrap();
        assert!(!sequences.is_empty());
        for (index, sequence) in sequences.iter().enumerate() {
            assert!(
                sequences[..index]
                    .iter()
                    .all(|previous| previous.id != sequence.id),
                "{}",
                sequence.id
            );
            for keyframe in &sequence.keyframes {
                assert!(
                    scenario_options(&sequence.view(keyframe.frame), 1920, 1080).is_ok(),
                    "{} frame {}",
                    sequence.id,
                    keyframe.frame
                );
            }
        }
    }

    fn frames(samples: &[f64]) -> TimedFrames {
        TimedFrames {
            frames: samples
                .iter()
                .map(|&ms| [Some(ms), None, Some(ms), None, Some(ms), None, None])
                .collect(),
            discarded: 2,
        }
    }

    #[test]
    fn sequence_report_keeps_frame_order_and_tail_statistics() {
        let samples: Vec<f64> = std::iter::once(100.0)
            .chain((1..20).rev().map(f64::from))
            .collect();
        let report = sequence_report(
            sequence(serde_json::json!([])),
            frames(&samples),
            &[0.5, 0.25, 1.1, 1.2],
            16.7,
            None,
        )
        .unwrap();
        let json = serde_json::to_value(&report).unwrap();
        for pass in ["fields", "scene", "total"] {
            assert_eq!(json[pass]["meanMs"], 14.5);
            assert_eq!(json[pass]["medianMs"], 10.5);
            assert_eq!(json[pass]["p95Ms"], 19.0);
            assert_eq!(json[pass]["maxMs"], 100.0);
            assert_eq!(json[pass]["samplesMs"], serde_json::json!(samples));
        }
        assert!(json["occluders"].is_null());
        assert!(json.get("upscale").is_none());
        assert_eq!(json["framesOverBudget"], 4);
        assert_eq!(json["texelsPerPixelMin"], 0.25);
        assert_eq!(json["texelsPerPixelLast"], 1.2_f32);
        assert_eq!(json["discardedFrames"], 2);

        let tight = sequence_report(
            sequence(serde_json::json!([])),
            frames(&samples),
            &[1.0],
            10.0,
            None,
        )
        .unwrap();
        assert_eq!(tight.frames_over_budget, 10);
    }

    #[test]
    fn sequence_report_accepts_one_frame() {
        let report = sequence_report(
            sequence(serde_json::json!([])),
            frames(&[7.0]),
            &[1.0],
            16.7,
            None,
        )
        .unwrap();
        let total = &serde_json::to_value(report).unwrap()["total"];
        for statistic in ["meanMs", "medianMs", "p95Ms", "maxMs"] {
            assert_eq!(total[statistic], 7.0);
        }
        assert_eq!(total["samplesMs"], serde_json::json!([7.0]));
    }

    #[test]
    fn sequence_report_compares_mean_and_p95_with_the_baseline() {
        let previous = sequence_report(
            sequence(serde_json::json!([])),
            frames(&[2.0, 4.0, 4.0, 10.0]),
            &[1.0],
            16.7,
            None,
        )
        .unwrap();
        let report = sequence_report(
            sequence(serde_json::json!([])),
            frames(&[3.0, 3.0, 6.0, 8.0]),
            &[1.0],
            16.7,
            Some(&previous),
        )
        .unwrap();
        let baseline = report.baseline.unwrap();

        assert_eq!(baseline.mean_ratio, 1.0);
        assert_eq!(baseline.p95_ratio, 0.8);
    }

    #[test]
    fn baseline_notes_flag_other_warmups_and_unmatched_sequences() {
        let mut report: BenchReport = serde_json::from_value(serde_json::json!({
            "adapter": "gpu",
            "timing": "timestamp-query",
            "width": 1920,
            "height": 1080,
            "warmupIterations": 5,
            "timedIterations": 20,
            "contactSheet": "contact-sheet.png",
            "scenarios": [],
        }))
        .unwrap();
        let mut removed = sequence(serde_json::json!([]));
        removed.id = "removed".to_owned();
        report
            .sequences
            .push(sequence_report(removed, frames(&[5.0]), &[1.0], 16.7, None).unwrap());
        let current = [sequence(serde_json::json!([]))];
        let warmup_notes = |report: &BenchReport| -> Vec<String> {
            baseline_notes(report, "gpu", &[], &current)
                .into_iter()
                .filter(|note| note.contains("warm-up"))
                .collect()
        };

        let notes = baseline_notes(&report, "gpu", &[], &current);
        assert!(notes.contains(&"baseline sequence removed is not in this run".to_owned()));
        assert!(notes.contains(&"sequence test has no baseline".to_owned()));
        assert_eq!(
            warmup_notes(&report),
            ["baseline predates the timed warm-up minimum; light-scenario ratios are biased"]
        );
        report.minimum_warmup_ms = Some(250.0);
        assert_eq!(
            warmup_notes(&report),
            [
                "baseline used a 250 ms timed warm-up minimum, this run 500 ms; light-scenario ratios are biased"
            ]
        );
        report.minimum_warmup_ms = Some(500.0);
        assert!(warmup_notes(&report).is_empty());
    }

    #[test]
    fn reports_without_sequences_stay_readable_as_baselines() {
        let json = serde_json::json!({
            "adapter": "gpu",
            "timing": "timestamp-query",
            "width": 1920,
            "height": 1080,
            "warmupIterations": 5,
            "timedIterations": 20,
            "contactSheet": "contact-sheet.png",
            "scenarios": [],
        });
        let report: BenchReport = serde_json::from_value(json.clone()).unwrap();

        assert!(report.sequences.is_empty());
        assert_eq!(serde_json::to_value(report).unwrap(), json);
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
