use browser::{sample_production_world_environment, sample_world_geometry};
use game::{procedural_match_environment, procedural_world_environment};
use rapier2d_f64::glamx::Vec2 as Vector;
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
fn server_browser_and_playground_center_use_the_production_environment_sampler() {
    let geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let center_m = Vector::new(60_000.0, 30_000.0);
    let browser = sample_production_world_environment(geometry, 0x5eed, 12_345, center_m);
    let production = procedural_world_environment(geometry, 0x5eed, 12_345, center_m);

    assert_eq!(browser, production);
    let boat = procedural_match_environment(geometry, 0x5eed, 12_345, center_m.as_dvec2(), 0.0);
    assert_eq!(
        browser.environment.true_wind_velocity_m_s,
        boat.true_wind_velocity_m_s
    );
    assert_eq!(browser.celestial.geographic_latitude_degrees, 45.0);
    assert_eq!(browser.celestial.day_phase, production.celestial.day_phase);
}
