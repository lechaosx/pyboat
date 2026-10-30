use std::cmp::Ordering;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use game::{
    TERRAIN_ELEVATION_SCALE, celestial_directions_at_height, celestial_lighting_at_height,
    celestial_orbit, world_celestial_conditions,
};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

/// The scene's window covers the view, so 16:9 views reach this budget near
/// 13 km tall.
pub const ENVIRONMENT_FIELD_TEXELS: u32 = 384;
/// The cloud weather window adds the farthest cloud weather read, about 19 km
/// on each side of a default-world view and 41 km on a 256 km world. At this
/// budget its spacing starts at 256 m on the default world and stays there for
/// 16:9 views up to about 8.5 km tall. The deck's drivers vary over 8 km and
/// more; what coarsens is mainly the land share across coasts, which moves
/// cumulus cover only where it is marginal.
const CLOUD_WEATHER_FIELD_TEXELS: u32 = 220;
/// The scene's five layers, then the cumulus and deck drivers and the
/// ground-sky bounce conditions on the cloud weather grid. Keep synchronized
/// with environment-fields.wgsl.
pub const ENVIRONMENT_FIELD_LAYERS: u32 = 8;
/// The finest stored octaves are the 960 m local pressure band and 512 m water
/// vapour. Eight or more texels per wavelength keep bilinear error below 8% of
/// that octave's amplitude, about 0.3% of the water-vapour field.
const ENVIRONMENT_FIELD_SPACING_M: f64 = 64.0;

/// Texels per axis of each occluder field layer; at 8 m this spans the 4 km
/// shadow footprint of a boat-zoom view under a light at the sea-level horizon.
pub const OCCLUDER_FIELD_TEXELS: u32 = 512;
/// Texels a frame evaluates per occluder layer at most. On a Radeon 780M the
/// occluder pass of the 16 km bench views then takes about 0.9 ms.
const OCCLUDER_LAYER_TEXELS: u32 = OCCLUDER_FIELD_TEXELS * OCCLUDER_FIELD_TEXELS / 2;
/// Each landform horizon texel marches about 30 landform samples per light
/// under a high light, about 65 at the sea-level horizon of the default world,
/// and about 80 at the highest receiver's horizon. At this budget 16:9 views up
/// to about 1 km tall keep 8 m texels, and the occluder pass of the 2.4 km polar
/// bench view takes about 0.7 ms rather than 0.9 ms at the full layer budget
/// on a Radeon 780M.
const LANDFORM_HORIZON_LAYER_TEXELS: u32 = OCCLUDER_LAYER_TEXELS / 4;
/// A cloud shadow layer keeps the layer budget while its footprint spans up to
/// `CLOUD_SHADOW_STRETCH_AREAS[0]` view areas and shrinks to this budget by
/// `CLOUD_SHADOW_STRETCH_AREAS[1]`, as a low light stretches the footprint:
/// texels beyond the view resolve shadow reach rather than detail on screen.
const CLOUD_SHADOW_LAYER_TEXELS: u32 = OCCLUDER_LAYER_TEXELS / 2;
const CLOUD_SHADOW_STRETCH_AREAS: [f64; 2] = [2.0, 4.0];
/// A layer keeps its previous spacing while that stays within budget and a
/// finer one would not fit in this share of the budgets, so a window that
/// hovers at a threshold while the camera pans keeps its lattice.
const OCCLUDER_REFINE_SHARE: f64 = 0.8;
/// Sun and Moon cloud shadow transmittance, broad landform elevation, and the
/// Sun and Moon landform horizons. Keep synchronized with occluder-fields.wgsl.
pub const OCCLUDER_FIELD_LAYERS: usize = 5;
const LANDFORM_LAYER: usize = 2;
/// The horizon layers march the landform layer, so a second dispatch fills
/// them after the others.
pub const LANDFORM_HORIZON_LAYER: usize = 3;
/// Cloud shadow rims span at least the disc's penumbra, about 9 m under the
/// lowest cumulus seen from sea level, so these texels put two or more across
/// the narrowest rim.
const CLOUD_SHADOW_SPACING_M: f64 = 4.0;
/// The broad landform's island octaves run from 1536 m down to 96 m. Over a
/// rugged island with 320 m peaks (seed 0x5eed around (-10000, 2000) m),
/// bilinear error at this spacing is 0.40 m at the 99th percentile and 0.71 m
/// at most over land, near the 0.6 m minimum landform penumbra.
const LANDFORM_FIELD_SPACING_M: f64 = 16.0;
/// Horizon heights bend where the blocker that defines them changes. In 240 m
/// views of that island under a light 15° high, 16 m texels scallop shadow
/// edges and leave 1.5% of sunlit pixels more than a quarter of the disc from
/// the largest occlusion of a dense per-pixel march over directly evaluated
/// landform; 8 m texels leave 0.5%.
const LANDFORM_HORIZON_SPACING_M: f64 = 8.0;
/// A grid's shortest resolved wavelength follows this multiple of the spacing
/// its window needs before rounding to a doubling, and spans at least two base
/// texels. Rounding and the refine share's hysteresis keep the actual spacing
/// below 2.5 times the needed one, so the wavelength stays at least two actual
/// texels while zoom changes it smoothly.
const CLOUD_SHADOW_WAVELENGTH_PER_SPACING: f64 = 5.0;
/// The Moon's grid is evaluated while its share of the clear-sky light
/// reaches this anywhere in view: half the share at which lighting.wgsl starts
/// fading its cloud shadows in, so the view's sample points need not land on
/// the brightest receiver. Keep synchronized with `MOON_CLOUD_SHADOW_SHARES`
/// in lighting.wgsl.
const MOON_CLOUD_SHADOW_GRID_SHARE: f64 = 0.01;

// Keep these synchronized with scene-state.wgsl and lighting.wgsl.
const CLOUD_SHADOW_PLANE_HEIGHT_M: f64 = 900.0;
/// The deck's weather read is the farthest along each light: cumulus lie below it.
const DECK_HEIGHT_M: f64 = 3600.0;
const LUNAR_DISTANCE_PLANET_RADII: f64 = 60.3;
const LARGEST_DISC_ANGULAR_RADIUS_RAD: f64 = 0.00465;
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
    /// Weather under every cloud that cloud shadow texels read.
    pub cloud_weather: FieldGrid,
    pub occluders: [FieldGrid; OCCLUDER_FIELD_LAYERS],
    /// The scene shader's `cloud_shadow_grids` row: the shortest wavelength
    /// the Sun's and the Moon's cloud shadow grids resolve, then the weight
    /// of the Moon's cloud shadows.
    pub cloud_shadow_grids: [f32; 4],
}

/// Each occluder layer's spacing doublings from the previous frame.
#[derive(Default)]
pub struct FieldGridHistory {
    occluder_doublings: [Option<[i32; 2]>; OCCLUDER_FIELD_LAYERS],
}

impl FieldGrids {
    /// One dispatch fills the environment and cloud weather grids.
    pub fn environment_workgroups(&self) -> [u32; 2] {
        let [scene, cloud] = [&self.environment, &self.cloud_weather].map(FieldGrid::workgroups);
        [scene[0].max(cloud[0]), scene[1].max(cloud[1])]
    }

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

    /// One z slice per layer before the landform horizons, each covering the
    /// largest of them.
    pub fn occluder_workgroups(&self) -> [u32; 3] {
        layer_workgroups(&self.occluders[..LANDFORM_HORIZON_LAYER])
    }

    /// One z slice per landform horizon layer, each covering the larger.
    pub fn landform_horizon_workgroups(&self) -> [u32; 3] {
        layer_workgroups(&self.occluders[LANDFORM_HORIZON_LAYER..])
    }
}

fn layer_workgroups(grids: &[FieldGrid]) -> [u32; 3] {
    let [columns, rows] = grids.iter().fold([0, 0], |largest, grid| {
        let workgroups = grid.workgroups();
        [largest[0].max(workgroups[0]), largest[1].max(workgroups[1])]
    });
    [columns, rows, grids.len() as u32]
}

/// Tuning that changes which world positions the scene samples.
#[derive(Clone, Copy)]
pub struct FieldGridTuning {
    pub landform_shadow_reach_m: f32,
}

impl FieldGridTuning {
    /// Keep synchronized with the `LANDFORM_SHADOW_REACH_M` override in
    /// scene-state.wgsl; the reach is the one under a high light.
    pub const PRODUCTION: Self = Self {
        landform_shadow_reach_m: 320.0,
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
    let highest_receiver_m = highest_receiver_m();
    let mut occluder_grid = |layer: usize, anchor: [f64; 2], region: Region, spacing_m: f64| {
        let region = wrapped_region(world_width_m, region.or_origin());
        let layer_texels = if layer >= LANDFORM_HORIZON_LAYER {
            LANDFORM_HORIZON_LAYER_TEXELS
        } else if layer < LANDFORM_LAYER {
            cloud_shadow_layer_texels(region, view)
        } else {
            OCCLUDER_LAYER_TEXELS
        };
        let doublings = occluder_doublings(
            world_width_m,
            region,
            spacing_m,
            layer_texels,
            history.occluder_doublings[layer],
        );
        history.occluder_doublings[layer] = Some(doublings);
        (
            field_grid_with_spacing(
                camera,
                anchor,
                region,
                [0, 1]
                    .map(|axis| lattice_spacing_m(world_width_m, spacing_m, axis, doublings[axis])),
            ),
            region,
        )
    };
    let landform_horizons = [LANDFORM_HORIZON_LAYER, LANDFORM_HORIZON_LAYER + 1]
        .map(|layer| occluder_grid(layer, [0.0; 2], view, LANDFORM_HORIZON_SPACING_M).0);
    // Horizon texels beyond the view march from there.
    let landform_receiver_margin_m = landform_horizons
        .iter()
        .map(|grid| {
            let extent = grid.extent();
            (0..2)
                .map(|axis| {
                    (extent.max[axis] - view.max[axis]).max(view.min[axis] - extent.min[axis])
                })
                .fold(0.0, f64::max)
        })
        .fold(0.0, f64::max);
    let footprints = shadow_footprints(
        world_geometry,
        world_time_seconds,
        camera,
        half_view_m,
        reach_m,
        highest_receiver_m,
        landform_receiver_margin_m,
    );
    let moon_grid = moon_clear_sky_share(
        world_geometry,
        world_time_seconds,
        camera,
        half_view_m,
        highest_receiver_m,
    ) >= MOON_CLOUD_SHADOW_GRID_SHARE;
    let mut cloud_shadow_footprints = footprints.cloud_shadow;
    if !moon_grid {
        cloud_shadow_footprints[1] = Region::EMPTY;
    }
    // Cloud shadow texels move with the cumulus transport, so the cumulus
    // texture is never resampled.
    let cloud_anchor = [
        f64::from(view_cloud_motion_m[0]),
        f64::from(view_cloud_motion_m[1]),
    ];
    let [sun_horizon, moon_horizon] = landform_horizons;
    let cloud_shadows = [0, 1].map(|source| {
        occluder_grid(
            source,
            cloud_anchor,
            cloud_shadow_footprints[source],
            CLOUD_SHADOW_SPACING_M,
        )
    });
    let [sun_wavelength_m, moon_wavelength_m] = cloud_shadows.each_ref().map(|(_, region)| {
        (CLOUD_SHADOW_WAVELENGTH_PER_SPACING
            * fitting_spacing_m(
                *region,
                OCCLUDER_FIELD_TEXELS,
                cloud_shadow_layer_texels(*region, view),
            ))
        .max(2.0 * CLOUD_SHADOW_SPACING_M)
    });
    let cloud_shadow_grids = [
        sun_wavelength_m as f32,
        moon_wavelength_m as f32,
        f32::from(u8::from(moon_grid)),
        0.0,
    ];
    let [(sun_cloud_shadow, _), (moon_cloud_shadow, _)] = cloud_shadows;
    let occluders = [
        sun_cloud_shadow,
        moon_cloud_shadow,
        occluder_grid(
            LANDFORM_LAYER,
            [0.0; 2],
            footprints.landform,
            LANDFORM_FIELD_SPACING_M,
        )
        .0,
        sun_horizon,
        moon_horizon,
    ];
    // Cloud shadow texels reconstruct what the weather under their clouds sets
    // from the cloud weather grid, which covers them. Its spacing fits the farthest
    // read any light could cause, so it follows zoom alone: reads end within
    // that reach of the view, and a cloud shadow grid's texels within two of
    // its spacings beyond its region. A layer inside the plane's bound fits
    // its budgets, and their refine share, once both axes reach the bound's
    // coarser doubling under them, so neither axis passes it.
    let world_radius_m = world_width_m / TAU;
    let receivers = Receivers {
        highest_m: highest_receiver_m,
    };
    let plane_reach_m = receivers
        .farthest_projection_m(
            world_radius_m,
            -FRAC_PI_2,
            FRAC_PI_2,
            CLOUD_SHADOW_PLANE_HEIGHT_M,
        )
        .unwrap_or(0.0);
    let coarsest_doublings = occluder_doubling_band(
        world_width_m,
        wrapped_region(world_width_m, view.expanded(plane_reach_m)),
        CLOUD_SHADOW_SPACING_M,
        CLOUD_SHADOW_LAYER_TEXELS,
    )
    .into_iter()
    .flatten()
    .fold(0, i32::max);
    let cloud_texel_reach_m = [0, 1]
        .map(|axis| {
            lattice_spacing_m(
                world_width_m,
                CLOUD_SHADOW_SPACING_M,
                axis,
                coarsest_doublings,
            )
        })
        .into_iter()
        .fold(0.0, f64::max);
    let cloud_weather_bound = wrapped_region(
        world_width_m,
        view.expanded(receivers.farthest_cloud_weather_read_m(world_radius_m)),
    );
    let mut cloud_weather_region = Region::EMPTY;
    for (source, footprint) in cloud_shadow_footprints.into_iter().enumerate() {
        // Texels beyond a footprint read no farther beyond it than they lie.
        let mut reads = deck_footprint(
            world_geometry,
            world_time_seconds,
            camera,
            footprint,
            source,
        )
        .expanded(2.0 * cloud_texel_reach_m);
        for axis in 0..2 {
            reads.min[axis] = reads.min[axis].max(cloud_weather_bound.min[axis]);
            reads.max[axis] = reads.max[axis].min(cloud_weather_bound.max[axis]);
        }
        if (0..2).all(|axis| reads.min[axis] <= reads.max[axis]) {
            cloud_weather_region.include(reads);
        }
    }
    let wrapped_view = wrapped_region(world_width_m, view);
    // Every receiver also reads its ground-sky bounce conditions there.
    cloud_weather_region.include(wrapped_view);
    let environment_spacing = |region: Region, texels: u32| {
        field_spacing_m(
            world_width_m,
            region,
            ENVIRONMENT_FIELD_SPACING_M,
            texels,
            texels * texels,
        )
    };
    FieldGrids {
        environment: field_grid_with_spacing(
            camera,
            [0.0; 2],
            wrapped_view,
            environment_spacing(wrapped_view, ENVIRONMENT_FIELD_TEXELS),
        ),
        cloud_weather: field_grid_with_spacing(
            camera,
            [0.0; 2],
            wrapped_region(world_width_m, cloud_weather_region.or_origin()),
            environment_spacing(
                wrapped_region(
                    world_width_m,
                    cloud_weather_bound.expanded(2.0 * cloud_texel_reach_m),
                ),
                CLOUD_WEATHER_FIELD_TEXELS,
            ),
        ),
        occluders,
        cloud_shadow_grids,
    }
}

fn cloud_shadow_layer_texels(footprint: Region, view: Region) -> u32 {
    let area = |region: Region| {
        (0..2)
            .map(|axis| (region.max[axis] - region.min[axis]).max(0.0))
            .product::<f64>()
    };
    let [full, fewest] = CLOUD_SHADOW_STRETCH_AREAS;
    let stretch =
        ((area(footprint) / area(view).max(1.0) - full) / (fewest - full)).clamp(0.0, 1.0);
    let texels = f64::from(OCCLUDER_LAYER_TEXELS);
    (texels - stretch * (texels - f64::from(CLOUD_SHADOW_LAYER_TEXELS))) as u32
}

/// The spacing at which `region` would just fit the budgets of
/// `field_doublings`, before rounding to a doubling.
fn fitting_spacing_m(region: Region, axis_texels: u32, layer_texels: u32) -> f64 {
    let size_m = [0, 1].map(|axis| (region.max[axis] - region.min[axis]).max(0.0));
    let axis_spacing_m = size_m.map(|size| size / (f64::from(axis_texels) - 5.0));
    // (w / s + 5)(h / s + 5) = texels, a quadratic in s.
    let texels = f64::from(layer_texels) - 25.0;
    let sum_m = size_m[0] + size_m[1];
    let layer_spacing_m = (5.0 * sum_m
        + (25.0 * sum_m * sum_m + 4.0 * texels * size_m[0] * size_m[1]).sqrt())
        / (2.0 * texels);
    axis_spacing_m[0]
        .max(axis_spacing_m[1])
        .max(layer_spacing_m)
}

/// An upper bound over the view on the share of a receiver's clear-sky light
/// that the Moon supplies, as `moon_clear_sky_share` in lighting.wgsl composes
/// it; keep the two synchronized. Each receiver takes the larger share at the
/// two ends of the regional turbidity range. Light colours keep their
/// luminance through the atmosphere, so sunlight has unit luminance and the
/// clear sky's light is at least as bright as the Moon's own colour.
pub fn moon_clear_sky_share(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    camera_m: [f64; 2],
    half_view_m: [f64; 2],
    highest_receiver_m: f64,
) -> f64 {
    // The regional aerosol optical depths of lighting.wgsl: dry air fully
    // washed out, then air at the 95% humidity where growth stops.
    let aerosol_optical_depths = [0.046 * 0.28, 0.046 / 0.05_f64.sqrt()];
    // Disc radii from scene-state.wgsl.
    const SUN_DISC_RADIUS_RAD: f64 = 0.00465;
    const MOON_DISC_RADIUS_RAD: f64 = 0.00452;
    let mut share: f64 = 0.0;
    for row in 0..=4 {
        for column in 0..=4 {
            let position_m = [
                camera_m[0] + half_view_m[0] * (f64::from(column) / 2.0 - 1.0),
                camera_m[1] + half_view_m[1] * (f64::from(row) / 2.0 - 1.0),
            ];
            let conditions = world_celestial_conditions(
                world_geometry,
                world_time_seconds,
                Vector::new(position_m[0] as f32, position_m[1] as f32),
            );
            for height_m in [0.0, highest_receiver_m] {
                let light =
                    celestial_lighting_at_height(world_geometry, conditions, height_m as f32);
                let [red, green, blue] = light.moon_color.map(f64::from);
                let eclipse = f64::from(light.solar_eclipse_fraction);
                let lunar_scale = f64::from(light.lunar_light_scale);
                let clear = f64::from(light.ambient_intensity);
                let moon_luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
                for aerosol_optical_depth in aerosol_optical_depths {
                    let sun_z = f64::from(light.sun_direction[2]);
                    let moon_z = f64::from(light.moon_direction[2]);
                    let sun_beam = (1.0 - eclipse)
                        * game::beam_normal_light(
                            aerosol_optical_depth,
                            height_m,
                            sun_z,
                            SUN_DISC_RADIUS_RAD,
                            [1.0; 3],
                        )
                        * sun_z.max(0.0);
                    let moon_beam = lunar_scale
                        * game::beam_normal_light(
                            aerosol_optical_depth,
                            height_m,
                            moon_z,
                            MOON_DISC_RADIUS_RAD,
                            [red, green, blue],
                        )
                        * moon_z.max(0.0);
                    let moon_direct = moon_beam * moon_luminance;
                    let diffuse = (clear - sun_beam - moon_beam).max(0.0) * moon_luminance.min(1.0);
                    let total = sun_beam + moon_direct + diffuse;
                    if total > 0.0 {
                        share = share.max(moon_direct / total);
                    }
                }
            }
        }
    }
    share
}

/// Share of a clear sky's light on level ground at sea level that arrives
/// diffuse, for air of the given 550 nm aerosol optical depth, as
/// `celestial_light_split` in lighting.wgsl leaves it when that air is the
/// reference air.
pub fn clear_sky_diffuse_fraction(
    sine_height: f64,
    aerosol_optical_depth: f64,
    air_column: f64,
) -> f64 {
    let diffuse = game::clear_sky_diffuse_light(sine_height, 2.35 + 7.79 * aerosol_optical_depth);
    let beam = game::clear_sky_beam_luminance(sine_height, aerosol_optical_depth, air_column);
    diffuse / (diffuse + sine_height.clamp(0.0, 1.0) * beam).max(1e-6)
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
    /// Sun, then Moon: the cloud shadow plane positions that taps sample.
    cloud_shadow: [Region; 2],
    /// Every landform sample position.
    landform: Region,
}

/// Bounds the cloud-layer positions that shadow taps sample. The view is cut
/// into cells; within a cell every receiver's light direction lies inside a
/// cone around the centre's, because moving across the surface rotates local
/// directions by at most the travelled angle of longitude and latitude. Each
/// cell then contributes itself plus every displacement such a cone can project,
/// including the landform march toward the light, as far as the cell's lowest
/// direction reaches. Landform marches start from receivers up to
/// `landform_receiver_margin_m` beyond the view.
fn shadow_footprints(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    camera_m: [f64; 2],
    half_view_m: [f64; 2],
    tuned_landform_reach_m: f64,
    highest_receiver_m: f64,
    landform_receiver_margin_m: f64,
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
    let shadow_lit_rad = horizon_light_rad(world_radius_m, highest_receiver_m);
    let receivers = Receivers {
        highest_m: highest_receiver_m,
    };
    let mut footprints = ShadowFootprints {
        cloud_shadow: [Region::EMPTY; 2],
        landform: Region {
            min: half_view_m.map(|half| -half),
            max: half_view_m,
        }
        .expanded(tuned_landform_reach_m + landform_receiver_margin_m),
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
            for (source, direction) in [surface.0, surface.1].into_iter().enumerate() {
                // Landform receivers lie below the deck, so its parallax bounds theirs.
                let landform_cone_rad = cell_rotation_rad
                    + landform_receiver_margin_m * (1.0 / world_radius_m + latitude_rad_per_m)
                    + LARGEST_DISC_ANGULAR_RADIUS_RAD
                    + DECK_HEIGHT_M / (LUNAR_DISTANCE_PLANET_RADII * world_radius_m)
                    + DIRECTION_TOLERANCE_RAD;
                let [east, north, up] = direction.map(f64::from);
                let elevation_rad = up.atan2(east.hypot(north));
                if elevation_rad + landform_cone_rad >= shadow_lit_rad {
                    footprints.landform.include(
                        cell.expanded(landform_receiver_margin_m)
                            .minkowski_sum(offsets_toward(
                                direction,
                                landform_cone_rad,
                                // Horizon texels march lights whose disc the
                                // highest receiver still partly sees.
                                landform_shadow_reach_m(
                                    world_radius_m,
                                    tuned_landform_reach_m,
                                    (elevation_rad - landform_cone_rad).max(
                                        shadow_lit_rad
                                            - LARGEST_DISC_ANGULAR_RADIUS_RAD
                                            - DIRECTION_TOLERANCE_RAD,
                                    ),
                                ),
                            )),
                    );
                }
                // Topocentric lunar parallax changes with receiver height up to the plane.
                let cone_rad = cell_rotation_rad
                    + LARGEST_DISC_ANGULAR_RADIUS_RAD
                    + CLOUD_SHADOW_PLANE_HEIGHT_M / (LUNAR_DISTANCE_PLANET_RADII * world_radius_m)
                    + DIRECTION_TOLERANCE_RAD;
                if let Some(offsets) = receivers.projected_offsets(
                    world_radius_m,
                    direction,
                    cone_rad,
                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                ) {
                    footprints.cloud_shadow[source].include(cell.minkowski_sum(offsets));
                }
            }
        }
    }
    footprints
}

/// Bounds the deck positions that cloud shadow texels in `plane_footprint`
/// read along their own light, which turns from the receiver's along the way.
/// The plane is cut into cells as `shadow_footprints` cuts the view, and a
/// texel is evaluated while any of its light's disc clears the plane's horizon.
fn deck_footprint(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    camera_m: [f64; 2],
    plane_footprint: Region,
    source: usize,
) -> Region {
    let world_radius_m = f64::from(world_geometry.width_m()) / TAU;
    let latitude_rad_per_m = PI / f64::from(world_geometry.height_m());
    let mut footprint = Region::EMPTY;
    if plane_footprint.min[0] > plane_footprint.max[0] {
        return footprint;
    }
    let cells = FOOTPRINT_CELLS / 2;
    let cell_half_m = [0, 1]
        .map(|axis| 0.5 * (plane_footprint.max[axis] - plane_footprint.min[axis]) / cells as f64);
    let cell_rotation_rad = cell_half_m[0] / world_radius_m + cell_half_m[1] * latitude_rad_per_m;
    let cone_rad = cell_rotation_rad
        + LARGEST_DISC_ANGULAR_RADIUS_RAD
        + DECK_HEIGHT_M / (LUNAR_DISTANCE_PLANET_RADII * world_radius_m)
        + DIRECTION_TOLERANCE_RAD;
    let radius_ratio = world_radius_m / (world_radius_m + CLOUD_SHADOW_PLANE_HEIGHT_M);
    let plane_horizon_rad = (-(1.0 - radius_ratio * radius_ratio).sqrt()).asin();
    let orbit = celestial_orbit(world_celestial_conditions(
        world_geometry,
        world_time_seconds,
        Vector::new(camera_m[0] as f32, camera_m[1] as f32),
    ));
    for row in 0..cells {
        for column in 0..cells {
            let center_m = [
                plane_footprint.min[0] + (2 * column + 1) as f64 * cell_half_m[0],
                plane_footprint.min[1] + (2 * row + 1) as f64 * cell_half_m[1],
            ];
            let conditions = world_celestial_conditions(
                world_geometry,
                world_time_seconds,
                Vector::new(
                    (camera_m[0] + center_m[0]) as f32,
                    (camera_m[1] + center_m[1]) as f32,
                ),
            );
            let lights = celestial_directions_at_height(
                world_geometry,
                orbit,
                conditions,
                CLOUD_SHADOW_PLANE_HEIGHT_M as f32,
            );
            let direction = [lights.0, lights.1][source];
            let [east, north, up] = direction.map(f64::from);
            let elevation_rad = up.atan2(east.hypot(north));
            if elevation_rad + cone_rad < plane_horizon_rad - LARGEST_DISC_ANGULAR_RADIUS_RAD {
                continue;
            }
            let distance_m = projection_distance_m(
                world_radius_m,
                (elevation_rad - cone_rad).max(plane_horizon_rad - LARGEST_DISC_ANGULAR_RADIUS_RAD),
                CLOUD_SHADOW_PLANE_HEIGHT_M,
                DECK_HEIGHT_M,
            );
            let cell = Region {
                min: [center_m[0] - cell_half_m[0], center_m[1] - cell_half_m[1]],
                max: [center_m[0] + cell_half_m[0], center_m[1] + cell_half_m[1]],
            };
            footprint.include(cell.minkowski_sum(offsets_toward(direction, cone_rad, distance_m)));
        }
    }
    footprint
}

/// Ground and water from the lowest shadow receiver up to `highest_m`.
#[derive(Clone, Copy)]
struct Receivers {
    highest_m: f64,
}

impl Receivers {
    /// Farthest surface distance from a receiver to the deck weather that the
    /// cloud shadow texel under its tap reads. The texel continues along its
    /// own light, which turns toward the zenith on the way there by at least
    /// nothing, so the deck lies at most as far beyond the plane as the
    /// receiver's tap direction reaches from the plane.
    fn farthest_cloud_weather_read_m(self, world_radius_m: f64) -> f64 {
        (0..=32)
            .map(|step| {
                let height_m = LOWEST_SHADOW_RECEIVER_M
                    + (self.highest_m - LOWEST_SHADOW_RECEIVER_M) * f64::from(step) / 32.0;
                let tap_rad = horizon_light_rad(world_radius_m, height_m)
                    - LARGEST_DISC_ANGULAR_RADIUS_RAD
                    - DIRECTION_TOLERANCE_RAD;
                projection_distance_m(
                    world_radius_m,
                    tap_rad,
                    height_m,
                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                ) + projection_distance_m(
                    world_radius_m,
                    tap_rad,
                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                    DECK_HEIGHT_M,
                )
            })
            .fold(0.0, f64::max)
    }

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
    /// farther. A receiver takes taps down to its own horizon less the disc
    /// radius, and raising it lowers that horizon; the farthest tap then comes
    /// from the lowest receiver or from the one whose horizon tap meets
    /// `lowest_rad`, because along horizon taps reach grows with receiver height
    /// above sea level while the receiver stays below half the layer height.
    fn farthest_projection_m(
        self,
        world_radius_m: f64,
        lowest_rad: f64,
        highest_rad: f64,
        layer_height_m: f64,
    ) -> Option<f64> {
        let highest_m = self.highest_m;
        if highest_rad < horizon_light_rad(world_radius_m, highest_m) {
            return None;
        }
        let tap_rad = |height_m: f64| {
            lowest_rad
                .max(horizon_light_rad(world_radius_m, height_m) - LARGEST_DISC_ANGULAR_RADIUS_RAD)
        };
        let reach_m = |height_m: f64| {
            projection_distance_m(world_radius_m, tap_rad(height_m), height_m, layer_height_m)
        };
        if highest_m > 0.5 * layer_height_m {
            return Some(projection_distance_m(
                world_radius_m,
                lowest_rad.max(
                    horizon_light_rad(world_radius_m, highest_m) - LARGEST_DISC_ANGULAR_RADIUS_RAD,
                ),
                LOWEST_SHADOW_RECEIVER_M,
                layer_height_m,
            ));
        }
        let meeting_m = receiver_height_with_horizon_m(
            world_radius_m,
            lowest_rad + LARGEST_DISC_ANGULAR_RADIUS_RAD,
        )
        .clamp(LOWEST_SHADOW_RECEIVER_M, highest_m);
        let lowest_lit = highest_rad >= horizon_light_rad(world_radius_m, LOWEST_SHADOW_RECEIVER_M);
        Some(if lowest_lit {
            reach_m(meeting_m).max(reach_m(LOWEST_SHADOW_RECEIVER_M))
        } else {
            reach_m(meeting_m)
        })
    }
}

/// The receiver height whose horizon is `elevation_rad`, the inverse of
/// `horizon_light_rad`: below sea level when every receiver's horizon is at
/// most that light, infinite when none reaches it.
fn receiver_height_with_horizon_m(world_radius_m: f64, elevation_rad: f64) -> f64 {
    let horizon_z = elevation_rad.sin();
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
    let climb_m = if elevation_rad < 0.0 {
        world_radius_m
            * ((world_radius_m / (world_radius_m + tuned_reach_m)).acos() - elevation_rad)
    } else {
        projection_distance_m(world_radius_m, elevation_rad, 0.0, tuned_reach_m)
    };
    tuned_reach_m.max(climb_m)
}

/// The highest surface a shadow can fall on: broad landform peaks at
/// 18 m × (1 − 0.30) × 20 per unit of elevation scale (`broad_landform_elevation_m`
/// in terrain.wgsl), and land detail adds less than 6 m.
fn highest_receiver_m() -> f64 {
    252.0 * TERRAIN_ELEVATION_SCALE + 6.0
}

/// Elevation of the horizon of a receiver at `height_m`, the lowest light whose
/// disc centre it sees; shadows reach down to it.
fn horizon_light_rad(world_radius_m: f64, height_m: f64) -> f64 {
    let radius_ratio = world_radius_m / (world_radius_m + height_m);
    (-(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt()).asin()
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
    layer_texels: u32,
) -> [[i32; 2]; 2] {
    [1.0, OCCLUDER_REFINE_SHARE].map(|share| {
        field_doublings(
            world_width_m,
            region,
            base_spacing_m,
            (share * f64::from(OCCLUDER_FIELD_TEXELS)) as u32,
            (share * f64::from(layer_texels)) as u32,
        )
    })
}

fn occluder_doublings(
    world_width_m: f64,
    region: Region,
    base_spacing_m: f64,
    layer_texels: u32,
    previous: Option<[i32; 2]>,
) -> [i32; 2] {
    let [finest, refine_limit] =
        occluder_doubling_band(world_width_m, region, base_spacing_m, layer_texels);
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
            let cloud_lattice = [
                f64::from(motion_m[0]) - camera[0],
                f64::from(motion_m[1]) - camera[1],
            ];
            assert_on_lattice(&grids.environment, world_lattice, "environment");
            assert_on_lattice(&grids.cloud_weather, world_lattice, "cloud weather");
            for layer in 0..2 {
                assert_on_lattice(&grids.occluders[layer], cloud_lattice, "cloud shadow");
            }
            assert_on_lattice(&grids.occluders[LANDFORM_LAYER], world_lattice, "landform");
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
            for grid in [&grids.environment, &grids.cloud_weather]
                .into_iter()
                .chain(&grids.occluders)
            {
                let columns = f64::from(world_width_m) / f64::from(grid.uniforms[2]);
                assert!(
                    (columns - columns.round()).abs() < 1.0e-3,
                    "{world_width_m}: {columns}"
                );
            }
        }
    }

    #[test]
    fn cloud_weather_grid_covers_the_view_at_every_zoom() {
        for view_height_m in [240.0, 1_500.0, 6_000.0, 16_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for step in 0..(28 * 24) {
                let world_time_seconds = f64::from(step) * 3_600.0;
                let grids = production_grids(
                    world_time_seconds,
                    [500.0, -14_500.0],
                    view_size_m,
                    NO_MOTION,
                );
                assert_covers(
                    &grids.cloud_weather,
                    view_region(view_size_m),
                    &format!("{view_size_m:?} at {world_time_seconds} s"),
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
                    for (grid, budget) in [
                        (&grids.environment, ENVIRONMENT_FIELD_TEXELS),
                        (&grids.cloud_weather, CLOUD_WEATHER_FIELD_TEXELS),
                    ] {
                        assert!(
                            grid.texels.iter().all(|&texels| texels <= budget),
                            "{label}",
                        );
                    }
                    for grid in &grids.occluders {
                        assert!(
                            grid.texels
                                .iter()
                                .all(|&texels| texels <= OCCLUDER_FIELD_TEXELS),
                            "{label}",
                        );
                    }
                    assert_covers(&grids.environment, view, &label);
                    assert_covers(
                        &grids.occluders[LANDFORM_LAYER],
                        view.expanded(reach_m),
                        &label,
                    );
                    assert_covers(&grids.occluders[LANDFORM_HORIZON_LAYER], view, &label);
                    assert_covers(&grids.occluders[LANDFORM_HORIZON_LAYER + 1], view, &label);
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
        let coarse = occluder_doublings(
            world_width_m,
            region(2_050.0),
            4.0,
            OCCLUDER_LAYER_TEXELS,
            None,
        );
        assert_eq!(coarse, [1, 0]);
        assert_eq!(
            occluder_doublings(
                world_width_m,
                region(1_900.0),
                4.0,
                OCCLUDER_LAYER_TEXELS,
                None
            ),
            [0, 0]
        );
        assert_eq!(
            occluder_doublings(
                world_width_m,
                region(1_900.0),
                4.0,
                OCCLUDER_LAYER_TEXELS,
                Some(coarse)
            ),
            coarse,
        );
        assert_eq!(
            occluder_doublings(
                world_width_m,
                region(1_500.0),
                4.0,
                OCCLUDER_LAYER_TEXELS,
                Some(coarse)
            ),
            [0, 0],
        );
        assert_eq!(
            occluder_doublings(
                world_width_m,
                region(2_050.0),
                4.0,
                OCCLUDER_LAYER_TEXELS,
                Some([0, 0])
            ),
            coarse,
        );
    }

    /// Every receiver that sees part of a light's disc, from low tide to the
    /// highest peak, takes its cloud-shadow taps inside the Sun and Moon
    /// windows, on worlds of different sizes.
    #[test]
    fn cloud_shadow_taps_of_every_receiver_height_land_inside_their_windows() {
        for world in [
            WorldGeometry::default(),
            WorldGeometry::new(128_000.0, 128_000.0).unwrap(),
        ] {
            cloud_shadow_taps_land_inside_their_windows(world);
        }
    }

    fn cloud_shadow_taps_land_inside_their_windows(world: WorldGeometry) {
        let world_radius_m = f64::from(world.width_m()) / TAU;
        let highest_m = highest_receiver_m();
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
                let grids = field_grids(
                    world,
                    world_time_seconds,
                    camera_m,
                    view_size_m,
                    NO_MOTION,
                    FieldGridTuning::PRODUCTION,
                    &mut FieldGridHistory::default(),
                );
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
                                let elevation_rad = up.atan2(east.hypot(north));
                                if elevation_rad + LARGEST_DISC_ANGULAR_RADIUS_RAD
                                    < horizon_z.asin()
                                    || (source == 1 && grids.cloud_shadow_grids[2] <= 0.0)
                                {
                                    continue;
                                }
                                let azimuth_rad = north.atan2(east);
                                let extent = grids.occluders[source].extent();
                                let distance_m = projection_distance_m(
                                    world_radius_m,
                                    elevation_rad,
                                    height_m,
                                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                                );
                                let tap_m = [
                                    receiver_m[0] + distance_m * azimuth_rad.cos(),
                                    receiver_m[1] + distance_m * azimuth_rad.sin(),
                                ];
                                assert!(
                                    (0..2).all(|axis| {
                                        (extent.min[axis]..=extent.max[axis]).contains(&tap_m[axis])
                                    }),
                                    "{world:?}, {view_height_m} m view at {world_time_seconds} s: \
                                     source {source} from {height_m} m at {receiver_m:?} taps at \
                                     {tap_m:?} outside {extent:?}",
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Every landform horizon texel, including those beyond the view, marches
    /// inside the landform window, bilinear neighbours included.
    #[test]
    fn landform_horizon_marches_land_inside_the_landform_window() {
        let world = WorldGeometry::default();
        let world_radius_m = f64::from(world.width_m()) / TAU;
        let tuned_reach_m = f64::from(FieldGridTuning::PRODUCTION.landform_shadow_reach_m);
        let highest_horizon_rad =
            -(world_radius_m / (world_radius_m + highest_receiver_m())).acos();
        let camera_m = [-23_500.0_f32, -16_500.0];
        for view_height_m in [15.0, 240.0, 1_500.0, 6_000.0, 16_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for step in 0..96 {
                let world_time_seconds = MIDNIGHT_SECONDS + f64::from(step) * 900.0;
                let grids = production_grids(world_time_seconds, camera_m, view_size_m, NO_MOTION);
                let landform = grids.occluders[LANDFORM_LAYER].extent();
                let spacing_m = [0, 1]
                    .map(|axis| f64::from(grids.occluders[LANDFORM_LAYER].uniforms[2 + axis]));
                let orbit = celestial_orbit(world_celestial_conditions(
                    world,
                    world_time_seconds,
                    Vector::ZERO,
                ));
                for (source, layer) in [LANDFORM_HORIZON_LAYER, LANDFORM_HORIZON_LAYER + 1]
                    .into_iter()
                    .enumerate()
                {
                    let horizons = grids.occluders[layer].extent();
                    for row in 0..=8 {
                        for column in 0..=8 {
                            let texel_m = [
                                horizons.min[0]
                                    + (horizons.max[0] - horizons.min[0]) * f64::from(column) / 8.0,
                                horizons.min[1]
                                    + (horizons.max[1] - horizons.min[1]) * f64::from(row) / 8.0,
                            ];
                            let conditions = world_celestial_conditions(
                                world,
                                world_time_seconds,
                                Vector::new(
                                    camera_m[0] + texel_m[0] as f32,
                                    camera_m[1] + texel_m[1] as f32,
                                ),
                            );
                            let lights =
                                celestial_directions_at_height(world, orbit, conditions, 0.0);
                            let [east, north, up] = [lights.0, lights.1][source].map(f64::from);
                            let elevation_rad = up.atan2(east.hypot(north));
                            if elevation_rad + LARGEST_DISC_ANGULAR_RADIUS_RAD < highest_horizon_rad
                            {
                                continue;
                            }
                            let reach_m = landform_shadow_reach_m(
                                world_radius_m,
                                tuned_reach_m,
                                elevation_rad,
                            );
                            let azimuth_rad = north.atan2(east);
                            let end_m = [
                                texel_m[0] + reach_m * azimuth_rad.cos(),
                                texel_m[1] + reach_m * azimuth_rad.sin(),
                            ];
                            for point_m in [texel_m, end_m] {
                                assert!(
                                    (0..2).all(|axis| {
                                        (landform.min[axis] + spacing_m[axis]
                                            ..=landform.max[axis] - spacing_m[axis])
                                            .contains(&point_m[axis])
                                    }),
                                    "{view_height_m} m view at {world_time_seconds} s: source \
                                     {source} texel {texel_m:?} marches to {point_m:?} outside \
                                     {landform:?}",
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// The plane texels under every receiver's cloud shadow tap read their
    /// cumulus and deck weather inside the cloud weather grid, and so do
    /// their bilinear neighbours, which lie within a texel of the tap.
    #[test]
    fn cloud_weather_grid_covers_every_cloud_weather_read() {
        let world = WorldGeometry::default();
        let world_radius_m = f64::from(world.width_m()) / TAU;
        let highest_m = highest_receiver_m();
        let camera_m = [-23_500.0_f32, -16_500.0];
        let horizon_z = |height_m: f64| {
            let radius_ratio = world_radius_m / (world_radius_m + height_m.max(0.0));
            -(1.0 - radius_ratio * radius_ratio).sqrt()
        };
        let toward = |point_m: [f64; 2], light: [f32; 3], from_m: f64, to_m: f64| {
            let [east, north, up] = light.map(f64::from);
            let azimuth_rad = north.atan2(east);
            let distance_m =
                projection_distance_m(world_radius_m, up.atan2(east.hypot(north)), from_m, to_m);
            [
                point_m[0] + distance_m * azimuth_rad.cos(),
                point_m[1] + distance_m * azimuth_rad.sin(),
            ]
        };
        let lights_at = |time_seconds: f64, point_m: [f64; 2], height_m: f64| {
            let orbit = celestial_orbit(world_celestial_conditions(
                world,
                time_seconds,
                Vector::ZERO,
            ));
            let conditions = world_celestial_conditions(
                world,
                time_seconds,
                Vector::new(
                    camera_m[0] + point_m[0] as f32,
                    camera_m[1] + point_m[1] as f32,
                ),
            );
            let lights = celestial_directions_at_height(world, orbit, conditions, height_m as f32);
            [lights.0, lights.1]
        };
        for view_height_m in [240.0, 1_500.0, 6_000.0, 16_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            for step in 0..96 {
                let world_time_seconds = MIDNIGHT_SECONDS + f64::from(step) * 900.0;
                let grids = production_grids(world_time_seconds, camera_m, view_size_m, NO_MOTION);
                let environment = grids.cloud_weather.extent();
                for row in 0..=8 {
                    for column in 0..=8 {
                        let receiver_m = [
                            f64::from(view_size_m[0]) * (f64::from(column) / 8.0 - 0.5),
                            f64::from(view_size_m[1]) * (f64::from(row) / 8.0 - 0.5),
                        ];
                        for height_m in [LOWEST_SHADOW_RECEIVER_M, 0.0, 150.0, 500.0, highest_m] {
                            let lights = lights_at(world_time_seconds, receiver_m, height_m);
                            for (source, light) in lights.into_iter().enumerate() {
                                let [east, north, up] = light.map(f64::from);
                                if up.atan2(east.hypot(north)) + LARGEST_DISC_ANGULAR_RADIUS_RAD
                                    < horizon_z(height_m).asin()
                                    || (source == 1 && grids.cloud_shadow_grids[2] <= 0.0)
                                {
                                    continue;
                                }
                                let spacing_m = f64::from(
                                    grids.occluders[source].uniforms[2]
                                        .max(grids.occluders[source].uniforms[3]),
                                );
                                let tap_m = toward(
                                    receiver_m,
                                    light,
                                    height_m,
                                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                                );
                                let plane_light = lights_at(
                                    world_time_seconds,
                                    tap_m,
                                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                                )[source];
                                for cloud_height_m in [1_000.0, 2_000.0, 3_000.0, DECK_HEIGHT_M] {
                                    let read_m = toward(
                                        tap_m,
                                        plane_light,
                                        CLOUD_SHADOW_PLANE_HEIGHT_M,
                                        cloud_height_m,
                                    );
                                    assert!(
                                        (0..2).all(|axis| {
                                            (environment.min[axis] + spacing_m
                                                ..=environment.max[axis] - spacing_m)
                                                .contains(&read_m[axis])
                                        }),
                                        "{view_height_m} m view at {world_time_seconds} s: \
                                         source {source} from {height_m} m at {receiver_m:?} \
                                         reads {cloud_height_m} m cloud weather at {read_m:?} \
                                         outside {environment:?}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Each cloud shadow grid resolves wavelengths of at least two texels, and
    /// that shortest wavelength follows a slow zoom from boat zoom to 16 km
    /// views without jumps.
    #[test]
    fn cloud_shadow_wavelengths_stay_resolved_and_follow_zoom_smoothly() {
        for hours in [12.0, 17.0] {
            let world_time_seconds = MIDNIGHT_SECONDS + hours * 3_600.0;
            let mut history = FieldGridHistory::default();
            let mut previous: [Option<f32>; 2] = [None; 2];
            for step in 0..440 {
                let view_height_m = 200.0 * 1.01_f32.powi(step);
                let grids = field_grids(
                    WorldGeometry::default(),
                    world_time_seconds,
                    [2_400.0, 2_000.0],
                    [view_width_m(16.0 / 9.0, view_height_m), view_height_m],
                    NO_MOTION,
                    FieldGridTuning::PRODUCTION,
                    &mut history,
                );
                let wavelengths_m = [grids.cloud_shadow_grids[0], grids.cloud_shadow_grids[1]];
                for source in 0..2 {
                    let grid = &grids.occluders[source];
                    if grid.texels.iter().all(|&texels| texels <= 4) {
                        previous[source] = None;
                        continue;
                    }
                    let spacing_m = grid.uniforms[2].max(grid.uniforms[3]);
                    assert!(
                        wavelengths_m[source] >= 2.0 * spacing_m * 0.999,
                        "{view_height_m} m view at {hours} h, source {source}: \
                         {} m wavelengths on {spacing_m} m texels",
                        wavelengths_m[source],
                    );
                    if let Some(previous) = previous[source] {
                        // Footprint cells that cross a light's horizon move the
                        // window by a few percent; a doubling would move it by half.
                        let ratio = wavelengths_m[source] / previous;
                        assert!(
                            (1.0 / 1.1..=1.1).contains(&ratio),
                            "{view_height_m} m view at {hours} h, source {source}: \
                             the shortest wavelength jumped from {previous} m to {} m",
                            wavelengths_m[source],
                        );
                    }
                    previous[source] = Some(wavelengths_m[source]);
                }
            }
        }
    }

    /// Under a high Sun a thin Moon's cloud shadows are negligible, so its grid
    /// shrinks to the camera; at night it covers the view.
    #[test]
    fn moon_cloud_shadows_fade_out_while_the_sun_outshines_the_moon() {
        let world = WorldGeometry::default();
        let view_size_m = [427.0, 240.0];
        let lighting = |time_seconds: f64| {
            celestial_lighting_at_height(
                world,
                world_celestial_conditions(world, time_seconds, Vector::ZERO),
                0.0,
            )
        };
        let times = (0..28 * 24 * 4).map(|quarter| MIDNIGHT_SECONDS + f64::from(quarter) * 900.0);
        let thin_moon_by_day = times
            .clone()
            .find(|&time_seconds| {
                let light = lighting(time_seconds);
                light.sun_intensity > 0.9
                    && light.moon_intensity > 0.0
                    && light.moon_intensity < 0.02 * game::FULL_MOON_SUNLIGHT_RATIO
            })
            .expect("a thin Moon should rise by day within a lunar month");
        let day = production_grids(thin_moon_by_day, [0.0, 0.0], view_size_m, NO_MOTION);
        assert_eq!(day.cloud_shadow_grids[2], 0.0, "at {thin_moon_by_day} s");
        assert!(day.occluders[1].texels.iter().all(|&texels| texels <= 4));
        let moonlit_night = times
            .clone()
            .find(|&time_seconds| {
                let light = lighting(time_seconds);
                light.sun_direction[2] < -0.3
                    && light.moon_intensity > 0.2 * game::FULL_MOON_SUNLIGHT_RATIO
            })
            .expect("a bright Moon should shine at night within a lunar month");
        let night = production_grids(moonlit_night, [0.0, 0.0], view_size_m, NO_MOTION);
        assert_eq!(night.cloud_shadow_grids[2], 1.0, "at {moonlit_night} s");
        assert_covers(
            &night.occluders[1],
            view_region(view_size_m),
            "moonlit night",
        );
    }

    #[test]
    fn environment_spacing_follows_zoom_alone() {
        for view_height_m in [240.0, 1_500.0, 12_400.0, 20_000.0] {
            let view_size_m = [view_width_m(16.0 / 9.0, view_height_m), view_height_m];
            let spacings: Vec<_> = (0..24)
                .map(|hour| {
                    let grids = production_grids(
                        MIDNIGHT_SECONDS + f64::from(hour) * 3_600.0,
                        [2_400.0, 2_000.0],
                        view_size_m,
                        NO_MOTION,
                    );
                    [
                        grids.environment.uniforms[3],
                        grids.cloud_weather.uniforms[3],
                    ]
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
        let sun = grids.occluders[0].extent();
        let spacing_m = f64::from(grids.occluders[0].uniforms[3]);
        assert!(
            sun.max[1] > 0.5 * f64::from(view_height_m) + 2.0 * spacing_m + 1_000.0,
            "Sun footprint stops at the view: {sun:?} with {spacing_m} m texels",
        );
    }

    #[test]
    fn shadow_grids_follow_the_light() {
        let view_size_m = [427.0, 240.0];
        let night = production_grids(MIDNIGHT_SECONDS, [0.0, 0.0], view_size_m, NO_MOTION);
        assert!(night.occluders[0].texels.iter().all(|&texels| texels <= 4));
        let noon = production_grids(
            MIDNIGHT_SECONDS + 43_200.0,
            [0.0, 0.0],
            view_size_m,
            NO_MOTION,
        );
        assert_covers(&noon.occluders[0], view_region(view_size_m), "noon");
        let afternoon = production_grids(
            MIDNIGHT_SECONDS + 64_000.0,
            [0.0, 0.0],
            view_size_m,
            NO_MOTION,
        );
        let afternoon_extent = afternoon.occluders[0].extent();
        let noon_extent = noon.occluders[0].extent();
        assert!(
            afternoon_extent.max[0] - afternoon_extent.min[0]
                > 2.0 * (noon_extent.max[0] - noon_extent.min[0]),
            "a low western Sun stretches the footprint: {afternoon_extent:?} vs {noon_extent:?}",
        );
        let afternoon_landform = afternoon.occluders[LANDFORM_LAYER].extent();
        let noon_landform = noon.occluders[LANDFORM_LAYER].extent();
        assert!(
            afternoon_landform.min[0] < noon_landform.min[0] - 1_000.0,
            "a low western Sun lengthens the landform march: \
             {afternoon_landform:?} vs {noon_landform:?}",
        );
    }

    /// Below 45°, down to the horizon of the highest receiver, the march ends
    /// where the ray toward the light from the lowest receiver that sees it
    /// has climbed the base reach above sea level, and at most a few percent
    /// more, on worlds of different sizes.
    #[test]
    fn landform_shadow_reach_follows_the_light_height_down_to_the_horizon() {
        let base_m = 320.0;
        for world_side_m in [32_000.0_f64, 64_000.0, 128_000.0] {
            let world_radius_m = world_side_m / TAU;
            let reach_m =
                |elevation_rad: f64| landform_shadow_reach_m(world_radius_m, base_m, elevation_rad);
            for elevation_deg in [45.0_f64, 60.0, 90.0] {
                assert_eq!(
                    reach_m(elevation_deg.to_radians()),
                    base_m,
                    "{world_side_m} m world, {elevation_deg}°"
                );
            }
            let horizon_rad = -(world_radius_m / (world_radius_m + highest_receiver_m())).acos();
            for step in 0..=32 {
                let elevation_rad =
                    horizon_rad + (40.0_f64.to_radians() - horizon_rad) * f64::from(step) / 32.0;
                // The lowest receiver that sees a light below the sea-level
                // horizon sees it on its own horizon, so its ray grazes sea level.
                let closest_m = world_radius_m * elevation_rad.max(0.0).cos();
                let angle_rad = reach_m(elevation_rad) / world_radius_m;
                let ray_height_m = closest_m / (angle_rad + elevation_rad).cos() - world_radius_m;
                assert!(
                    (base_m - 0.5..1.05 * base_m).contains(&ray_height_m),
                    "{world_side_m} m world, {}°: the ray is {ray_height_m} m up at the reach",
                    elevation_rad.to_degrees(),
                );
            }
        }
    }
}
