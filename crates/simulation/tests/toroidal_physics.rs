use std::f32::consts::PI;

use rapier2d::prelude::Vector;
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, Environment, HullContact, HullParameters,
    MAXIMUM_SHARED_HULL_POINT_SPEED_M_S, SharedSimulation, WorldGeometry,
};

fn parameters() -> BoatParameters {
    BoatParameters {
        hull: HullParameters {
            mass_kg: 222.0,
            length_m: 4.0,
            beam_m: 1.5,
            bow_length_m: 0.7,
        },
        sail_mass_kg: 12.0,
        boom_mass_kg: 10.0,
        boom_length_m: 2.0,
        mast_position_m: Vector::new(0.4, 0.0),
        max_sail_angle_rad: PI / 2.0,
        air_density_kg_m3: 0.0,
        sail_area_m2: 8.0,
        sail_center_of_effort_from_mast_m: 0.7,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 0.0,
        keel_area_m2: 0.6,
        keel_position_m: Vector::ZERO,
        keel_base_drag_coefficient: 0.02,
        keel_stall_angle_rad: 15.0_f32.to_radians(),
        keel_stall_lift_coefficient: 0.8,
        hull_reference_area_m2: 0.3,
        hull_drag_coefficient: 0.08,
        hull_lateral_drag_coefficient: 0.02,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: PI / 6.0,
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        rudder_stall_angle_rad: 15.0_f32.to_radians(),
        rudder_stall_lift_coefficient: 0.8,
        max_sail_actuator_torque_n_m: 375.0,
        max_rudder_actuator_torque_n_m: 60.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
        draft_m: 0.6,
        ground_static_friction_coefficient: 1.0,
        ground_kinetic_friction_coefficient: 0.6,
    }
}

fn motion(position_m: Vector, velocity_m_s: Vector, angle_rad: f32) -> BoatMotion {
    BoatMotion {
        position_m,
        velocity_m_s,
        angle_rad,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    }
}

fn advance(simulation: &mut SharedSimulation, boat_count: usize) -> Vec<HullContact> {
    let torques = vec![ActuatorTorques::default(); boat_count];
    simulation.advance(Environment::default(), &torques)
}

#[test]
fn logical_positions_wrap_after_repeated_x_y_and_corner_laps() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    let positions = [
        Vector::new(3.0 * world.width_m() + 17.0, -11.0),
        Vector::new(13.0, -4.0 * world.height_m() + 19.0),
        Vector::new(-5.0 * world.width_m() + 23.0, 6.0 * world.height_m() - 29.0),
    ];
    for position_m in positions {
        simulation.add_boat(parameters(), motion(position_m, Vector::ZERO, 0.0));
    }

    advance(&mut simulation, positions.len());

    for (boat, position_m) in positions.into_iter().enumerate() {
        let expected = world.wrap_position_m(position_m.into());
        let actual = simulation.motion(boat).position_m;
        assert!((actual - Vector::from(expected)).length() < 1.0e-3);
    }
}

#[test]
fn moving_boats_complete_repeated_x_y_and_corner_laps() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    for (velocity_m_s, ticks) in [
        (Vector::new(400.0, 0.0), 2_400),
        (Vector::new(0.0, 400.0), 2_400),
        (Vector::new(240.0, 240.0), 4_000),
    ] {
        let mut simulation = SharedSimulation::new_with_world_geometry(world);
        simulation.add_boat(parameters(), motion(Vector::ZERO, velocity_m_s, 0.0));
        for _ in 0..ticks {
            advance(&mut simulation, 1);
        }
        assert!(
            simulation.motion(0).position_m.length() < 10.0,
            "{:?}",
            simulation.motion(0)
        );
    }
}

#[test]
fn wrapped_radius_query_finds_x_y_and_corner_neighbors() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let half = world.width_m() / 2.0;
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    for position_m in [
        Vector::new(half - 1.0, half - 1.0),
        Vector::new(-half + 2.0, half - 1.0),
        Vector::new(half - 1.0, -half + 2.0),
        Vector::new(-half + 2.0, -half + 2.0),
    ] {
        simulation.add_boat(parameters(), motion(position_m, Vector::ZERO, 0.0));
    }
    advance(&mut simulation, 4);

    assert_eq!(
        simulation.boats_within_radius(Vector::new(half - 1.0, half - 1.0), 4.5),
        vec![0, 1, 2, 3]
    );
}

fn ram(offset_m: Vector) -> (f32, [Vector; 2], usize) {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let direction = Vector::new(1.0, 1.0).normalize();
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    simulation.add_boat(
        parameters(),
        motion(offset_m - direction * 2.5, direction * 6.0, PI / 4.0),
    );
    simulation.add_boat(
        parameters(),
        motion(offset_m + direction * 2.5, -direction * 6.0, PI / 4.0),
    );
    let mut maximum_impulse = 0.0_f32;
    let mut contact_ticks = 0;
    let mut contact_ended = false;
    for _ in 0..30 {
        let contacts = advance(&mut simulation, 2);
        assert!(
            contacts.len() <= 1,
            "one hull pair must not be reported twice"
        );
        if let Some(contact) = contacts.first() {
            assert!(
                !contact_ended,
                "an ongoing contact must not drop and restart"
            );
            maximum_impulse = maximum_impulse.max(contact.impulse_magnitude_n_s);
            contact_ticks += 1;
        } else if contact_ticks > 0 {
            contact_ended = true;
        }
    }
    (
        maximum_impulse,
        [
            simulation.motion(0).velocity_m_s,
            simulation.motion(1).velocity_m_s,
        ],
        contact_ticks,
    )
}

#[test]
fn corner_seam_ram_matches_the_same_ram_at_world_center() {
    let central = ram(Vector::ZERO);
    let seam = ram(Vector::new(4_000.0, 4_000.0));

    assert!(central.0 > 100.0);
    assert!(
        (central.0 - seam.0).abs() < 20.0,
        "central {}, seam {}",
        central.0,
        seam.0
    );
    assert!((central.1[0] - seam.1[0]).length() < 0.1);
    assert!((central.1[1] - seam.1[1]).length() < 0.1);
    assert!(central.2 > 1 && seam.2 > 1);
}

#[test]
fn fast_swept_hulls_collide_across_a_seam() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let half = world.width_m() / 2.0;
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    simulation.add_boat(
        parameters(),
        motion(Vector::new(half - 8.0, 0.0), Vector::new(600.0, 0.0), 0.0),
    );
    simulation.add_boat(
        parameters(),
        motion(Vector::new(-half + 8.0, 0.0), Vector::new(-600.0, 0.0), 0.0),
    );

    let contacts = advance(&mut simulation, 2);

    assert!(
        contacts
            .iter()
            .any(|contact| contact.impulse_magnitude_n_s > 0.0),
        "velocities: {:?}, {:?}",
        simulation.motion(0).velocity_m_s,
        simulation.motion(1).velocity_m_s
    );
    assert!(simulation.motion(0).velocity_m_s.x < 600.0);
    assert!(simulation.motion(1).velocity_m_s.x > -600.0);
}

fn over_limit_dynamic_ram(offset_x_m: f32) -> (f32, [Vector; 2]) {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    simulation.add_boat(
        parameters(),
        motion(
            Vector::new(offset_x_m - 8.0, 0.0),
            Vector::new(10_000.0, 0.0),
            0.0,
        ),
    );
    simulation.add_boat(
        parameters(),
        motion(
            Vector::new(offset_x_m + 8.0, 0.0),
            Vector::new(-10_000.0, 0.0),
            0.0,
        ),
    );

    let contacts = advance(&mut simulation, 2);
    let contact = contacts
        .iter()
        .find(|contact| contact.impulse_magnitude_n_s > 0.0)
        .expect("the bounded dynamic pair must not tunnel");
    let velocities = [
        simulation.motion(0).velocity_m_s,
        simulation.motion(1).velocity_m_s,
    ];
    assert!(
        velocities
            .iter()
            .all(|velocity| velocity.length() <= MAXIMUM_SHARED_HULL_POINT_SPEED_M_S)
    );
    (contact.impulse_magnitude_n_s, velocities)
}

#[test]
fn out_of_domain_dynamic_ram_is_bounded_and_matches_across_the_x_seam() {
    let central = over_limit_dynamic_ram(0.0);
    let seam = over_limit_dynamic_ram(4_000.0);

    assert!(
        (central.0 - seam.0).abs() < 5.0,
        "central {}, seam {}",
        central.0,
        seam.0
    );
    assert!((central.1[0] - seam.1[0]).length() < 0.1);
    assert!((central.1[1] - seam.1[1]).length() < 0.1);
}

#[test]
fn shared_simulation_replaces_nonfinite_hull_velocity_state() {
    let mut simulation = SharedSimulation::new();
    let mut invalid_motion = motion(Vector::ZERO, Vector::new(f32::NAN, 1.0), 0.0);
    invalid_motion.angular_velocity_rad_s = f32::INFINITY;

    simulation.add_boat(parameters(), invalid_motion);

    assert_eq!(simulation.motion(0).velocity_m_s, Vector::ZERO);
    assert_eq!(simulation.motion(0).angular_velocity_rad_s, 0.0);
}

#[test]
#[should_panic(expected = "shared simulation hull beam must be at least 1.5 metres")]
fn shared_simulation_rejects_hulls_narrower_than_its_collision_domain() {
    let mut simulation = SharedSimulation::new();
    let mut narrow = parameters();
    narrow.hull.beam_m = 1.49;

    simulation.add_boat(narrow, motion(Vector::ZERO, Vector::ZERO, 0.0));
}

#[test]
fn fast_sweep_does_not_attribute_one_impact_to_an_unrelated_third_boat() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    for offset_x_m in [0.0, world.width_m() / 2.0] {
        let mut simulation = SharedSimulation::new_with_world_geometry(world);
        for (position_m, velocity_m_s) in [
            (Vector::new(offset_x_m - 8.0, 0.0), Vector::new(600.0, 0.0)),
            (Vector::new(offset_x_m, 0.0), Vector::ZERO),
            (Vector::new(offset_x_m + 6.0, 0.0), Vector::ZERO),
        ] {
            simulation.add_boat(parameters(), motion(position_m, velocity_m_s, 0.0));
        }

        let contacts = advance(&mut simulation, 3);
        let pairs = contacts
            .iter()
            .map(|contact| {
                (
                    contact.first_boat.min(contact.second_boat),
                    contact.first_boat.max(contact.second_boat),
                )
            })
            .collect::<std::collections::HashSet<_>>();

        assert!(pairs.contains(&(0, 1)));
        assert!(!pairs.contains(&(0, 2)));
        assert!(!pairs.contains(&(1, 2)));
    }
}

#[test]
fn three_boat_contact_component_is_resolved_once_across_the_seam() {
    let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
    let half = world.width_m() / 2.0;
    let mut simulation = SharedSimulation::new_with_world_geometry(world);
    for (position_m, velocity_m_s) in [
        (Vector::new(half - 4.5, 0.0), Vector::new(5.0, 0.0)),
        (Vector::new(half - 0.5, 0.0), Vector::ZERO),
        (Vector::new(-half + 3.5, 0.0), Vector::new(-5.0, 0.0)),
    ] {
        simulation.add_boat(parameters(), motion(position_m, velocity_m_s, 0.0));
    }

    let mut pairs = std::collections::HashSet::new();
    for _ in 0..20 {
        for contact in advance(&mut simulation, 3) {
            let pair = if contact.first_boat < contact.second_boat {
                (contact.first_boat, contact.second_boat)
            } else {
                (contact.second_boat, contact.first_boat)
            };
            pairs.insert(pair);
        }
    }

    assert!(pairs.contains(&(0, 1)));
    assert!(pairs.contains(&(1, 2)));
    assert_eq!(pairs.len(), 2);
}
