use simulation::WorldGeometry;

#[test]
fn cylinder_wraps_longitude_and_preserves_north_south_position() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    assert_eq!(world.width_m(), 80_000.0);
    assert_eq!(world.height_m(), 40_000.0);
    assert_eq!(world.wrap_position_m([80_125.0, -25.0]), [125.0, -25.0]);
    assert_eq!(
        world.wrap_position_m([40_000.0, 20_000.0]),
        [-40_000.0, 20_000.0]
    );
}

#[test]
fn longitude_wrapping_is_periodic_and_idempotent() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let position = [-12_345.5, 98_765.25];
    let wrapped = world.wrap_position_m(position);

    assert_eq!(
        world.wrap_position_m([position[0] + 3.0 * 80_000.0, position[1]]),
        wrapped,
    );
    assert_eq!(wrapped[1], position[1]);
    assert_eq!(world.wrap_position_m(wrapped), wrapped);
}

#[test]
fn minimum_image_wraps_longitude_only() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    assert_eq!(
        world.minimum_image_displacement_m([79_000.0, -39_000.0]),
        [-1_000.0, -39_000.0],
    );
    assert_eq!(
        world.minimum_image_displacement_m([40_000.0, -20_000.0]),
        [40_000.0, -20_000.0],
    );
}

#[test]
fn latitude_mirror_repeats_between_the_poles() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    assert_eq!(world.latitude_degrees(-20_000.0), -90.0);
    assert_eq!(world.latitude_degrees(0.0), 0.0);
    assert_eq!(world.latitude_degrees(10_000.0), 45.0);
    assert_eq!(world.latitude_degrees(20_000.0), 90.0);
    assert_eq!(world.latitude_degrees(30_000.0), 45.0);
    assert_eq!(world.latitude_degrees(40_000.0), 0.0);
    assert_eq!(world.latitude_degrees(50_000.0), -45.0);
    assert_eq!(world.latitude_degrees(80_000.0), 0.0);
    assert_eq!(world.latitude_degrees(-30_000.0), -45.0);
    assert_eq!(world.latitude_degrees(-40_000.0), 0.0);
    assert_eq!(world.latitude_degrees(-80_000.0), 0.0);
    assert!(world.latitude_degrees(20_000.0 - 0.125) > 89.99);

    assert_eq!(world.latitude_phase_degrees(10_000.0), 45.0);
    assert_eq!(world.latitude_phase_degrees(30_000.0), 135.0);
    assert_eq!(world.latitude_phase_degrees(50_000.0), -135.0);
    assert_eq!(world.latitude_phase_degrees(70_000.0), -45.0);

    let gradient = 180.0 / world.height_m();
    assert_eq!(world.latitude_gradient_degrees_per_m(10_000.0), gradient);
    assert_eq!(world.latitude_gradient_degrees_per_m(30_000.0), -gradient);
    assert_eq!(world.latitude_gradient_degrees_per_m(50_000.0), -gradient);
    assert_eq!(world.latitude_gradient_degrees_per_m(70_000.0), gradient);
}

#[test]
fn local_solar_phase_wraps_with_longitude() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    assert_eq!(world.local_solar_phase(0.125, 0.0), 0.125);
    assert_eq!(world.local_solar_phase(0.125, 20_000.0), 0.375);
    assert_eq!(world.local_solar_phase(0.875, 20_000.0), 0.125);
    assert_eq!(
        world.local_solar_phase(0.125, -20_000.0),
        world.local_solar_phase(0.125, 60_000.0),
    );
}

#[test]
fn normalized_world_coordinates_wrap_longitude_and_clamp_at_the_poles() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();

    assert_eq!(world.normalized_world_coordinates([0.0, 0.0]), [0.5, 0.5]);
    assert_eq!(
        world.normalized_world_coordinates([-40_000.0, -20_000.0]),
        [0.0, 0.0]
    );
    assert_eq!(
        world.normalized_world_coordinates([60_000.0, 30_000.0]),
        [0.25, 1.0]
    );
}

#[test]
fn geometry_rejects_non_finite_and_non_positive_dimensions() {
    assert!(WorldGeometry::new(0.0, 40_000.0).is_err());
    assert!(WorldGeometry::new(80_000.0, -1.0).is_err());
    assert!(WorldGeometry::new(f32::INFINITY, 40_000.0).is_err());
    assert!(WorldGeometry::new(80_000.0, f32::NAN).is_err());
}

#[test]
fn playable_dimensions_are_bounded_to_the_selectable_range() {
    assert!(WorldGeometry::new(7_999.0, 40_000.0).is_err());
    assert!(WorldGeometry::new(80_000.0, 256_001.0).is_err());

    let smallest = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    assert_eq!(
        (smallest.width_m(), smallest.height_m()),
        (8_000.0, 8_000.0)
    );
    let largest = WorldGeometry::new(256_000.0, 256_000.0).unwrap();
    assert_eq!(
        (largest.width_m(), largest.height_m()),
        (256_000.0, 256_000.0)
    );
}

#[test]
fn default_geometry_is_a_64_kilometre_square() {
    let world = WorldGeometry::default();

    assert_eq!(world.width_m(), 64_000.0);
    assert_eq!(world.height_m(), 64_000.0);
}

#[test]
fn upper_adjacent_coordinate_remains_on_the_upper_branch() {
    let world = WorldGeometry::default();
    let upper_adjacent_m = f32::from_bits((world.width_m() / 2.0).to_bits() - 1);

    assert_eq!(
        world.wrap_position_m([upper_adjacent_m, upper_adjacent_m]),
        [upper_adjacent_m; 2]
    );
    assert!(world.latitude_degrees(upper_adjacent_m) > 89.0);
    assert!(world.normalized_world_coordinates([upper_adjacent_m; 2])[0] < 1.0);
    assert_eq!(
        world.normalized_world_coordinates([upper_adjacent_m; 2])[1],
        1.0
    );
}

#[test]
fn extreme_finite_coordinates_produce_finite_results() {
    let default = WorldGeometry::default();
    for coordinate_m in [f32::MIN, f32::MAX] {
        let wrapped = default.wrap_position_m([coordinate_m; 2]);
        assert!(wrapped.into_iter().all(f32::is_finite));
        assert_eq!(default.wrap_position_m(wrapped), wrapped);
        assert!(default.latitude_degrees(coordinate_m).is_finite());
    }
}

#[test]
fn dimensions_outside_the_playable_range_are_rejected() {
    let smallest_f32 = f32::from_bits(1);
    assert!(WorldGeometry::new(smallest_f32, 1.0).is_err());
    assert!(WorldGeometry::new(f32::from_bits(3), 1.0).is_err());
    assert!(WorldGeometry::new(f32::MAX, f32::MAX).is_err());
}
