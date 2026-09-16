use browser::{
    periodic_effect_time, periodic_effect_time_interpolated, sample_production_world_environment,
    sample_world_geometry,
};
use game::{procedural_match_environment, procedural_world_environment};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

#[test]
fn playground_geometry_sample_uses_canonical_world_math() {
    let geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    let sample = sample_world_geometry(geometry, [60_000.0, 30_000.0], 0.875);

    assert_eq!(sample.raw_center_m, [60_000.0, 30_000.0]);
    assert_eq!(sample.canonical_center_m, [-20_000.0, 30_000.0]);
    assert_eq!(sample.normalized_world, [0.25, 1.0]);
    assert_eq!(sample.latitude_degrees, 45.0);
    assert_eq!(sample.local_solar_phase, 0.625);
}

#[test]
fn periodic_effect_time_keeps_integer_epochs_and_bounded_ages() {
    let before = periodic_effect_time(107_999, 108_000);
    assert_eq!(before.epoch, 0);
    assert!(before.phase < 1.0);

    let boundary = periodic_effect_time(108_000, 108_000);
    assert_eq!(boundary.epoch, 1);
    assert_eq!(boundary.age_ticks, 0);
    assert_eq!(boundary.phase, 0.0);

    let huge = periodic_effect_time(u64::MAX, 108_000);
    assert_eq!(huge.epoch, u64::MAX / 108_000);
    assert!(huge.age_ticks < 108_000);
    assert!((0.0..1.0).contains(&huge.phase));
}

#[test]
fn periodic_effect_time_preserves_fractional_render_progress() {
    let sample = periodic_effect_time_interpolated(90, 0.5, 180);

    assert_eq!(sample.epoch, 0);
    assert_eq!(sample.age_ticks, 90);
    assert!((sample.phase - 0.502_777_76).abs() < 1.0e-7);
}

#[test]
fn server_browser_and_playground_center_use_the_production_environment_sampler() {
    let geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let center_m = Vector::new(60_000.0, 30_000.0);
    let browser = sample_production_world_environment(geometry, 0x5eed, 12_345, center_m);
    let production = procedural_world_environment(geometry, 0x5eed, 12_345, center_m);

    assert_eq!(browser, production);
    assert_eq!(
        browser.environment,
        procedural_match_environment(geometry, 0x5eed, 12_345, center_m)
    );
    assert_eq!(browser.celestial.geographic_latitude_degrees, 45.0);
    assert_eq!(browser.celestial.day_phase, production.celestial.day_phase);
}
