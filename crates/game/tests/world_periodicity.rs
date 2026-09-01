use game::{
    AtmospherePrimary, PeriodicNoiseBasis, ProceduralWeather, periodic_noise_octave_diagnostic,
    procedural_atmosphere_primary, procedural_match_environment,
    procedural_solid_surface_elevation_m, procedural_terrain_elevation_m,
    procedural_tidal_conditions, procedural_unsheltered_wind_velocity_m_s,
    procedural_water_depth_m, procedural_water_height_m, procedural_weather,
};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed_cafe;
const TICK: u64 = 98_765;

fn geometry() -> WorldGeometry {
    WorldGeometry::new(80_000.0, 40_000.0).unwrap()
}

#[test]
fn octave_basis_is_fixed_across_supported_world_rectangles() {
    for (wavelength_m, expected) in [
        (8192.0, PeriodicNoiseBasis::LongitudeCircleSimplex4d),
        (
            256.0,
            PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d,
        ),
    ] {
        for world in [
            WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
            geometry(),
            WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
        ] {
            assert_eq!(
                periodic_noise_octave_diagnostic(world, expected, wavelength_m).basis,
                expected,
            );
        }
    }
}

#[test]
fn diagnostics_report_requested_and_effective_wavelengths() {
    for world in [
        WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
        geometry(),
        WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
    ] {
        let circle = periodic_noise_octave_diagnostic(
            world,
            PeriodicNoiseBasis::LongitudeCircleSimplex4d,
            8192.0,
        );
        assert_eq!(circle.effective_wavelength_x_m, 8192.0);
        assert_eq!(circle.effective_wavelength_y_m, 8192.0);
        assert_eq!(circle.relative_error_x, 0.0);
        assert_eq!(circle.relative_error_y, 0.0);

        let periodic = periodic_noise_octave_diagnostic(
            world,
            PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d,
            256.0,
        );
        assert!(periodic.effective_wavelength_x_m.is_finite());
        assert!(periodic.effective_wavelength_y_m.is_finite());
    }
}

#[track_caller]
fn assert_close(left: f32, right: f32, tolerance: f32) {
    assert!((left - right).abs() <= tolerance, "{left} != {right}");
}

#[track_caller]
fn assert_vector_close(left: Vector, right: Vector, tolerance: f32) {
    assert_close(left.x, right.x, tolerance);
    assert_close(left.y, right.y, tolerance);
}

fn assert_primary_close(left: AtmospherePrimary, right: AtmospherePrimary) {
    assert_close(left.pressure_hpa, right.pressure_hpa, 1.0e-5);
    assert_vector_close(
        left.pressure_gradient_hpa_per_m,
        right.pressure_gradient_hpa_per_m,
        1.0e-6,
    );
    assert_vector_close(
        left.synoptic_pressure_gradient_hpa_per_m,
        right.synoptic_pressure_gradient_hpa_per_m,
        1.0e-6,
    );
    assert_close(
        left.surface_air_temperature_anomaly_c,
        right.surface_air_temperature_anomaly_c,
        1.0e-5,
    );
    assert_close(
        left.air_temperature_500_hpa_c,
        right.air_temperature_500_hpa_c,
        1.0e-5,
    );
    assert_vector_close(
        left.local_temperature_gradient_c_per_m,
        right.local_temperature_gradient_c_per_m,
        1.0e-6,
    );
    assert_close(left.water_vapor_g_kg, right.water_vapor_g_kg, 1.0e-5);
}

fn assert_primary_seam_close(left: AtmospherePrimary, right: AtmospherePrimary) {
    assert_close(left.pressure_hpa, right.pressure_hpa, 0.002);
    assert_vector_close(
        left.pressure_gradient_hpa_per_m,
        right.pressure_gradient_hpa_per_m,
        5.0e-5,
    );
    assert_vector_close(
        left.synoptic_pressure_gradient_hpa_per_m,
        right.synoptic_pressure_gradient_hpa_per_m,
        5.0e-5,
    );
    assert_close(
        left.surface_air_temperature_anomaly_c,
        right.surface_air_temperature_anomaly_c,
        0.01,
    );
    assert_close(
        left.air_temperature_500_hpa_c,
        right.air_temperature_500_hpa_c,
        0.01,
    );
    assert_vector_close(
        left.local_temperature_gradient_c_per_m,
        right.local_temperature_gradient_c_per_m,
        2.0e-5,
    );
    assert_close(left.water_vapor_g_kg, right.water_vapor_g_kg, 0.01);
}

fn assert_weather_seam_close(left: ProceduralWeather, right: ProceduralWeather) {
    assert_primary_seam_close(left.primary, right.primary);
    assert_close(
        left.surface_air_temperature_c,
        right.surface_air_temperature_c,
        0.02,
    );
    assert_vector_close(left.wind_velocity_m_s, right.wind_velocity_m_s, 0.25);
    assert_vector_close(
        left.upper_wind_velocity_m_s,
        right.upper_wind_velocity_m_s,
        0.25,
    );
    assert_vector_close(left.wind_shear_m_s, right.wind_shear_m_s, 0.25);
    assert_close(left.relative_humidity, right.relative_humidity, 0.005);
    assert_close(left.lift, right.lift, 0.005);
    assert_close(left.cloudiness, right.cloudiness, 0.005);
    assert_close(left.instability, right.instability, 0.005);
    assert_close(left.upper_cloudiness, right.upper_cloudiness, 0.005);
    assert_close(left.rain_intensity, right.rain_intensity, 0.01);
}

#[test]
fn terrain_repeats_across_longitude_and_continues_north_south() {
    let world = geometry();
    let positions = [
        Vector::new(13.25, -7.5),
        Vector::new(-12_345.0, 8_765.0),
        Vector::new(31_000.0, 19_000.0),
    ];

    for position in positions {
        let elevation = procedural_terrain_elevation_m(world, SEED, position);
        assert_close(
            elevation,
            procedural_terrain_elevation_m(
                world,
                SEED,
                position + Vector::new(world.width_m(), 0.0),
            ),
            0.02,
        );
    }

    assert!(positions.into_iter().any(|position| {
        let elevation = procedural_terrain_elevation_m(world, SEED, position);
        let north = procedural_terrain_elevation_m(
            world,
            SEED,
            position + Vector::new(0.0, world.height_m()),
        );
        (elevation - north).abs() > 0.02
    }));
}

#[test]
fn terrain_value_and_gradient_are_continuous_across_longitude() {
    let world = geometry();
    let epsilon = 0.5;

    for y in [-61_000.0, -20_000.0, 6_250.0, 20_000.0, 73_000.0] {
        let left = Vector::new(-40_000.0, y);
        let right = Vector::new(40_000.0, y);
        assert_close(
            procedural_terrain_elevation_m(world, SEED, left),
            procedural_terrain_elevation_m(world, SEED, right),
            1.0e-4,
        );
        let left_gradient = Vector::new(
            (procedural_terrain_elevation_m(world, SEED, left + Vector::new(epsilon, 0.0))
                - procedural_terrain_elevation_m(world, SEED, left - Vector::new(epsilon, 0.0)))
                / (2.0 * epsilon),
            (procedural_terrain_elevation_m(world, SEED, left + Vector::new(0.0, epsilon))
                - procedural_terrain_elevation_m(world, SEED, left - Vector::new(0.0, epsilon)))
                / (2.0 * epsilon),
        );
        let right_gradient = Vector::new(
            (procedural_terrain_elevation_m(world, SEED, right + Vector::new(epsilon, 0.0))
                - procedural_terrain_elevation_m(world, SEED, right - Vector::new(epsilon, 0.0)))
                / (2.0 * epsilon),
            (procedural_terrain_elevation_m(world, SEED, right + Vector::new(0.0, epsilon))
                - procedural_terrain_elevation_m(world, SEED, right - Vector::new(0.0, epsilon)))
                / (2.0 * epsilon),
        );
        assert_vector_close(left_gradient, right_gradient, 1.0e-3);
        assert_close(
            procedural_terrain_elevation_m(world, SEED, left + Vector::new(0.0, epsilon)),
            procedural_terrain_elevation_m(world, SEED, right + Vector::new(0.0, epsilon)),
            1.0e-4,
        );
    }
}

#[test]
fn terrain_one_sided_values_and_derivatives_meet_at_longitude_seam() {
    let epsilon = 0.03125;
    let mut checked_x_seam = 0;
    for (world, seed) in [
        (WorldGeometry::new(8_000.0, 256_000.0).unwrap(), SEED),
        (geometry(), 0x1234_5678_9abc_def0),
        (WorldGeometry::new(256_000.0, 8_000.0).unwrap(), u64::MAX),
    ] {
        let half_width = world.width_m() / 2.0;
        for sample_index in 0..32 {
            let y = -1.5 * world.height_m()
                + (sample_index as f32 + 0.5) * 3.0 * world.height_m() / 32.0;
            let edge = procedural_terrain_elevation_m(world, seed, Vector::new(-half_width, y));
            if edge.abs() <= 100.0 {
                checked_x_seam += 1;
                let inside_low = procedural_terrain_elevation_m(
                    world,
                    seed,
                    Vector::new(-half_width + epsilon, y),
                );
                let inside_high = procedural_terrain_elevation_m(
                    world,
                    seed,
                    Vector::new(half_width - epsilon, y),
                );
                let low_derivative = (inside_low - edge) / epsilon;
                let high_derivative = (edge - inside_high) / epsilon;
                assert!((inside_low - edge).abs() <= 0.5);
                assert!((inside_high - edge).abs() <= 0.5);
                assert!(
                    (low_derivative - high_derivative).abs() <= 1.0,
                    "x seam in {world:?} seed {seed:#x} at y={y}: edge={edge}, low={inside_low}, high={inside_high}, derivatives {low_derivative} and {high_derivative}",
                );
            }
        }
    }
    assert!(checked_x_seam > 0);
}

#[test]
fn every_primary_atmosphere_field_joins_across_longitude() {
    let world = geometry();
    let position = Vector::new(-31.75, 14.5);
    let primary = procedural_atmosphere_primary(world, SEED, TICK, position);

    assert_primary_close(
        primary,
        procedural_atmosphere_primary(world, SEED, TICK, position + Vector::new(160_000.0, 0.0)),
    );

    for (left, right) in [(
        Vector::new(-40_000.0, 6_250.0),
        Vector::new(40_000.0, 6_250.0),
    )] {
        assert_primary_close(
            procedural_atmosphere_primary(world, SEED, TICK, left),
            procedural_atmosphere_primary(world, SEED, TICK, right),
        );
    }
}

#[test]
fn authoritative_fields_approach_each_seam_from_both_sides() {
    let epsilon = 0.25;
    for (world, seed, tick) in [
        (WorldGeometry::new(8_000.0, 256_000.0).unwrap(), SEED, 0),
        (geometry(), 0x1234_5678_9abc_def0, TICK),
        (
            WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
            u64::MAX,
            u64::MAX,
        ),
    ] {
        let half_width = world.width_m() / 2.0;
        for sample_index in 0..8 {
            let tangent_phase = (sample_index as f32 + 0.5) / 8.0;
            let y = -0.5 * world.height_m() + tangent_phase * world.height_m();
            let x_sides = (
                Vector::new(-half_width + epsilon, y),
                Vector::new(half_width - epsilon, y),
            );
            assert_field_sides_close(world, seed, tick, x_sides.0, x_sides.1);
        }
    }
}

fn assert_field_sides_close(world: WorldGeometry, seed: u64, tick: u64, low: Vector, high: Vector) {
    assert_primary_seam_close(
        procedural_atmosphere_primary(world, seed, tick, low),
        procedural_atmosphere_primary(world, seed, tick, high),
    );
    assert_weather_seam_close(
        procedural_weather(world, seed, tick, low),
        procedural_weather(world, seed, tick, high),
    );
    let time_seconds = tick as f64 / 60.0;
    assert_close(
        procedural_water_height_m(world, seed, time_seconds, low),
        procedural_water_height_m(world, seed, time_seconds, high),
        0.01,
    );
    assert_vector_close(
        procedural_unsheltered_wind_velocity_m_s(world, seed, tick, low),
        procedural_unsheltered_wind_velocity_m_s(world, seed, tick, high),
        0.25,
    );
    let low_environment = procedural_match_environment(world, seed, tick, low);
    let high_environment = procedural_match_environment(world, seed, tick, high);
    assert!(
        (low_environment.true_wind_velocity_m_s - high_environment.true_wind_velocity_m_s).length()
            <= 0.3,
        "environment wind differs across {world:?} seed {seed:#x} tick {tick} between {low:?} and {high:?}: {:?} != {:?}",
        low_environment.true_wind_velocity_m_s,
        high_environment.true_wind_velocity_m_s,
    );
    assert!(
        (low_environment.water_depth_m - high_environment.water_depth_m).abs() <= 10.0,
        "water depth differs across {world:?} seed {seed:#x} tick {tick} between {low:?} and {high:?}: {} != {}",
        low_environment.water_depth_m,
        high_environment.water_depth_m,
    );
}

#[test]
fn fields_repeat_across_longitude_and_tide_continues_north_south() {
    let world = geometry();
    let position = Vector::new(29.0, -13.0);
    let repeated = position + Vector::new(-80_000.0, 0.0);
    let time_seconds = TICK as f64 / 60.0;

    assert_eq!(
        procedural_tidal_conditions(world, SEED, position),
        procedural_tidal_conditions(world, SEED, repeated)
    );
    assert_ne!(
        procedural_tidal_conditions(world, SEED, position),
        procedural_tidal_conditions(world, SEED, position + Vector::new(0.0, world.height_m()),),
    );
    assert_close(
        procedural_water_height_m(world, SEED, time_seconds, position),
        procedural_water_height_m(world, SEED, time_seconds, repeated),
        2.0e-4,
    );
    assert_close(
        procedural_water_depth_m(world, SEED, time_seconds, position),
        procedural_water_depth_m(world, SEED, time_seconds, repeated),
        1.0e-4,
    );
    assert_close(
        procedural_solid_surface_elevation_m(world, SEED, position, 14.0, 14.0),
        procedural_solid_surface_elevation_m(world, SEED, repeated, 14.0, 14.0),
        1.0e-4,
    );
    assert_field_sides_close(world, SEED, TICK, position, repeated);
}

#[test]
fn periodic_fields_remain_finite_and_repeating_at_the_largest_tick() {
    let world = geometry();
    let position = Vector::new(7.25, -9.5);
    let repeated = position + Vector::new(80_000.0, 0.0);
    let primary = procedural_atmosphere_primary(world, SEED, u64::MAX, position);
    let repeated_primary = procedural_atmosphere_primary(world, SEED, u64::MAX, repeated);

    assert_primary_close(primary, repeated_primary);
    assert!(primary.pressure_hpa.is_finite());
    assert!(primary.pressure_gradient_hpa_per_m.x.is_finite());
    assert!(primary.pressure_gradient_hpa_per_m.y.is_finite());
    assert!(primary.surface_air_temperature_anomaly_c.is_finite());
    assert!(primary.air_temperature_500_hpa_c.is_finite());
    assert!(primary.local_temperature_gradient_c_per_m.x.is_finite());
    assert!(primary.local_temperature_gradient_c_per_m.y.is_finite());
    assert!(primary.water_vapor_g_kg.is_finite());
    assert_ne!(
        primary,
        procedural_atmosphere_primary(world, SEED, u64::MAX - 60, position)
    );
}

#[test]
fn pressure_does_not_loop_between_the_poles() {
    let world = geometry();
    let south = procedural_atmosphere_primary(
        world,
        SEED,
        TICK,
        Vector::new(7.25, -0.5 * world.height_m()),
    );
    let north =
        procedural_atmosphere_primary(world, SEED, TICK, Vector::new(7.25, 0.5 * world.height_m()));

    assert!(
        (south.pressure_hpa - north.pressure_hpa).abs() > 0.01,
        "pressure repeats between poles: {} hPa",
        south.pressure_hpa,
    );
}

#[test]
fn changing_world_geometry_changes_the_procedural_realization() {
    let first = geometry();
    let second = WorldGeometry::new(160_000.0, 40_000.0).unwrap();
    let position = Vector::new(13.25, -7.5);

    assert_ne!(
        procedural_terrain_elevation_m(first, SEED, position),
        procedural_terrain_elevation_m(second, SEED, position)
    );
    assert_ne!(
        procedural_atmosphere_primary(first, SEED, TICK, position),
        procedural_atmosphere_primary(second, SEED, TICK, position)
    );
}
