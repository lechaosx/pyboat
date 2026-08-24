use game::{
    current_air_temperature_c, procedural_world_environment, world_celestial_conditions,
    world_climate_conditions,
};
use rapier2d::prelude::Vector;
use simulation::{DT_SECONDS, WorldGeometry};

const SEED: u64 = 0x5eed;
const TICK: u64 = 12_345;

fn geometry() -> WorldGeometry {
    WorldGeometry::new(80_000.0, 40_000.0).unwrap()
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
    assert_eq!(reference.moon_phase, east.moon_phase);
}

#[test]
fn latitude_calibrates_equatorial_temperate_and_polar_climate() {
    let equator = world_climate_conditions(0.0);
    let temperate = world_climate_conditions(45.0);
    let pole = world_climate_conditions(90.0);

    assert!((equator.annual_mean_air_temperature_c - 27.0).abs() < 0.01);
    assert!((pole.annual_mean_air_temperature_c + 18.0).abs() < 0.01);
    assert!((10.0..13.0).contains(&temperate.annual_mean_air_temperature_c));
    assert!(equator.seasonal_temperature_range_c <= 3.01);
    assert!((18.0..21.0).contains(&temperate.seasonal_temperature_range_c));
    assert!(pole.seasonal_temperature_range_c >= 43.99);
    assert!(equator.diurnal_temperature_range_c > pole.diurnal_temperature_range_c);
}

#[test]
fn hemispheres_have_opposite_seasons_while_global_phases_match() {
    let world = geometry();
    let north = procedural_world_environment(world, SEED, TICK, Vector::new(0.0, 15_000.0));
    let south = procedural_world_environment(world, SEED, TICK, Vector::new(0.0, -15_000.0));

    assert_eq!(north.celestial.season_phase, south.celestial.season_phase);
    assert_eq!(north.celestial.moon_phase, south.celestial.moon_phase);
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
fn polar_loop_keeps_the_endorsed_seasonal_forcing_discontinuity() {
    let world = geometry();
    let epsilon_m = 0.125;
    let lower =
        procedural_world_environment(world, SEED, TICK, Vector::new(0.0, -world.height_m() / 2.0));
    let upper = procedural_world_environment(
        world,
        SEED,
        TICK,
        Vector::new(0.0, world.height_m() / 2.0 - epsilon_m),
    );

    assert_eq!(lower.celestial.geographic_latitude_degrees, -90.0);
    assert!(upper.celestial.geographic_latitude_degrees > 89.99);
    let lower_temperature = current_air_temperature_c(lower.climate, lower.celestial);
    let upper_temperature = current_air_temperature_c(upper.climate, upper.celestial);
    assert!((lower_temperature - upper_temperature).abs() > 20.0);
}

#[test]
fn world_environment_is_deterministic_and_continuous_away_from_the_polar_branch() {
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
