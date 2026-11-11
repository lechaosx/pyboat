use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, Environment, SharedSimulation};

use game::dinghy_parameters;

fn motion() -> BoatMotion {
    BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    }
}

#[test]
fn control_observation_has_positive_inertias_and_no_load_in_still_fluid() {
    let mut sim = SharedSimulation::new();
    let boat = sim.add_boat(dinghy_parameters(), motion());
    let state = sim.control_dynamics(boat, Environment::default());
    assert!(state.locked_inertia_kg_m2 > state.rudder_hinge_inertia_kg_m2);
    assert!(state.rudder_hinge_inertia_kg_m2 > 0.0);
    assert_eq!(state.fluid_hinge_torques_n_m, [0.0; 2]);
    assert_eq!(state.passive_yaw_moment_n_m, 0.0);
}

#[test]
fn moving_surfaces_observe_resistance_even_with_stationary_hull() {
    let mut sim = SharedSimulation::new();
    let mut m = motion();
    m.sail_angular_velocity_rad_s = 1.0;
    m.rudder_angular_velocity_rad_s = -1.0;
    let boat = sim.add_boat(dinghy_parameters(), m);
    let state = sim.control_dynamics(boat, Environment::default());
    assert!(state.fluid_hinge_torques_n_m[0] < 0.0);
    assert!(state.fluid_hinge_torques_n_m[1] > 0.0);
    assert_eq!(sim.motion(boat), m);
}

#[test]
fn predicted_moment_reference_changes_by_force_times_lever_arm() {
    let parameters = dinghy_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(4.0, 2.0),
        true_water_velocity_m_s: Vector::new(-2.0, 0.1),
        ..Environment::default()
    };
    let origin = simulation::surface_control_input(parameters, motion(), environment, 1, 0.1);
    let shifted = simulation::surface_control_input_about_point(
        parameters,
        motion(),
        environment,
        1,
        0.1,
        Vector::new(0.0, 0.2),
    );
    for (first, second) in origin
        .sail_effects
        .iter()
        .chain(&origin.rudder_effects)
        .zip(shifted.sail_effects.iter().chain(&shifted.rudder_effects))
    {
        assert!(
            (second.yaw_moment_n_m - first.yaw_moment_n_m - 0.2 * first.drive_force_n).abs()
                < 1e-10
        );
        assert_eq!(first.drive_force_n, second.drive_force_n);
    }
}
