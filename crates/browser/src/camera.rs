use game::{BROAD_LANDFORM_HIGHEST_M, broad_landform_elevation_m};
use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::WorldGeometry;

// The terrain depth's near plane lies at least this share of the camera's
// height below it, where f32 depths still resolve 1 − depth to a relative 1e-4;
// a camera at or below the highest landform clips the terrain above the plane.
const LEAST_TERRAIN_DEPTH_NEAR_SCALE: f32 = 1e-3;
// Over the default world's tall island, archipelago and rugged island in 300 m
// to 20 km views, a march with 64 m steps lands within a 1080p pixel of a 1 m
// march on a 96 × 54 grid of view points, and 128 m steps miss some crests;
// this step keeps a 2× margin.
const TERRAIN_MARCH_STEP_M: f32 = 32.0;
const TERRAIN_ELEVATION_TOLERANCE_M: f32 = 0.001;

/// A camera looking straight down at `center_m` from `height_m` above mean
/// sea level through a rectilinear lens. Its image is `view_height_m` of sea
/// level tall, so zoom at a fixed height changes only the focal length.
///
/// View points are offsets from the image centre in image heights, x and y
/// along world x and y. The eye ray through view point u sees sea level at
/// `center_m + u * view_height_m` and passes height z at that offset scaled
/// by 1 − z / `height_m`.
#[derive(Clone, Copy)]
pub struct DroneCamera {
    pub world_geometry: WorldGeometry,
    pub center_m: Vector,
    pub height_m: f32,
    pub view_height_m: f32,
}

impl DroneCamera {
    /// The upper cloud field's lowest height, the highest that keeps every
    /// upper cloud above the camera. Keep synchronized with
    /// `CUMULUS_LOWEST_HEIGHT_M` in clouds.wgsl.
    pub const PRODUCTION_HEIGHT_M: f32 = 1000.0;

    pub fn production(world_geometry: WorldGeometry, center_m: Vector, view_height_m: f32) -> Self {
        Self {
            world_geometry,
            center_m,
            height_m: Self::PRODUCTION_HEIGHT_M,
            view_height_m,
        }
    }

    /// Where the eye ray through `view_point` passes `elevation_m`, wrapped to
    /// the world's canonical longitude.
    pub fn world_point_m(&self, view_point: Vector, elevation_m: f32) -> Vector {
        let offset_m = view_point * (self.view_height_m * (1.0 - elevation_m / self.height_m));
        Vector::from(
            self.world_geometry
                .wrap_position_m((self.center_m + offset_m).as_dvec2().into())
                .map(|v| v as f32),
        )
    }

    /// The scale n that the eye ray's offset takes at the near plane of the
    /// terrain depth, the highest broad landform for a camera above it. A
    /// surface at elevation z lies at depth 1 − n / (1 − z / H), and every
    /// depth from sea level's, 1 − n, up to the cleared 1 decodes to sea
    /// level. Keep synchronized with `vertex_terrain_depth` in
    /// terrain-depth-entry.wgsl and `terrain_hit_scale` in terrain-depth.wgsl.
    pub fn terrain_depth_near_scale(&self) -> f32 {
        (1.0 - BROAD_LANDFORM_HIGHEST_M as f32 / self.height_m).max(LEAST_TERRAIN_DEPTH_NEAR_SCALE)
    }

    /// The scene uniforms' `drone_camera` row, laid out in scene-state.wgsl.
    pub fn scene_uniform_row(&self) -> [f32; 4] {
        [self.height_m, self.terrain_depth_near_scale(), 0.0, 0.0]
    }

    pub fn elevation_at_terrain_depth_m(&self, depth: f32) -> f32 {
        let near_scale = self.terrain_depth_near_scale();
        self.height_m * (1.0 - near_scale / (1.0 - depth).max(near_scale))
    }

    /// The first broad landform or sea surface point on the eye ray through
    /// `view_point`, with its elevation. The sea surface lies at mean sea
    /// level. A camera inside terrain sees its own position.
    ///
    /// Where a pixel of an image `image_height_px` tall is wider at sea level
    /// than the march step, the march steps a pixel at a time instead, so its
    /// cost stops growing with the view. A crest that slips between those
    /// steps is narrower than a pixel at sea level.
    pub fn terrain_point(
        &self,
        match_seed: u64,
        view_point: Vector,
        image_height_px: u32,
    ) -> (Vector, f32) {
        let clearance_m = |elevation_m: f32| {
            let position_m = self.world_point_m(view_point, elevation_m);
            elevation_m
                - broad_landform_elevation_m(self.world_geometry, match_seed, position_m).max(0.0)
        };
        let top_m = self.height_m.min(BROAD_LANDFORM_HIGHEST_M as f32);
        let mut above_m = top_m;
        let mut below_m = top_m;
        if clearance_m(top_m) > 0.0 {
            // Sea level never lies above the surface, so the last step ends
            // the march.
            let travel_m = (view_point * self.view_height_m).length() * top_m / self.height_m;
            let step_m = TERRAIN_MARCH_STEP_M.max(self.view_height_m / image_height_px as f32);
            let steps = (travel_m / step_m).ceil().max(1.0) as u32;
            for step in 1..=steps {
                below_m = top_m * (1.0 - step as f32 / steps as f32);
                if clearance_m(below_m) <= 0.0 {
                    break;
                }
                above_m = below_m;
            }
            while above_m - below_m > TERRAIN_ELEVATION_TOLERANCE_M {
                let middle_m = 0.5 * (above_m + below_m);
                if clearance_m(middle_m) > 0.0 {
                    above_m = middle_m;
                } else {
                    below_m = middle_m;
                }
            }
        }
        (self.world_point_m(view_point, below_m), below_m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x5eed;
    const IMAGE_HEIGHT_PX: u32 = 1080;
    // The default world's highest broad landform, 459 m.
    const TALL_ISLAND_PEAK_M: Vector = Vector::new(-30600.0, 10600.0);

    fn view_points() -> impl Iterator<Item = Vector> {
        (0..=18).flat_map(|column| {
            (0..=10).map(move |row| {
                Vector::new(
                    -0.89 + 0.89 * column as f32 / 9.0,
                    -0.5 + 0.5 * row as f32 / 5.0,
                )
            })
        })
    }

    /// The view point that sees `world_point_m` at `elevation_m`, or `None` at
    /// or above the camera. In a view wider than the world, it sees the copy
    /// of the point nearest the image centre.
    fn view_point_seeing(
        camera: DroneCamera,
        world_point_m: Vector,
        elevation_m: f32,
    ) -> Option<Vector> {
        let scale_m = camera.view_height_m * (1.0 - elevation_m / camera.height_m);
        (scale_m > 0.0).then(|| {
            Vector::from(
                camera
                    .world_geometry
                    .minimum_image_displacement_m(
                        (world_point_m - camera.center_m).as_dvec2().into(),
                    )
                    .map(|v| v as f32),
            ) / scale_m
        })
    }

    fn landform_m(position_m: Vector) -> f32 {
        broad_landform_elevation_m(WorldGeometry::default(), SEED, position_m).max(0.0)
    }

    fn assert_near(actual: Vector, expected: Vector, tolerance: f32) {
        assert!(
            (actual - expected).length() <= tolerance,
            "{actual:?} is not within {tolerance} of {expected:?}",
        );
    }

    fn assert_near_across_seam(actual: Vector, expected: Vector, tolerance_m: f32) {
        let displacement_m = Vector::from(
            WorldGeometry::default()
                .minimum_image_displacement_m((actual - expected).as_dvec2().into())
                .map(|v| v as f32),
        );
        assert!(
            displacement_m.length() <= tolerance_m,
            "{actual:?} is not within {tolerance_m} m of {expected:?}",
        );
    }

    #[test]
    fn sea_level_is_a_uniformly_scaled_chart_that_round_trips() {
        let world = WorldGeometry::default();
        for center_m in [
            Vector::new(-8420.0, 60.0),
            Vector::new(world.width_m() as f32 / 2.0, 0.0),
        ] {
            let camera = DroneCamera::production(world, center_m, 2000.0);
            for view_point in view_points() {
                let world_point_m = camera.world_point_m(view_point, 0.0);
                assert!(world_point_m.x.abs() <= world.width_m() as f32 / 2.0);
                assert_near_across_seam(world_point_m, center_m + view_point * 2000.0, 0.01);
                assert_near(
                    view_point_seeing(camera, world_point_m, 0.0).unwrap(),
                    view_point,
                    1e-5,
                );
            }
        }
    }

    #[test]
    fn terrain_hits_are_the_first_landform_on_the_ray_and_reproject_onto_their_view_points() {
        let mut raised_hits = 0;
        // The tall island, then a 20 km archipelago view whose rays cross
        // several islands.
        for (center_m, view_height_m) in [
            (TALL_ISLAND_PEAK_M, 2000.0),
            (Vector::new(-4000.0, 0.0), 20000.0),
        ] {
            let camera = DroneCamera::production(WorldGeometry::default(), center_m, view_height_m);
            for view_point in view_points() {
                let (position_m, elevation_m) =
                    camera.terrain_point(SEED, view_point, IMAGE_HEIGHT_PX);
                assert!((elevation_m - landform_m(position_m)).abs() < 0.05);
                assert_near(
                    view_point_seeing(camera, position_m, elevation_m).unwrap(),
                    view_point,
                    1e-5,
                );
                for sample in 1..=400 {
                    let sample_elevation_m = elevation_m
                        + (BROAD_LANDFORM_HIGHEST_M as f32 - elevation_m) * sample as f32 / 400.0;
                    let sample_m = camera.world_point_m(view_point, sample_elevation_m);
                    assert!(
                        landform_m(sample_m) < sample_elevation_m,
                        "{view_point:?} passes through terrain at {sample_m:?} above its hit",
                    );
                }
                if elevation_m > 100.0 {
                    raised_hits += 1;
                }
            }
        }
        assert!(raised_hits > 5, "{raised_hits} hits on raised land");
    }

    #[test]
    fn terrain_hits_are_continuous_across_the_seam() {
        let world = WorldGeometry::default();
        // Seed 4 puts land about 260 m high across the seam there.
        let seed = 4;
        let center_m = Vector::new(world.width_m() as f32 / 2.0, 6750.0);
        let camera = DroneCamera::production(world, center_m, 3000.0);
        let canonical = DroneCamera {
            center_m: Vector::from(
                world
                    .wrap_position_m(center_m.as_dvec2().into())
                    .map(|v| v as f32),
            ),
            ..camera
        };
        let terrain_m = |position_m| broad_landform_elevation_m(world, seed, position_m);
        assert!(terrain_m(center_m) > 200.0);
        for y in [-0.2, 0.0, 0.2] {
            // Steps of 0.3 m at sea level across the seam at the image centre.
            let hits: Vec<_> = (-100..=100)
                .map(|step| {
                    camera.terrain_point(seed, Vector::new(1e-4 * step as f32, y), IMAGE_HEIGHT_PX)
                })
                .collect();
            assert!(hits[0].0.x > 0.0 && hits[200].0.x < 0.0);
            for pair in hits.windows(2) {
                assert_near_across_seam(pair[0].0, pair[1].0, 1.0);
                assert!((pair[0].1 - pair[1].1).abs() < 1.0, "{pair:?}");
            }
            for view_point in [Vector::new(-0.9, y), Vector::new(0.9, y)] {
                let (position_m, elevation_m) =
                    camera.terrain_point(seed, view_point, IMAGE_HEIGHT_PX);
                let (canonical_position_m, canonical_elevation_m) =
                    canonical.terrain_point(seed, view_point, IMAGE_HEIGHT_PX);
                assert_near(position_m, canonical_position_m, 0.01);
                assert!((elevation_m - canonical_elevation_m).abs() < 0.01);
            }
        }
    }

    #[test]
    fn halving_the_view_shows_the_central_half() {
        let world = WorldGeometry::default();
        let wide = DroneCamera::production(world, TALL_ISLAND_PEAK_M, 4000.0);
        let narrow = DroneCamera::production(world, TALL_ISLAND_PEAK_M, 2000.0);
        for view_point in view_points() {
            let (wide_position_m, wide_elevation_m) =
                wide.terrain_point(SEED, view_point * 0.5, IMAGE_HEIGHT_PX);
            let (position_m, elevation_m) = narrow.terrain_point(SEED, view_point, IMAGE_HEIGHT_PX);
            assert_near(position_m, wide_position_m, 0.01);
            assert!((elevation_m - wide_elevation_m).abs() < 0.01);
        }
    }

    #[test]
    fn picks_in_a_view_far_wider_than_the_world_stay_on_the_landform() {
        let widest =
            DroneCamera::production(WorldGeometry::default(), Vector::new(-4000.0, 0.0), 1e7);
        for view_point in view_points() {
            let (position_m, elevation_m) = widest.terrain_point(SEED, view_point, IMAGE_HEIGHT_PX);
            // f32 rounds positions millions of metres out to half a metre,
            // which moves steep land by metres.
            assert!((elevation_m - landform_m(position_m)).abs() < 10.0);
        }
    }

    #[test]
    fn a_camera_just_above_the_peak_sees_finite_terrain_below_it() {
        let camera = DroneCamera {
            height_m: 470.0,
            ..DroneCamera::production(WorldGeometry::default(), TALL_ISLAND_PEAK_M, 200.0)
        };
        for view_point in view_points() {
            let (position_m, elevation_m) = camera.terrain_point(SEED, view_point, IMAGE_HEIGHT_PX);
            assert!(position_m.is_finite() && (0.0..470.0).contains(&elevation_m));
            assert!((elevation_m - landform_m(position_m)).abs() < 0.05);
            // f32 positions resolve about a 1080p pixel this close to the camera.
            assert_near(
                view_point_seeing(camera, position_m, elevation_m).unwrap(),
                view_point,
                1e-3,
            );
        }
    }

    #[test]
    fn a_camera_inside_terrain_sees_its_own_position() {
        let camera = DroneCamera {
            height_m: 400.0,
            ..DroneCamera::production(WorldGeometry::default(), TALL_ISLAND_PEAK_M, 200.0)
        };
        for view_point in view_points() {
            assert_eq!(
                camera.terrain_point(SEED, view_point, IMAGE_HEIGHT_PX),
                (TALL_ISLAND_PEAK_M, 400.0)
            );
        }
        assert_eq!(view_point_seeing(camera, TALL_ISLAND_PEAK_M, 400.0), None);
        assert_eq!(view_point_seeing(camera, TALL_ISLAND_PEAK_M, 450.0), None);
    }

    #[test]
    fn a_camera_at_or_below_the_peak_decodes_every_terrain_depth_below_itself() {
        for height_m in [60.0, 300.0, BROAD_LANDFORM_HIGHEST_M as f32] {
            let camera = DroneCamera {
                height_m,
                ..DroneCamera::production(WorldGeometry::default(), TALL_ISLAND_PEAK_M, 200.0)
            };
            assert!(camera.terrain_depth_near_scale() > 0.0);
            assert_eq!(camera.elevation_at_terrain_depth_m(1.0), 0.0);
            for step in 0..=100 {
                let elevation_m = camera.elevation_at_terrain_depth_m(step as f32 / 100.0);
                assert!(
                    (0.0..height_m).contains(&elevation_m),
                    "{height_m} m camera: {elevation_m} m",
                );
            }
        }
    }
}
