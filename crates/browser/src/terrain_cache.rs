use bytemuck::{Pod, Zeroable};
use simulation::WorldGeometry;

/// Texels per axis of every clipmap level, a power of two so shaders wrap slots
/// by masking. With the layers below, the cache takes 21 MB, inside a 32 MB
/// mobile budget.
pub const TERRAIN_CACHE_TEXELS: u32 = 256;
/// Levels from 8 m to 128 m spacing hold views up to about 32 km wide; coarser
/// levels would cache only the cluster noise's longest octaves.
pub const TERRAIN_CACHE_LEVELS: u32 = 5;
/// Cluster, island, ruggedness, and coastal substrate. Keep synchronized with
/// the noise order in terrain.wgsl.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
const NOISES_PER_LEVEL: u32 = 4;
/// rgba32float noise layers.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
pub const TERRAIN_CACHE_LAYERS: u32 = TERRAIN_CACHE_LEVELS * NOISES_PER_LEVEL;
/// The active level and its two neighbours fill at most four strips each.
/// Keep synchronized with terrain-cache-generate.wgsl.
const MAX_FILL_RECTS: usize = 3 * 4;
#[cfg(all(target_arch = "wasm32", feature = "playground"))]
pub const TERRAIN_CACHE_FILL_BYTES: usize = size_of::<TerrainCacheFill>();
/// Keep synchronized with `@workgroup_size` in terrain-cache-generate.wgsl.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
const FILL_WORKGROUP_SIZE: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct FillRect {
    /// Column within one longitude lap, then row, on the level's lattice.
    first_texel: [i32; 2],
    texels: [u32; 2],
    first_slot: [u32; 2],
    spacing_m: [f32; 2],
    first_invocation: u32,
    level: u32,
    lap_columns: u32,
    padding: u32,
}

/// The fill pass's uniform table.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainCacheFill {
    rect_count: u32,
    /// Zero keeps the shaders' production tuning.
    horizontal_scale: f32,
    padding: [u32; 2],
    rects: [FillRect; MAX_FILL_RECTS],
}

pub struct TerrainCacheFrame {
    fill: TerrainCacheFill,
    /// Texels the fill pass evaluates, one invocation each.
    fill_texels: u32,
    /// The scene shader's `terrain_cache_grid_m`, `terrain_cache_texels`, and
    /// `terrain_cache_level` rows.
    pub grid_m: [f32; 4],
    pub texels: [u32; 4],
    pub level: [u32; 4],
}

impl TerrainCacheFrame {
    pub fn has_fills(&self) -> bool {
        self.fill_texels > 0
    }

    #[cfg(any(target_arch = "wasm32", feature = "playground"))]
    fn fill_workgroups(&self) -> u32 {
        self.fill_texels.div_ceil(FILL_WORKGROUP_SIZE)
    }

    /// The lab's fill table, dispatched as `fill_workgroups` workgroups of the
    /// fill shader when positive.
    #[cfg(all(target_arch = "wasm32", feature = "playground"))]
    pub fn fill_bytes(&self) -> (&[u8], u32) {
        (bytemuck::bytes_of(&self.fill), self.fill_workgroups())
    }
}

/// The finest level's spacing, holding views up to about 2 km wide with every
/// circle octave of the broad terrain noises cached.
const FINEST_SPACING_M: f64 = 8.0;
const LEVEL_TEXELS: i64 = TERRAIN_CACHE_TEXELS as i64 * TERRAIN_CACHE_TEXELS as i64;
/// Texels per frame for the active level's neighbours, less what the active
/// level took. The active level always completes in the frame that reads it, so
/// a teleport costs one level once and its neighbours fill over later frames.
const FILL_BUDGET_TEXELS: i64 = LEVEL_TEXELS / 4;
/// Texels kept between the view and a window edge beyond the bicubic stencil,
/// absorbing single-precision rounding of shader positions.
const VIEW_MARGIN_TEXELS: f64 = 1.0;

/// Fill schedule for the persistent terrain cache: a clipmap of world-anchored
/// lattices whose spacing doubles per level, each windowed around the camera
/// and addressed toroidally, so a texel keeps its slot while it stays in view.
#[derive(Default)]
pub struct TerrainCache {
    key: Option<CacheKey>,
    /// Camera position with longitude unwrapped across seam crossings, so
    /// windows and slots stay continuous.
    camera_m: [f64; 2],
    canonical_camera_x_m: f64,
    /// Lattice cells holding current values, per level.
    valid: [Cells; TERRAIN_CACHE_LEVELS as usize],
    /// The cells valid once the latest frame's fills have run.
    planned: [Cells; TERRAIN_CACHE_LEVELS as usize],
}

#[derive(Clone, Copy, PartialEq)]
struct CacheKey {
    world_size_m: [u32; 2],
    seed: u64,
    horizontal_scale: Option<u32>,
}

/// Lattice columns and rows from `min` up to, not including, `max`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cells {
    min: [i64; 2],
    max: [i64; 2],
}

impl Cells {
    fn size(self) -> [i64; 2] {
        [0, 1].map(|axis| (self.max[axis] - self.min[axis]).max(0))
    }

    fn is_empty(self) -> bool {
        self.size().contains(&0)
    }

    fn intersection(self, other: Self) -> Self {
        Self {
            min: [0, 1].map(|axis| self.min[axis].max(other.min[axis])),
            max: [0, 1].map(|axis| self.max[axis].min(other.max[axis])),
        }
    }
}

struct Level {
    spacing_m: [f64; 2],
    lap_columns: u32,
    window: Cells,
    /// The camera in lattice units.
    camera: [f64; 2],
}

impl Level {
    fn new(level: u32, world_width_m: f64, camera_m: [f64; 2]) -> Self {
        let spacing_m = FINEST_SPACING_M * f64::from(1 << level);
        // Whole columns per lap keep the lattice continuous across the seam.
        let lap_columns = (world_width_m / spacing_m).round().max(1.0);
        let spacing_m = [world_width_m / lap_columns, spacing_m];
        let camera = [0, 1].map(|axis| camera_m[axis] / spacing_m[axis]);
        let half = i64::from(TERRAIN_CACHE_TEXELS / 2);
        let min = camera.map(|camera| camera.floor() as i64 - half + 1);
        Self {
            spacing_m,
            lap_columns: lap_columns as u32,
            window: Cells {
                min,
                max: min.map(|min| min + i64::from(TERRAIN_CACHE_TEXELS)),
            },
            camera,
        }
    }

    /// Whether every view position's bicubic stencil, which reaches the next
    /// texel, lies in the window.
    fn holds(&self, half_view_m: [f64; 2]) -> bool {
        (0..2).all(|axis| {
            let half_view = half_view_m[axis] / self.spacing_m[axis] + VIEW_MARGIN_TEXELS;
            self.camera[axis] - half_view >= self.window.min[axis] as f64
                && self.camera[axis] + half_view < (self.window.max[axis] - 1) as f64
        })
    }
}

impl TerrainCache {
    /// Plans this frame's fills and returns scene rows that assume they run.
    /// A lab terrain scale replaces the scale of the shaders' production tuning.
    pub fn frame(
        &mut self,
        world_geometry: WorldGeometry,
        seed: u64,
        horizontal_scale: Option<f32>,
        camera_m: [f32; 2],
        view_size_m: [f32; 2],
    ) -> TerrainCacheFrame {
        let world_width_m = f64::from(world_geometry.width_m());
        let canonical_camera_m = world_geometry.wrap_position_m(camera_m).map(f64::from);
        let key = CacheKey {
            world_size_m: [world_geometry.width_m(), world_geometry.height_m()].map(f32::to_bits),
            seed,
            horizontal_scale: horizontal_scale.map(f32::to_bits),
        };
        if self.key != Some(key) {
            self.key = Some(key);
            self.invalidate();
            self.camera_m = canonical_camera_m;
        } else {
            let step_m = canonical_camera_m[0] - self.canonical_camera_x_m;
            self.camera_m[0] += step_m - world_width_m * (step_m / world_width_m).round();
            self.camera_m[1] = canonical_camera_m[1];
        }
        self.canonical_camera_x_m = canonical_camera_m[0];

        let levels: Vec<_> = (0..TERRAIN_CACHE_LEVELS)
            .map(|level| Level::new(level, world_width_m, self.camera_m))
            .collect();
        let half_view_m = [
            (0.5 * f64::from(view_size_m[0])).min(0.5 * world_width_m),
            0.5 * f64::from(view_size_m[1]),
        ];
        let active = levels.iter().position(|level| level.holds(half_view_m));
        // Coarser first, since zooming out reveals more ground per frame.
        let order = active.map_or_else(Vec::new, |active| {
            [Some(active), Some(active + 1), active.checked_sub(1)]
                .into_iter()
                .flatten()
                .filter(|&level| level < levels.len())
                .collect()
        });
        self.planned = self.valid;

        let mut fill = TerrainCacheFill {
            rect_count: 0,
            horizontal_scale: horizontal_scale.unwrap_or(0.0),
            padding: [0; 2],
            rects: [FillRect::default(); MAX_FILL_RECTS],
        };
        let mut fill_texels = 0;
        for index in order {
            let level = &levels[index];
            let valid = &mut self.planned[index];
            *valid = self.valid[index].intersection(level.window);
            let mut budget = if Some(index) == active {
                LEVEL_TEXELS
            } else {
                (FILL_BUDGET_TEXELS - i64::from(fill_texels)).max(0)
            };
            for cells in grow(valid, level.window, &mut budget) {
                let size = cells.size();
                fill.rects[fill.rect_count as usize] = FillRect {
                    first_texel: [
                        cells.min[0].rem_euclid(i64::from(level.lap_columns)) as i32,
                        cells.min[1] as i32,
                    ],
                    texels: size.map(|size| size as u32),
                    first_slot: cells.min.map(slot),
                    spacing_m: level.spacing_m.map(|spacing| spacing as f32),
                    first_invocation: fill_texels,
                    level: index as u32,
                    lap_columns: level.lap_columns,
                    padding: 0,
                };
                fill.rect_count += 1;
                fill_texels += (size[0] * size[1]) as u32;
            }
        }

        let mut frame = TerrainCacheFrame {
            fill,
            fill_texels,
            grid_m: [0.0, 0.0, FINEST_SPACING_M as f32, FINEST_SPACING_M as f32],
            texels: [0; 4],
            level: [0; 4],
        };
        if let Some(index) = active {
            let level = &levels[index];
            let valid = self.planned[index];
            if !valid.is_empty() {
                let size = valid.size();
                frame.grid_m = [
                    (valid.min[0] as f64 * level.spacing_m[0] - self.camera_m[0]) as f32,
                    (valid.min[1] as f64 * level.spacing_m[1] - self.camera_m[1]) as f32,
                    level.spacing_m[0] as f32,
                    level.spacing_m[1] as f32,
                ];
                frame.texels = [
                    slot(valid.min[0]),
                    slot(valid.min[1]),
                    size[0] as u32,
                    size[1] as u32,
                ];
                frame.level[0] = index as u32;
            }
        }
        frame
    }

    /// Records that the latest frame's fills were encoded; until then, the
    /// next frame schedules them again.
    pub fn commit(&mut self) {
        self.valid = self.planned;
    }

    /// Refills every level from the next frame on.
    pub fn invalidate(&mut self) {
        self.valid = Default::default();
    }
}

fn slot(cell: i64) -> u32 {
    cell.rem_euclid(i64::from(TERRAIN_CACHE_TEXELS)) as u32
}

/// Extends `valid` towards `window` within `budget`, returning the cells to
/// fill. Columns beside the valid rows come first, then rows across the valid
/// columns, so the valid cells stay one rectangle; an empty level grows rows
/// across the whole window.
fn grow(valid: &mut Cells, window: Cells, budget: &mut i64) -> Vec<Cells> {
    if valid.is_empty() {
        *valid = Cells {
            min: window.min,
            max: [window.max[0], window.min[1]],
        };
    }
    let mut fills = Vec::new();
    for axis in [0, 1] {
        let across = 1 - axis;
        let length = valid.size()[across];
        if length == 0 {
            continue;
        }
        let below = (valid.min[axis] - window.min[axis]).min(*budget / length);
        if below > 0 {
            let mut cells = *valid;
            cells.max[axis] = valid.min[axis];
            cells.min[axis] = valid.min[axis] - below;
            valid.min[axis] = cells.min[axis];
            *budget -= below * length;
            fills.push(cells);
        }
        let above = (window.max[axis] - valid.max[axis]).min(*budget / length);
        if above > 0 {
            let mut cells = *valid;
            cells.min[axis] = valid.max[axis];
            cells.max[axis] = valid.max[axis] + above;
            valid.max[axis] = cells.max[axis];
            *budget -= above * length;
            fills.push(cells);
        }
    }
    fills
}

/// The cache textures and the fill pass that writes them, shared by every
/// renderer of the production scene.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
pub struct TerrainCachePass {
    _texture: wgpu::Texture,
    /// The scene's binding 4: rgba32float noise layers.
    pub noise_view: wgpu::TextureView,
    pipeline: wgpu::ComputePipeline,
    fill: wgpu::Buffer,
    fill_bind_group: wgpu::BindGroup,
}

#[cfg(any(target_arch = "wasm32", feature = "playground"))]
impl TerrainCachePass {
    /// `scene_uniform_layout` holds the scene uniform buffer alone at binding 0.
    pub fn new(device: &wgpu::Device, scene_uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let noise_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terrain cache noises"),
            size: wgpu::Extent3d {
                width: TERRAIN_CACHE_TEXELS,
                height: TERRAIN_CACHE_TEXELS,
                depth_or_array_layers: TERRAIN_CACHE_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let noise_view = noise_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let fill = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain cache fill"),
            size: size_of::<TerrainCacheFill>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terrain cache fill layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain cache shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                crate::TERRAIN_CACHE_SHADER,
            )),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain cache pipeline layout"),
            bind_group_layouts: &[Some(scene_uniform_layout), Some(&fill_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Terrain cache pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("generate_terrain_cache"),
            compilation_options: Default::default(),
            cache: None,
        });
        let fill_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terrain cache fill bind group"),
            layout: &fill_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: fill.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&noise_view),
                },
            ],
        });
        Self {
            _texture: noise_texture,
            noise_view,
            pipeline,
            fill,
            fill_bind_group,
        }
    }

    /// Encodes the frame's fills. The fill table is written through the queue,
    /// so each submission carries at most one frame's fills.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_uniform_bind_group: &wgpu::BindGroup,
        frame: &TerrainCacheFrame,
        timestamp_writes: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        if frame.fill_texels == 0 && timestamp_writes.is_none() {
            return;
        }
        queue.write_buffer(&self.fill, 0, bytemuck::bytes_of(&frame.fill));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Terrain cache pass"),
            timestamp_writes,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_uniform_bind_group, &[]);
        pass.set_bind_group(1, &self.fill_bind_group, &[]);
        if frame.fill_texels > 0 {
            pass.dispatch_workgroups(frame.fill_workgroups(), 1, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    const SEED: u64 = 0x5eed;
    const N: i64 = TERRAIN_CACHE_TEXELS as i64;
    const BOAT_VIEW_M: [f32; 2] = [427.0, 240.0];

    /// Each cached lattice point's toroidal slot, by level, unwrapped column, and row.
    type Filled = HashMap<(u32, i64, i64), [u32; 2]>;

    fn rects(frame: &TerrainCacheFrame) -> &[FillRect] {
        &frame.fill.rects[..frame.fill.rect_count as usize]
    }

    /// Records what each rect writes, unwrapping lap columns near `camera_x_m`.
    fn record(
        frame: &TerrainCacheFrame,
        camera_x_m: f32,
        filled: &mut Filled,
    ) -> Vec<(u32, i64, i64)> {
        let mut cells = Vec::new();
        let mut invocation = 0;
        for rect in rects(frame) {
            assert_eq!(rect.first_invocation, invocation);
            invocation += rect.texels[0] * rect.texels[1];
            let level = rect.level;
            let lap = i64::from(rect.lap_columns);
            let camera_column =
                (f64::from(camera_x_m) / f64::from(rect.spacing_m[0])).round() as i64;
            for row in 0..i64::from(rect.texels[1]) {
                for column in 0..i64::from(rect.texels[0]) {
                    let lap_column = i64::from(rect.first_texel[0]) + column;
                    let laps = ((camera_column - lap_column) as f64 / lap as f64).round() as i64;
                    let unwrapped = lap_column + lap * laps;
                    let cell = (level, unwrapped, i64::from(rect.first_texel[1]) + row);
                    let slot = [
                        (rect.first_slot[0] + column as u32) % TERRAIN_CACHE_TEXELS,
                        (rect.first_slot[1] + row as u32) % TERRAIN_CACHE_TEXELS,
                    ];
                    filled.insert(cell, slot);
                    cells.push(cell);
                }
            }
        }
        assert_eq!(invocation, frame.fill_texels);
        cells
    }

    /// The active level's valid cells, as the scene rows describe them.
    fn active_cells(frame: &TerrainCacheFrame, camera_m: [f32; 2]) -> Vec<(u32, i64, i64)> {
        let level = frame.level[0];
        let first = [0, 1].map(|axis| {
            ((f64::from(frame.grid_m[axis]) + f64::from(camera_m[axis]))
                / f64::from(frame.grid_m[2 + axis]))
            .round() as i64
        });
        let mut cells = Vec::new();
        for row in 0..i64::from(frame.texels[3]) {
            for column in 0..i64::from(frame.texels[2]) {
                cells.push((level, first[0] + column, first[1] + row));
            }
        }
        cells
    }

    fn settle(
        cache: &mut TerrainCache,
        world: WorldGeometry,
        camera_m: [f32; 2],
        view_m: [f32; 2],
        filled: &mut Filled,
    ) -> TerrainCacheFrame {
        for _ in 0..16 {
            let frame = cache.frame(world, SEED, None, camera_m, view_m);
            cache.commit();
            record(&frame, camera_m[0], filled);
            if frame.fill_texels == 0 {
                return frame;
            }
        }
        panic!("the cache keeps filling at a fixed camera");
    }

    /// Asserts that every view corner's bicubic stencil lies inside the valid extent.
    fn assert_view_cached(frame: &TerrainCacheFrame, world: WorldGeometry, view_m: [f32; 2]) {
        let half_m = [
            (0.5 * view_m[0]).min(0.5 * world.width_m()),
            0.5 * view_m[1],
        ];
        for corner in [[-1.0, -1.0], [1.0, 1.0], [-1.0, 1.0], [1.0, -1.0]] {
            for axis in 0..2 {
                let texel = ((corner[axis] * half_m[axis] - frame.grid_m[axis])
                    / frame.grid_m[2 + axis])
                    .floor();
                assert!(
                    texel >= 0.0 && texel + 1.0 < frame.texels[2 + axis] as f32,
                    "{view_m:?} corner {corner:?} axis {axis}: texel {texel} of {:?}",
                    frame.texels,
                );
            }
        }
    }

    /// A 16:9 view that level `level` is the finest to hold.
    fn level_view_m(level: u32) -> [f32; 2] {
        let height_m = 1_000.0 * 2.0_f32.powi(level as i32);
        [16.0 / 9.0 * height_m, height_m]
    }

    #[test]
    fn level_lattices_tile_the_longitude_circumference() {
        for width_m in [8_000.0, 64_000.0, 100_003.0] {
            let world = WorldGeometry::new(width_m, 64_000.0).unwrap();
            let mut cache = TerrainCache::default();
            let mut spacings = HashMap::new();
            for level in 0..TERRAIN_CACHE_LEVELS {
                for _ in 0..16 {
                    let frame = cache.frame(world, SEED, None, [0.0, 0.0], level_view_m(level));
                    cache.commit();
                    for rect in rects(&frame) {
                        spacings.insert(rect.level, (rect.spacing_m, rect.lap_columns));
                    }
                }
            }
            assert_eq!(spacings.len(), TERRAIN_CACHE_LEVELS as usize, "{width_m}");
            for (level, (spacing_m, lap_columns)) in spacings {
                assert_eq!(spacing_m[1], 8.0 * 2.0_f32.powi(level as i32));
                let lap_m = f64::from(spacing_m[0]) * f64::from(lap_columns);
                assert!(
                    (lap_m - f64::from(width_m)).abs() < 0.01,
                    "{width_m}: {lap_m}"
                );
            }
        }
    }

    #[test]
    fn the_finest_level_that_holds_the_view_is_active() {
        let world = WorldGeometry::default();
        for (view_m, level) in [
            (BOAT_VIEW_M, Some(0)),
            ([1_990.0, 1_119.0], Some(0)),
            ([2_667.0, 1_500.0], Some(1)),
            ([4_267.0, 2_400.0], Some(2)),
            ([1_350.0, 2_400.0], Some(1)),
            ([31_000.0, 17_400.0], Some(4)),
            ([35_556.0, 20_000.0], None),
        ] {
            for camera_m in [[0.0, 0.0], [-14_900.0, -5_330.0], [31_990.0, 2.0e5]] {
                let mut cache = TerrainCache::default();
                let frame = cache.frame(world, SEED, None, camera_m, view_m);
                match level {
                    Some(level) => {
                        assert_eq!(frame.level[0], level, "{view_m:?}");
                        assert_eq!(frame.grid_m[3], 8.0 * 2.0_f32.powi(level as i32));
                        assert_view_cached(&frame, world, view_m);
                    }
                    None => assert_eq!(frame.texels[2..], [0, 0], "{view_m:?}"),
                }
            }
        }
    }

    #[test]
    fn a_teleport_fills_one_level_then_spreads_its_neighbours() {
        let world = WorldGeometry::default();
        let camera_m = [-5_560.0, -2_990.0];
        for (view_m, levels) in [
            (BOAT_VIEW_M, vec![0, 1]),
            (level_view_m(2), vec![1, 2, 3]),
            (level_view_m(4), vec![3, 4]),
        ] {
            let mut cache = TerrainCache::default();
            let mut filled = Filled::new();
            let first = cache.frame(world, SEED, None, camera_m, view_m);
            cache.commit();
            record(&first, camera_m[0], &mut filled);
            assert_eq!(i64::from(first.fill_texels), N * N, "{view_m:?}");
            assert_eq!(first.texels[2..], [TERRAIN_CACHE_TEXELS; 2]);
            let mut frame = first;
            for _ in 0..16 {
                frame = cache.frame(world, SEED, None, camera_m, view_m);
                cache.commit();
                record(&frame, camera_m[0], &mut filled);
                assert!(i64::from(frame.fill_texels) <= N * N / 4, "{view_m:?}");
            }
            assert_eq!(frame.fill_texels, 0, "{view_m:?} keeps filling");
            for level in 0..TERRAIN_CACHE_LEVELS {
                let cells = filled.keys().filter(|cell| cell.0 == level).count() as i64;
                let expected = if levels.contains(&level) { N * N } else { 0 };
                assert_eq!(cells, expected, "{view_m:?} level {level}");
            }
        }
    }

    #[test]
    fn no_level_fills_while_none_holds_the_view() {
        let world = WorldGeometry::default();
        let mut cache = TerrainCache::default();
        for _ in 0..4 {
            let frame = cache.frame(world, SEED, None, [0.0, 0.0], [35_556.0, 20_000.0]);
            cache.commit();
            assert_eq!(frame.fill_texels, 0);
        }
    }

    #[test]
    fn a_zoom_completes_the_new_active_level_in_its_first_frame() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut cache = TerrainCache::default();
        settle(&mut cache, world, camera_m, BOAT_VIEW_M, &mut Filled::new());
        for level in [1, 3, 0] {
            let frame = cache.frame(world, SEED, None, camera_m, level_view_m(level));
            cache.commit();
            assert_eq!(frame.level[0], level);
            assert_eq!(
                frame.texels[2..],
                [TERRAIN_CACHE_TEXELS; 2],
                "level {level}"
            );
            assert!(i64::from(frame.fill_texels) <= N * N, "level {level}");
        }
    }

    #[test]
    fn panning_fills_only_newly_exposed_texels_into_their_lattice_slots() {
        let world = WorldGeometry::default();
        for (start_m, step_m) in [
            ([-5_560.0, -2_990.0], [28.0, -16.0]),
            ([-5_560.0, -2_990.0], [-3.0, 900.0]),
            // East across the longitude seam.
            ([31_990.0, 0.0], [20.0, 0.0]),
        ] {
            let mut cache = TerrainCache::default();
            let mut filled = Filled::new();
            let before = settle(&mut cache, world, start_m, BOAT_VIEW_M, &mut filled);
            let previous: HashSet<_> = active_cells(&before, start_m).into_iter().collect();
            let camera_m = world.wrap_position_m([start_m[0] + step_m[0], start_m[1] + step_m[1]]);
            let frame = cache.frame(world, SEED, None, camera_m, BOAT_VIEW_M);
            cache.commit();
            let new_cells = record(&frame, start_m[0] + step_m[0], &mut filled);
            let active: Vec<_> = active_cells(&frame, [start_m[0] + step_m[0], camera_m[1]]);
            assert_eq!(frame.texels[2..], [TERRAIN_CACHE_TEXELS; 2]);
            let level_zero_cells = new_cells.iter().filter(|cell| cell.0 == 0).count() as i64;
            let exposed = [0, 1].map(|axis| (step_m[axis] / 8.0).abs().ceil() as i64 + 1);
            assert!(
                level_zero_cells <= N * (exposed[0] + exposed[1]),
                "{step_m:?}: {level_zero_cells} level-zero texels",
            );
            for cell in &new_cells {
                if cell.0 == 0 {
                    assert!(!previous.contains(cell), "{step_m:?} refilled {cell:?}");
                }
            }
            for cell in &active {
                assert!(
                    previous.contains(cell) || new_cells.contains(cell),
                    "{step_m:?}: {cell:?} was never filled",
                );
            }
            let first_slot = filled[&active[0]];
            assert_eq!([frame.texels[0], frame.texels[1]], first_slot, "{step_m:?}");
            let slots: HashSet<_> = active.iter().map(|cell| filled[cell]).collect();
            assert_eq!(slots.len(), active.len(), "{step_m:?}: slots collide");
        }
    }

    #[test]
    fn fills_of_uncommitted_frames_are_scheduled_again() {
        let world = WorldGeometry::default();
        let mut cache = TerrainCache::default();
        let start_m = [-5_560.0, -2_990.0];
        settle(&mut cache, world, start_m, BOAT_VIEW_M, &mut Filled::new());
        let mut skipped = Filled::new();
        let mut camera_m = start_m;
        for _ in 0..3 {
            camera_m[0] += 40.0;
            record(
                &cache.frame(world, SEED, None, camera_m, BOAT_VIEW_M),
                camera_m[0],
                &mut skipped,
            );
        }
        let mut filled = Filled::new();
        let frame = cache.frame(world, SEED, None, camera_m, BOAT_VIEW_M);
        cache.commit();
        record(&frame, camera_m[0], &mut filled);
        for cell in skipped.keys() {
            assert!(filled.contains_key(cell), "{cell:?} was never filled");
        }
        assert_eq!(frame.texels[2..], [TERRAIN_CACHE_TEXELS; 2]);
    }

    #[test]
    fn seed_world_scale_and_invalidation_refill_the_active_level() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let full_level = (N * N) as u32;
        let changes: [&dyn Fn(&mut TerrainCache) -> TerrainCacheFrame; 4] = [
            &|cache| cache.frame(world, SEED + 1, None, camera_m, BOAT_VIEW_M),
            &|cache| {
                cache.frame(
                    WorldGeometry::new(64_000.0, 32_000.0).unwrap(),
                    SEED,
                    None,
                    camera_m,
                    BOAT_VIEW_M,
                )
            },
            &|cache| cache.frame(world, SEED, Some(1.5), camera_m, BOAT_VIEW_M),
            &|cache| {
                cache.invalidate();
                cache.frame(world, SEED, None, camera_m, BOAT_VIEW_M)
            },
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut cache = TerrainCache::default();
            settle(&mut cache, world, camera_m, BOAT_VIEW_M, &mut Filled::new());
            let frame = change(&mut cache);
            cache.commit();
            assert_eq!(frame.fill_texels, full_level, "change {index}");
            assert_eq!(
                frame.texels[2..],
                [TERRAIN_CACHE_TEXELS; 2],
                "change {index}"
            );
        }
    }
}
