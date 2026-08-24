use browser::{cloud_anchor_time, sample_world_geometry};
use simulation::WorldGeometry;

#[test]
fn playground_geometry_sample_uses_canonical_world_math() {
    let geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    let sample = sample_world_geometry(geometry, [60_000.0, 30_000.0], 0.875);

    assert_eq!(sample.raw_center_m, [60_000.0, 30_000.0]);
    assert_eq!(sample.wrapped_center_m, [-20_000.0, -10_000.0]);
    assert_eq!(sample.normalized_torus, [0.25, 0.25]);
    assert_eq!(sample.latitude_degrees, -45.0);
    assert_eq!(sample.local_solar_phase, 0.625);
}

#[test]
fn cloud_anchor_time_keeps_integer_epochs_and_bounded_ages() {
    let before = cloud_anchor_time(107_999, 108_000);
    assert_eq!(before.epoch, 0);
    assert!(before.phase < 1.0);

    let boundary = cloud_anchor_time(108_000, 108_000);
    assert_eq!(boundary.epoch, 1);
    assert_eq!(boundary.age_ticks, 0);
    assert_eq!(boundary.phase, 0.0);

    let huge = cloud_anchor_time(u64::MAX, 108_000);
    assert_eq!(huge.epoch, u64::MAX / 108_000);
    assert!(huge.age_ticks < 108_000);
    assert!((0.0..1.0).contains(&huge.phase));
}
