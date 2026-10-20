use std::cmp::Ordering;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use game::{celestial_directions_at_height, celestial_orbit, world_celestial_conditions};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

/// The window adds the widest shadow footprint, about 8.5 km on each side of a
/// default-world view under the lowest light that still reaches the highest
/// peaks, so columns reach this budget in 16:9 views near 4 km tall and rows
/// near 7 km.
pub const ENVIRONMENT_FIELD_TEXELS: u32 = 384;
/// Keep synchronized with the layer packing in environment-fields.wgsl.
pub const ENVIRONMENT_FIELD_LAYERS: u32 = 5;
/// The finest stored octaves are the 960 m local pressure band and 512 m water
/// vapour. Eight or more texels per wavelength keep bilinear error below 8% of
/// that octave's amplitude, about 0.3% of the water-vapour field.
const ENVIRONMENT_FIELD_SPACING_M: f64 = 64.0;

/// Texels per axis of each occluder field layer; at 8 m this spans the 4 km
/// shadow footprint of a boat-zoom view under a light at the sea-level cutoff.
pub const OCCLUDER_FIELD_TEXELS: u32 = 512;
/// Texels a frame evaluates per occluder layer at most. On a Radeon 780M the
/// occluder pass of the 16 km bench views then takes about 0.8 ms, and the
/// low-cloud body grid of 16:9 views up to 1.5 km tall keeps its spacing on
/// both axes.
const OCCLUDER_LAYER_TEXELS: u32 = OCCLUDER_FIELD_TEXELS * OCCLUDER_FIELD_TEXELS / 2;
/// A layer keeps its previous spacing while that stays within budget and a
/// finer one would not fit in this share of the budgets, so a window that
/// hovers at a threshold while the camera pans keeps its lattice.
const OCCLUDER_REFINE_SHARE: f64 = 0.8;
/// Low-cloud body, Sun and Moon low-cloud shadow depth, canopy depth, and broad
/// landform elevation. Keep synchronized with occluder-fields.wgsl.
pub const OCCLUDER_FIELD_LAYERS: usize = 5;
/// The finest low-cloud latent octave spans 40 m per unit body size; ten texels
/// per wavelength keep bilinear error near 5% of that octave.
const LOW_CLOUD_FIELD_SPACING_M: f64 = 4.0;
/// The canopy threshold turns its 2 km finest latent octave into edges about
/// 100 m wide; three texels span such an edge.
const CANOPY_FIELD_SPACING_M: f64 = 32.0;
/// The broad landform's island octaves run from 1536 m down to 96 m. Over a
/// rugged island with 320 m peaks (seed 0x5eed around (-10000, 2000) m),
/// bilinear error at this spacing is 0.40 m at the 99th percentile and 0.71 m
/// at most over land, near the 0.6 m minimum landform penumbra. On a shadow
/// edge at (-9650, 1572) m under a light 3.5° high (world time 285240 s), grid
/// shadows at full strength differ from direct evaluation of direct sunlight by
/// at most 30 of 255 levels (22 at the 99th percentile) in a 15 m view, 34 (12)
/// in a 240 m view, and 43 (5) in a 3.5 km view.
const LANDFORM_FIELD_SPACING_M: f64 = 16.0;

// Keep these synchronized with scene-state.wgsl and lighting.wgsl.
const LOW_CLOUD_BASE_HEIGHT_M: f64 = 900.0;
const WEATHER_CANOPY_HEIGHT_M: f64 = 3600.0;
const LUNAR_DISTANCE_PLANET_RADII: f64 = 60.3;
const LARGEST_DISC_ANGULAR_RADIUS_RAD: f64 = 0.00465;
/// `shadow_horizon_cutoff` in lighting.wgsl removes every shadow at and below
/// this light height above the receiver's horizon.
const SHADOW_CUTOFF_Z: f64 = 0.04;
/// Below the lowest astronomical tide, the lowest shadow receiver.
const LOWEST_SHADOW_RECEIVER_M: f64 = -5.0;
/// Single-precision light directions in WGSL differ from these by far less.
const DIRECTION_TOLERANCE_RAD: f64 = 0.001;
/// Receiver cells per view axis for bounding projected shadow footprints.
const FOOTPRINT_CELLS: usize = 16;

pub struct FieldGrid {
    /// First texel centre relative to the camera, then column and row spacing,
    /// as the scene shader's grid rows read them.
    pub uniforms: [f32; 4],
    /// Texels per axis that the field pass evaluates.
    pub texels: [u32; 2],
}

impl FieldGrid {
    pub fn texel_uniforms(&self) -> [u32; 4] {
        [self.texels[0], self.texels[1], 0, 0]
    }

    /// Keep synchronized with `@workgroup_size` in the field generation shaders.
    pub fn workgroups(&self) -> [u32; 2] {
        self.texels.map(|texels| texels.div_ceil(8))
    }

    fn extent(&self) -> Region {
        let mut extent = Region::EMPTY;
        for axis in 0..2 {
            let first_m = f64::from(self.uniforms[axis]);
            extent.min[axis] = first_m;
            extent.max[axis] =
                first_m + f64::from(self.texels[axis] - 1) * f64::from(self.uniforms[2 + axis]);
        }
        extent
    }
}

pub struct FieldGrids {
    pub environment: FieldGrid,
    pub occluders: [FieldGrid; OCCLUDER_FIELD_LAYERS],
}

/// Each occluder layer's spacing doublings from the previous frame.
#[derive(Default)]
pub struct FieldGridHistory {
    occluder_doublings: [Option<[i32; 2]>; OCCLUDER_FIELD_LAYERS],
}

impl FieldGrids {
    /// The scene shader's `occluder_field_grid_m` and `occluder_field_texels` rows.
    pub fn occluder_uniforms(
        &self,
    ) -> (
        [[f32; 4]; OCCLUDER_FIELD_LAYERS],
        [[u32; 4]; OCCLUDER_FIELD_LAYERS],
    ) {
        (
            self.occluders.each_ref().map(|grid| grid.uniforms),
            self.occluders.each_ref().map(FieldGrid::texel_uniforms),
        )
    }

    /// One z slice per occluder layer, each covering the largest layer.
    pub fn occluder_workgroups(&self) -> [u32; 3] {
        let [columns, rows] = self.occluders.iter().fold([0, 0], |largest, grid| {
            let workgroups = grid.workgroups();
            [largest[0].max(workgroups[0]), largest[1].max(workgroups[1])]
        });
        [columns, rows, OCCLUDER_FIELD_LAYERS as u32]
    }
}

/// Tuning that changes which world positions the scene samples or how finely
/// it needs them.
#[derive(Clone, Copy)]
pub struct FieldGridTuning {
    pub low_cloud_body_size: f32,
    pub landform_shadow_reach_m: f32,
    pub terrain_elevation_scale: f32,
}

impl FieldGridTuning {
    /// Keep synchronized with `production_tuning` in environment.wgsl and the
    /// `LANDFORM_SHADOW_REACH_M` and `TERRAIN_ELEVATION_SCALE` overrides in
    /// scene-state.wgsl; the reach is the one under a high light.
    pub const PRODUCTION: Self = Self {
        low_cloud_body_size: 1.0,
        landform_shadow_reach_m: 320.0,
        terrain_elevation_scale: 3.0,
    };
}

/// Lattices for the per-frame field passes. Texel centres sit on multiples of
/// each grid's spacing in a frame that moves only with cloud transport, so
/// panning never moves them; views too wide for a grid's budget double its
/// spacing. Shadow windows bound every receiver's finite-disc projection, so
/// each tap the scene takes lands inside the written extent.
pub fn field_grids(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    camera_m: [f32; 2],
    view_size_m: [f32; 2],
    view_cloud_motion_m: [f32; 4],
    tuning: FieldGridTuning,
    history: &mut FieldGridHistory,
) -> FieldGrids {
    let world_width_m = f64::from(world_geometry.width_m());
    let camera = camera_m.map(f64::from);
    let half_view_m = view_size_m.map(|size| 0.5 * f64::from(size));
    let view = Region {
        min: half_view_m.map(|half| -half),
        max: half_view_m,
    };
    let reach_m = f64::from(tuning.landform_shadow_reach_m.max(1.0));
    let highest_receiver_m = highest_receiver_m(tuning.terrain_elevation_scale);
    let footprints = shadow_footprints(
        world_geometry,
        world_time_seconds,
        camera,
        half_view_m,
        reach_m,
        highest_receiver_m,
    );
    let low_cloud_anchor = [
        f64::from(view_cloud_motion_m[0]),
        f64::from(view_cloud_motion_m[1]),
    ];
    let canopy_anchor = [
        f64::from(view_cloud_motion_m[2]),
        f64::from(view_cloud_motion_m[3]),
    ];
    let low_cloud_spacing_m =
        LOW_CLOUD_FIELD_SPACING_M * f64::from(tuning.low_cloud_body_size.clamp(0.25, 4.0));
    let layers = [
        (low_cloud_anchor, view, low_cloud_spacing_m),
        (
            low_cloud_anchor,
            footprints.low_cloud[0],
            low_cloud_spacing_m,
        ),
        (
            low_cloud_anchor,
            footprints.low_cloud[1],
            low_cloud_spacing_m,
        ),
        (canopy_anchor, footprints.canopy, CANOPY_FIELD_SPACING_M),
        ([0.0; 2], footprints.landform, LANDFORM_FIELD_SPACING_M),
    ];
    let mut layer = 0;
    let occluders = layers.map(|(anchor, region, spacing_m)| {
        let region = wrapped_region(world_width_m, region.or_origin());
        let doublings = occluder_doublings(
            world_width_m,
            region,
            spacing_m,
            history.occluder_doublings[layer],
        );
        history.occluder_doublings[layer] = Some(doublings);
        layer += 1;
        field_grid_with_spacing(
            camera,
            anchor,
            region,
            [0, 1].map(|axis| lattice_spacing_m(world_width_m, spacing_m, axis, doublings[axis])),
        )
    });
    // Low-cloud texels reconstruct their weather from the environment grid,
    // which covers them. Its spacing fits the farthest footprint any light
    // could cast, so it follows zoom alone: footprints end within that reach
    // of the view, and a low-cloud grid's texels within two of its spacings
    // beyond its region. A layer inside that bound fits its budgets, and
    // their refine share, once both axes reach the bound's coarser doubling
    // under them, so neither axis passes it.
    let world_radius_m = world_width_m / TAU;
    let footprint_reach_m = Receivers::Surface {
        highest_m: highest_receiver_m,
    }
    .farthest_projection_m(
        world_radius_m,
        -FRAC_PI_2,
        FRAC_PI_2,
        LOW_CLOUD_BASE_HEIGHT_M,
    )
    .unwrap_or(0.0);
    let footprint_bound = wrapped_region(world_width_m, view.expanded(footprint_reach_m));
    let coarsest_doublings =
        occluder_doubling_band(world_width_m, footprint_bound, low_cloud_spacing_m)
            .into_iter()
            .flatten()
            .fold(0, i32::max);
    let low_cloud_texel_reach_m = [0, 1]
        .map(|axis| lattice_spacing_m(world_width_m, low_cloud_spacing_m, axis, coarsest_doublings))
        .into_iter()
        .fold(0.0, f64::max);
    let environment_spacing = field_spacing_m(
        world_width_m,
        wrapped_region(
            world_width_m,
            view.expanded(footprint_reach_m + 2.0 * low_cloud_texel_reach_m),
        ),
        ENVIRONMENT_FIELD_SPACING_M,
        ENVIRONMENT_FIELD_TEXELS,
        ENVIRONMENT_FIELD_TEXELS * ENVIRONMENT_FIELD_TEXELS,
    );
    let mut environment_region = view;
    for grid in &occluders[..3] {
        environment_region.include(grid.extent());
    }
    FieldGrids {
        environment: field_grid_with_spacing(
            camera,
            [0.0; 2],
            wrapped_region(world_width_m, environment_region),
            environment_spacing,
        ),
        occluders,
    }
}

/// Camera-relative bounds; empty while `min` exceeds `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Region {
    min: [f64; 2],
    max: [f64; 2],
}

impl Region {
    const EMPTY: Self = Self {
        min: [f64::INFINITY; 2],
        max: [f64::NEG_INFINITY; 2],
    };

    fn include(&mut self, other: Self) {
        for axis in 0..2 {
            self.min[axis] = self.min[axis].min(other.min[axis]);
            self.max[axis] = self.max[axis].max(other.max[axis]);
        }
    }

    fn include_point(&mut self, point: [f64; 2]) {
        self.include(Self {
            min: point,
            max: point,
        });
    }

    fn expanded(self, margin_m: f64) -> Self {
        Self {
            min: self.min.map(|min| min - margin_m),
            max: self.max.map(|max| max + margin_m),
        }
    }

    fn minkowski_sum(self, other: Self) -> Self {
        Self {
            min: [self.min[0] + other.min[0], self.min[1] + other.min[1]],
            max: [self.max[0] + other.max[0], self.max[1] + other.max[1]],
        }
    }

    /// A source lit nowhere in view still gets a minimal grid at the camera.
    fn or_origin(self) -> Self {
        if self.min[0] > self.max[0] {
            Self {
                min: [0.0; 2],
                max: [0.0; 2],
            }
        } else {
            self
        }
    }
}

struct ShadowFootprints {
    /// Sun, then Moon.
    low_cloud: [Region; 2],
    canopy: Region,
    /// Every landform sample position.
    landform: Region,
}

/// Bounds the cloud-layer positions that shadow taps sample. The view is cut
/// into cells; within a cell every receiver's light direction lies inside a
/// cone around the centre's, because moving across the surface rotates local
/// directions by at most the travelled angle of longitude and latitude. Each
/// cell then contributes itself plus every displacement such a cone can project,
/// including the landform march toward the light, as far as the cell's lowest
/// direction reaches.
fn shadow_footprints(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    camera_m: [f64; 2],
    half_view_m: [f64; 2],
    tuned_landform_reach_m: f64,
    highest_receiver_m: f64,
) -> ShadowFootprints {
    let world_radius_m = f64::from(world_geometry.width_m()) / TAU;
    let latitude_rad_per_m = PI / f64::from(world_geometry.height_m());
    let cell_half_m = half_view_m.map(|half| half / FOOTPRINT_CELLS as f64);
    let cell_rotation_rad = cell_half_m[0] / world_radius_m + cell_half_m[1] * latitude_rad_per_m;
    let orbit = celestial_orbit(world_celestial_conditions(
        world_geometry,
        world_time_seconds,
        Vector::new(camera_m[0] as f32, camera_m[1] as f32),
    ));
    let horizon_z = |height_m: f64| {
        let radius_ratio = world_radius_m / (world_radius_m + height_m);
        -(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt()
    };
    let low_cloud_horizon_rad = horizon_z(LOW_CLOUD_BASE_HEIGHT_M).asin();
    let shadow_lit_rad = lowest_shadow_light_rad(world_radius_m, highest_receiver_m);
    let mut footprints = ShadowFootprints {
        low_cloud: [Region::EMPTY; 2],
        canopy: Region::EMPTY,
        landform: Region {
            min: half_view_m.map(|half| -half),
            max: half_view_m,
        }
        .expanded(tuned_landform_reach_m),
    };
    for row in 0..FOOTPRINT_CELLS {
        for column in 0..FOOTPRINT_CELLS {
            let center_m = [
                -half_view_m[0] + (2 * column + 1) as f64 * cell_half_m[0],
                -half_view_m[1] + (2 * row + 1) as f64 * cell_half_m[1],
            ];
            let cell = Region {
                min: [center_m[0] - cell_half_m[0], center_m[1] - cell_half_m[1]],
                max: [center_m[0] + cell_half_m[0], center_m[1] + cell_half_m[1]],
            };
            let conditions = world_celestial_conditions(
                world_geometry,
                world_time_seconds,
                Vector::new(
                    (camera_m[0] + center_m[0]) as f32,
                    (camera_m[1] + center_m[1]) as f32,
                ),
            );
            let surface = celestial_directions_at_height(world_geometry, orbit, conditions, 0.0);
            let low_cloud = celestial_directions_at_height(
                world_geometry,
                orbit,
                conditions,
                LOW_CLOUD_BASE_HEIGHT_M as f32,
            );
            for (source, (surface_direction, low_cloud_direction)) in
                [(surface.0, low_cloud.0), (surface.1, low_cloud.1)]
                    .into_iter()
                    .enumerate()
            {
                let surface_receivers = Receivers::Surface {
                    highest_m: highest_receiver_m,
                };
                let low_cloud_receivers = Receivers::LowCloud {
                    horizon_rad: low_cloud_horizon_rad,
                };
                // Landform receivers lie below the canopy, so its parallax bounds theirs.
                let landform_cone_rad = cell_rotation_rad
                    + LARGEST_DISC_ANGULAR_RADIUS_RAD
                    + WEATHER_CANOPY_HEIGHT_M / (LUNAR_DISTANCE_PLANET_RADII * world_radius_m)
                    + DIRECTION_TOLERANCE_RAD;
                let [east, north, up] = surface_direction.map(f64::from);
                let elevation_rad = up.atan2(east.hypot(north));
                if elevation_rad + landform_cone_rad >= shadow_lit_rad {
                    footprints
                        .landform
                        .include(cell.minkowski_sum(offsets_toward(
                            surface_direction,
                            landform_cone_rad,
                            landform_shadow_reach_m(
                                world_radius_m,
                                tuned_landform_reach_m,
                                elevation_rad - landform_cone_rad,
                            ),
                        )));
                }
                let projections = [
                    (
                        surface_receivers,
                        surface_direction,
                        LOW_CLOUD_BASE_HEIGHT_M,
                    ),
                    (
                        surface_receivers,
                        surface_direction,
                        WEATHER_CANOPY_HEIGHT_M,
                    ),
                    (
                        low_cloud_receivers,
                        low_cloud_direction,
                        WEATHER_CANOPY_HEIGHT_M,
                    ),
                ];
                for (index, (receivers, direction, layer_height_m)) in
                    projections.into_iter().enumerate()
                {
                    // Topocentric lunar parallax changes with receiver height up to the layer.
                    let cone_rad = cell_rotation_rad
                        + LARGEST_DISC_ANGULAR_RADIUS_RAD
                        + layer_height_m / (LUNAR_DISTANCE_PLANET_RADII * world_radius_m)
                        + DIRECTION_TOLERANCE_RAD;
                    let Some(offsets) = receivers.projected_offsets(
                        world_radius_m,
                        direction,
                        cone_rad,
                        layer_height_m,
                    ) else {
                        continue;
                    };
                    let footprint = cell.minkowski_sum(offsets);
                    if index == 0 {
                        footprints.low_cloud[source].include(footprint);
                    } else {
                        footprints.canopy.include(footprint);
                    }
                }
            }
        }
    }
    footprints
}

#[derive(Clone, Copy)]
enum Receivers {
    /// Ground and water from the lowest shadow receiver up to `highest_m`.
    Surface { highest_m: f64 },
    /// The lower-cloud base, shadowed while any of the disc clears its horizon.
    LowCloud { horizon_rad: f64 },
}

impl Receivers {
    /// Displacements from these receivers to a layer along directions within
    /// `cone_rad` of `direction`; `None` when no such direction is lit.
    fn projected_offsets(
        self,
        world_radius_m: f64,
        direction: [f32; 3],
        cone_rad: f64,
        layer_height_m: f64,
    ) -> Option<Region> {
        let [east, north, up] = direction.map(f64::from);
        let elevation_rad = up.atan2(east.hypot(north));
        let distance_m = self.farthest_projection_m(
            world_radius_m,
            elevation_rad - cone_rad,
            elevation_rad + cone_rad,
            layer_height_m,
        )?;
        Some(offsets_toward(direction, cone_rad, distance_m))
    }

    /// Farthest surface distance to a layer from a receiver shadowed by a light
    /// between the two elevations. Lower taps and lower receivers project
    /// farther. A surface receiver takes taps down to its own cutoff less the
    /// disc radius, and raising it lowers that cutoff; the farthest tap then
    /// comes from the lowest receiver or from the one whose cutoff tap meets
    /// `lowest_rad`, because along cutoff taps reach grows with receiver height
    /// above sea level while the receiver stays below half the layer height.
    fn farthest_projection_m(
        self,
        world_radius_m: f64,
        lowest_rad: f64,
        highest_rad: f64,
        layer_height_m: f64,
    ) -> Option<f64> {
        match self {
            Self::LowCloud { horizon_rad } => {
                (highest_rad >= horizon_rad - LARGEST_DISC_ANGULAR_RADIUS_RAD).then(|| {
                    projection_distance_m(
                        world_radius_m,
                        lowest_rad.max(horizon_rad),
                        LOW_CLOUD_BASE_HEIGHT_M,
                        layer_height_m,
                    )
                })
            }
            Self::Surface { highest_m } => {
                if highest_rad < lowest_shadow_light_rad(world_radius_m, highest_m) {
                    return None;
                }
                let tap_rad = |height_m: f64| {
                    lowest_rad.max(
                        lowest_shadow_light_rad(world_radius_m, height_m)
                            - LARGEST_DISC_ANGULAR_RADIUS_RAD,
                    )
                };
                let reach_m = |height_m: f64| {
                    projection_distance_m(
                        world_radius_m,
                        tap_rad(height_m),
                        height_m,
                        layer_height_m,
                    )
                };
                if highest_m > 0.5 * layer_height_m {
                    return Some(projection_distance_m(
                        world_radius_m,
                        lowest_rad.max(
                            lowest_shadow_light_rad(world_radius_m, highest_m)
                                - LARGEST_DISC_ANGULAR_RADIUS_RAD,
                        ),
                        LOWEST_SHADOW_RECEIVER_M,
                        layer_height_m,
                    ));
                }
                let meeting_m = receiver_height_with_cutoff_m(
                    world_radius_m,
                    lowest_rad + LARGEST_DISC_ANGULAR_RADIUS_RAD,
                )
                .clamp(LOWEST_SHADOW_RECEIVER_M, highest_m);
                let lowest_lit = highest_rad
                    >= lowest_shadow_light_rad(world_radius_m, LOWEST_SHADOW_RECEIVER_M);
                Some(if lowest_lit {
                    reach_m(meeting_m).max(reach_m(LOWEST_SHADOW_RECEIVER_M))
                } else {
                    reach_m(meeting_m)
                })
            }
        }
    }
}

/// The receiver height whose shadow cutoff is `elevation_rad`, the inverse of
/// `lowest_shadow_light_rad`: below sea level when every receiver's cutoff is at
/// most that light, infinite when none reaches it.
fn receiver_height_with_cutoff_m(world_radius_m: f64, elevation_rad: f64) -> f64 {
    let horizon_z = (elevation_rad.sin() - SHADOW_CUTOFF_Z) / (1.0 - SHADOW_CUTOFF_Z);
    if horizon_z >= 0.0 {
        f64::NEG_INFINITY
    } else if horizon_z <= -1.0 {
        f64::INFINITY
    } else {
        world_radius_m / (1.0 - horizon_z * horizon_z).sqrt() - world_radius_m
    }
}

/// Displacements up to `distance_m` along the horizontal directions of every
/// light within `cone_rad` of `direction`.
fn offsets_toward(direction: [f32; 3], cone_rad: f64, distance_m: f64) -> Region {
    let [east, north, up] = direction.map(f64::from);
    let elevation_rad = up.atan2(east.hypot(north));
    let highest_rad = (elevation_rad + cone_rad).min(FRAC_PI_2);
    let mut offsets = Region::EMPTY;
    offsets.include_point([0.0; 2]);
    if cone_rad >= FRAC_PI_2 || cone_rad.sin() >= highest_rad.cos() {
        return offsets.expanded(distance_m);
    }
    let azimuth_rad = north.atan2(east);
    let spread_rad = (cone_rad.sin() / highest_rad.cos()).asin();
    let along = |angle_rad: f64| [distance_m * angle_rad.cos(), distance_m * angle_rad.sin()];
    offsets.include_point(along(azimuth_rad - spread_rad));
    offsets.include_point(along(azimuth_rad + spread_rad));
    for quarter in 0..4 {
        let axis_rad = f64::from(quarter) * FRAC_PI_2;
        let difference_rad = (axis_rad - azimuth_rad + PI).rem_euclid(TAU) - PI;
        if difference_rad.abs() <= spread_rad {
            offsets.include_point(along(axis_rad));
        }
    }
    offsets
}

/// Surface distance from a receiver to where a ray at `elevation_rad` meets a
/// layer, mirroring `projected_cloud_position` in lighting.wgsl.
fn projection_distance_m(
    world_radius_m: f64,
    elevation_rad: f64,
    receiver_height_m: f64,
    layer_height_m: f64,
) -> f64 {
    if receiver_height_m >= layer_height_m {
        return 0.0;
    }
    let (sine, cosine) = elevation_rad.sin_cos();
    let receiver_radius_m = world_radius_m + receiver_height_m;
    let layer_radius_m = world_radius_m + layer_height_m;
    let along_ray_m = receiver_radius_m * sine;
    let ray_m = -along_ray_m
        + (along_ray_m * along_ray_m + layer_radius_m * layer_radius_m
            - receiver_radius_m * receiver_radius_m)
            .sqrt();
    world_radius_m * (ray_m * cosine).atan2(receiver_radius_m + ray_m * sine)
}

/// How far the landform shadow march reaches toward a light at `elevation_rad`,
/// as `landform_shadow_reach_m` in terrain.wgsl computes it per pixel.
fn landform_shadow_reach_m(world_radius_m: f64, tuned_reach_m: f64, elevation_rad: f64) -> f64 {
    let (sine, cosine) = elevation_rad.max(SHADOW_CUTOFF_Z.asin()).sin_cos();
    let along_ray_m = world_radius_m * sine;
    let ray_m = -along_ray_m
        + (along_ray_m * along_ray_m + tuned_reach_m * (2.0 * world_radius_m + tuned_reach_m))
            .sqrt();
    tuned_reach_m.max(ray_m * cosine)
}

/// The highest surface a shadow can fall on: broad landform peaks at
/// 18 m × (1 − 0.30) × 20 per unit of elevation scale (`broad_landform_elevation_m`
/// in terrain.wgsl), and land detail adds less than 6 m.
fn highest_receiver_m(terrain_elevation_scale: f32) -> f64 {
    252.0 * f64::from(terrain_elevation_scale.max(0.0)) + 6.0
}

/// Elevation of the lowest light that casts shadows on any receiver up to
/// `highest_receiver_m`: the cutoff above that receiver's lower horizon.
fn lowest_shadow_light_rad(world_radius_m: f64, highest_receiver_m: f64) -> f64 {
    let radius_ratio = world_radius_m / (world_radius_m + highest_receiver_m);
    let horizon_z = -(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt();
    (horizon_z + SHADOW_CUTOFF_Z * (1.0 - horizon_z)).asin()
}

/// Shader lookups wrap longitude to within half a lap of the camera, so a
/// region crossing that seam needs every column.
fn wrapped_region(world_width_m: f64, region: Region) -> Region {
    let half_world_m = 0.5 * world_width_m;
    let mut region = region;
    if region.min[0] < -half_world_m || region.max[0] > half_world_m {
        region.min[0] = -half_world_m;
        region.max[0] = half_world_m;
    }
    region
}

/// Spacing along `axis` after `doublings` of the base spacing.
fn lattice_spacing_m(world_width_m: f64, base_spacing_m: f64, axis: usize, doublings: i32) -> f64 {
    let spacing_m = base_spacing_m * 2.0_f64.powi(doublings);
    if axis == 1 {
        return spacing_m;
    }
    // Whole columns per longitude lap keep the lattice continuous across the seam.
    let laps_per_column = spacing_m / world_width_m;
    if laps_per_column < 1.0 {
        world_width_m / laps_per_column.recip().round()
    } else {
        world_width_m * laps_per_column.round()
    }
}

/// Doublings of the base column and row spacing for a region of this size
/// anywhere on the lattice: a placement adds a texel beyond each end and rounds
/// each end outwards, so it never needs more than the extent plus five texels
/// per axis.
fn field_doublings(
    world_width_m: f64,
    region: Region,
    base_spacing_m: f64,
    axis_texels: u32,
    layer_texels: u32,
) -> [i32; 2] {
    let texels = |axis: usize, doublings: i32| {
        (region.max[axis] - region.min[axis])
            / lattice_spacing_m(world_width_m, base_spacing_m, axis, doublings)
            + 5.0
    };
    // Each axis coarsens alone, so within the layer budget a window long in one
    // direction keeps its spacing across the other.
    let mut doublings = [0; 2];
    for (axis, axis_doublings) in doublings.iter_mut().enumerate() {
        while texels(axis, *axis_doublings) > f64::from(axis_texels) {
            *axis_doublings += 1;
        }
    }
    // A layer over its budget coarsens its finer axis first.
    loop {
        let counts = [texels(0, doublings[0]), texels(1, doublings[1])];
        if counts[0] * counts[1] <= f64::from(layer_texels) {
            return doublings;
        }
        let axis = match doublings[0].cmp(&doublings[1]) {
            Ordering::Less => 0,
            Ordering::Greater => 1,
            Ordering::Equal => usize::from(counts[1] > counts[0]),
        };
        doublings[axis] += 1;
    }
}

fn field_spacing_m(
    world_width_m: f64,
    region: Region,
    base_spacing_m: f64,
    axis_texels: u32,
    layer_texels: u32,
) -> [f64; 2] {
    let doublings = field_doublings(
        world_width_m,
        region,
        base_spacing_m,
        axis_texels,
        layer_texels,
    );
    [0, 1].map(|axis| lattice_spacing_m(world_width_m, base_spacing_m, axis, doublings[axis]))
}

/// The finest doublings that fit an occluder layer's budgets, then the finest
/// that fit their refine share.
fn occluder_doubling_band(
    world_width_m: f64,
    region: Region,
    base_spacing_m: f64,
) -> [[i32; 2]; 2] {
    [1.0, OCCLUDER_REFINE_SHARE].map(|share| {
        field_doublings(
            world_width_m,
            region,
            base_spacing_m,
            (share * f64::from(OCCLUDER_FIELD_TEXELS)) as u32,
            (share * f64::from(OCCLUDER_LAYER_TEXELS)) as u32,
        )
    })
}

fn occluder_doublings(
    world_width_m: f64,
    region: Region,
    base_spacing_m: f64,
    previous: Option<[i32; 2]>,
) -> [i32; 2] {
    let [finest, refine_limit] = occluder_doubling_band(world_width_m, region, base_spacing_m);
    match previous {
        Some(previous)
            if (0..2).all(|axis| (finest[axis]..=refine_limit[axis]).contains(&previous[axis])) =>
        {
            previous
        }
        _ => finest,
    }
}

/// `region` must already be wrapped, and `spacing` must fit any region
/// containing it.
fn field_grid_with_spacing(
    camera_m: [f64; 2],
    anchor_m: [f64; 2],
    region: Region,
    spacing: [f64; 2],
) -> FieldGrid {
    let mut grid = FieldGrid {
        uniforms: [0.0; 4],
        texels: [0; 2],
    };
    for axis in 0..2 {
        let lattice_camera_m = camera_m[axis] - anchor_m[axis];
        // A texel beyond each edge's bilinear neighbours absorbs
        // single-precision rounding of shader positions.
        let first = ((lattice_camera_m + region.min[axis]) / spacing[axis]).floor() - 1.0;
        let last = ((lattice_camera_m + region.max[axis]) / spacing[axis]).ceil() + 1.0;
        grid.uniforms[axis] = (first * spacing[axis] - lattice_camera_m) as f32;
        grid.uniforms[2 + axis] = spacing[axis] as f32;
        grid.texels[axis] = (last - first + 1.0) as u32;
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_width_m;

    const NO_MOTION: [f32; 4] = [0.0; 4];
    /// Local midnight at zero longitude in the default world.
    const MIDNIGHT_SECONDS: f64 = 36_001.0;

    fn production_grids(
        world_time_seconds: f64,
        camera_m: [f32; 2],
        view_size_m: [f32; 2],
        view_cloud_motion_m: [f32; 4],
    ) -> FieldGrids {
        field_grids(
            WorldGeometry::default(),
            world_time_seconds,
            camera_m,
            view_size_m,
            view_cloud_motion_m,
            FieldGridTuning::PRODUCTION,
            &mut FieldGridHistory::default(),
        )
    }

    fn assert_on_lattice(grid: &FieldGrid, lattice_origin_m: [f64; 2], label: &str) {
        for (axis, origin_m) in lattice_origin_m.into_iter().enumerate() {
            let index =
                (f64::from(grid.uniforms[axis]) - origin_m) / f64::from(grid.uniforms[2 + axis]);
            assert!(
                (index - index.round()).abs() < 1.0e-3,
                "{label} axis {axis}: {index}",
            );
        }
    }

    /// The written extent reaches one texel beyond `region` on every side,
    /// clipped in longitude to the half lap that shader lookups wrap into.
    fn assert_covers(grid: &FieldGrid, region: Region, label: &str) {
        let extent = grid.extent();
        let half_world_m = 0.5 * f64::from(WorldGeometry::default().width_m());
        for axis in 0..2 {
            let spacing_m = f64::from(grid.uniforms[2 + axis]);
            let (mut min_m, mut max_m) = (region.min[axis], region.max[axis]);
            if axis == 0 {
                min_m = min_m.max(-half_world_m);
                max_m = max_m.min(half_world_m);
            }
            assert!(
                extent.min[axis] <= min_m - spacing_m + 1.0e-3,
                "{label} axis {axis}"
            );
            assert!(
                extent.max[axis] >= max_m + spacing_m - 1.0e-3,
                "{label} axis {axis}"
            );
        }
    }

    fn view_region(view_size_m: [f32; 2]) -> Region {
        let half_m = view_size_m.map(|size| 0.5 * f64::from(size));
        Region {
            min: half_m.map(|half| -half),
            max: half_m,
        }
    }

    #[test]
    fn field_texels_stay_on_their_lattices_while_panning_and_transporting_clouds() {
        let view_size_m = [427.0, 240.0];
        let motion_m = [313.25, -71.5, -2_041.0, 88.75];
        for camera_m in [[1234.5, -987.25], [1244.5, -980.25], [-31_990.0, 2.0e6]] {
            let grids = production_grids(43_200.0, camera_m, view_size_m, motion_m);
            let camera = camera_m.map(f64::from);
            let world_lattice = [-camera[0], -camera[1]];
            let low_cloud_lattice = [
                f64::from(motion_m[0]) - camera[0],
                f64::from(motion_m[1]) - camera[1],
            ];
            let canopy_lattice = [
                f64::from(motion_m[2]) - camera[0],
                f64::from(motion_m[3]) - camera[1],
            ];
            assert_on_lattice(&grids.environment, world_lattice, "environment");
            for layer in 0..3 {
                assert_on_lattice(&grids.occluders[layer], low_cloud_lattice, "low cloud");
            }
            assert_on_lattice(&grids.occluders[3], canopy_lattice, "canopy");
            assert_on_lattice(&grids.occluders[4], world_lattice, "landform");
        }
    }

    #[test]
    fn field_columns_tile_the_world_circumference() {
        for world_width_m in [8_000.0, 64_000.0, 100_003.0] {
            let world_geometry = WorldGeometry::new(world_width_m, 64_000.0).unwrap();
            let grids = field_grids(
                world_geometry,
                43_200.0,
                [0.0, 0.0],
                [100.0, 100.0],
                NO_MOTION,
                FieldGridTuning::PRODUCTION,
                &mut FieldGridHistory::default(),
            );
            for grid in std::iter::once(&grids.environment).chain(&grids.occluders) {
                let columns = f64::from(world_width_m) / f64::from(grid.uniforms[2]);
                assert!(
                    (columns - columns.round()).abs() < 1.0e-3,
                    "{world_width_m}: {columns}"
                );
            }
        }
    }

    #[test]
    fn field_grids_stay_bounded_and_cover_the_view_at_any_zoom() {
        let reach_m = f64::from(FieldGridTuning::PRODUCTION.landform_shadow_reach_m);
        for view_height_m in [15.0, 1_500.0, 20_000.0, 1.0e6, 1.0e9] {
            for aspect in [16.0 / 9.0, 9.0 / 16.0] {
                let view_size_m = [view_width_m(aspect, view_height_m), view_height_m];
                for world_time_seconds in [0.0, 21_600.0, 43_200.0] {
                    let grids = production_grids(
                        world_time_seconds,
                        [500.0, 1.0e5],
                        view_size_m,
                        NO_MOTION,
                    );
                    let view = view_region(view_size_m);
                    let label = format!("{view_size_m:?} at {world_time_seconds} s");
                    assert!(
                        grids
                            .environment
                            .texels
                            .iter()
                            .all(|&texels| texels <= ENVIRONMENT_FIELD_TEXELS),
                        "{label}",
                    );
                    for grid in &grids.occluders {
                        assert!(
                            grid.texels
                                .iter()
                                .all(|&texels| texels <= OCCLUDER_FIELD_TEXELS),
                            "{label}",
                        );
                    }
                    assert_covers(&grids.environment, view, &label);
                    assert_covers(&grids.occluders[0], view, &label);
                    assert_covers(&grids.occluders[3], view, &label);
                    assert_covers(&grids.occluders[4], view.expanded(reach_m), &label);
                }
            }
        }
    }

    #[test]
    fn occluder_layers_stay_within_their_texel_budget() {
        for view_height_m in [1_500.0, 6_000.0, 16_000.0, 20_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for hour in [0, 6, 12, 18] {
                let world_time_seconds = MIDNIGHT_SECONDS + f64::from(hour) * 3_600.0;
                let grids = production_grids(
                    world_time_seconds,
                    [-23_500.0, -16_500.0],
                    view_size_m,
                    NO_MOTION,
                );
                for (layer, grid) in grids.occluders.iter().enumerate() {
                    assert!(
                        grid.texels[0] * grid.texels[1] <= OCCLUDER_LAYER_TEXELS,
                        "{view_height_m} m view at {world_time_seconds} s, layer {layer}: {:?}",
                        grid.texels,
                    );
                }
            }
        }
    }

    fn occluder_doublings_for(width_m: f64, height_m: f64) -> [i32; 2] {
        field_doublings(
            f64::from(WorldGeometry::default().width_m()),
            Region {
                min: [0.0; 2],
                max: [width_m, height_m],
            },
            4.0,
            OCCLUDER_FIELD_TEXELS,
            OCCLUDER_LAYER_TEXELS,
        )
    }

    #[test]
    fn a_window_within_the_layer_budget_keeps_each_axis_spacing() {
        // 380 x 255 texels at 8 m by 4 m.
        assert_eq!(occluder_doublings_for(3_000.0, 1_000.0), [1, 0]);
    }

    #[test]
    fn a_layer_over_its_budget_coarsens_its_finer_axis_first() {
        // 455 x 455 texels at 8 m by 4 m exceed the budget; 455 x 230 fit.
        assert_eq!(occluder_doublings_for(3_600.0, 1_800.0), [1, 1]);
    }

    #[test]
    fn a_layer_over_its_budget_at_equal_spacing_coarsens_the_axis_with_more_texels() {
        // 505 x 380 texels at 8 m exceed the budget; 255 x 380 fit.
        assert_eq!(occluder_doublings_for(4_000.0, 3_000.0), [2, 1]);
        assert_eq!(occluder_doublings_for(3_000.0, 4_000.0), [1, 2]);
    }

    #[test]
    fn an_occluder_layer_keeps_its_spacing_until_a_finer_one_fits_comfortably() {
        let world_width_m = f64::from(WorldGeometry::default().width_m());
        let region = |width_m: f64| Region {
            min: [0.0; 2],
            max: [width_m, 1_000.0],
        };
        // 2050 m needs 8 m columns; at 1900 m 4 m columns fit, though not in the
        // refine share, and at 1500 m they fit comfortably.
        let coarse = occluder_doublings(world_width_m, region(2_050.0), 4.0, None);
        assert_eq!(coarse, [1, 0]);
        assert_eq!(
            occluder_doublings(world_width_m, region(1_900.0), 4.0, None),
            [0, 0]
        );
        assert_eq!(
            occluder_doublings(world_width_m, region(1_900.0), 4.0, Some(coarse)),
            coarse,
        );
        assert_eq!(
            occluder_doublings(world_width_m, region(1_500.0), 4.0, Some(coarse)),
            [0, 0],
        );
        assert_eq!(
            occluder_doublings(world_width_m, region(2_050.0), 4.0, Some([0, 0])),
            coarse,
        );
    }

    /// Every receiver the shader shadows, from low tide to the highest peak,
    /// takes its cloud-shadow taps inside the Sun, Moon and canopy windows.
    #[test]
    fn cloud_shadow_taps_of_every_receiver_height_land_inside_their_windows() {
        let world = WorldGeometry::default();
        let world_radius_m = f64::from(world.width_m()) / TAU;
        let highest_m = highest_receiver_m(FieldGridTuning::PRODUCTION.terrain_elevation_scale);
        let heights_m = [
            LOWEST_SHADOW_RECEIVER_M,
            0.0,
            50.0,
            150.0,
            300.0,
            500.0,
            highest_m,
        ];
        let camera_m = [-23_500.0_f32, -16_500.0];
        for view_height_m in [240.0, 1_500.0, 6_000.0, 16_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for step in 0..96 {
                let world_time_seconds = MIDNIGHT_SECONDS + f64::from(step) * 900.0;
                let grids = production_grids(world_time_seconds, camera_m, view_size_m, NO_MOTION);
                let orbit = celestial_orbit(world_celestial_conditions(
                    world,
                    world_time_seconds,
                    Vector::ZERO,
                ));
                for row in 0..=8 {
                    for column in 0..=8 {
                        let receiver_m = [
                            f64::from(view_size_m[0]) * (f64::from(column) / 8.0 - 0.5),
                            f64::from(view_size_m[1]) * (f64::from(row) / 8.0 - 0.5),
                        ];
                        let conditions = world_celestial_conditions(
                            world,
                            world_time_seconds,
                            Vector::new(
                                camera_m[0] + receiver_m[0] as f32,
                                camera_m[1] + receiver_m[1] as f32,
                            ),
                        );
                        for height_m in heights_m {
                            let lights = celestial_directions_at_height(
                                world,
                                orbit,
                                conditions,
                                height_m as f32,
                            );
                            let radius_ratio =
                                world_radius_m / (world_radius_m + height_m.max(0.0));
                            let horizon_z = -(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt();
                            for (source, light) in [lights.0, lights.1].into_iter().enumerate() {
                                let [east, north, up] = light.map(f64::from);
                                if (up - horizon_z) / (1.0 - horizon_z) <= SHADOW_CUTOFF_Z {
                                    continue;
                                }
                                let azimuth_rad = north.atan2(east);
                                let elevation_rad = up.atan2(east.hypot(north));
                                for (layer, layer_height_m) in [
                                    (1 + source, LOW_CLOUD_BASE_HEIGHT_M),
                                    (3, WEATHER_CANOPY_HEIGHT_M),
                                ] {
                                    let extent = grids.occluders[layer].extent();
                                    let distance_m = projection_distance_m(
                                        world_radius_m,
                                        elevation_rad,
                                        height_m,
                                        layer_height_m,
                                    );
                                    let tap_m = [
                                        receiver_m[0] + distance_m * azimuth_rad.cos(),
                                        receiver_m[1] + distance_m * azimuth_rad.sin(),
                                    ];
                                    assert!(
                                        (0..2).all(|axis| {
                                            (extent.min[axis]..=extent.max[axis])
                                                .contains(&tap_m[axis])
                                        }),
                                        "{view_height_m} m view at {world_time_seconds} s: \
                                         source {source} from {height_m} m at {receiver_m:?} \
                                         taps layer {layer} at {tap_m:?} outside {extent:?}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn environment_grid_covers_every_low_cloud_texel() {
        for (camera_m, view_height_m) in [
            ([800.0, 0.0], 300.0),
            ([2400.0, 2000.0], 1_500.0),
            ([-4000.0, 0.0], 20_000.0),
        ] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for hour in 0..24 {
                let world_time_seconds = MIDNIGHT_SECONDS + f64::from(hour) * 3_600.0;
                let grids = production_grids(
                    world_time_seconds,
                    camera_m,
                    view_size_m,
                    [120.0, -40.0, 0.0, 0.0],
                );
                for grid in &grids.occluders[..3] {
                    assert_covers(
                        &grids.environment,
                        grid.extent(),
                        &format!("{view_height_m} m view at {world_time_seconds} s"),
                    );
                }
            }
        }
    }

    #[test]
    fn environment_spacing_follows_zoom_alone() {
        for view_height_m in [240.0, 1_500.0, 12_400.0, 20_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            let spacings: Vec<_> = (0..24)
                .map(|hour| {
                    production_grids(
                        MIDNIGHT_SECONDS + f64::from(hour) * 3_600.0,
                        [2_400.0, 2_000.0],
                        view_size_m,
                        NO_MOTION,
                    )
                    .environment
                    .uniforms[3]
                })
                .collect();
            assert!(
                spacings.iter().all(|&spacing| spacing == spacings[0]),
                "{view_height_m} m view: {spacings:?}",
            );
        }
    }

    #[test]
    fn shadow_footprints_survive_cones_past_the_pole() {
        // A short pole span turns each view cell through more than half a turn
        // of latitude, while a wide world casts footprints far beyond the
        // lattice's snapping margin.
        let world_geometry = WorldGeometry::new(256_000.0, 8_000.0).unwrap();
        let view_height_m = 300_000.0;
        let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
        let grids = field_grids(
            world_geometry,
            MIDNIGHT_SECONDS + 43_200.0,
            [0.0, 0.0],
            view_size_m,
            NO_MOTION,
            FieldGridTuning::PRODUCTION,
            &mut FieldGridHistory::default(),
        );
        let sun = grids.occluders[1].extent();
        let spacing_m = f64::from(grids.occluders[1].uniforms[3]);
        assert!(
            sun.max[1] > 0.5 * f64::from(view_height_m) + 2.0 * spacing_m + 1_000.0,
            "Sun footprint stops at the view: {sun:?} with {spacing_m} m texels",
        );
    }

    #[test]
    fn shadow_grids_follow_the_light() {
        let view_size_m = [427.0, 240.0];
        let night = production_grids(MIDNIGHT_SECONDS, [0.0, 0.0], view_size_m, NO_MOTION);
        assert!(night.occluders[1].texels.iter().all(|&texels| texels <= 4));
        let noon = production_grids(
            MIDNIGHT_SECONDS + 43_200.0,
            [0.0, 0.0],
            view_size_m,
            NO_MOTION,
        );
        assert_covers(&noon.occluders[1], view_region(view_size_m), "noon");
        let afternoon = production_grids(
            MIDNIGHT_SECONDS + 64_000.0,
            [0.0, 0.0],
            view_size_m,
            NO_MOTION,
        );
        let afternoon_extent = afternoon.occluders[1].extent();
        let noon_extent = noon.occluders[1].extent();
        assert!(
            afternoon_extent.max[0] - afternoon_extent.min[0]
                > 2.0 * (noon_extent.max[0] - noon_extent.min[0]),
            "a low western Sun stretches the footprint: {afternoon_extent:?} vs {noon_extent:?}",
        );
        let afternoon_landform = afternoon.occluders[4].extent();
        let noon_landform = noon.occluders[4].extent();
        assert!(
            afternoon_landform.min[0] < noon_landform.min[0] - 1_000.0,
            "a low western Sun lengthens the landform march: \
             {afternoon_landform:?} vs {noon_landform:?}",
        );
    }

    #[test]
    fn landform_shadow_reach_follows_the_light_height() {
        let world_radius_m = f64::from(WorldGeometry::default().width_m()) / TAU;
        let base_m = 320.0;
        let reach_m = |elevation_deg: f64| {
            landform_shadow_reach_m(world_radius_m, base_m, elevation_deg.to_radians())
        };
        for elevation_deg in [45.0, 60.0, 90.0] {
            assert_eq!(reach_m(elevation_deg), base_m, "{elevation_deg}°");
        }
        // Below 45°, a ray from sea level toward the light has climbed the
        // base reach above the curved surface where the march ends, and at
        // most a few percent more.
        let cutoff_deg = SHADOW_CUTOFF_Z.asin().to_degrees();
        for elevation_deg in [cutoff_deg, 3.0, 5.0, 10.0, 20.0, 40.0] {
            let elevation_rad = elevation_deg.to_radians();
            let angle_rad = reach_m(elevation_deg) / world_radius_m;
            let ray_height_m = world_radius_m * elevation_rad.cos()
                / (angle_rad + elevation_rad).cos()
                - world_radius_m;
            assert!(
                (base_m - 0.5..1.05 * base_m).contains(&ray_height_m),
                "{elevation_deg}°: the ray is {ray_height_m} m up at the reach",
            );
        }
        for elevation_deg in [0.0, 1.0, -3.0] {
            assert_eq!(
                reach_m(elevation_deg),
                reach_m(cutoff_deg),
                "{elevation_deg}°"
            );
        }
    }
}
