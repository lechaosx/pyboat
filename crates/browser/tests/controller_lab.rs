#![cfg(feature = "playground")]

use browser::controller_lab::{LabController, LabControllerKind};
use game::{computed_yaw_rate_targets_in_environment, dinghy_parameters};
use rapier2d_f64::prelude::{Rotation, Vector};
use simulation::{BoatMotion, Environment, MotionIntent, SharedSimulation};

fn motion() -> BoatMotion {
    BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::new(2.0, 0.0),
        angle_rad: 0.0,
        angular_velocity_rad_s: 0.1,
        sail_angle_rad: 0.3,
        sail_angular_velocity_rad_s: -0.2,
        rudder_angle_rad: 0.1,
        rudder_angular_velocity_rad_s: 0.2,
    }
}

#[test]
fn current_controller_retains_exact_targets_for_fractional_and_reversed_inputs() {
    let parameters = dinghy_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(5.0, 2.0),
        true_water_velocity_m_s: Vector::new(0.1, -0.2),
        ..Environment::default()
    };
    for angle in [0.0, 1.0, 3.0] {
        let mut sim = SharedSimulation::new();
        let mut m = motion();
        m.angle_rad = angle;
        m.velocity_m_s = Rotation::new(angle).transform_vector(m.velocity_m_s);
        let boat = sim.add_boat(parameters, m);
        let mut controller = LabController::new(LabControllerKind::Current);
        for forward in [-1.0, -0.5, -1e-6, 0.0, 0.1, 0.5, 1.0] {
            for turn in [-1.0, -0.5, 0.0, 0.1, 0.5, 1.0] {
                let intent = MotionIntent { forward, turn };
                assert_eq!(
                    controller.targets(&sim, boat, parameters, environment, intent),
                    computed_yaw_rate_targets_in_environment(sim.motion(boat), intent, environment)
                );
            }
        }
    }
}

#[test]
fn switching_and_resetting_controller_never_changes_physical_state() {
    let parameters = dinghy_parameters();
    let mut sim = SharedSimulation::new();
    let boat = sim.add_boat(parameters, motion());
    let before = sim.motion(boat);
    let environment = Environment::default();
    let intent = MotionIntent {
        forward: 0.5,
        turn: -0.5,
    };
    for kind in [LabControllerKind::Classical, LabControllerKind::Predictive] {
        let mut controller = LabController::new(kind);
        for _ in 0..30 {
            controller.targets(&sim, boat, parameters, environment, intent);
        }
        let mut continued = controller.clone();
        controller.select(kind);
        assert_eq!(
            controller.targets(&sim, boat, parameters, environment, intent),
            continued.targets(&sim, boat, parameters, environment, intent)
        );
        controller.reset();
        let mut fresh = LabController::new(kind);
        assert_eq!(
            controller.targets(&sim, boat, parameters, environment, intent),
            fresh.targets(&sim, boat, parameters, environment, intent)
        );
        controller.select(LabControllerKind::Current);
        controller.select(kind);
        fresh.reset();
        assert_eq!(
            controller.targets(&sim, boat, parameters, environment, intent),
            fresh.targets(&sim, boat, parameters, environment, intent)
        );
    }
    assert_eq!(sim.motion(boat), before);
}

#[test]
fn joint_policies_leave_surfaces_at_their_actual_angles_without_fluid_authority() {
    let mut parameters = dinghy_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    let mut sim = SharedSimulation::new();
    let boat = sim.add_boat(parameters, motion());
    for kind in [LabControllerKind::Physical, LabControllerKind::Classical] {
        let mut controller = LabController::new(kind);
        let targets = controller.targets(
            &sim,
            boat,
            parameters,
            Environment::default(),
            MotionIntent {
                forward: 1.0,
                turn: 1.0,
            },
        );
        assert!((targets.rudder_target_angle_rad - motion().rudder_angle_rad).abs() < 1e-14);
        assert!((targets.sail_target_angle_rad.unwrap() - motion().sail_angle_rad).abs() < 1e-14);
    }
}

#[test]
fn experimental_targets_respect_exact_parameter_limits_despite_rounded_sampling_grid() {
    let mut parameters = dinghy_parameters();
    parameters.max_sail_angle_rad -= 0.004;
    parameters.max_rudder_angle_rad -= 0.004;
    let mut sim = SharedSimulation::new();
    let mut m = motion();
    m.sail_angle_rad = parameters.max_sail_angle_rad + 0.003;
    m.rudder_angle_rad = parameters.max_rudder_angle_rad + 0.003;
    let boat = sim.add_boat(parameters, m);
    for kind in [
        LabControllerKind::Physical,
        LabControllerKind::Classical,
        LabControllerKind::Predictive,
        LabControllerKind::Learned,
    ] {
        let mut controller = LabController::new(kind);
        let targets = controller.targets(
            &sim,
            boat,
            parameters,
            Environment::default(),
            MotionIntent {
                forward: 1.0,
                turn: 1.0,
            },
        );
        assert!(targets.sheet_target_angle_rad.abs() <= parameters.max_sail_angle_rad);
        assert!(targets.rudder_target_angle_rad.abs() <= parameters.max_rudder_angle_rad);
    }
}

#[test]
fn restored_controller_memory_replays_identical_real_fixed_ticks() {
    let parameters = dinghy_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(0.0, 6.0),
        ..Environment::default()
    };
    for kind in [LabControllerKind::Classical, LabControllerKind::Predictive] {
        let mut first = SharedSimulation::new();
        first.add_boat(parameters, motion());
        let mut controller = LabController::new(kind);
        for _ in 0..12 {
            let target = controller.targets(
                &first,
                0,
                parameters,
                environment,
                MotionIntent {
                    forward: 0.5,
                    turn: 0.5,
                },
            );
            first.advance_controlled_with_environments(&[environment], &[Some(target)]);
        }
        // Both branches restore the same public state, so neither inherits solver warm-start impulses.
        let anchor = first.motion(0);
        let mut first = SharedSimulation::new();
        let mut second = SharedSimulation::new();
        first.add_boat(parameters, anchor);
        second.add_boat(parameters, anchor);
        let mut restored = controller.clone();
        for tick in 0..12 {
            let intent = MotionIntent {
                forward: 0.5,
                turn: if tick < 6 { -0.5 } else { 0.0 },
            };
            let a = controller.targets(&first, 0, parameters, environment, intent);
            let b = restored.targets(&second, 0, parameters, environment, intent);
            assert_eq!(a, b);
            first.advance_controlled_with_environments(&[environment], &[Some(a)]);
            second.advance_controlled_with_environments(&[environment], &[Some(b)]);
            assert_eq!(first.motion(0), second.motion(0));
        }
    }
}
