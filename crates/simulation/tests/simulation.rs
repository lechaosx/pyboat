use std::f32::consts::PI;

use pyboat_simulation::{
    ActuatorTorques, AutoSheetIntent, BoatMotion, BoatParameters, ControlInput, DT_SECONDS,
    Environment, HullParameters, MotionIntent, Simulation, SurfaceEffect, auto_sheet_control,
    computed_maximum_yaw_rate_rad_s, flat_plate_force, hull_resistance_force,
    sheet_control_torques, streamlined_foil_force, surface_control_input,
    surface_target_control_torques, target_control_torques, yaw_rate_move_control,
};
use rapier2d::prelude::{RigidBody, Rotation, Vector};

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
        sail_center_of_effort_from_mast_m: 0.7,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 1025.0,
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
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        rudder_stall_angle_rad: 15.0_f32.to_radians(),
        rudder_stall_lift_coefficient: 0.8,
        max_sail_actuator_torque_n_m: 375.0,
        max_rudder_actuator_torque_n_m: 60.0,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: PI / 6.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
    }
}

fn vacuum_parameters() -> BoatParameters {
    let mut parameters = reference_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    parameters
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

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
}

fn world_point(body: &RigidBody, local_point: Vector) -> Vector {
    body.translation() + body.rotation().transform_vector(local_point)
}

fn angular_momentum(simulation: &Simulation) -> f32 {
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

fn kinetic_energy(simulation: &Simulation) -> f32 {
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
        world_point(sail, Vector::new(1.0, 0.0)),
    );
    assert_vector_close(
        world_point(hull, parameters.rudder_position_m),
        rudder.translation(),
    );
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
        + Rotation::new(initial.angle_rad + initial.sail_angle_rad).transform_vector(Vector::new(
            -parameters.sail_center_of_effort_from_mast_m,
            0.0,
        ));
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(environment, ActuatorTorques::default());

    assert_vector_close(linear_momentum(&simulation), expected_force_n * DT_SECONDS);
    assert_close(
        angular_momentum(&simulation),
        center_of_effort_m.perp_dot(expected_force_n) * DT_SECONDS,
    );
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
    parameters.keel_area_m2 = 0.0;
    parameters.hull_reference_area_m2 = 0.0;
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.rudder_angle_rad = 5.0_f32.to_radians();
    let environment = Environment {
        true_water_velocity_m_s: Vector::new(2.0, 0.0),
        ..Default::default()
    };
    let expected_force_n = streamlined_foil_force(
        environment.true_water_velocity_m_s,
        initial.angle_rad + initial.rudder_angle_rad,
        parameters.water_density_kg_m3,
        parameters.rudder_area_m2,
        parameters.rudder_base_drag_coefficient,
        parameters.rudder_stall_angle_rad,
        parameters.rudder_stall_lift_coefficient,
    );
    let hinge_position_m = initial.position_m
        + Rotation::new(initial.angle_rad).transform_vector(parameters.rudder_position_m);
    let center_of_effort_m = hinge_position_m
        + Rotation::new(initial.angle_rad + initial.rudder_angle_rad)
            .transform_vector(Vector::new(-parameters.rudder_draw_length_m / 2.0, 0.0));
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(environment, ActuatorTorques::default());

    assert_vector_close(linear_momentum(&simulation), expected_force_n * DT_SECONDS);
    assert_close(
        angular_momentum(&simulation),
        center_of_effort_m.perp_dot(expected_force_n) * DT_SECONDS,
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
    initial.angle_rad = 5.0_f32.to_radians();
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
fn target_control_torques_reach_and_respect_actuator_limits() {
    let parameters = reference_parameters();
    let mut current = motion();
    current.sail_angle_rad = 0.0;
    current.sail_angular_velocity_rad_s = 0.0;
    current.rudder_angle_rad = 0.0;
    current.rudder_angular_velocity_rad_s = 0.0;

    let torques = target_control_torques(
        parameters,
        current,
        parameters.max_sail_angle_rad,
        -parameters.max_rudder_angle_rad,
    );

    assert_eq!(
        torques,
        ActuatorTorques {
            sail_n_m: parameters.max_sail_actuator_torque_n_m,
            rudder_n_m: -parameters.max_rudder_actuator_torque_n_m,
        }
    );
}

#[test]
fn target_control_torques_critically_damp_motion_at_the_target() {
    let parameters = reference_parameters();
    let mut current = motion();
    current.sail_angle_rad = 0.25;
    current.sail_angular_velocity_rad_s = 0.1;
    current.rudder_angle_rad = -0.1;
    current.rudder_angular_velocity_rad_s = -0.1;
    let sail_hinge_inertia_kg_m2 = (parameters.sail_mass_kg + parameters.boom_mass_kg)
        * parameters.boom_length_m.powi(2)
        / 3.0;
    let rudder_hinge_inertia_kg_m2 =
        parameters.rudder_mass_kg * parameters.rudder_draw_length_m.powi(2) / 3.0
            + parameters.tiller_mass_kg * parameters.tiller_length_m.powi(2) / 3.0;
    let sail_stiffness_n_m_rad =
        parameters.max_sail_actuator_torque_n_m / (parameters.max_sail_angle_rad / 10.0);
    let rudder_stiffness_n_m_rad =
        parameters.max_rudder_actuator_torque_n_m / (parameters.max_rudder_angle_rad / 10.0);

    let torques = target_control_torques(
        parameters,
        current,
        current.sail_angle_rad,
        current.rudder_angle_rad,
    );

    assert_close(
        torques.sail_n_m,
        -2.0 * (sail_stiffness_n_m_rad * sail_hinge_inertia_kg_m2).sqrt() * 0.1,
    );
    assert_close(
        torques.rudder_n_m,
        2.0 * (rudder_stiffness_n_m_rad * rudder_hinge_inertia_kg_m2).sqrt() * 0.1,
    );
}

#[test]
fn sheet_control_never_pushes_the_sail_outward() {
    let parameters = reference_parameters();
    for (angle_rad, outward_target_rad) in [(0.25, 0.5), (-0.25, -0.5), (0.0, 0.5)] {
        let mut current = motion();
        current.sail_angle_rad = angle_rad;
        current.sail_angular_velocity_rad_s = 0.0;

        let torques = sheet_control_torques(parameters, current, outward_target_rad, 0.2);

        assert_eq!(torques.sail_n_m, 0.0);
        assert_ne!(torques.rudder_n_m, 0.0);
    }

    let mut current = motion();
    current.sail_angle_rad = 0.5;
    current.sail_angular_velocity_rad_s = 0.0;
    assert!(sheet_control_torques(parameters, current, 0.25, 0.0).sail_n_m < 0.0);
}

fn controller_input() -> ControlInput {
    ControlInput {
        boat_angular_velocity_rad_s: 0.0,
        free_sail_angle_rad: -PI / 2.0,
        sail_angle_rad: -PI / 4.0,
        apparent_wind_angle_from_boat_forward_rad: 0.0,
        sail_side_hysteresis_angle_rad: 10.0_f32.to_radians(),
        max_rudder_angle_rad: PI / 6.0,
        sail_effects: vec![
            SurfaceEffect {
                angle_rad: -PI / 2.0,
                drive_force_n: 0.0,
                yaw_moment_n_m: 0.0,
            },
            SurfaceEffect {
                angle_rad: -PI / 4.0,
                drive_force_n: 10.0,
                yaw_moment_n_m: 2.0,
            },
            SurfaceEffect {
                angle_rad: 0.0,
                drive_force_n: 5.0,
                yaw_moment_n_m: 0.0,
            },
            SurfaceEffect {
                angle_rad: PI / 4.0,
                drive_force_n: 20.0,
                yaw_moment_n_m: -2.0,
            },
            SurfaceEffect {
                angle_rad: PI / 2.0,
                drive_force_n: -8.0,
                yaw_moment_n_m: -1.0,
            },
        ],
        rudder_effects: vec![
            SurfaceEffect {
                angle_rad: -PI / 6.0,
                drive_force_n: -1.0,
                yaw_moment_n_m: 10.0,
            },
            SurfaceEffect {
                angle_rad: -PI / 12.0,
                drive_force_n: -0.5,
                yaw_moment_n_m: 2.0,
            },
            SurfaceEffect {
                angle_rad: 0.0,
                drive_force_n: 0.0,
                yaw_moment_n_m: 0.0,
            },
            SurfaceEffect {
                angle_rad: PI / 12.0,
                drive_force_n: -0.5,
                yaw_moment_n_m: -2.0,
            },
            SurfaceEffect {
                angle_rad: PI / 6.0,
                drive_force_n: -1.0,
                yaw_moment_n_m: -10.0,
            },
        ],
    }
}

#[test]
fn surface_control_input_samples_quasi_static_sail_and_rudder_effects() {
    let parameters = reference_parameters();
    let mut current = motion();
    current.velocity_m_s = Vector::new(1.0, -0.5);
    current.angle_rad = 0.4;
    current.angular_velocity_rad_s = 0.2;
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 1.0),
        true_water_velocity_m_s: Vector::new(0.5, -0.25),
    };

    let input = surface_control_input(parameters, current, environment, 30, 10.0_f32.to_radians());

    assert_eq!(input.sail_effects.len(), 7);
    assert_eq!(input.rudder_effects.len(), 3);
    let sail_effect = input
        .sail_effects
        .iter()
        .find(|effect| effect.angle_rad == 0.0)
        .unwrap();
    let sail_center_local_m = parameters.mast_position_m
        + Vector::new(-parameters.sail_center_of_effort_from_mast_m, 0.0);
    let sail_center_world_m =
        Rotation::new(current.angle_rad).transform_vector(sail_center_local_m);
    let sail_apparent_wind_m_s = environment.true_wind_velocity_m_s
        - current.velocity_m_s
        - Vector::new(-sail_center_world_m.y, sail_center_world_m.x)
            * current.angular_velocity_rad_s;
    let sail_force_n = flat_plate_force(
        sail_apparent_wind_m_s,
        current.angle_rad,
        parameters.air_density_kg_m3,
        parameters.sail_area_m2,
        parameters.sail_base_drag_coefficient,
    );
    let boat_forward = Rotation::new(current.angle_rad).transform_vector(Vector::new(1.0, 0.0));
    assert_close(sail_effect.drive_force_n, sail_force_n.dot(boat_forward));
    assert_close(
        sail_effect.yaw_moment_n_m,
        sail_center_world_m.perp_dot(sail_force_n),
    );

    let rudder_effect = input
        .rudder_effects
        .iter()
        .find(|effect| effect.angle_rad == 0.0)
        .unwrap();
    let rudder_center_local_m =
        parameters.rudder_position_m + Vector::new(-parameters.rudder_draw_length_m / 2.0, 0.0);
    let rudder_center_world_m =
        Rotation::new(current.angle_rad).transform_vector(rudder_center_local_m);
    let rudder_apparent_water_m_s = environment.true_water_velocity_m_s
        - current.velocity_m_s
        - Vector::new(-rudder_center_world_m.y, rudder_center_world_m.x)
            * current.angular_velocity_rad_s;
    let rudder_force_n = streamlined_foil_force(
        rudder_apparent_water_m_s,
        current.angle_rad,
        parameters.water_density_kg_m3,
        parameters.rudder_area_m2,
        parameters.rudder_base_drag_coefficient,
        parameters.rudder_stall_angle_rad,
        parameters.rudder_stall_lift_coefficient,
    );
    assert_close(
        rudder_effect.drive_force_n,
        rudder_force_n.dot(boat_forward),
    );
    assert_close(
        rudder_effect.yaw_moment_n_m,
        rudder_center_world_m.perp_dot(rudder_force_n),
    );
}

#[test]
fn computed_maximum_yaw_rate_uses_local_water_speed_and_steering_baseline() {
    let parameters = reference_parameters();
    let mut current = motion();
    current.velocity_m_s = Vector::new(1.0, -0.5);
    current.angle_rad = 0.4;
    current.angular_velocity_rad_s = 0.2;
    let environment = Environment {
        true_water_velocity_m_s: Vector::new(0.5, -0.25),
        ..Default::default()
    };
    let rudder_position_world_m =
        Rotation::new(current.angle_rad).transform_vector(parameters.rudder_position_m);
    let rudder_apparent_water_m_s = environment.true_water_velocity_m_s
        - current.velocity_m_s
        - Vector::new(-rudder_position_world_m.y, rudder_position_world_m.x)
            * current.angular_velocity_rad_s;
    let steering_baseline_m = (parameters.rudder_position_m - parameters.keel_position_m).length();
    let expected_rad_s = rudder_apparent_water_m_s.length() * parameters.max_rudder_angle_rad.tan()
        / steering_baseline_m;

    assert_close(
        computed_maximum_yaw_rate_rad_s(parameters, current, environment),
        expected_rad_s,
    );
}

#[test]
fn auto_sheet_selects_drive_and_maps_rudder_position() {
    let input = controller_input();
    let targets = auto_sheet_control(
        &input,
        AutoSheetIntent {
            power: 1.0,
            rudder: 0.5,
        },
    );

    assert_close(targets.sheet_target_angle_rad, -PI / 4.0);
    assert_close(targets.rudder_target_angle_rad, PI / 12.0);
    assert_eq!(targets.sail_target_angle_rad, None);

    let eased = auto_sheet_control(
        &input,
        AutoSheetIntent {
            power: 0.0,
            rudder: 0.0,
        },
    );
    assert_close(eased.sheet_target_angle_rad, -PI / 2.0);
}

#[test]
fn auto_sheet_retains_the_sail_side_only_inside_downwind_hysteresis() {
    let mut input = controller_input();
    input.sail_angle_rad = PI / 4.0;
    input.apparent_wind_angle_from_boat_forward_rad = 5.0_f32.to_radians();
    let retained = auto_sheet_control(
        &input,
        AutoSheetIntent {
            power: 1.0,
            rudder: 0.0,
        },
    );
    assert_close(retained.sheet_target_angle_rad, PI / 4.0);

    input.apparent_wind_angle_from_boat_forward_rad = 11.0_f32.to_radians();
    let switched = auto_sheet_control(
        &input,
        AutoSheetIntent {
            power: 1.0,
            rudder: 0.0,
        },
    );
    assert_close(switched.sheet_target_angle_rad, -PI / 4.0);
}

#[test]
fn yaw_rate_control_preserves_analog_turn_and_backing_behavior() {
    let mut input = controller_input();
    input.boat_angular_velocity_rad_s = 0.25;
    for (turn, expected_rudder_angle_rad) in [(0.25, PI / 12.0), (0.5, 0.0), (0.75, -PI / 12.0)] {
        let targets = yaw_rate_move_control(&input, MotionIntent { forward: 0.5, turn }, 0.5);
        assert_close(targets.rudder_target_angle_rad, expected_rudder_angle_rad);
    }

    let backing = yaw_rate_move_control(
        &controller_input(),
        MotionIntent {
            forward: -1.0,
            turn: 1.0,
        },
        0.5,
    );
    assert!(backing.rudder_target_angle_rad < 0.0);
    assert!(backing.sail_target_angle_rad.is_some());
}

#[test]
fn surface_targets_choose_sheet_limited_or_explicit_sail_control() {
    let parameters = reference_parameters();
    let mut current = motion();
    current.sail_angle_rad = 0.25;
    current.sail_angular_velocity_rad_s = 0.0;
    let sheet_targets = pyboat_simulation::SurfaceTargets {
        sheet_target_angle_rad: 0.5,
        rudder_target_angle_rad: 0.2,
        sail_target_angle_rad: None,
    };
    let explicit_targets = pyboat_simulation::SurfaceTargets {
        sail_target_angle_rad: Some(0.5),
        ..sheet_targets
    };

    let sheet_torques = surface_target_control_torques(parameters, current, sheet_targets);
    let explicit_torques = surface_target_control_torques(parameters, current, explicit_targets);

    assert_eq!(sheet_torques.sail_n_m, 0.0);
    assert!(explicit_torques.sail_n_m > 0.0);
    assert_eq!(sheet_torques.rudder_n_m, explicit_torques.rudder_n_m);
}
