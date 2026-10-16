use bytemuck::{Pod, Zeroable};
use simulation::WorldGeometry;

use crate::terrain_cache::{Cells, grow};

/// Level spacings form a ladder of `2^(1 / RUNGS_PER_OCTAVE)` steps through
/// one metre. Two rungs per octave keep the read level between one and 1.41
/// texels per pixel while a pan bakes at most a few strips a frame.
pub const RUNGS_PER_OCTAVE: i32 = 2;
/// The read level and a coarser standby, so a one-rung zoom out reads a
/// complete level.
pub const BAKED_SURFACE_LEVELS: u32 = 2;
/// Eight-bit layers per level. Keep synchronized with
/// `BAKED_SURFACE_COLOR_LAYERS` in terrain.wgsl.
pub const BAKED_SURFACE_COLOR_LAYERS: u32 = 5;
/// One rgba16float layer and the rgba8unorm layers.
const BYTES_PER_TEXEL: u64 = 8 + 4 * BAKED_SURFACE_COLOR_LAYERS as u64;
/// The WebGPU default limit, which the renderer requests.
const MAX_TEXTURE_SIDE: u32 = 8192;
/// The fallback, read, and standby bakes take at most a seed and four strips
/// each. Keep synchronized with baked-surface-generate.wgsl.
const MAX_FILL_RECTS: usize = 16;
#[cfg(all(target_arch = "wasm32", feature = "playground"))]
pub const BAKED_SURFACE_FILL_BYTES: usize = size_of::<BakedSurfaceFill>();
/// Keep synchronized with `@workgroup_size` in baked-surface-generate.wgsl.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
const FILL_WORKGROUP_SIZE: u32 = 64;
/// Texels kept between the view and a window edge beyond the bilinear stencil,
/// absorbing single-precision rounding of shader positions.
const VIEW_MARGIN_TEXELS: f64 = 1.0;
/// Texels added to each texture side for the view margins and the stencil.
const SIDE_MARGIN_TEXELS: f64 = 2.0 * (VIEW_MARGIN_TEXELS + 2.0);
/// A finer rung replaces the read one only once it holds a view this much
/// wider, so a view near a rung boundary does not alternate between rebakes.
const ZOOM_IN_MARGIN: f64 = 1.05;
/// A view no level holds bakes a level this many octaves coarser first, a
/// sixteenth of the texels, which one frame's budget covers.
const COLD_START_OCTAVES: i32 = 2;
/// A frame bakes an eighth of a level, so a teleport reads a complete level
/// within eight frames, but at most `FILL_CAP_TEXELS`, and never less than a
/// sixteenth, which holds a cold start's coarse view.
const FILL_FRACTION: f64 = 1.0 / 8.0;
const MINIMUM_FILL_FRACTION: f64 = 1.0 / 16.0;
/// An eighth of a 1080p scene's level: in 1080p zoom and teleport sequences on
/// a Radeon 780M the shipped bake pass took 4.9-8.0 ms for these texels.
const FILL_CAP_TEXELS: f64 = (1 << 19) as f64;
/// Without a rung change in sight, the standby bakes at least this share of a
/// level per frame, completing within half a second.
const STANDBY_FLOOR_FRACTION: f64 = 1.0 / 32.0;
/// The standby completes this many frames before its rung is due.
const STANDBY_LEAD_FRAMES: f64 = 2.0;

fn rung_ratio() -> f64 {
    2.0_f64.powf(f64::from(RUNGS_PER_OCTAVE).recip())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedSurfaceSizes {
    pub texels: [u32; 2],
    /// Texels per pixel at the read rung's lower bound; below one where
    /// memory binds.
    pub density: f64,
    pub fill_budget_texels: i64,
}

impl BakedSurfaceSizes {
    pub fn bytes(&self) -> u64 {
        bake_bytes(self.texels)
    }
}

fn bake_bytes(texels: [u32; 2]) -> u64 {
    u64::from(BAKED_SURFACE_LEVELS) * u64::from(texels[0]) * u64::from(texels[1]) * BYTES_PER_TEXEL
}

/// Texture sides for a scene of `scene_size` pixels: one texel per pixel at
/// the read rung's lower bound, or as many as `memory_bytes` and texture
/// limits allow, preserving the scene's aspect.
pub fn baked_surface_sizes(scene_size: (u32, u32), memory_bytes: u64) -> BakedSurfaceSizes {
    let scene = [scene_size.0, scene_size.1].map(|side| f64::from(side.max(1)) * rung_ratio());
    let texels =
        |density: f64| scene.map(|side| (side * density + SIDE_MARGIN_TEXELS).ceil() as u32);
    let limit = f64::from(MAX_TEXTURE_SIDE) - SIDE_MARGIN_TEXELS;
    let mut density = scene
        .iter()
        .fold(1.0, |density, side| f64::min(density, limit / side));
    while density > 0.0 && bake_bytes(texels(density)) > memory_bytes {
        density *= 0.99;
    }
    let texels = texels(density);
    let level = f64::from(texels[0]) * f64::from(texels[1]);
    BakedSurfaceSizes {
        texels,
        density,
        fill_budget_texels: (FILL_FRACTION * level)
            .min(FILL_CAP_TEXELS)
            .max(MINIMUM_FILL_FRACTION * level)
            .ceil() as i64,
    }
}

/// How long a new scene size must hold before the bake textures follow it. A
/// drag resize changes the size every frame; a quarter second skips its
/// intermediate sizes while a maximise or rotation still reallocates within
/// about 15 frames, reading coarser levels meanwhile.
const BAKED_SURFACE_RESIZE_SETTLE_SECONDS: f64 = 0.25;

/// Follows the bake sizes a scene needs once a change has settled.
#[derive(Default)]
pub struct BakedSurfaceResize {
    pending: Option<(BakedSurfaceSizes, f64)>,
}

impl BakedSurfaceResize {
    /// The sizes to reallocate the textures to this frame, if any.
    pub fn follow(
        &mut self,
        current: BakedSurfaceSizes,
        wanted: BakedSurfaceSizes,
        elapsed_seconds: f64,
    ) -> Option<BakedSurfaceSizes> {
        if wanted == current {
            self.pending = None;
            return None;
        }
        let held_seconds = match self.pending {
            Some((pending, held_seconds)) if pending == wanted => held_seconds + elapsed_seconds,
            _ => 0.0,
        };
        if held_seconds < BAKED_SURFACE_RESIZE_SETTLE_SECONDS {
            self.pending = Some((wanted, held_seconds));
            return None;
        }
        self.pending = None;
        Some(wanted)
    }
}

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

/// The bake pass's uniform table.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakedSurfaceFill {
    rect_count: u32,
    padding: [u32; 3],
    rects: [FillRect; MAX_FILL_RECTS],
}

/// A frame's bakes and scene rows.
pub struct BakedSurfaceFrame {
    #[cfg_attr(
        not(any(target_arch = "wasm32", feature = "playground")),
        allow(dead_code)
    )]
    fill: BakedSurfaceFill,
    /// Texels the bake pass evaluates, one invocation each.
    fill_texels: u32,
    /// The scene shader's `baked_surface_grid_m`, `baked_surface_texels`, and
    /// `baked_surface_level` rows.
    pub grid_m: [f32; 4],
    pub texels: [u32; 4],
    pub level: [u32; 4],
}

impl BakedSurfaceFrame {
    /// A frame that bakes nothing and leaves every position to direct
    /// evaluation.
    pub fn direct() -> Self {
        Self {
            fill: BakedSurfaceFill {
                rect_count: 0,
                padding: [0; 3],
                rects: [FillRect::default(); MAX_FILL_RECTS],
            },
            fill_texels: 0,
            grid_m: [0.0, 0.0, 1.0, 1.0],
            texels: [0; 4],
            level: [0; 4],
        }
    }

    pub fn has_fills(&self) -> bool {
        self.fill_texels > 0
    }

    #[cfg(any(target_arch = "wasm32", feature = "playground"))]
    fn fill_workgroups(&self) -> u32 {
        self.fill_texels.div_ceil(FILL_WORKGROUP_SIZE)
    }

    /// The lab's fill table, dispatched as `fill_workgroups` workgroups of the
    /// bake shader when positive.
    #[cfg(all(target_arch = "wasm32", feature = "playground"))]
    pub fn fill_bytes(&self) -> (&[u8], u32) {
        (bytemuck::bytes_of(&self.fill), self.fill_workgroups())
    }
}

/// Bake schedule for the static surface: levels on a ladder of world-anchored
/// lattices, each windowed around the camera and addressed toroidally, so a
/// texel keeps its slot while it stays in view. Seed and world changes rebake;
/// lab tuning needs `invalidate`.
pub struct BakedSurface {
    sizes: BakedSurfaceSizes,
    key: Option<SurfaceKey>,
    /// Camera position with longitude unwrapped across seam crossings, so
    /// windows and slots stay continuous.
    camera_m: [f64; 2],
    canonical_camera_x_m: f64,
    slots: [Slot; BAKED_SURFACE_LEVELS as usize],
    /// The slots once the latest frame's fills have run.
    planned: [Slot; BAKED_SURFACE_LEVELS as usize],
    /// The rung the latest frame wanted to read, and its half view.
    primary: Option<i32>,
    half_view_m: Option<[f64; 2]>,
    /// The half view at which the read rung rose by one through the pixel
    /// footprint alone, keeping the finer level while the view holds.
    finer_kept_at_m: Option<[f64; 2]>,
    /// The latest frame's pixel footprint, from which the standby's bake is
    /// paced to the next rung change.
    footprint_m: Option<f64>,
}

#[derive(Clone, Copy, PartialEq)]
struct SurfaceKey {
    world_size_m: [u32; 2],
    seed: u64,
}

/// A texture level's rung and the lattice cells holding current values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Slot {
    rung: Option<i32>,
    valid: Cells,
}

struct Level {
    spacing_m: [f64; 2],
    lap_columns: u32,
    window: Cells,
    /// The camera in lattice units.
    camera: [f64; 2],
}

impl Level {
    fn new(rung: i32, texels: [u32; 2], world_width_m: f64, camera_m: [f64; 2]) -> Self {
        let spacing_m = 2.0_f64.powf(f64::from(rung) / f64::from(RUNGS_PER_OCTAVE));
        // Whole columns per lap keep the lattice continuous across the seam.
        let lap_columns = (world_width_m / spacing_m).round().max(1.0);
        let spacing_m = [world_width_m / lap_columns, spacing_m];
        let camera = [0, 1].map(|axis| camera_m[axis] / spacing_m[axis]);
        let min = [0, 1].map(|axis| camera[axis].floor() as i64 - i64::from(texels[axis] / 2) + 1);
        Self {
            spacing_m,
            lap_columns: lap_columns as u32,
            window: Cells {
                min,
                max: [0, 1].map(|axis| min[axis] + i64::from(texels[axis])),
            },
            camera,
        }
    }

    /// The cells whose bilinear stencils cover a view `scale` times as wide.
    fn view(&self, half_view_m: [f64; 2], scale: f64) -> Cells {
        let half = [0, 1]
            .map(|axis| scale * half_view_m[axis] / self.spacing_m[axis] + VIEW_MARGIN_TEXELS);
        Cells {
            min: [0, 1].map(|axis| (self.camera[axis] - half[axis]).floor() as i64),
            max: [0, 1].map(|axis| (self.camera[axis] + half[axis]).floor() as i64 + 2),
        }
    }

    fn holds(&self, half_view_m: [f64; 2], scale: f64) -> bool {
        let view = self.view(half_view_m, scale);
        view.intersection(self.window) == view
    }
}

impl BakedSurface {
    pub fn new(sizes: BakedSurfaceSizes) -> Self {
        Self {
            sizes,
            key: None,
            camera_m: [0.0; 2],
            canonical_camera_x_m: 0.0,
            slots: Default::default(),
            planned: Default::default(),
            primary: None,
            half_view_m: None,
            finer_kept_at_m: None,
            footprint_m: None,
        }
    }

    pub fn sizes(&self) -> BakedSurfaceSizes {
        self.sizes
    }

    /// Plans this frame's bakes and returns scene rows that assume they run.
    /// `pixel_footprint_m` bounds the read level's spacing from below.
    pub fn frame(
        &mut self,
        world_geometry: WorldGeometry,
        seed: u64,
        camera_m: [f32; 2],
        view_size_m: [f32; 2],
        pixel_footprint_m: f32,
    ) -> BakedSurfaceFrame {
        let (world_width_m, half_view_m) = self.follow(world_geometry, seed, camera_m, view_size_m);
        self.plan(world_width_m, half_view_m, f64::from(pixel_footprint_m))
    }

    /// `frame`, with one frame budget of the read level's texels baked again,
    /// as a fast pan bakes, while reading what `frame` would.
    pub fn frame_rebaking_budget(
        &mut self,
        world_geometry: WorldGeometry,
        seed: u64,
        camera_m: [f32; 2],
        view_size_m: [f32; 2],
        pixel_footprint_m: f32,
    ) -> BakedSurfaceFrame {
        let (world_width_m, half_view_m) = self.follow(world_geometry, seed, camera_m, view_size_m);
        let footprint_m = f64::from(pixel_footprint_m);
        let read = self.plan(world_width_m, half_view_m, footprint_m).level[0];
        // Rows regrow before columns, so one budget of rows bakes again.
        let valid = &mut self.slots[read as usize].valid;
        valid.min[1] = (valid.min[1] + self.sizes.fill_budget_texels / valid.size()[0].max(1))
            .min(valid.max[1]);
        self.plan(world_width_m, half_view_m, footprint_m)
    }

    /// Records that the latest frame's bakes were encoded; until then, the
    /// next frame schedules them again.
    pub fn commit(&mut self) {
        self.slots = self.planned;
    }

    /// Rebakes every level from the next frame on.
    pub fn invalidate(&mut self) {
        self.slots = Default::default();
        self.planned = Default::default();
        self.primary = None;
        self.half_view_m = None;
        self.finer_kept_at_m = None;
        self.footprint_m = None;
    }

    /// Tracks the camera and returns the world width and the half view.
    fn follow(
        &mut self,
        world_geometry: WorldGeometry,
        seed: u64,
        camera_m: [f32; 2],
        view_size_m: [f32; 2],
    ) -> (f64, [f64; 2]) {
        let world_width_m = f64::from(world_geometry.width_m());
        let canonical_camera_m = world_geometry.wrap_position_m(camera_m).map(f64::from);
        let key = SurfaceKey {
            world_size_m: [world_geometry.width_m(), world_geometry.height_m()].map(f32::to_bits),
            seed,
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
        let half_view_m = [
            (0.5 * f64::from(view_size_m[0])).min(0.5 * world_width_m),
            0.5 * f64::from(view_size_m[1]),
        ];
        (world_width_m, half_view_m)
    }

    fn plan(
        &mut self,
        world_width_m: f64,
        half_view_m: [f64; 2],
        pixel_footprint_m: f64,
    ) -> BakedSurfaceFrame {
        // A canvas without a size reads and bakes nothing.
        let measurable = |length: f64| length.is_finite() && length > 0.0;
        if !measurable(pixel_footprint_m) || !half_view_m.into_iter().all(measurable) {
            self.planned = self.slots;
            return BakedSurfaceFrame::direct();
        }
        let mut fill = BakedSurfaceFill {
            rect_count: 0,
            padding: [0; 3],
            rects: [FillRect::default(); MAX_FILL_RECTS],
        };
        let rungs = f64::from(RUNGS_PER_OCTAVE);
        let texels = self.sizes.texels;
        let camera_m = self.camera_m;
        let level = |rung| Level::new(rung, texels, world_width_m, camera_m);
        // The finest rung with at most `density` times the rung ratio texels
        // per pixel that holds the view; finer rungs than the latest need the
        // zoom-in margin.
        let finest_rung =
            (rungs * (pixel_footprint_m / self.sizes.density).log2()).ceil() as i32 - 1;
        let needed_spacing_m = (0..2)
            .map(|axis| half_view_m[axis] / (0.5 * f64::from(texels[axis])))
            .fold(0.0, f64::max);
        let mut primary = finest_rung.max((rungs * needed_spacing_m.log2()).floor() as i32 - 1);
        loop {
            let scale = if self.primary.is_some_and(|latest| primary < latest) {
                ZOOM_IN_MARGIN
            } else {
                1.0
            };
            if level(primary).holds(half_view_m, scale) {
                break;
            }
            primary += 1;
        }
        if self.primary == Some(primary - 1) && self.half_view_m == Some(half_view_m) {
            self.finer_kept_at_m = Some(half_view_m);
        } else if self.finer_kept_at_m != Some(half_view_m) {
            self.finer_kept_at_m = None;
        }
        self.primary = Some(primary);
        self.half_view_m = Some(half_view_m);

        let mut planned = self.slots;
        let mut levels = planned.map(|slot| slot.rung.map(level));
        for (slot, level) in planned.iter_mut().zip(&levels) {
            if let Some(level) = level {
                slot.valid = slot.valid.intersection(level.window);
            }
        }
        let covers = |planned: &[Slot], levels: &[Option<Level>], index: usize| {
            levels[index].as_ref().is_some_and(|level| {
                let view = level.view(half_view_m, 1.0);
                planned[index].valid.intersection(view) == view
            })
        };
        let holding = |planned: &[Slot], rung: i32| {
            (0..planned.len()).find(|&index| planned[index].rung == Some(rung))
        };
        let other = |index: usize| 1 - index;
        // A covering level, finer or coarser, serves while the primary bakes.
        let mut fallback = (0..planned.len())
            .filter(|&index| {
                planned[index].rung.is_some_and(|rung| rung != primary)
                    && covers(&planned, &levels, index)
            })
            .min_by_key(|&index| planned[index].rung.map(|rung| (rung - primary).abs()));
        let primary_index = holding(&planned, primary).unwrap_or_else(|| {
            // Keep the fallback, else the standby rung, else the finer slot.
            let index = match fallback {
                Some(fallback) => other(fallback),
                None => (0..planned.len())
                    .min_by_key(|&index| {
                        (
                            planned[index].rung == Some(primary + 1),
                            planned[index].rung,
                        )
                    })
                    .unwrap(),
            };
            planned[index] = Slot {
                rung: Some(primary),
                valid: Cells::default(),
            };
            levels[index] = Some(level(primary));
            index
        });

        let mut fill_texels = 0;
        let mut budget = self.sizes.fill_budget_texels;
        let mut bake =
            |planned: &mut [Slot], levels: &[Option<Level>], index: usize, whole, limit: i64| {
                let level = levels[index].as_ref().unwrap();
                let mut allowance = budget.min(limit);
                let granted = allowance;
                let fills = bake_cells(
                    &mut planned[index].valid,
                    level,
                    half_view_m,
                    whole,
                    &mut allowance,
                );
                budget -= granted - allowance;
                for cells in fills {
                    assert!(
                        (fill.rect_count as usize) < MAX_FILL_RECTS,
                        "too many fill rects"
                    );
                    let size = cells.size();
                    fill.rects[fill.rect_count as usize] = FillRect {
                        first_texel: [
                            cells.min[0].rem_euclid(i64::from(level.lap_columns)) as i32,
                            cells.min[1] as i32,
                        ],
                        texels: size.map(|size| size as u32),
                        first_slot: [0, 1].map(|axis| slot(cells.min[axis], texels[axis])),
                        spacing_m: level.spacing_m.map(|spacing| spacing as f32),
                        first_invocation: fill_texels,
                        level: index as u32,
                        lap_columns: level.lap_columns,
                        padding: 0,
                    };
                    fill.rect_count += 1;
                    fill_texels += (size[0] * size[1]) as u32;
                }
            };
        if fallback.is_none() && !covers(&planned, &levels, primary_index) {
            let coarse_rung = primary + COLD_START_OCTAVES * RUNGS_PER_OCTAVE;
            let index = other(primary_index);
            if planned[index].rung != Some(coarse_rung) {
                planned[index] = Slot {
                    rung: Some(coarse_rung),
                    valid: Cells::default(),
                };
                levels[index] = Some(level(coarse_rung));
            }
            fallback = Some(index);
        }
        // A fallback keeps only its view baked while the primary level bakes.
        let fallback = fallback.filter(|_| !covers(&planned, &levels, primary_index));
        if let Some(index) = fallback {
            bake(&mut planned, &levels, index, false, i64::MAX);
        }
        bake(&mut planned, &levels, primary_index, true, i64::MAX);
        let display = match fallback {
            Some(index)
                if !covers(&planned, &levels, primary_index)
                    && covers(&planned, &levels, index) =>
            {
                index
            }
            _ => primary_index,
        };
        // The standby replaces the other level only once the primary holds the
        // view, so a fallback still baking its view survives. A rung change
        // through the pixel footprint alone keeps the finer level, so a
        // resolution controller stepping back across the rung reads it without
        // a rebake.
        if covers(&planned, &levels, primary_index) && holding(&planned, primary + 1).is_none() {
            let index = other(primary_index);
            if !(planned[index].rung == Some(primary - 1)
                && self.finer_kept_at_m == Some(half_view_m))
            {
                planned[index] = Slot {
                    rung: Some(primary + 1),
                    valid: Cells::default(),
                };
                levels[index] = Some(level(primary + 1));
            }
        }
        if let Some(index) = holding(&planned, primary + 1).filter(|&index| Some(index) != fallback)
        {
            // Spread the rest of the standby over the frames until the
            // footprint, at its latest rate of change, reaches its rung.
            let level_texels = f64::from(texels[0]) * f64::from(texels[1]);
            let remaining =
                level_texels - planned[index].valid.size().iter().product::<i64>() as f64;
            let switch_footprint_m =
                self.sizes.density * 2.0_f64.powf(f64::from(primary + 1) / rungs);
            let growth = self
                .footprint_m
                .map_or(1.0, |latest| pixel_footprint_m / latest);
            let frames = if growth > 1.0 {
                (switch_footprint_m / pixel_footprint_m).max(1.0).ln() / growth.ln()
            } else {
                f64::INFINITY
            };
            let limit = (remaining / (frames - STANDBY_LEAD_FRAMES).max(1.0))
                .max(STANDBY_FLOOR_FRACTION * level_texels)
                .ceil() as i64;
            bake(&mut planned, &levels, index, true, limit);
        }
        self.planned = planned;
        self.footprint_m = Some(pixel_footprint_m);

        let read = levels[display].as_ref().unwrap();
        let valid = planned[display].valid;
        let mut frame = BakedSurfaceFrame {
            fill,
            fill_texels,
            grid_m: [0.0, 0.0, read.spacing_m[0] as f32, read.spacing_m[1] as f32],
            texels: [0; 4],
            level: [display as u32, 0, 0, 0],
        };
        if !valid.is_empty() {
            let size = valid.size();
            frame.grid_m[0] = (valid.min[0] as f64 * read.spacing_m[0] - camera_m[0]) as f32;
            frame.grid_m[1] = (valid.min[1] as f64 * read.spacing_m[1] - camera_m[1]) as f32;
            frame.texels = [
                slot(valid.min[0], texels[0]),
                slot(valid.min[1], texels[1]),
                size[0] as u32,
                size[1] as u32,
            ];
        }
        frame
    }
}

fn slot(cell: i64, texels: u32) -> u32 {
    cell.rem_euclid(i64::from(texels)) as u32
}

/// Extends `valid` within `budget` towards the level's window when `whole`,
/// otherwise towards the view, returning the cells to bake. An empty level
/// starts with a band across the view through the camera, then grows rows
/// before columns, so the view bakes first.
fn bake_cells(
    valid: &mut Cells,
    level: &Level,
    half_view_m: [f64; 2],
    whole: bool,
    budget: &mut i64,
) -> Vec<Cells> {
    let view = level.view(half_view_m, 1.0).intersection(level.window);
    let mut fills = Vec::new();
    if valid.is_empty() {
        let [width, height] = view.size();
        let rows = (*budget / width.max(1)).min(height);
        if rows == 0 {
            return fills;
        }
        let first_row =
            (level.camera[1].floor() as i64 - rows / 2).clamp(view.min[1], view.max[1] - rows);
        *valid = Cells {
            min: [view.min[0], first_row],
            max: [view.max[0], first_row + rows],
        };
        *budget -= width * rows;
        fills.push(*valid);
    }
    let extent = if whole { level.window } else { view };
    fills.extend(grow(valid, extent, budget, [1, 0]));
    fills
}

/// Textures for one size of bake.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
pub struct BakedSurfaceTextures {
    sizes: BakedSurfaceSizes,
    textures: [wgpu::Texture; 2],
    /// The scene's binding 5: rgba16float elevation, gradient, and bed
    /// reflectance, one layer per level.
    pub height_view: wgpu::TextureView,
    /// The scene's binding 6: rgba8unorm land layers.
    pub color_view: wgpu::TextureView,
}

#[cfg(any(target_arch = "wasm32", feature = "playground"))]
impl BakedSurfaceTextures {
    pub fn new(device: &wgpu::Device, sizes: BakedSurfaceSizes) -> Self {
        let texture = |label, layers, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: sizes.texels[0],
                    height: sizes.texels[1],
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
        let textures = [
            texture(
                "Baked surface height",
                BAKED_SURFACE_LEVELS,
                wgpu::TextureFormat::Rgba16Float,
            ),
            texture(
                "Baked surface colour",
                BAKED_SURFACE_LEVELS * BAKED_SURFACE_COLOR_LAYERS,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        ];
        let [height_view, color_view] = textures.each_ref().map(|texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        });
        Self {
            sizes,
            textures,
            height_view,
            color_view,
        }
    }

    pub fn sizes(&self) -> BakedSurfaceSizes {
        self.sizes
    }

    pub fn destroy(&self) {
        for texture in &self.textures {
            texture.destroy();
        }
    }
}

/// The bake textures and the pass that writes them, shared by every renderer of
/// the production scene.
#[cfg(any(target_arch = "wasm32", feature = "playground"))]
pub struct BakedSurfacePass {
    textures: BakedSurfaceTextures,
    input_layout: wgpu::BindGroupLayout,
    fill_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    fill: wgpu::Buffer,
    fill_bind_group: wgpu::BindGroup,
}

#[cfg(any(target_arch = "wasm32", feature = "playground"))]
impl BakedSurfacePass {
    /// `group_1_layout` is the playground uniform layout for the playground
    /// shader, or any layout for the production one, which leaves group 1
    /// empty. `constants` override the bake shader's, such as forcing every
    /// branch for timing.
    pub fn new(
        device: &wgpu::Device,
        textures: BakedSurfaceTextures,
        shader_source: &str,
        constants: &[(&str, f64)],
        group_1_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Baked surface input layout"),
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
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let storage_entry = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2Array,
            },
            count: None,
        };
        let fill_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Baked surface fill layout"),
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
                storage_entry(1, wgpu::TextureFormat::Rgba16Float),
                storage_entry(2, wgpu::TextureFormat::Rgba8Unorm),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Baked surface shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Baked surface pipeline layout"),
            bind_group_layouts: &[
                Some(&input_layout),
                Some(group_1_layout),
                Some(&fill_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Baked surface pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("generate_baked_surface"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            cache: None,
        });
        let fill = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Baked surface fill"),
            size: size_of::<BakedSurfaceFill>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fill_bind_group = fill_bind_group(device, &fill_layout, &fill, &textures);
        Self {
            textures,
            input_layout,
            fill_layout,
            pipeline,
            fill,
            fill_bind_group,
        }
    }

    pub fn textures(&self) -> &BakedSurfaceTextures {
        &self.textures
    }

    /// Bakes into `textures` from now on, releasing the old ones; the scene's
    /// bind group and the schedule need replacing too.
    pub fn replace_textures(&mut self, device: &wgpu::Device, textures: BakedSurfaceTextures) {
        self.fill_bind_group = fill_bind_group(device, &self.fill_layout, &self.fill, &textures);
        std::mem::replace(&mut self.textures, textures).destroy();
    }

    /// The bake's group 0: scene uniforms and the terrain cache it reads.
    pub fn input_bind_group(
        &self,
        device: &wgpu::Device,
        scene_uniforms: &wgpu::Buffer,
        terrain_cache_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Baked surface input bind group"),
            layout: &self.input_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(terrain_cache_view),
                },
            ],
        })
    }

    /// Encodes the frame's bakes after the terrain cache fill they read. The
    /// fill table is written through the queue, so each submission carries at
    /// most one frame's bakes.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input_bind_group: &wgpu::BindGroup,
        group_1: &wgpu::BindGroup,
        frame: &BakedSurfaceFrame,
        timestamp_writes: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        if !frame.has_fills() && timestamp_writes.is_none() {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Baked surface pass"),
            timestamp_writes,
        });
        if frame.has_fills() {
            queue.write_buffer(&self.fill, 0, bytemuck::bytes_of(&frame.fill));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, input_bind_group, &[]);
            pass.set_bind_group(1, group_1, &[]);
            pass.set_bind_group(2, &self.fill_bind_group, &[]);
            pass.dispatch_workgroups(frame.fill_workgroups(), 1, 1);
        }
    }
}

#[cfg(any(target_arch = "wasm32", feature = "playground"))]
fn fill_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    fill: &wgpu::Buffer,
    textures: &BakedSurfaceTextures,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Baked surface fill bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: fill.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&textures.height_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&textures.color_view),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    const SEED: u64 = 0x5eed;
    const DESKTOP_BYTES: u64 = 256 << 20;
    const TOUCH_BYTES: u64 = 64 << 20;
    /// A small scene, so levels bake in a few frames.
    const SCENE_PX: (u32, u32) = (848, 477);
    const BOAT_VIEW_M: [f32; 2] = [426.67, 240.0];

    /// A baked lattice point by rung, unwrapped column, and row.
    type Cell = (i32, i64, i64);
    /// Each baked lattice point's toroidal slot.
    type Filled = HashMap<Cell, [u32; 2]>;

    fn sizes() -> BakedSurfaceSizes {
        baked_surface_sizes(SCENE_PX, DESKTOP_BYTES)
    }

    fn footprint_m(view_m: [f32; 2]) -> f32 {
        view_m[1] / SCENE_PX.1 as f32
    }

    fn view_m(height_m: f32) -> [f32; 2] {
        [16.0 / 9.0 * height_m, height_m]
    }

    fn rung(spacing_m: f32) -> i32 {
        (f64::from(RUNGS_PER_OCTAVE) * f64::from(spacing_m).log2()).round() as i32
    }

    fn rects(frame: &BakedSurfaceFrame) -> &[FillRect] {
        &frame.fill.rects[..frame.fill.rect_count as usize]
    }

    fn spacing_y_m(frame: &BakedSurfaceFrame) -> f32 {
        frame.grid_m[3]
    }

    /// Records what each rect writes, unwrapping lap columns near `camera_x_m`.
    fn record(frame: &BakedSurfaceFrame, camera_x_m: f32, filled: &mut Filled) -> Vec<Cell> {
        let texels = sizes().texels;
        let mut cells = Vec::new();
        let mut invocation = 0;
        for rect in rects(frame) {
            assert_eq!(rect.first_invocation, invocation);
            invocation += rect.texels[0] * rect.texels[1];
            let lap = i64::from(rect.lap_columns);
            let camera_column =
                (f64::from(camera_x_m) / f64::from(rect.spacing_m[0])).round() as i64;
            for row in 0..i64::from(rect.texels[1]) {
                for column in 0..i64::from(rect.texels[0]) {
                    let lap_column = i64::from(rect.first_texel[0]) + column;
                    let laps = ((camera_column - lap_column) as f64 / lap as f64).round() as i64;
                    let cell = (
                        rung(rect.spacing_m[1]),
                        lap_column + lap * laps,
                        i64::from(rect.first_texel[1]) + row,
                    );
                    let slot = [
                        (rect.first_slot[0] + column as u32) % texels[0],
                        (rect.first_slot[1] + row as u32) % texels[1],
                    ];
                    filled.insert(cell, slot);
                    cells.push(cell);
                }
            }
        }
        assert_eq!(invocation, frame.fill_texels);
        cells
    }

    /// The read level's valid cells, as the scene rows describe them.
    fn read_cells(frame: &BakedSurfaceFrame, camera_m: [f32; 2]) -> Vec<Cell> {
        let first = [0, 1].map(|axis| {
            ((f64::from(frame.grid_m[axis]) + f64::from(camera_m[axis]))
                / f64::from(frame.grid_m[2 + axis]))
            .round() as i64
        });
        let mut cells = Vec::new();
        for row in 0..i64::from(frame.texels[3]) {
            for column in 0..i64::from(frame.texels[2]) {
                cells.push((rung(spacing_y_m(frame)), first[0] + column, first[1] + row));
            }
        }
        cells
    }

    fn step(
        surface: &mut BakedSurface,
        world: WorldGeometry,
        camera_m: [f32; 2],
        view_m: [f32; 2],
        filled: &mut Filled,
    ) -> BakedSurfaceFrame {
        let frame = surface.frame(world, SEED, camera_m, view_m, footprint_m(view_m));
        surface.commit();
        record(&frame, camera_m[0], filled);
        frame
    }

    fn settle(
        surface: &mut BakedSurface,
        world: WorldGeometry,
        camera_m: [f32; 2],
        view_m: [f32; 2],
        filled: &mut Filled,
    ) -> BakedSurfaceFrame {
        for _ in 0..64 {
            let frame = step(surface, world, camera_m, view_m, filled);
            if !frame.has_fills() {
                return frame;
            }
        }
        panic!("the baked surface keeps filling at a fixed camera");
    }

    /// Whether every view corner's bilinear stencil lies inside the valid extent.
    fn view_baked(frame: &BakedSurfaceFrame, world: WorldGeometry, view_m: [f32; 2]) -> bool {
        let half_m = [
            (0.5 * view_m[0]).min(0.5 * world.width_m()),
            0.5 * view_m[1],
        ];
        [[-1.0, -1.0], [1.0, 1.0], [-1.0, 1.0], [1.0, -1.0]]
            .iter()
            .all(|corner| {
                (0..2).all(|axis| {
                    let texel = ((corner[axis] * half_m[axis] - frame.grid_m[axis])
                        / frame.grid_m[2 + axis])
                        .floor();
                    texel >= 0.0 && texel + 1.0 < frame.texels[2 + axis] as f32
                })
            })
    }

    #[test]
    fn the_bake_holds_one_texel_per_pixel_unless_memory_binds() {
        let aspect = |texels: [u32; 2]| f64::from(texels[0]) / f64::from(texels[1]);
        let desktop = baked_surface_sizes((1280, 720), DESKTOP_BYTES);
        assert_eq!(desktop.density, 1.0);
        assert!(
            f64::from(desktop.texels[1]) >= 720.0 * rung_ratio(),
            "{desktop:?}"
        );
        assert!(desktop.bytes() <= DESKTOP_BYTES, "{}", desktop.bytes());
        for (scene, budget) in [
            ((3840, 2160), DESKTOP_BYTES),
            ((1536, 2049), TOUCH_BYTES),
            ((2880, 1800), TOUCH_BYTES),
        ] {
            let sizes = baked_surface_sizes(scene, budget);
            assert!(sizes.density < 1.0, "{scene:?}: {sizes:?}");
            assert!(
                (0.95 * budget as f64..=budget as f64).contains(&(sizes.bytes() as f64)),
                "{scene:?}: {} of {budget} bytes",
                sizes.bytes()
            );
            let scene_aspect = f64::from(scene.0) / f64::from(scene.1);
            assert!(
                (aspect(sizes.texels) - scene_aspect).abs() < 0.01 * scene_aspect,
                "{scene:?}: {sizes:?}"
            );
        }
        let wide = baked_surface_sizes((8192, 400), DESKTOP_BYTES);
        assert!(wide.texels[0] <= MAX_TEXTURE_SIDE, "{wide:?}");
    }

    #[test]
    fn bake_textures_follow_a_scene_size_once_it_settles() {
        let small = baked_surface_sizes((1280, 720), DESKTOP_BYTES);
        let large = baked_surface_sizes((1920, 1080), DESKTOP_BYTES);
        let mut resize = BakedSurfaceResize::default();
        // A drag passes through sizes for a frame each.
        for scene in [(1300, 730), (1400, 790), (1600, 900), (1920, 1080)] {
            let wanted = baked_surface_sizes(scene, DESKTOP_BYTES);
            assert_eq!(resize.follow(small, wanted, 1.0 / 60.0), None, "{scene:?}");
        }
        let frames = (1..600)
            .find(|_| resize.follow(small, large, 1.0 / 60.0).is_some())
            .expect("a settled size reallocates");
        assert!(frames > 1, "{frames} frames");
        assert_eq!(resize.follow(large, large, 1.0 / 60.0), None);
    }

    #[test]
    fn a_frame_bakes_a_bounded_share_of_a_large_level_and_a_cold_start_level_at_once() {
        for scene in [(3840, 2160), (2560, 1440)] {
            let sizes = baked_surface_sizes(scene, 512 << 20);
            let level = i64::from(sizes.texels[0]) * i64::from(sizes.texels[1]);
            assert!(sizes.fill_budget_texels * 8 < level, "{scene:?}: {sizes:?}");
            assert!(
                sizes.fill_budget_texels * 16 >= level,
                "{scene:?}: {sizes:?}"
            );
        }
        let small = sizes();
        let level = i64::from(small.texels[0]) * i64::from(small.texels[1]);
        assert!(small.fill_budget_texels * 8 >= level, "{small:?}");
    }

    #[test]
    fn a_gentle_zoom_out_spreads_the_standby_bake_over_the_frames_before_it_is_read() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let sizes = baked_surface_sizes((1920, 1080), DESKTOP_BYTES);
        let mut surface = BakedSurface::new(sizes);
        let mut frame_at = |view: [f32; 2]| {
            let frame = surface.frame(world, SEED, camera_m, view, view[1] / 1080.0);
            surface.commit();
            frame
        };
        while frame_at(BOAT_VIEW_M).has_fills() {}
        // Two octaves over four seconds at 60 frames per second.
        let mut view = BOAT_VIEW_M;
        let step = 4.0_f32.powf(1.0 / 240.0);
        for frame_index in 0..240 {
            view = view.map(|side| side * step);
            let frame = frame_at(view);
            assert!(
                view_baked(&frame, world, view),
                "frame {frame_index} leaves the view to direct evaluation",
            );
            assert!(
                i64::from(frame.fill_texels) * 2 <= sizes.fill_budget_texels,
                "frame {frame_index} bakes {} of {} texels",
                frame.fill_texels,
                sizes.fill_budget_texels,
            );
        }
    }

    #[test]
    fn a_footprint_jump_of_several_rungs_keeps_a_covering_level_instead_of_a_cold_start() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let fine = footprint_m(BOAT_VIEW_M);
        let mut settled = |footprint| {
            for _ in 0..64 {
                let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint);
                surface.commit();
                if !frame.has_fills() {
                    return spacing_y_m(&frame);
                }
            }
            panic!("the baked surface keeps filling at a fixed footprint");
        };
        let fine_spacing = settled(fine);
        let coarse = fine * (rung_ratio() as f32).powi(3) * 1.01;
        let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, coarse);
        surface.commit();
        assert!(view_baked(&frame, world, BOAT_VIEW_M));
        let read_spacing = spacing_y_m(&frame);
        assert!(read_spacing > fine_spacing);
        for rect in rects(&frame) {
            assert!(
                rect.spacing_m[1] <= read_spacing * 1.001,
                "bakes {} m texels to read {read_spacing} m ones",
                rect.spacing_m[1]
            );
        }
    }

    #[test]
    fn a_scene_whose_budget_the_coarse_level_exhausts_still_settles() {
        let world = WorldGeometry::default();
        let mut surface = BakedSurface::new(baked_surface_sizes((1, 1), DESKTOP_BYTES));
        let settled = (0..64).find(|_| {
            let frame = surface.frame(world, SEED, [-8_000.0, 0.0], [1.0, 1.0], 1.0);
            surface.commit();
            !frame.has_fills()
        });
        assert!(settled.is_some(), "a one-pixel scene keeps baking");
    }

    #[test]
    fn a_degenerate_view_bakes_nothing() {
        let world = WorldGeometry::default();
        for (view_m, footprint_m) in [
            ([426.0, f32::NAN], 0.2),
            ([426.0, 0.0], 0.2),
            ([426.0, 240.0], f32::INFINITY),
            ([426.0, 240.0], f32::NAN),
            ([426.0, 240.0], 0.0),
        ] {
            let mut surface = BakedSurface::new(sizes());
            let frame = surface.frame(world, SEED, [5_000.0, 9_000.0], view_m, footprint_m);
            assert!(!frame.has_fills(), "{view_m:?} at {footprint_m} m");
            assert_eq!(frame.texels[2..], [0, 0], "{view_m:?} at {footprint_m} m");
        }
    }

    #[test]
    fn level_lattices_tile_the_longitude_circumference() {
        for width_m in [8_000.0, 64_000.0, 100_003.0] {
            let world = WorldGeometry::new(width_m, 64_000.0).unwrap();
            for height_m in [15.0, 240.0, 2_400.0, 20_000.0] {
                let mut surface = BakedSurface::new(sizes());
                let view = view_m(height_m);
                let frame = surface.frame(world, SEED, [0.0; 2], view, footprint_m(view));
                for rect in rects(&frame) {
                    let lap_m = f64::from(rect.spacing_m[0]) * f64::from(rect.lap_columns);
                    assert!(
                        (lap_m - f64::from(width_m)).abs() < 0.01,
                        "{width_m} {height_m}: {lap_m}"
                    );
                }
                assert!(frame.has_fills(), "{width_m} {height_m}");
            }
        }
    }

    #[test]
    fn the_read_level_has_about_one_texel_per_pixel() {
        let world = WorldGeometry::default();
        for height_m in [15.0, 60.0, 240.0, 300.0, 1_500.0, 2_400.0, 20_000.0] {
            for camera_m in [[0.0, 0.0], [-14_900.0, -5_330.0], [31_990.0, 2.0e5]] {
                let view = view_m(height_m);
                let mut surface = BakedSurface::new(sizes());
                let frame = settle(&mut surface, world, camera_m, view, &mut Filled::new());
                let texels_per_pixel = footprint_m(view) / spacing_y_m(&frame);
                assert!(
                    (1.0..=1.001 * rung_ratio() as f32).contains(&texels_per_pixel),
                    "{height_m} m: {texels_per_pixel} texels per pixel",
                );
                assert!(
                    view_baked(&frame, world, view),
                    "{height_m} m at {camera_m:?}"
                );
            }
        }
    }

    #[test]
    fn a_small_scene_still_reads_at_most_one_rung_finer_than_its_pixels() {
        let world = WorldGeometry::default();
        let view = view_m(240.0);
        let footprint = 2.5 * footprint_m(view);
        let mut surface = BakedSurface::new(sizes());
        let frame = surface.frame(world, SEED, [0.0; 2], view, footprint);
        assert!(frame.has_fills());
        for rect in rects(&frame) {
            assert!(
                f64::from(rect.spacing_m[1]) >= f64::from(footprint) / rung_ratio() * 0.999,
                "{} m spacing for {footprint} m pixels",
                rect.spacing_m[1],
            );
        }
    }

    #[test]
    fn a_teleport_reads_a_coarse_level_from_its_first_frame_and_bakes_each_texel_once() {
        let world = WorldGeometry::default();
        let camera_m = [-5_560.0, -2_990.0];
        let mut surface = BakedSurface::new(sizes());
        let mut filled = Filled::new();
        let mut baked = Vec::new();
        let mut read_spacings = Vec::new();
        for frame_index in 0..64 {
            let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint_m(BOAT_VIEW_M));
            surface.commit();
            baked.extend(record(&frame, camera_m[0], &mut filled));
            assert!(i64::from(frame.fill_texels) <= sizes().fill_budget_texels);
            assert!(
                view_baked(&frame, world, BOAT_VIEW_M),
                "frame {frame_index} leaves the view to direct evaluation"
            );
            read_spacings.push(spacing_y_m(&frame));
            if !frame.has_fills() {
                break;
            }
        }
        let settled = *read_spacings.last().unwrap();
        // A quarter of the full density.
        assert!(
            (read_spacings[0] / settled - 4.0).abs() < 0.01,
            "{read_spacings:?}"
        );
        // The read level switches once, from the coarse level to the full one.
        let switches = read_spacings
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count();
        assert_eq!(switches, 1, "{read_spacings:?}");
        let mut seen = HashSet::new();
        for cell in &baked {
            assert!(seen.insert(cell), "{cell:?} baked twice");
        }
    }

    #[test]
    fn a_zoom_out_past_the_standby_reads_a_coarse_level_at_once() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let mut filled = Filled::new();
        settle(&mut surface, world, camera_m, BOAT_VIEW_M, &mut filled);
        let wide = BOAT_VIEW_M.map(|side| side * 8.0);
        let frame = step(&mut surface, world, camera_m, wide, &mut filled);
        assert!(view_baked(&frame, world, wide));
        let settled = settle(&mut surface, world, camera_m, wide, &mut filled);
        assert!(spacing_y_m(&frame) > spacing_y_m(&settled));
    }

    #[test]
    fn a_zoom_in_reads_the_coarser_level_until_the_finer_one_holds_the_view() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let mut filled = Filled::new();
        let wide = view_m(300.0);
        let before = settle(&mut surface, world, camera_m, wide, &mut filled);
        let narrow = view_m(160.0);
        let mut previous_spacing = spacing_y_m(&before);
        for frame_index in 0..32 {
            let frame = step(&mut surface, world, camera_m, narrow, &mut filled);
            assert!(i64::from(frame.fill_texels) <= sizes().fill_budget_texels);
            assert!(
                view_baked(&frame, world, narrow),
                "frame {frame_index} leaves the view to direct evaluation",
            );
            assert!(spacing_y_m(&frame) <= previous_spacing);
            previous_spacing = spacing_y_m(&frame);
        }
        // The zoom-in margin may leave slightly under one texel per pixel.
        let texels_per_pixel = footprint_m(narrow) / previous_spacing;
        assert!(
            (0.95..=1.001 * rung_ratio() as f32).contains(&texels_per_pixel),
            "{texels_per_pixel}"
        );
    }

    #[test]
    fn a_one_rung_zoom_out_reads_the_complete_standby_at_once() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let mut filled = Filled::new();
        let before = settle(&mut surface, world, camera_m, BOAT_VIEW_M, &mut filled);
        let wider = BOAT_VIEW_M.map(|side| side * 1.01 * rung_ratio() as f32);
        let frame = step(&mut surface, world, camera_m, wider, &mut filled);
        assert!(view_baked(&frame, world, wider));
        assert!(spacing_y_m(&frame) > spacing_y_m(&before));
    }

    #[test]
    fn a_slow_continuous_zoom_out_stays_fully_baked() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        // A 1080p scene, whose levels take several frames each to bake.
        let mut surface = BakedSurface::new(baked_surface_sizes((1920, 1080), DESKTOP_BYTES));
        let mut frame_at = |view: [f32; 2]| {
            let frame = surface.frame(world, SEED, camera_m, view, view[1] / 1080.0);
            surface.commit();
            frame
        };
        let mut first = frame_at(BOAT_VIEW_M);
        while first.has_fills() {
            first = frame_at(BOAT_VIEW_M);
        }
        let mut view = BOAT_VIEW_M;
        let mut spacing_m = spacing_y_m(&first);
        let frames = 90;
        for frame_index in 0..frames {
            view = view.map(|side| side * 1.02);
            let frame = frame_at(view);
            assert!(
                view_baked(&frame, world, view),
                "frame {frame_index} at {view:?} leaves the view to direct evaluation",
            );
            spacing_m = spacing_y_m(&frame);
        }
        let zoom = 1.02_f64.powi(frames);
        assert!(
            f64::from(spacing_m) >= 0.999 * zoom / rung_ratio() * f64::from(spacing_y_m(&first)),
            "{spacing_m} m after a {zoom}x zoom from {} m",
            spacing_y_m(&first)
        );
    }

    #[test]
    fn a_footprint_alternating_across_a_rung_does_not_rebake() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let fine = footprint_m(BOAT_VIEW_M);
        // A render scale change of just over a rung.
        let coarse = fine * rung_ratio() as f32 * 1.01;
        let mut settled = |footprint| {
            for _ in 0..64 {
                let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint);
                surface.commit();
                if !frame.has_fills() {
                    return spacing_y_m(&frame);
                }
            }
            panic!("the baked surface keeps filling at a fixed footprint");
        };
        let fine_spacing = settled(fine);
        let coarse_spacing = settled(coarse);
        assert!(coarse_spacing > fine_spacing);
        for footprint in [fine, coarse, fine, coarse] {
            let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint);
            surface.commit();
            assert_eq!(frame.fill_texels, 0, "{footprint} m pixels rebake");
        }
    }

    #[test]
    fn a_view_near_a_rung_boundary_does_not_rebake() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let mut filled = Filled::new();
        let base = spacing_y_m(&settle(
            &mut surface,
            world,
            camera_m,
            BOAT_VIEW_M,
            &mut filled,
        ));
        // Find a view just wide enough to need the standby rung.
        let height_m = (1..200)
            .map(|step_index| BOAT_VIEW_M[1] * 1.005_f32.powi(step_index))
            .find(|&height_m| {
                let frame = step(&mut surface, world, camera_m, view_m(height_m), &mut filled);
                spacing_y_m(&frame) > base
            })
            .expect("a wider view reads a coarser rung");
        settle(&mut surface, world, camera_m, view_m(height_m), &mut filled);
        for scale in [0.99, 1.0, 0.985, 1.0, 0.99] {
            let frame = step(
                &mut surface,
                world,
                camera_m,
                view_m(height_m * scale),
                &mut filled,
            );
            assert_eq!(frame.fill_texels, 0, "{scale}: a 1% zoom rebakes");
        }
    }

    #[test]
    fn panning_bakes_only_newly_exposed_texels_into_their_lattice_slots() {
        let world = WorldGeometry::default();
        let texels = sizes().texels;
        for (start_m, step_m) in [
            ([-5_560.0, -2_990.0], [28.0, -16.0]),
            ([-5_560.0, -2_990.0], [-3.0, 30.0]),
            // East across the longitude seam.
            ([31_990.0, 0.0], [20.0, 0.0]),
        ] {
            let mut surface = BakedSurface::new(sizes());
            let mut filled = Filled::new();
            let before = settle(&mut surface, world, start_m, BOAT_VIEW_M, &mut filled);
            let previous: HashSet<_> = read_cells(&before, start_m).into_iter().collect();
            let camera_m = world.wrap_position_m([start_m[0] + step_m[0], start_m[1] + step_m[1]]);
            let frame = surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint_m(BOAT_VIEW_M));
            surface.commit();
            let new_cells = record(&frame, start_m[0] + step_m[0], &mut filled);
            let read = read_cells(&frame, [start_m[0] + step_m[0], camera_m[1]]);
            assert!(view_baked(&frame, world, BOAT_VIEW_M), "{step_m:?}");
            let spacing = f64::from(spacing_y_m(&frame));
            let read_rung = rung(spacing_y_m(&frame));
            let exposed =
                [0, 1].map(|axis| (f64::from(step_m[axis]) / spacing).abs().ceil() as i64 + 1);
            let read_level_cells =
                new_cells.iter().filter(|cell| cell.0 == read_rung).count() as i64;
            assert!(
                read_level_cells
                    <= i64::from(texels[1]) * exposed[0] + i64::from(texels[0]) * exposed[1],
                "{step_m:?}: {read_level_cells} texels",
            );
            for cell in &new_cells {
                if cell.0 == read_rung {
                    assert!(!previous.contains(cell), "{step_m:?} rebaked {cell:?}");
                }
            }
            for cell in &read {
                assert!(
                    previous.contains(cell) || new_cells.contains(cell),
                    "{step_m:?}: {cell:?} was never baked",
                );
            }
            assert_eq!(
                [frame.texels[0], frame.texels[1]],
                filled[&read[0]],
                "{step_m:?}"
            );
            let slots: HashSet<_> = read.iter().map(|cell| filled[cell]).collect();
            assert_eq!(slots.len(), read.len(), "{step_m:?}: slots collide");
        }
    }

    #[test]
    fn a_rebaking_frame_bakes_one_budget_and_reads_the_settled_level() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let mut surface = BakedSurface::new(sizes());
        let settled = settle(
            &mut surface,
            world,
            camera_m,
            BOAT_VIEW_M,
            &mut Filled::new(),
        );
        let budget = sizes().fill_budget_texels;
        for _ in 0..3 {
            let frame = surface.frame_rebaking_budget(
                world,
                SEED,
                camera_m,
                BOAT_VIEW_M,
                footprint_m(BOAT_VIEW_M),
            );
            surface.commit();
            assert!(
                (budget * 9 / 10..=budget).contains(&i64::from(frame.fill_texels)),
                "{} texels",
                frame.fill_texels,
            );
            assert_eq!(
                (frame.grid_m, frame.texels, frame.level),
                (settled.grid_m, settled.texels, settled.level),
            );
        }
    }

    #[test]
    fn bakes_of_uncommitted_frames_are_scheduled_again() {
        let world = WorldGeometry::default();
        let mut surface = BakedSurface::new(sizes());
        let start_m = [-5_560.0, -2_990.0];
        settle(
            &mut surface,
            world,
            start_m,
            BOAT_VIEW_M,
            &mut Filled::new(),
        );
        let mut skipped = Filled::new();
        let mut camera_m = start_m;
        for _ in 0..3 {
            camera_m[0] += 20.0;
            record(
                &surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint_m(BOAT_VIEW_M)),
                camera_m[0],
                &mut skipped,
            );
        }
        let mut filled = Filled::new();
        let frame = settle(&mut surface, world, camera_m, BOAT_VIEW_M, &mut filled);
        for cell in skipped.keys() {
            assert!(filled.contains_key(cell), "{cell:?} was never baked");
        }
        assert!(view_baked(&frame, world, BOAT_VIEW_M));
    }

    #[test]
    fn seed_world_and_invalidation_rebake_from_the_view() {
        let world = WorldGeometry::default();
        let camera_m = [800.0, 0.0];
        let changes: [&dyn Fn(&mut BakedSurface) -> BakedSurfaceFrame; 3] = [
            &|surface| {
                surface.frame(
                    world,
                    SEED + 1,
                    camera_m,
                    BOAT_VIEW_M,
                    footprint_m(BOAT_VIEW_M),
                )
            },
            &|surface| {
                surface.frame(
                    WorldGeometry::new(64_000.0, 32_000.0).unwrap(),
                    SEED,
                    camera_m,
                    BOAT_VIEW_M,
                    footprint_m(BOAT_VIEW_M),
                )
            },
            &|surface| {
                surface.invalidate();
                surface.frame(world, SEED, camera_m, BOAT_VIEW_M, footprint_m(BOAT_VIEW_M))
            },
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut surface = BakedSurface::new(sizes());
            settle(
                &mut surface,
                world,
                camera_m,
                BOAT_VIEW_M,
                &mut Filled::new(),
            );
            let frame = change(&mut surface);
            surface.commit();
            let baked: HashSet<_> = record(&frame, camera_m[0], &mut Filled::new())
                .into_iter()
                .collect();
            let read = read_cells(&frame, camera_m);
            assert!(!read.is_empty(), "change {index} reads nothing");
            for cell in read {
                assert!(baked.contains(&cell), "change {index} reads stale {cell:?}");
            }
        }
    }
}
