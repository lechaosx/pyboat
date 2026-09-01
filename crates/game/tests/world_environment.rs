use game::{
    celestial_orbit, cloud_field_time_coordinates, current_air_temperature_c,
    procedural_world_environment, scalar_field_time_coordinates,
    scalar_field_time_coordinates_interpolated, vector_field_time_coordinates,
    vector_field_time_coordinates_interpolated, world_celestial_conditions,
    world_climate_conditions,
};
use rapier2d::prelude::Vector;
use simulation::{DT_SECONDS, WorldGeometry};

const SEED: u64 = 0x5eed;
const TICK: u64 = 12_345;
const WORLD_DAY_SECONDS: f64 = 86_400.0;

fn geometry() -> WorldGeometry {
    WorldGeometry::new(80_000.0, 40_000.0).unwrap()
}

#[test]
fn field_time_coordinates_interpolate_between_simulation_ticks() {
    let world = geometry();
    let scalar_before = scalar_field_time_coordinates(TICK);
    let scalar_halfway = scalar_field_time_coordinates_interpolated(TICK, 0.5);
    let scalar_after = scalar_field_time_coordinates(TICK + 1);
    assert!(
        (scalar_halfway.air_temperature
            - (scalar_before.air_temperature + scalar_after.air_temperature) * 0.5)
            .abs()
            < 1.0e-6
    );

    let vector_before = vector_field_time_coordinates(world, TICK);
    let vector_halfway = vector_field_time_coordinates_interpolated(world, TICK, 0.5);
    let vector_after = vector_field_time_coordinates(world, TICK + 1);
    assert!(
        (vector_halfway.ocean_current
            - (vector_before.ocean_current + vector_after.ocean_current) * 0.5)
            .abs()
            < 1.0e-6
    );
}

#[test]
fn cloud_field_time_coordinates_use_each_visual_fields_wavelength() {
    let after_one_second = cloud_field_time_coordinates(60);

    assert!((after_one_second.low - 1.0 / 640.0).abs() < 1.0e-7);
    assert!((after_one_second.upper - 1.0 / 4_000.0).abs() < 1.0e-7);
}

#[test]
fn longitude_sets_local_solar_time_without_changing_global_cycles() {
    let world = geometry();
    let time_seconds = 12_345.0;
    let reference = world_celestial_conditions(world, time_seconds, Vector::new(0.0, 4_000.0));
    let east = world_celestial_conditions(world, time_seconds, Vector::new(20_000.0, 4_000.0));
    let repeated = world_celestial_conditions(world, time_seconds, Vector::new(100_000.0, 4_000.0));

    assert_eq!(east.day_phase, (reference.day_phase + 0.25).rem_euclid(1.0));
    assert_eq!(east.day_phase, repeated.day_phase);
    assert_eq!(reference.season_phase, east.season_phase);
    assert_eq!(reference.lunar_longitude_phase, east.lunar_longitude_phase,);
}

#[test]
fn northern_june_is_summer_and_northern_december_is_winter() {
    let world = geometry();
    let northern_position_m = Vector::new(0.0, world.height_m() / 3.0);
    let world_time_at_midmonth = |month: u32| f64::from((month - 1) * 28 + 13) * WORLD_DAY_SECONDS;
    let june = world_celestial_conditions(world, world_time_at_midmonth(6), northern_position_m);
    let december =
        world_celestial_conditions(world, world_time_at_midmonth(12), northern_position_m);

    assert!(celestial_orbit(june).solar_declination_rad > 0.0);
    assert!(celestial_orbit(december).solar_declination_rad < 0.0);
    let climate = world_climate_conditions(june.geographic_latitude_degrees);
    assert!(
        current_air_temperature_c(climate, june) > current_air_temperature_c(climate, december)
    );
}

#[test]
fn latitude_calibrates_equatorial_temperate_and_polar_climate() {
    let equator = world_climate_conditions(0.0);
    let subtropical = world_climate_conditions(30.0);
    let temperate = world_climate_conditions(50.0);
    let subpolar = world_climate_conditions(70.0);
    let pole = world_climate_conditions(90.0);

    assert!((equator.annual_mean_air_temperature_c - 27.4).abs() < 0.2);
    assert!((subtropical.annual_mean_air_temperature_c - 21.9).abs() < 0.2);
    assert!((temperate.annual_mean_air_temperature_c - 8.0).abs() < 0.2);
    assert!((subpolar.annual_mean_air_temperature_c + 7.9).abs() < 0.2);
    assert!((pole.annual_mean_air_temperature_c + 15.0).abs() < 0.01);
    assert!((equator.seasonal_temperature_range_c - 2.0).abs() < 0.2);
    assert!((subtropical.seasonal_temperature_range_c - 15.3).abs() < 0.2);
    assert!((temperate.seasonal_temperature_range_c - 21.3).abs() < 0.2);
    assert!((subpolar.seasonal_temperature_range_c - 31.6).abs() < 0.2);
    assert!((pole.seasonal_temperature_range_c - 20.0).abs() < 0.2);
    assert!((equator.diurnal_temperature_range_c - 4.4).abs() < 0.2);
    assert!((subtropical.diurnal_temperature_range_c - 9.5).abs() < 0.2);
    assert!((temperate.diurnal_temperature_range_c - 6.1).abs() < 0.2);
    assert!((subpolar.diurnal_temperature_range_c - 4.5).abs() < 0.2);
    assert!((pole.diurnal_temperature_range_c - 2.5).abs() < 0.2);
}

#[test]
fn hemispheres_have_opposite_seasons_while_global_phases_match() {
    let world = geometry();
    let north = procedural_world_environment(world, SEED, TICK, Vector::new(0.0, 15_000.0));
    let south = procedural_world_environment(world, SEED, TICK, Vector::new(0.0, -15_000.0));

    assert_eq!(north.celestial.season_phase, south.celestial.season_phase);
    assert_eq!(
        north.celestial.lunar_longitude_phase,
        south.celestial.lunar_longitude_phase,
    );
    let north_temperature = current_air_temperature_c(north.climate, north.celestial);
    let south_temperature = current_air_temperature_c(south.climate, south.celestial);
    let southern_opposite_season = current_air_temperature_c(
        south.climate,
        game::CelestialConditions {
            season_phase: (south.celestial.season_phase + 0.5).rem_euclid(1.0),
            ..south.celestial
        },
    );
    assert!((north_temperature - southern_opposite_season).abs() < 0.01);
    assert!((north_temperature - south_temperature).abs() > 10.0);
}

#[test]
fn latitude_mirror_repeats_between_successive_poles() {
    let world = geometry();
    let half_height = world.height_m() / 2.0;
    for (y_m, expected_latitude) in [
        (-2.0 * half_height, 0.0),
        (-half_height, -90.0),
        (0.0, 0.0),
        (half_height, 90.0),
        (2.0 * half_height, 0.0),
        (3.0 * half_height, -90.0),
        (4.0 * half_height, 0.0),
    ] {
        let conditions = world_celestial_conditions(world, 0.0, Vector::new(0.0, y_m));
        assert_eq!(conditions.geographic_latitude_degrees, expected_latitude);
    }
}

#[test]
fn repeated_latitude_mirrors_climate_without_repeating_seeded_fields() {
    let world = geometry();
    let sixty_degrees =
        procedural_world_environment(world, SEED, TICK, Vector::new(0.0, world.height_m() / 3.0));
    let mirrored_120_degrees = procedural_world_environment(
        world,
        SEED,
        TICK,
        Vector::new(0.0, 2.0 * world.height_m() / 3.0),
    );

    assert_eq!(sixty_degrees.climate, mirrored_120_degrees.climate);
    assert_eq!(sixty_degrees.celestial.geographic_latitude_degrees, 60.0);
    assert_eq!(
        mirrored_120_degrees.celestial.geographic_latitude_degrees,
        60.0
    );
    assert_ne!(sixty_degrees.weather, mirrored_120_degrees.weather);
}

#[test]
fn world_environment_is_deterministic_and_locally_continuous() {
    let world = geometry();
    let position = Vector::new(12_345.0, 4_321.0);
    let sample = procedural_world_environment(world, SEED, TICK, position);
    assert_eq!(
        sample,
        procedural_world_environment(world, SEED, TICK, position)
    );

    let nearby =
        procedural_world_environment(world, SEED, TICK + 1, position + Vector::new(0.125, -0.125));
    assert!(
        (sample.weather.surface_air_temperature_c - nearby.weather.surface_air_temperature_c).abs()
            < 0.1
    );
    assert!((sample.weather.wind_velocity_m_s - nearby.weather.wind_velocity_m_s).length() < 0.1);
    assert!((sample.celestial.day_phase - nearby.celestial.day_phase).abs() < 0.001);
    let expected = world_celestial_conditions(world, TICK as f64 * f64::from(DT_SECONDS), position);
    assert_eq!(sample.celestial, expected);
}

#[test]
fn evolving_weather_fields_advance_over_an_hour_at_huge_ticks_without_offsets() {
    let world = geometry();
    let scalar_before = scalar_field_time_coordinates(u64::MAX - 216_000);
    let scalar_after = scalar_field_time_coordinates(u64::MAX);
    let vector_before = vector_field_time_coordinates(world, u64::MAX - 216_000);
    let vector_after = vector_field_time_coordinates(world, u64::MAX);

    assert_ne!(scalar_before.air_temperature, scalar_after.air_temperature);
    assert_ne!(
        scalar_before.upper_air_temperature,
        scalar_after.upper_air_temperature
    );
    assert_ne!(scalar_before.water_vapor, scalar_after.water_vapor);
    assert_ne!(
        scalar_before.decorative_water,
        scalar_after.decorative_water
    );
    assert_ne!(vector_before.pressure, vector_after.pressure);
    assert_ne!(vector_before.ocean_current, vector_after.ocean_current);
}
