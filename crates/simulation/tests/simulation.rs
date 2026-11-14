use std::f64::consts::PI;

use rapier2d::prelude::{RigidBody, Rotation, Vector};
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullParameters,
    SharedSimulation, Simulation, flat_plate_force, hull_resistance_force, rudder_foil_force,
    streamlined_foil_force,
};

fn reference_parameters() -> BoatParameters {
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
        air_density_kg_m3: 1.225,
        sail_area_m2: 8.0,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 1025.0,
        keel_area_m2: 0.6,
        keel_position_m: Vector::ZERO,
        keel_base_drag_coefficient: 0.02,
        keel_stall_angle_rad: 15.0_f64.to_radians(),
        keel_stall_lift_coefficient: 0.8,
        hull_reference_area_m2: 0.3,
        hull_drag_coefficient: 0.08,
        hull_lateral_drag_coefficient: 0.02,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        max_sail_actuator_torque_n_m: 375.0,
        max_tiller_hand_force_n: 60.0,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: PI / 6.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
        draft_m: 0.6,
        ground_static_friction_coefficient: 1.0,
        ground_kinetic_friction_coefficient: 0.6,
    }
}

fn vacuum_parameters() -> BoatParameters {
    let mut parameters = reference_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    parameters
}

#[test]
fn zero_bearing_friction_allows_a_free_rudder_to_accelerate() {
    let mut parameters = vacuum_parameters();
    parameters.rudder_bearing_damping_n_m_s_rad = 0.0;
    let initial = BoatMotion {
        velocity_m_s: Vector::ZERO,
        angular_velocity_rad_s: 0.0,
        angle_rad: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
        ..motion()
    };
    let mut simulation = Simulation::new(parameters, initial);
    simulation.advance(
        Environment::default(),
        ActuatorTorques {
            sail_n_m: 0.0,
            rudder_n_m: 1.0,
        },
    );
    assert!(simulation.motion().rudder_angular_velocity_rad_s > 0.01);
}

#[test]
fn gripping_at_the_rudder_hinge_cannot_turn_it() {
    let mut parameters = vacuum_parameters();
    parameters.tiller_length_m = 0.0;
    let initial = BoatMotion {
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        ..motion()
    };
    let mut simulation = Simulation::new(parameters, initial);
    for _ in 0..30 {
        simulation.advance_targets(
            Environment::default(),
            rudder_targets(parameters.max_rudder_angle_rad),
        );
    }
    assert!(simulation.motion().rudder_angle_rad.abs() < 1.0e-6);
}

fn rudder_targets(angle_rad: f64) -> simulation::SurfaceTargets {
    simulation::SurfaceTargets {
        sheet_target_angle_rad: 0.0,
        sail_target_angle_rad: Some(0.0),
        rudder_target_angle_rad: angle_rad,
    }
}

#[test]
fn tiller_force_and_leverage_bound_the_delivered_angular_impulse() {
    for (force_n, length_m) in [(60.0, 1.0), (30.0, 1.0), (60.0, 0.5)] {
        let mut parameters = vacuum_parameters();
        parameters.max_tiller_hand_force_n = force_n;
        parameters.tiller_length_m = length_m;
        let initial = BoatMotion {
            velocity_m_s: Vector::ZERO,
            angle_rad: 0.0,
            ..motion()
        };
        let mut simulation = Simulation::new(parameters, initial);
        simulation.advance_targets(
            Environment::default(),
            rudder_targets(parameters.max_rudder_angle_rad),
        );
        let rudder = &simulation.world.bodies[simulation.boat.rudder];
        let angular_impulse = rudder.mass_properties().local_mprops.principal_inertia()
            * rudder.angvel()
            + (rudder.translation() - parameters.rudder_position_m)
                .perp_dot(rudder.linvel() * rudder.mass());
        let limit = force_n * length_m * DT_SECONDS;
        assert!(
            angular_impulse > limit * 0.95,
            "{force_n}/{length_m}: {angular_impulse} vs {limit}"
        );
        assert!(
            angular_impulse <= limit * 1.001,
            "{force_n}/{length_m}: {angular_impulse} vs {limit}"
        );
        assert!(angular_momentum(&simulation).abs() < 1.0e-4);
        assert!(linear_momentum(&simulation).length() < 1.0e-4);
    }
}

#[test]
fn zero_hand_force_leaves_the_rudder_passive() {
    let mut parameters = vacuum_parameters();
    parameters.max_tiller_hand_force_n = 0.0;
    let initial = BoatMotion {
        rudder_angle_rad: 0.1,
        rudder_angular_velocity_rad_s: 0.2,
        ..motion()
    };
    let mut controlled = Simulation::new(parameters, initial);
    let mut passive = Simulation::new(parameters, initial);
    for _ in 0..60 {
        controlled.advance_targets(Environment::default(), rudder_targets(-0.3));
        passive.advance(Environment::default(), ActuatorTorques::default());
        assert_close(
            controlled.motion().rudder_angle_rad,
            passive.motion().rudder_angle_rad,
        );
        assert_close(
            controlled.motion().rudder_angular_velocity_rad_s,
            passive.motion().rudder_angular_velocity_rad_s,
        );
    }
}

#[test]
fn shared_target_actuation_matches_single_boat_physics_and_disables_for_wrecks() {
    let parameters = vacuum_parameters();
    let initial = BoatMotion {
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        ..motion()
    };
    let mut single = Simulation::new(parameters, initial);
    let mut shared = SharedSimulation::new();
    shared.add_boat(parameters, initial);
    for _ in 0..3 {
        single.advance_targets(Environment::default(), rudder_targets(0.3));
        shared.advance_controlled_with_environments(
            &[Environment::default()],
            &[Some(rudder_targets(0.3))],
        );
        assert_close(
            single.motion().rudder_angle_rad,
            shared.motion(0).rudder_angle_rad,
        );
        assert_close(
            single.motion().rudder_angular_velocity_rad_s,
            shared.motion(0).rudder_angular_velocity_rad_s,
        );
    }
    let mut passive = Simulation::new(parameters, single.motion());
    let mut raw = Simulation::new(parameters, single.motion());
    for _ in 0..30 {
        let torque = ActuatorTorques {
            sail_n_m: 0.0,
            rudder_n_m: -15.0,
        };
        single.advance(Environment::default(), torque);
        raw.advance(Environment::default(), torque);
        shared.advance_controlled_with_environments(&[Environment::default()], &[None]);
        passive.advance(Environment::default(), ActuatorTorques::default());
        assert_close(
            single.motion().rudder_angle_rad,
            raw.motion().rudder_angle_rad,
        );
        assert_close(
            single.motion().rudder_angular_velocity_rad_s,
            raw.motion().rudder_angular_velocity_rad_s,
        );
        assert_close(
            shared.motion(0).rudder_angle_rad,
            passive.motion().rudder_angle_rad,
        );
        assert_close(
            shared.motion(0).rudder_angular_velocity_rad_s,
            passive.motion().rudder_angular_velocity_rad_s,
        );
    }
    assert!(single.motion().rudder_angle_rad < passive.motion().rudder_angle_rad);
}

fn motion() -> BoatMotion {
    BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::new(2.0, -1.0),
        angle_rad: 0.7,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
}

fn world_point(body: &RigidBody, local_point: Vector) -> Vector {
    body.translation() + body.rotation().transform_vector(local_point)
}

fn angular_momentum(simulation: &Simulation) -> f64 {
    simulation
        .world
        .bodies
        .iter()
        .map(|(_, body)| {
            body.mass_properties().local_mprops.principal_inertia() * body.angvel()
                + body.translation().perp_dot(body.linvel() * body.mass())
        })
        .sum()
}

fn linear_momentum(simulation: &Simulation) -> Vector {
    simulation
        .world
        .bodies
        .iter()
        .map(|(_, body)| body.linvel() * body.mass())
        .sum()
}

fn kinetic_energy(simulation: &Simulation) -> f64 {
    simulation
        .world
        .bodies
        .iter()
        .map(|(_, body)| {
            0.5 * body.mass() * body.linvel().length_squared()
                + 0.5
                    * body.mass_properties().local_mprops.principal_inertia()
                    * body.angvel().powi(2)
        })
        .sum()
}

#[test]
fn fixed_ticks_preserve_uniform_motion_and_connected_hinges() {
    let parameters = vacuum_parameters();
    let mut simulation = Simulation::new(parameters, motion());

    assert_eq!(simulation.world.integration_parameters.dt, DT_SECONDS);
    assert_eq!(simulation.world.gravity, Vector::ZERO);
    for _ in 0..60 {
        simulation.advance(Environment::default(), ActuatorTorques::default());
    }

    let result = simulation.motion();
    assert_vector_close(result.position_m, Vector::new(2.0, -1.0));
    assert_vector_close(result.velocity_m_s, Vector::new(2.0, -1.0));
    assert_close(result.angle_rad, 0.7);
    assert_close(result.angular_velocity_rad_s, 0.0);
    assert_close(result.sail_angle_rad, 0.0);
    assert_close(result.rudder_angle_rad, 0.0);

    let hull = &simulation.world.bodies[simulation.boat.hull.body];
    let sail = &simulation.world.bodies[simulation.boat.sail];
    let rudder = &simulation.world.bodies[simulation.boat.rudder];
    assert_vector_close(
        world_point(hull, parameters.mast_position_m),
        world_point(sail, Vector::new(9.0 / 11.0, 0.0)),
    );
    assert_vector_close(
        world_point(hull, parameters.rudder_position_m),
        rudder.translation(),
    );
}

#[test]
fn shared_simulation_advances_multiple_boats_in_one_world() {
    let parameters = vacuum_parameters();
    let mut first = motion();
    first.position_m = Vector::new(0.0, -10.0);
    let mut second = motion();
    second.position_m = Vector::new(0.0, 4.0);
    second.velocity_m_s = Vector::new(-1.0, 0.5);
    let mut simulation = SharedSimulation::new();
    let first_boat = simulation.add_boat(parameters, first);
    let second_boat = simulation.add_boat(parameters, second);

    assert_eq!(simulation.world.bodies.len(), 6);
    assert_eq!(simulation.world.colliders.len(), 2);
    for _ in 0..60 {
        simulation.advance(
            Environment::default(),
            &[ActuatorTorques::default(), ActuatorTorques::default()],
        );
    }

    let first_result = simulation.motion(first_boat);
    let second_result = simulation.motion(second_boat);
    assert_vector_close(first_result.position_m, Vector::new(2.0, -11.0));
    assert_vector_close(first_result.velocity_m_s, first.velocity_m_s);
    assert_vector_close(second_result.position_m, Vector::new(-1.0, 4.5));
    assert_vector_close(second_result.velocity_m_s, second.velocity_m_s);
}

#[test]
fn shared_simulation_applies_a_distinct_environment_to_each_boat() {
    let parameters = reference_parameters();
    let mut first = motion();
    first.position_m = Vector::new(0.0, -10.0);
    first.velocity_m_s = Vector::ZERO;
    first.angle_rad = 0.0;
    let mut second = first;
    second.position_m = Vector::new(0.0, 10.0);
    let mut simulation = SharedSimulation::new();
    let first_boat = simulation.add_boat(parameters, first);
    let second_boat = simulation.add_boat(parameters, second);

    simulation.advance_with_environments(
        &[
            Environment {
                true_wind_velocity_m_s: Vector::new(6.0, 0.0),
                true_water_velocity_m_s: Vector::ZERO,
                ..Default::default()
            },
            Environment {
                true_wind_velocity_m_s: Vector::new(-6.0, 0.0),
                true_water_velocity_m_s: Vector::ZERO,
                ..Default::default()
            },
        ],
        &[ActuatorTorques::default(), ActuatorTorques::default()],
    );

    assert!(simulation.motion(first_boat).velocity_m_s.x > 0.0);
    assert!(simulation.motion(second_boat).velocity_m_s.x < 0.0);
}

#[test]
fn lost_bottom_clearance_slows_the_boat_and_exposed_ground_stops_it() {
    fn speed_after_tick(water_depth_m: f64) -> f64 {
        let parameters = vacuum_parameters();
        let mut initial = motion();
        initial.velocity_m_s = Vector::new(2.0, 0.0);
        initial.angle_rad = 0.0;
        let mut simulation = Simulation::new(parameters, initial);
        simulation.advance(
            Environment {
                water_depth_m,
                ..Default::default()
            },
            ActuatorTorques::default(),
        );
        simulation.motion().velocity_m_s.length()
    }

    let deep_speed_m_s = speed_after_tick(1.5);
    let shallow_speed_m_s = speed_after_tick(0.3);
    let grounded_speed_m_s = speed_after_tick(0.0);

    assert_close(deep_speed_m_s, 2.0);
    assert!(grounded_speed_m_s < shallow_speed_m_s);
    assert!(shallow_speed_m_s < deep_speed_m_s);

    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::new(2.0, 0.0);
    initial.angle_rad = 0.0;
    initial.angular_velocity_rad_s = 1.0;
    let mut grounded = Simulation::new(parameters, initial);
    for _ in 0..120 {
        grounded.advance(
            Environment {
                water_depth_m: 0.0,
                ..Default::default()
            },
            ActuatorTorques::default(),
        );
    }

    assert!(grounded.motion().velocity_m_s.length() < 0.01);
    assert!(grounded.motion().angular_velocity_rad_s.abs() < 0.01);
}

#[test]
fn exposed_ground_prevents_creep_in_strong_wind() {
    let parameters = reference_parameters();
    let mut initial = motion();
    initial.position_m = Vector::new(12.0, -7.0);
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.angular_velocity_rad_s = 0.0;
    initial.sail_angle_rad = PI / 4.0;
    let mut grounded = Simulation::new(parameters, initial);
    let strong_wind_environment = Environment {
        true_wind_velocity_m_s: Vector::new(15.0, 0.0),
        water_depth_m: 0.0,
        ..Default::default()
    };

    for _ in 0..600 {
        grounded.advance(strong_wind_environment, ActuatorTorques::default());
    }

    assert_vector_close(grounded.motion().position_m, initial.position_m);
    assert_close(grounded.motion().angle_rad, initial.angle_rad);
    assert_eq!(grounded.motion().velocity_m_s, Vector::ZERO);
    assert_eq!(grounded.motion().angular_velocity_rad_s, 0.0);

    let mut extreme_wind = Simulation::new(parameters, initial);
    let extreme_wind_environment = Environment {
        true_wind_velocity_m_s: Vector::new(40.0, 0.0),
        water_depth_m: 0.0,
        ..Default::default()
    };
    for _ in 0..60 {
        extreme_wind.advance(extreme_wind_environment, ActuatorTorques::default());
    }
    assert!((extreme_wind.motion().position_m - initial.position_m).length() > 0.1);
}

#[test]
fn shared_simulation_removes_an_articulated_boat_and_compacts_indices() {
    let parameters = vacuum_parameters();
    let mut first = motion();
    first.position_m = Vector::new(0.0, -10.0);
    let mut removed = motion();
    removed.position_m = Vector::ZERO;
    let mut third = motion();
    third.position_m = Vector::new(0.0, 10.0);
    let mut simulation = SharedSimulation::new();
    simulation.add_boat(parameters, first);
    simulation.add_boat(parameters, removed);
    simulation.add_boat(parameters, third);

    simulation.remove_boat(1);

    assert_eq!(simulation.world.bodies.len(), 6);
    assert_eq!(simulation.world.colliders.len(), 2);
    assert_vector_close(simulation.motion(0).position_m, first.position_m);
    assert_vector_close(simulation.motion(1).position_m, third.position_m);
    simulation.advance(
        Environment::default(),
        &[ActuatorTorques::default(), ActuatorTorques::default()],
    );
    simulation.remove_boat(1);
    simulation.remove_boat(0);
    assert!(simulation.world.impulse_joints.is_empty());
}

#[test]
fn shared_simulation_queries_boats_by_exact_radius() {
    let parameters = vacuum_parameters();
    let positions = [
        Vector::ZERO,
        Vector::new(3.0, 4.0),
        Vector::new(5.0, 5.0),
        Vector::new(20.0, 0.0),
    ];
    let mut simulation = SharedSimulation::new();
    for position_m in positions {
        let mut initial = motion();
        initial.position_m = position_m;
        initial.velocity_m_s = Vector::ZERO;
        simulation.add_boat(parameters, initial);
    }
    simulation.advance(
        Environment::default(),
        &vec![ActuatorTorques::default(); positions.len()],
    );

    assert_eq!(
        simulation.boats_within_radius(Vector::ZERO, 5.1),
        vec![0, 1]
    );
}

#[test]
fn shared_simulation_resolves_and_reports_hull_collisions() {
    let parameters = vacuum_parameters();
    let mut port = motion();
    port.position_m = Vector::new(-2.5, 0.0);
    port.velocity_m_s = Vector::new(5.0, 0.0);
    port.angle_rad = 0.0;
    let mut starboard = port;
    starboard.position_m = Vector::new(2.5, 0.0);
    starboard.velocity_m_s = Vector::new(-5.0, 0.0);
    let mut simulation = SharedSimulation::new();
    let port_boat = simulation.add_boat(parameters, port);
    let starboard_boat = simulation.add_boat(parameters, starboard);

    let mut contact = None;
    for _ in 0..30 {
        contact = simulation
            .advance(
                Environment::default(),
                &[ActuatorTorques::default(), ActuatorTorques::default()],
            )
            .into_iter()
            .find(|contact| contact.impulse_magnitude_n_s > 0.0)
            .or(contact);
    }

    let contact = contact.expect("the converging hulls should collide");
    let mut boats = [contact.first_boat, contact.second_boat];
    boats.sort();
    assert_eq!(boats, [port_boat, starboard_boat]);
    let port_contact_m = if contact.first_boat == port_boat {
        contact.first_contact_local_m
    } else {
        contact.second_contact_local_m
    };
    let starboard_contact_m = if contact.first_boat == starboard_boat {
        contact.first_contact_local_m
    } else {
        contact.second_contact_local_m
    };
    assert!(port_contact_m.x > 1.0);
    assert!(starboard_contact_m.x < -1.0);
    assert!(simulation.motion(port_boat).velocity_m_s.x < port.velocity_m_s.x);
    assert!(simulation.motion(starboard_boat).velocity_m_s.x > starboard.velocity_m_s.x);
}

#[test]
fn sail_joint_stops_outward_motion_at_its_limit() {
    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.sail_angular_velocity_rad_s = 4.0;
    let mut simulation = Simulation::new(parameters, initial);

    for _ in 0..120 {
        simulation.advance(Environment::default(), ActuatorTorques::default());
    }

    let sail_angle_rad = simulation.motion().sail_angle_rad;
    assert!(sail_angle_rad > 0.5);
    assert!(sail_angle_rad <= parameters.max_sail_angle_rad + 1.0e-3);
}

#[test]
fn actuator_torques_rotate_surfaces_and_conserve_angular_momentum() {
    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(
        Environment::default(),
        ActuatorTorques {
            sail_n_m: 100.0,
            rudder_n_m: 20.0,
        },
    );

    let result = simulation.motion();
    assert!(result.sail_angular_velocity_rad_s > 0.0);
    assert!(result.rudder_angular_velocity_rad_s > 0.0);
    assert!(result.angular_velocity_rad_s < 0.0);
    assert!(angular_momentum(&simulation).abs() < 1.0e-4);
}

#[test]
fn actuator_torques_cannot_drive_surfaces_past_joint_limits() {
    let loaded_limit_tolerance_rad = 1.0e-3;
    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    let mut simulation = Simulation::new(parameters, initial);

    for _ in 0..240 {
        simulation.advance(
            Environment::default(),
            ActuatorTorques {
                sail_n_m: 100.0,
                rudder_n_m: 20.0,
            },
        );
    }

    let result = simulation.motion();
    assert!(result.sail_angle_rad > 0.5);
    assert!(
        result.sail_angle_rad <= parameters.max_sail_angle_rad + loaded_limit_tolerance_rad,
        "{} > {}",
        result.sail_angle_rad,
        parameters.max_sail_angle_rad
    );
    assert!(result.rudder_angle_rad > 0.1);
    assert!(
        result.rudder_angle_rad <= parameters.max_rudder_angle_rad + loaded_limit_tolerance_rad,
        "{} > {}",
        result.rudder_angle_rad,
        parameters.max_rudder_angle_rad
    );
}

#[test]
fn bearing_damping_reduces_relative_motion_and_conserves_angular_momentum() {
    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.sail_angular_velocity_rad_s = 1.0;
    initial.rudder_angular_velocity_rad_s = -1.0;
    let mut simulation = Simulation::new(parameters, initial);
    let initial_energy = kinetic_energy(&simulation);
    let initial_angular_momentum = angular_momentum(&simulation);

    for _ in 0..120 {
        simulation.advance(Environment::default(), ActuatorTorques::default());
    }

    let result = simulation.motion();
    assert!(
        result.sail_angular_velocity_rad_s.abs() < 0.5,
        "sail: {}",
        result.sail_angular_velocity_rad_s
    );
    assert!(
        result.rudder_angular_velocity_rad_s.abs() < 0.5,
        "rudder: {}",
        result.rudder_angular_velocity_rad_s
    );
    let final_energy = kinetic_energy(&simulation);
    assert!(
        final_energy < initial_energy * 0.5,
        "{final_energy} >= {initial_energy} * 0.5"
    );
    let final_angular_momentum = angular_momentum(&simulation);
    assert!(
        (final_angular_momentum - initial_angular_momentum).abs()
            < initial_angular_momentum.abs() * 1.0e-3,
        "{final_angular_momentum} != {initial_angular_momentum}"
    );
}

#[test]
fn bearing_damping_does_not_resist_common_rotation() {
    let parameters = vacuum_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.angular_velocity_rad_s = 1.0;
    let mut simulation = Simulation::new(parameters, initial);

    for _ in 0..60 {
        simulation.advance(Environment::default(), ActuatorTorques::default());
    }

    let result = simulation.motion();
    assert!((result.angular_velocity_rad_s - 1.0).abs() < 1.0e-3);
    assert!(result.sail_angular_velocity_rad_s.abs() < 1.0e-3);
    assert!(result.rudder_angular_velocity_rad_s.abs() < 1.0e-3);
}

#[test]
fn sail_load_applies_the_expected_linear_and_angular_impulse() {
    let parameters = reference_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.sail_angle_rad = PI / 4.0;
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        ..Default::default()
    };
    let expected_force_n = flat_plate_force(
        environment.true_wind_velocity_m_s,
        initial.angle_rad + initial.sail_angle_rad,
        parameters.air_density_kg_m3,
        parameters.sail_area_m2,
        parameters.sail_base_drag_coefficient,
    );
    let mast_position_m = initial.position_m
        + Rotation::new(initial.angle_rad).transform_vector(parameters.mast_position_m);
    let center_of_effort_m = mast_position_m
        + Rotation::new(initial.angle_rad + initial.sail_angle_rad)
            .transform_vector(Vector::new(-(parameters.boom_length_m / 3.0), 0.0));
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(environment, ActuatorTorques::default());

    assert_vector_close(linear_momentum(&simulation), expected_force_n * DT_SECONDS);
    assert_close(
        angular_momentum(&simulation),
        center_of_effort_m.perp_dot(expected_force_n) * DT_SECONDS,
    );
}

#[test]
fn rudder_steady_load_tracks_whole_blade_aspect_ratio() {
    for (chord, area) in [(0.45_f64, 0.12_f64), (0.9, 0.48), (0.45, 0.24)] {
        let mut parameters = reference_parameters();
        parameters.rudder_draw_length_m = chord;
        parameters.rudder_area_m2 = area;
        let flow = Vector::new(2.0, 0.0);
        parameters.air_density_kg_m3 = 0.0;
        // Weak loading isolates steady force before the blade starts moving.
        parameters.water_density_kg_m3 = 0.0001;
        for angle_deg in [-30_i32, -15, -1, 1, 15, 30] {
            let angle = (angle_deg as f64).to_radians();
            let mut simulation = Simulation::new(
                parameters,
                BoatMotion {
                    position_m: Vector::ZERO,
                    velocity_m_s: Vector::ZERO,
                    angle_rad: 0.0,
                    angular_velocity_rad_s: 0.0,
                    sail_angle_rad: 0.0,
                    sail_angular_velocity_rad_s: 0.0,
                    rudder_angle_rad: angle,
                    rudder_angular_velocity_rad_s: 0.0,
                },
            );
            let initial_center = simulation.world.bodies[simulation.boat.rudder].center_of_mass();
            simulation.advance(
                Environment {
                    true_water_velocity_m_s: flow,
                    ..Default::default()
                },
                ActuatorTorques::default(),
            );
            let rudder = &simulation.world.bodies[simulation.boat.rudder];
            let force = rudder.user_force();
            let torque = rudder.user_torque() + initial_center.perp_dot(force);
            let aspect_ratio = area / chord.powi(2);
            let slope = 6.13 * aspect_ratio / (aspect_ratio + 2.25);
            let normal = Vector::new(-angle.sin(), angle.cos());
            let expected_force = 0.5
                * parameters.water_density_kg_m3
                * area
                * flow.length()
                * (parameters.rudder_base_drag_coefficient * flow
                    + slope * flow.dot(normal) * normal);
            let center =
                parameters.rudder_position_m - Vector::new(angle.cos(), angle.sin()) * chord / 2.0;
            assert!((force - expected_force).length() < expected_force.length() * 1.0e-6);
            let expected_torque = center.perp_dot(expected_force);
            assert!((torque - expected_torque).abs() < expected_torque.abs() * 1.0e-6);
        }
    }
}

#[test]
fn rotating_foils_resist_still_fluid_with_their_integrated_area_moments() {
    for foil in ["rudder", "sail", "keel"] {
        let mut parameters = vacuum_parameters();
        parameters.sail_bearing_damping_n_m_s_rad = 0.0;
        parameters.rudder_bearing_damping_n_m_s_rad = 0.0;
        parameters.hull_reference_area_m2 = 0.0;
        let mut initial = motion();
        initial.position_m = Vector::ZERO;
        initial.velocity_m_s = Vector::ZERO;
        initial.angle_rad = 0.0;
        initial.sail_angle_rad = 0.0;
        initial.rudder_angle_rad = 0.0;
        let omega = 0.001_f64;
        let (area, third_area_moment, hinge, body_index) = match foil {
            "rudder" => {
                parameters.water_density_kg_m3 = 1.0;
                parameters.keel_area_m2 = 0.0;
                initial.rudder_angular_velocity_rad_s = omega;
                (
                    parameters.rudder_area_m2,
                    parameters.rudder_draw_length_m.powi(3) / 4.0,
                    parameters.rudder_position_m,
                    2,
                )
            }
            "sail" => {
                parameters.air_density_kg_m3 = 1.0;
                initial.sail_angular_velocity_rad_s = omega;
                (
                    parameters.sail_area_m2,
                    parameters.boom_length_m.powi(3) / 10.0,
                    parameters.mast_position_m,
                    1,
                )
            }
            _ => {
                parameters.water_density_kg_m3 = 1.0;
                parameters.rudder_area_m2 = 0.0;
                initial.angular_velocity_rad_s = omega;
                let chord = parameters.keel_area_m2 / parameters.draft_m;
                (
                    parameters.keel_area_m2,
                    chord.powi(3) / 32.0,
                    parameters.keel_position_m,
                    0,
                )
            }
        };
        let mut simulation = Simulation::new(parameters, initial);
        let handle = [
            simulation.boat.hull.body,
            simulation.boat.sail,
            simulation.boat.rudder,
        ][body_index];
        let lever = simulation.world.bodies[handle].center_of_mass() - hinge;
        simulation.advance(Environment::default(), ActuatorTorques::default());
        let body = &simulation.world.bodies[handle];
        let torque = body.user_torque() + lever.perp_dot(body.user_force());
        let broadside_drag = if foil == "rudder" {
            let aspect_ratio = area / parameters.rudder_draw_length_m.powi(2);
            0.02 + 6.13 * aspect_ratio / (aspect_ratio + 2.25)
        } else {
            2.02
        };
        // Integrating r|r|r over the planform gives the resisting hinge moment.
        let expected = -0.5 * broadside_drag * area * omega.powi(2) * third_area_moment;
        assert!(
            (torque - expected).abs() < expected.abs() * 0.002,
            "{foil}: torque {torque}, integrated moment {expected}"
        );
    }
}

#[test]
fn tick_torques_replace_previous_tick_loads() {
    let mut simulation = Simulation::new(vacuum_parameters(), motion());

    simulation.advance(
        Environment::default(),
        ActuatorTorques {
            sail_n_m: 100.0,
            rudder_n_m: 20.0,
        },
    );
    simulation.advance(Environment::default(), ActuatorTorques::default());

    assert_eq!(
        simulation.world.bodies[simulation.boat.sail].user_torque(),
        0.0
    );
    assert_eq!(
        simulation.world.bodies[simulation.boat.rudder].user_torque(),
        0.0
    );
    assert_eq!(
        simulation.world.bodies[simulation.boat.hull.body].user_torque(),
        0.0
    );
}

#[test]
fn rudder_load_applies_the_expected_linear_and_angular_impulse() {
    let mut parameters = reference_parameters();
    parameters.air_density_kg_m3 = 0.0;
    // Weak loading makes the uniform-flow, first-order impulse measurable
    // before rotation and translation appreciably change the sampled flow.
    parameters.water_density_kg_m3 = 1.0;
    parameters.keel_area_m2 = 0.0;
    parameters.hull_reference_area_m2 = 0.0;
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.rudder_angle_rad = 5.0_f64.to_radians();
    let environment = Environment {
        true_water_velocity_m_s: Vector::new(2.0, 0.0),
        ..Default::default()
    };
    let expected_force_n = rudder_foil_force(
        environment.true_water_velocity_m_s,
        initial.angle_rad + initial.rudder_angle_rad,
        parameters.water_density_kg_m3,
        parameters.rudder_area_m2,
        parameters.rudder_area_m2,
        parameters.rudder_draw_length_m,
        parameters.rudder_base_drag_coefficient,
    );
    let hinge_position_m = initial.position_m
        + Rotation::new(initial.angle_rad).transform_vector(parameters.rudder_position_m);
    let center_of_effort_m = hinge_position_m
        + Rotation::new(initial.angle_rad + initial.rudder_angle_rad)
            .transform_vector(Vector::new(-parameters.rudder_draw_length_m / 2.0, 0.0));
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(environment, ActuatorTorques::default());

    let expected_impulse = expected_force_n * DT_SECONDS;
    assert!(
        (linear_momentum(&simulation) - expected_impulse).length()
            < expected_impulse.length() * 0.001
    );
    let expected_angular_impulse = center_of_effort_m.perp_dot(expected_impulse);
    assert!(
        (angular_momentum(&simulation) - expected_angular_impulse).abs()
            < expected_angular_impulse.abs() * 0.001
    );
}

#[test]
fn keel_load_applies_the_expected_linear_and_angular_impulse() {
    let mut parameters = reference_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.rudder_area_m2 = 0.0;
    parameters.hull_reference_area_m2 = 0.0;
    parameters.keel_position_m = Vector::new(-1.0, 0.0);
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 5.0_f64.to_radians();
    let environment = Environment {
        true_water_velocity_m_s: Vector::new(2.0, 0.0),
        ..Default::default()
    };
    let expected_force_n = streamlined_foil_force(
        environment.true_water_velocity_m_s,
        initial.angle_rad,
        parameters.water_density_kg_m3,
        parameters.keel_area_m2,
        parameters.keel_base_drag_coefficient,
        parameters.keel_stall_angle_rad,
        parameters.keel_stall_lift_coefficient,
    );
    let center_of_effort_m = initial.position_m
        + Rotation::new(initial.angle_rad).transform_vector(parameters.keel_position_m);
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(environment, ActuatorTorques::default());

    assert_vector_close(linear_momentum(&simulation), expected_force_n * DT_SECONDS);
    assert_close(
        angular_momentum(&simulation),
        center_of_effort_m.perp_dot(expected_force_n) * DT_SECONDS,
    );
}

#[test]
fn bow_and_stern_hull_resistance_apply_the_expected_force_and_yaw_torque() {
    let mut parameters = reference_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.keel_area_m2 = 0.0;
    parameters.rudder_area_m2 = 0.0;
    let mut initial = motion();
    initial.position_m = Vector::ZERO;
    initial.velocity_m_s = Vector::new(2.0, 1.0);
    initial.angle_rad = 0.0;
    initial.angular_velocity_rad_s = 0.5;
    let half_length_m = parameters.hull.length_m / 2.0;
    let bow_position_m = Vector::new(half_length_m, 0.0);
    let stern_position_m = Vector::new(-half_length_m, 0.0);
    let bow_velocity_m_s =
        initial.velocity_m_s + Vector::new(0.0, half_length_m * initial.angular_velocity_rad_s);
    let stern_velocity_m_s =
        initial.velocity_m_s - Vector::new(0.0, half_length_m * initial.angular_velocity_rad_s);
    let half_reference_area_m2 = parameters.hull_reference_area_m2 / 2.0;
    let bow_force_n = hull_resistance_force(
        -bow_velocity_m_s,
        initial.angle_rad,
        parameters.water_density_kg_m3,
        half_reference_area_m2,
        parameters.hull_drag_coefficient,
        parameters.hull_lateral_drag_coefficient,
    );
    let stern_force_n = hull_resistance_force(
        -stern_velocity_m_s,
        initial.angle_rad,
        parameters.water_density_kg_m3,
        half_reference_area_m2,
        parameters.hull_drag_coefficient,
        parameters.hull_lateral_drag_coefficient,
    );
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(Environment::default(), ActuatorTorques::default());

    let hull = &simulation.world.bodies[simulation.boat.hull.body];
    assert_vector_close(hull.user_force(), bow_force_n + stern_force_n);
    assert_close(
        hull.user_torque(),
        bow_position_m.perp_dot(bow_force_n) + stern_position_m.perp_dot(stern_force_n),
    );
    assert!(hull.user_torque() * initial.angular_velocity_rad_s < 0.0);
}

#[test]
fn sail_actuation_reaches_its_torque_limit() {
    let parameters = vacuum_parameters();
    let mut simulation = Simulation::new(parameters, motion());
    simulation.advance_targets(
        Environment::default(),
        simulation::SurfaceTargets {
            sail_target_angle_rad: Some(parameters.max_sail_angle_rad),
            sheet_target_angle_rad: 0.0,
            rudder_target_angle_rad: 0.0,
        },
    );
    assert_eq!(
        simulation.world.bodies[simulation.boat.sail].user_torque(),
        parameters.max_sail_actuator_torque_n_m
    );
}

#[test]
fn target_actuation_reduces_hinge_motion_at_the_target() {
    let parameters = vacuum_parameters();
    let mut current = motion();
    current.sail_angle_rad = 0.25;
    current.sail_angular_velocity_rad_s = 0.1;
    current.rudder_angle_rad = -0.1;
    current.rudder_angular_velocity_rad_s = -0.1;
    let mut simulation = Simulation::new(parameters, current);
    simulation.advance_targets(
        Environment::default(),
        simulation::SurfaceTargets {
            sail_target_angle_rad: Some(current.sail_angle_rad),
            sheet_target_angle_rad: 0.0,
            rudder_target_angle_rad: current.rudder_angle_rad,
        },
    );
    let next = simulation.motion();
    assert!(next.sail_angular_velocity_rad_s.abs() < current.sail_angular_velocity_rad_s.abs());
    assert!(next.rudder_angular_velocity_rad_s.abs() < current.rudder_angular_velocity_rad_s.abs());
}

#[test]
fn sheet_control_never_pushes_the_sail_outward() {
    let parameters = vacuum_parameters();
    for (angle_rad, outward_target_rad) in [(0.25, 0.5), (-0.25, -0.5), (0.0, 0.5), (0.5, 0.25)] {
        let mut current = motion();
        current.sail_angle_rad = angle_rad;
        let mut simulation = Simulation::new(parameters, current);
        simulation.advance_targets(
            Environment::default(),
            simulation::SurfaceTargets {
                sail_target_angle_rad: None,
                sheet_target_angle_rad: outward_target_rad,
                rudder_target_angle_rad: 0.2,
            },
        );
        let torque = simulation.world.bodies[simulation.boat.sail].user_torque();
        if angle_rad == 0.5 {
            assert!(torque < 0.0);
        } else {
            assert_eq!(torque, 0.0);
        }
        assert_ne!(
            simulation.motion().rudder_angle_rad,
            current.rudder_angle_rad
        );
    }
}

#[test]
fn surface_targets_choose_sheet_limited_or_explicit_sail_control() {
    let parameters = vacuum_parameters();
    let mut current = motion();
    current.sail_angle_rad = 0.25;
    current.sail_angular_velocity_rad_s = 0.0;
    let sheet_targets = simulation::SurfaceTargets {
        sheet_target_angle_rad: 0.5,
        rudder_target_angle_rad: 0.2,
        sail_target_angle_rad: None,
    };
    let explicit_targets = simulation::SurfaceTargets {
        sail_target_angle_rad: Some(0.5),
        ..sheet_targets
    };

    let mut sheet = Simulation::new(parameters, current);
    let mut explicit = Simulation::new(parameters, current);
    sheet.advance_targets(Environment::default(), sheet_targets);
    explicit.advance_targets(Environment::default(), explicit_targets);
    assert_eq!(sheet.world.bodies[sheet.boat.sail].user_torque(), 0.0);
    assert!(explicit.world.bodies[explicit.boat.sail].user_torque() > 0.0);
    assert!(sheet.motion().rudder_angle_rad > current.rudder_angle_rad);
    assert!(explicit.motion().rudder_angle_rad > current.rudder_angle_rad);
}

#[test]
fn mirrored_surface_targets_produce_mirrored_motion() {
    let parameters = reference_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        true_water_velocity_m_s: Vector::ZERO,
        ..Default::default()
    };
    let mut port_motion = motion();
    port_motion.velocity_m_s = Vector::ZERO;
    port_motion.angle_rad = 135.0_f64.to_radians();
    for _ in 0..6 * 60 {
        let mirrored = BoatMotion {
            position_m: Vector::new(port_motion.position_m.x, -port_motion.position_m.y),
            velocity_m_s: Vector::new(port_motion.velocity_m_s.x, -port_motion.velocity_m_s.y),
            angle_rad: -port_motion.angle_rad,
            angular_velocity_rad_s: -port_motion.angular_velocity_rad_s,
            sail_angle_rad: -port_motion.sail_angle_rad,
            sail_angular_velocity_rad_s: -port_motion.sail_angular_velocity_rad_s,
            rudder_angle_rad: -port_motion.rudder_angle_rad,
            rudder_angular_velocity_rad_s: -port_motion.rudder_angular_velocity_rad_s,
        };
        let mut port = Simulation::new(parameters, port_motion);
        let mut starboard = Simulation::new(parameters, mirrored);
        port.advance_targets(
            environment,
            simulation::SurfaceTargets {
                sheet_target_angle_rad: -0.4,
                rudder_target_angle_rad: -0.25,
                sail_target_angle_rad: None,
            },
        );
        starboard.advance_targets(
            environment,
            simulation::SurfaceTargets {
                sheet_target_angle_rad: 0.4,
                rudder_target_angle_rad: 0.25,
                sail_target_angle_rad: None,
            },
        );
        let port = port.motion();
        let starboard = starboard.motion();
        assert_close(port.position_m.x, starboard.position_m.x);
        assert_close(port.position_m.y, -starboard.position_m.y);
        assert_close(port.velocity_m_s.x, starboard.velocity_m_s.x);
        assert_close(port.velocity_m_s.y, -starboard.velocity_m_s.y);
        assert_close(port.angle_rad, -starboard.angle_rad);
        assert_close(
            port.angular_velocity_rad_s,
            -starboard.angular_velocity_rad_s,
        );
        port_motion = port;
    }
}
