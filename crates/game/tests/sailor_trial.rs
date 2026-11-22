#![cfg(feature = "sailor-trial")]

use game::{dinghy_parameters, sailor_trial};
use physics::prelude::Vector;
use simulation::{BoatMotion, Environment, MotionIntent, Simulation, flat_plate_force};

fn pinned_boat(sail_degrees: f64, rudder_degrees: f64) -> Simulation {
    let mut simulation = Simulation::new(
        dinghy_parameters(),
        BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::ZERO,
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: sail_degrees.to_radians(),
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: rudder_degrees.to_radians(),
            rudder_angular_velocity_rad_s: 0.0,
        },
    );
    let hull = &mut simulation.world.bodies[simulation.boat.hull.body];
    hull.lock_translations(true, true);
    hull.lock_rotations(true, true);
    simulation
}

#[test]
fn full_backing_holds_against_a_loaded_boom_on_the_opposite_side() {
    let thrust = |side: f64, forward| {
        let mut simulation = pinned_boat(-45.0 * side, 0.0);
        let heading = (125.0 * side).to_radians();
        let parameters = simulation.parameters;
        let environment = Environment {
            true_wind_velocity_m_s: -18.0 * Vector::new(heading.cos(), heading.sin()),
            ..Environment::default()
        };
        for _ in 0..240 {
            let targets = sailor_trial::targets(
                parameters,
                simulation.motion(),
                environment,
                MotionIntent { forward, turn: 0.0 },
                true,
                false,
            );
            simulation.advance_targets(environment, targets);
        }
        let motion = simulation.motion();
        assert!(motion.sail_angular_velocity_rad_s.abs() < 0.01);
        flat_plate_force(
            environment.true_wind_velocity_m_s,
            motion.sail_angle_rad,
            parameters.air_density_kg_m3,
            parameters.sail_area_m2,
            parameters.sail_base_drag_coefficient,
        )
        .x
    };
    for side in [-1.0, 1.0] {
        let full = thrust(side, -1.0);
        let partial = thrust(side, -0.875);
        assert!(full < partial && partial < 0.0, "{full} N, {partial} N");
    }
}

#[test]
fn loaded_helm_can_recenter_and_reverse_from_sternway() {
    for side in [-1.0, 1.0] {
        let mut simulation = pinned_boat(0.0, side);
        let parameters = simulation.parameters;
        let environment = Environment {
            true_water_velocity_m_s: Vector::new(3.0, 0.0),
            ..Environment::default()
        };
        for turn in [0.0, side, 0.0, -side] {
            for _ in 0..180 {
                let targets = sailor_trial::targets(
                    parameters,
                    simulation.motion(),
                    environment,
                    MotionIntent { forward: 0.0, turn },
                    false,
                    true,
                );
                simulation.advance_targets(environment, targets);
            }
            let angle = simulation.motion().rudder_angle_rad;
            if turn == 0.0 {
                assert!(angle.abs() < 0.01, "{angle}");
            } else {
                assert!(angle * turn < 0.0, "turn {turn}, angle {angle}");
            }
        }
    }
}

#[test]
fn gentle_crossflow_leaves_both_ends_of_the_helm_available() {
    for side in [-1.0, 1.0] {
        let mut simulation = pinned_boat(0.0, 0.0);
        let parameters = simulation.parameters;
        let environment = Environment {
            true_water_velocity_m_s: Vector::new(0.0, 0.1 * side),
            ..Environment::default()
        };
        for turn in [-1.0, 1.0] {
            for _ in 0..120 {
                let targets = sailor_trial::targets(
                    parameters,
                    simulation.motion(),
                    environment,
                    MotionIntent { forward: 0.0, turn },
                    false,
                    true,
                );
                simulation.advance_targets(environment, targets);
            }
            let actual = simulation.motion().rudder_angle_rad;
            assert!(
                (actual + turn * parameters.max_rudder_angle_rad).abs() < 0.01,
                "{actual}"
            );
        }
    }
}
