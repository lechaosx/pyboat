#![cfg(feature = "playground")]

use browser::controller_lab::{LabController, LabControllerKind};
use game::{computed_yaw_rate_targets_in_environment, dinghy_parameters};
use rapier2d_f64::prelude::{Rotation, Vector};
use simulation::{BoatMotion, Environment, MotionIntent, SharedSimulation};

#[test]
fn selectable_controller_ids_preserve_neural_and_retire_predictive() {
    let offered = LabControllerKind::ALL;
    assert!(offered.iter().all(|kind| *kind as u32 != 3));
    assert_eq!(LabControllerKind::Learned as u32, 4);
    assert_eq!(LabControllerKind::CurrentRefined as u32, 5);
    assert_eq!(LabControllerKind::StandaloneNeural as u32, 6);
    assert_eq!(LabControllerKind::Guide as u32, 7);
    assert_eq!(LabControllerKind::Advanced as u32, 8);
    assert_eq!(offered.len(), 8);
}

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
fn refined_sail_target_is_continuous_when_power_crosses_zero_on_retained_side() {
    let parameters = dinghy_parameters();
    for side in [-1.0, 1.0] {
        let environment = Environment {
            true_wind_velocity_m_s: Vector::new(6.0, 0.3 * side),
            ..Environment::default()
        };
        let mut state = motion();
        state.velocity_m_s = Vector::ZERO;
        state.sail_angle_rad = 0.4 * side;
        let mut sim = SharedSimulation::new();
        let boat = sim.add_boat(parameters, state);
        let mut controller = LabController::new(LabControllerKind::CurrentRefined);
        let target = |controller: &mut LabController, forward| {
            controller
                .targets(
                    &sim,
                    boat,
                    parameters,
                    environment,
                    MotionIntent { forward, turn: 0.0 },
                )
                .sheet_target_angle_rad
        };
        let neutral = target(&mut controller, 0.0);
        for power in [-1e-6, 1e-6] {
            let difference = (target(&mut controller, power) - neutral).abs();
            assert!(
                difference <= 2.0 * parameters.max_sail_angle_rad * power.abs(),
                "power={power}, side={side}, sail jump={difference}"
            );
        }
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
    for kind in LabControllerKind::ALL {
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
fn allocation_holds_the_rudder_without_fluid_authority() {
    let mut parameters = dinghy_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    let mut sim = SharedSimulation::new();
    let boat = sim.add_boat(parameters, motion());
    for kind in [
        LabControllerKind::Physical,
        LabControllerKind::CurrentRefined,
        LabControllerKind::Classical,
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
        assert!((targets.rudder_target_angle_rad - motion().rudder_angle_rad).abs() < 1e-14);
        if kind == LabControllerKind::Classical {
            assert!(
                (targets.sail_target_angle_rad.unwrap() - motion().sail_angle_rad).abs() < 1e-14
            );
        } else {
            let reference = LabController::new(LabControllerKind::Current).targets(
                &sim,
                boat,
                parameters,
                Environment::default(),
                MotionIntent {
                    forward: 1.0,
                    turn: 1.0,
                },
            );
            assert_eq!(
                targets.sail_target_angle_rad,
                reference.sail_target_angle_rad
            );
            assert_eq!(
                targets.sheet_target_angle_rad,
                reference.sheet_target_angle_rad
            );
        }
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
        LabControllerKind::CurrentRefined,
        LabControllerKind::Classical,
        LabControllerKind::Learned,
        LabControllerKind::StandaloneNeural,
        LabControllerKind::Guide,
        LabControllerKind::Advanced,
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
        assert!(targets.sheet_target_angle_rad.is_finite());
        assert!(targets.rudder_target_angle_rad.is_finite());
        assert!(
            targets.sail_target_angle_rad.is_none_or(
                |angle| angle.is_finite() && angle.abs() <= parameters.max_sail_angle_rad
            )
        );
    }
}

#[test]
fn restored_controller_memory_replays_identical_real_fixed_ticks() {
    let parameters = dinghy_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(0.0, 6.0),
        ..Environment::default()
    };
    for kind in [LabControllerKind::Classical, LabControllerKind::Guide] {
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

#[test]
fn physical_allocation_preserves_current_sail_policy_across_fractional_inputs() {
    let parameters = dinghy_parameters();
    for wind in [
        Vector::new(0.0, 6.0),
        Vector::new(-6.0, 0.0),
        Vector::new(6.0, 0.0),
        Vector::new(6.0, 0.3),
        Vector::new(6.0, -0.3),
    ] {
        let environment = Environment {
            true_wind_velocity_m_s: wind,
            ..Environment::default()
        };
        for angle in [-0.7, 0.0, 1.2] {
            for sail_side in [-1.0, 1.0] {
                let mut sim = SharedSimulation::new();
                let mut state = motion();
                state.angle_rad = angle;
                state.sail_angle_rad *= sail_side;
                let boat = sim.add_boat(parameters, state);
                for forward in [-1.0, -0.5, -1e-6, 0.0, 0.1, 0.5, 1.0] {
                    for turn in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                        let intent = MotionIntent { forward, turn };
                        let reference = LabController::new(LabControllerKind::Current).targets(
                            &sim,
                            boat,
                            parameters,
                            environment,
                            intent,
                        );
                        let allocated = LabController::new(LabControllerKind::Physical).targets(
                            &sim,
                            boat,
                            parameters,
                            environment,
                            intent,
                        );
                        assert_eq!(
                            allocated.sheet_target_angle_rad,
                            reference.sheet_target_angle_rad
                        );
                        assert_eq!(
                            allocated.sail_target_angle_rad,
                            reference.sail_target_angle_rad
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn refined_controller_preserves_physical_targets_at_nonnegative_power_and_full_reverse() {
    let parameters = dinghy_parameters();
    for wind in [
        Vector::new(0.0, 6.0),
        Vector::new(-6.0, 0.0),
        Vector::new(6.0, 0.3),
        Vector::new(6.0, -0.3),
    ] {
        for heading in [-0.7, 0.0, 1.2] {
            for side in [-1.0, 1.0] {
                let environment = Environment {
                    true_wind_velocity_m_s: wind,
                    ..Environment::default()
                };
                let mut state = motion();
                state.angle_rad = heading;
                state.sail_angle_rad *= side;
                let mut sim = SharedSimulation::new();
                let boat = sim.add_boat(parameters, state);
                for forward in [-1.0, 0.0, 1e-6, 0.1, 0.5, 1.0] {
                    for turn in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                        let intent = MotionIntent { forward, turn };
                        let original = LabController::new(LabControllerKind::Physical).targets(
                            &sim,
                            boat,
                            parameters,
                            environment,
                            intent,
                        );
                        let refined = LabController::new(LabControllerKind::CurrentRefined)
                            .targets(&sim, boat, parameters, environment, intent);
                        assert_eq!(
                            refined, original,
                            "wind={wind:?}, heading={heading}, side={side}, intent={intent:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn refined_fractional_reverse_interpolates_from_retained_neutral_to_full_backing() {
    let parameters = dinghy_parameters();
    for side in [-1.0, 1.0] {
        let environment = Environment {
            true_wind_velocity_m_s: Vector::new(6.0, 0.3 * side),
            ..Environment::default()
        };
        let mut state = motion();
        state.velocity_m_s = Vector::ZERO;
        state.sail_angle_rad = 0.4 * side;
        let mut sim = SharedSimulation::new();
        let boat = sim.add_boat(parameters, state);
        let targets = |forward| {
            LabController::new(LabControllerKind::CurrentRefined).targets(
                &sim,
                boat,
                parameters,
                environment,
                MotionIntent { forward, turn: 0.0 },
            )
        };
        let neutral = targets(0.0).sheet_target_angle_rad;
        let backed = targets(-1.0).sheet_target_angle_rad;
        for fraction in [1e-6, 0.1, 0.5, 0.9] {
            let target = targets(-fraction);
            let expected = neutral + fraction * (backed - neutral);
            assert!((target.sheet_target_angle_rad - expected).abs() < 1e-14);
            assert_eq!(
                target.sail_target_angle_rad,
                Some(target.sheet_target_angle_rad)
            );
        }
    }
}

#[test]
fn refined_targets_reflect_with_the_boat_flow_and_intent() {
    let parameters = dinghy_parameters();
    let state = motion();
    let mirrored = BoatMotion {
        position_m: Vector::new(state.position_m.x, -state.position_m.y),
        velocity_m_s: Vector::new(state.velocity_m_s.x, -state.velocity_m_s.y),
        angle_rad: -state.angle_rad,
        angular_velocity_rad_s: -state.angular_velocity_rad_s,
        sail_angle_rad: -state.sail_angle_rad,
        sail_angular_velocity_rad_s: -state.sail_angular_velocity_rad_s,
        rudder_angle_rad: -state.rudder_angle_rad,
        rudder_angular_velocity_rad_s: -state.rudder_angular_velocity_rad_s,
    };
    let mut first = SharedSimulation::new();
    let mut second = SharedSimulation::new();
    first.add_boat(parameters, state);
    second.add_boat(parameters, mirrored);
    for forward in [-1.0, -0.5, -1e-6, 0.0, 0.5, 1.0] {
        for turn in [-0.5, 0.0, 0.5] {
            let environment = Environment {
                true_wind_velocity_m_s: Vector::new(6.0, 0.3),
                true_water_velocity_m_s: Vector::new(0.2, 0.1),
                ..Environment::default()
            };
            let reflected_flow = Environment {
                true_wind_velocity_m_s: Vector::new(6.0, -0.3),
                true_water_velocity_m_s: Vector::new(0.2, -0.1),
                ..Environment::default()
            };
            let a = LabController::new(LabControllerKind::CurrentRefined).targets(
                &first,
                0,
                parameters,
                environment,
                MotionIntent { forward, turn },
            );
            let b = LabController::new(LabControllerKind::CurrentRefined).targets(
                &second,
                0,
                parameters,
                reflected_flow,
                MotionIntent {
                    forward,
                    turn: -turn,
                },
            );
            assert!((a.sheet_target_angle_rad + b.sheet_target_angle_rad).abs() < 1e-12);
            assert!((a.rudder_target_angle_rad + b.rudder_target_angle_rad).abs() < 1e-12);
            assert_eq!(
                a.sail_target_angle_rad.is_some(),
                b.sail_target_angle_rad.is_some()
            );
            if let (Some(a), Some(b)) = (a.sail_target_angle_rad, b.sail_target_angle_rad) {
                assert!((a + b).abs() < 1e-12);
            }
        }
    }
}

#[test]
fn standalone_policy_applies_known_output_biases_with_physical_angle_limits() {
    use browser::controller_lab::standalone::{WEIGHTS, targets_with_weights};
    let parameters = dinghy_parameters();
    let mut sim = SharedSimulation::new();
    let boat = sim.add_boat(parameters, motion());
    let mut weights = [0.0; WEIGHTS];
    weights[320] = 2.0;
    weights[337] = -1.0;
    let targets = targets_with_weights(
        &sim,
        boat,
        parameters,
        Environment::default(),
        MotionIntent::default(),
        &weights,
    );
    assert_eq!(
        targets.sheet_target_angle_rad,
        2.0 / 3.0 * parameters.max_sail_angle_rad
    );
    assert_eq!(
        targets.sail_target_angle_rad,
        Some(targets.sheet_target_angle_rad)
    );
    assert_eq!(
        targets.rudder_target_angle_rad,
        -0.5 * parameters.max_rudder_angle_rad
    );
    for sign in [-1.0, 1.0] {
        weights[320] = sign * 1e100;
        weights[337] = sign * 1e100;
        let targets = targets_with_weights(
            &sim,
            boat,
            parameters,
            Environment::default(),
            MotionIntent {
                forward: 1.0,
                turn: -1.0,
            },
            &weights,
        );
        assert!(targets.sheet_target_angle_rad.is_finite());
        assert!(targets.rudder_target_angle_rad.is_finite());
        assert!(targets.sheet_target_angle_rad.abs() <= parameters.max_sail_angle_rad);
        assert!(targets.rudder_target_angle_rad.abs() <= parameters.max_rudder_angle_rad);
    }
}

fn guide_target(
    controller: &mut LabController,
    parameters: simulation::BoatParameters,
    state: BoatMotion,
    environment: Environment,
    intent: MotionIntent,
) -> simulation::SurfaceTargets {
    let mut simulation = SharedSimulation::new();
    let boat = simulation.add_boat(parameters, state);
    let before = simulation.motion(boat);
    let targets = controller.targets(&simulation, boat, parameters, environment, intent);
    assert_eq!(simulation.motion(boat), before);
    targets
}

#[test]
fn guide_power_regimes_use_fractional_sheet_and_direct_helm_targets() {
    let p = dinghy_parameters();
    for side in [-1.0, 1.0] {
        let state = BoatMotion {
            velocity_m_s: Vector::ZERO,
            sail_angle_rad: side * 0.3,
            ..motion()
        };
        let environment = Environment {
            true_wind_velocity_m_s: Vector::new(0.0, -6.0 * side),
            ..Environment::default()
        };
        let mut controller = LabController::new(LabControllerKind::Guide);
        for (power, expected_degrees) in [
            (0.0, 90.0),
            (1e-6, 90.0 - 45e-6),
            (0.5, 67.5),
            (1.0, 45.0),
            (-1e-6, -45e-6),
            (-0.5, -22.5),
            (-1.0, -45.0),
        ] {
            let targets = guide_target(
                &mut controller,
                p,
                state,
                environment,
                MotionIntent {
                    forward: power,
                    turn: 0.5,
                },
            );
            assert!(
                (targets.sheet_target_angle_rad.to_degrees() - side * expected_degrees).abs()
                    < 1e-10
            );
            assert_eq!(
                targets.sail_target_angle_rad,
                (power < 0.0).then_some(targets.sheet_target_angle_rad)
            );
            assert_eq!(
                targets.rudder_target_angle_rad,
                -0.5 * p.max_rudder_angle_rad
            );
        }
        let neutral = guide_target(
            &mut controller,
            p,
            state,
            environment,
            MotionIntent::default(),
        );
        assert_eq!(neutral.rudder_target_angle_rad, 0.0);
    }
}

#[test]
fn guide_sternway_uses_hinge_relative_flow_with_hysteresis_and_resets() {
    let p = dinghy_parameters();
    let mut state = motion();
    state.velocity_m_s = Vector::ZERO;
    state.angular_velocity_rad_s = 0.0;
    let intent = MotionIntent {
        forward: 0.0,
        turn: 0.5,
    };
    let mut controller = LabController::new(LabControllerKind::Guide);
    for (speed, polarity) in [
        (2.0, 1.0),
        (0.05, 1.0),
        (-0.05, 1.0),
        (-0.11, -1.0),
        (-2.0, -1.0),
        (-0.05, -1.0),
        (0.05, -1.0),
        (0.11, 1.0),
    ] {
        let environment = Environment {
            true_water_velocity_m_s: Vector::new(-speed, 0.0),
            ..Environment::default()
        };
        let target = guide_target(&mut controller, p, state, environment, intent);
        assert_eq!(
            target.rudder_target_angle_rad,
            -polarity * 0.5 * p.max_rudder_angle_rad
        );
    }
    let mut offset_hinge = p;
    offset_hinge.rudder_position_m.y = 0.4;
    state.angular_velocity_rad_s = 1.0;
    let target = guide_target(
        &mut controller,
        offset_hinge,
        state,
        Environment::default(),
        intent,
    );
    assert_eq!(target.rudder_target_angle_rad, 0.5 * p.max_rudder_angle_rad);
    state.angular_velocity_rad_s = 0.0;
    let retained = guide_target(&mut controller, p, state, Environment::default(), intent);
    assert_eq!(
        retained.rudder_target_angle_rad,
        target.rudder_target_angle_rad
    );
    controller.select(LabControllerKind::Guide);
    assert_eq!(
        guide_target(&mut controller, p, state, Environment::default(), intent),
        retained
    );
    let mut switched = controller.clone();
    switched.select(LabControllerKind::Current);
    switched.select(LabControllerKind::Guide);
    assert_eq!(
        guide_target(&mut switched, p, state, Environment::default(), intent)
            .rudder_target_angle_rad,
        -0.5 * p.max_rudder_angle_rad
    );
    controller.reset();
    let reset = guide_target(&mut controller, p, state, Environment::default(), intent);
    assert_eq!(reset.rudder_target_angle_rad, -0.5 * p.max_rudder_angle_rad);
}

#[test]
fn guide_backing_uses_raw_wind_side_even_when_drive_retains_boom_side() {
    let p = dinghy_parameters();
    let state = BoatMotion {
        velocity_m_s: Vector::ZERO,
        sail_angle_rad: -0.3,
        ..motion()
    };
    let beta = 175_f64.to_radians();
    let environment = Environment {
        true_wind_velocity_m_s: Rotation::new(beta).transform_vector(Vector::new(-6.0, 0.0)),
        ..Environment::default()
    };
    let mut controller = LabController::new(LabControllerKind::Guide);
    let drive = guide_target(
        &mut controller,
        p,
        state,
        environment,
        MotionIntent {
            forward: 1.0,
            turn: 0.0,
        },
    );
    assert!((drive.sheet_target_angle_rad.to_degrees() + 87.5).abs() < 1e-10);
    let back = guide_target(
        &mut controller,
        p,
        state,
        environment,
        MotionIntent {
            forward: -1.0,
            turn: 0.0,
        },
    );
    assert!((back.sheet_target_angle_rad.to_degrees() + 2.5).abs() < 1e-10);
}

#[test]
fn guide_targets_reflect_and_rotate_with_the_boat_and_flows() {
    let p = dinghy_parameters();
    let state = motion();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(-3.0, 5.0),
        true_water_velocity_m_s: Vector::new(0.3, -0.2),
        ..Environment::default()
    };
    let intent = MotionIntent {
        forward: -0.5,
        turn: 0.25,
    };
    let target = guide_target(
        &mut LabController::new(LabControllerKind::Guide),
        p,
        state,
        environment,
        intent,
    );
    let reflected = BoatMotion {
        position_m: Vector::new(state.position_m.x, -state.position_m.y),
        velocity_m_s: Vector::new(state.velocity_m_s.x, -state.velocity_m_s.y),
        angle_rad: -state.angle_rad,
        angular_velocity_rad_s: -state.angular_velocity_rad_s,
        sail_angle_rad: -state.sail_angle_rad,
        sail_angular_velocity_rad_s: -state.sail_angular_velocity_rad_s,
        rudder_angle_rad: -state.rudder_angle_rad,
        rudder_angular_velocity_rad_s: -state.rudder_angular_velocity_rad_s,
    };
    let reflected_environment = Environment {
        true_wind_velocity_m_s: Vector::new(
            environment.true_wind_velocity_m_s.x,
            -environment.true_wind_velocity_m_s.y,
        ),
        true_water_velocity_m_s: Vector::new(
            environment.true_water_velocity_m_s.x,
            -environment.true_water_velocity_m_s.y,
        ),
        ..environment
    };
    let mirrored = guide_target(
        &mut LabController::new(LabControllerKind::Guide),
        p,
        reflected,
        reflected_environment,
        MotionIntent {
            turn: -intent.turn,
            ..intent
        },
    );
    assert!((target.sheet_target_angle_rad + mirrored.sheet_target_angle_rad).abs() < 1e-12);
    assert_eq!(
        target.rudder_target_angle_rad,
        -mirrored.rudder_target_angle_rad
    );
    let rotation = Rotation::new(0.734);
    let rotated = BoatMotion {
        position_m: rotation.transform_vector(state.position_m),
        velocity_m_s: rotation.transform_vector(state.velocity_m_s),
        angle_rad: state.angle_rad + 0.734,
        ..state
    };
    let rotated_environment = Environment {
        true_wind_velocity_m_s: rotation.transform_vector(environment.true_wind_velocity_m_s),
        true_water_velocity_m_s: rotation.transform_vector(environment.true_water_velocity_m_s),
        ..environment
    };
    let turned = guide_target(
        &mut LabController::new(LabControllerKind::Guide),
        p,
        rotated,
        rotated_environment,
        intent,
    );
    assert!((target.sheet_target_angle_rad - turned.sheet_target_angle_rad).abs() < 1e-12);
    assert_eq!(
        target.rudder_target_angle_rad,
        turned.rudder_target_angle_rad
    );
}

#[test]
fn advanced_sailor_coordinates_sail_with_turn_without_changing_the_plant() {
    let p = dinghy_parameters();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(0.0, 6.0),
        ..Environment::default()
    };
    let mut state = motion();
    state.angular_velocity_rad_s = 0.0;
    let mut sim = SharedSimulation::new();
    sim.add_boat(p, state);
    let mut controller = LabController::new(LabControllerKind::Advanced);
    let before = sim.motion(0);
    let left = controller.targets(
        &sim,
        0,
        p,
        environment,
        MotionIntent {
            forward: 0.5,
            turn: -1.0,
        },
    );
    let right = controller.targets(
        &sim,
        0,
        p,
        environment,
        MotionIntent {
            forward: 0.5,
            turn: 1.0,
        },
    );
    assert_ne!(left.sheet_target_angle_rad, right.sheet_target_angle_rad);
    assert_eq!(sim.motion(0), before);
    for target in [left, right] {
        assert!(target.sheet_target_angle_rad.abs() <= p.max_sail_angle_rad);
        assert!(target.rudder_target_angle_rad.abs() <= p.max_rudder_angle_rad);
        assert_eq!(target.sail_target_angle_rad, None);
    }
}

#[test]
fn advanced_sailor_returns_finite_bounded_targets_without_fluid_authority() {
    let mut p = dinghy_parameters();
    p.air_density_kg_m3 = 0.0;
    p.water_density_kg_m3 = 0.0;
    let mut sim = SharedSimulation::new();
    sim.add_boat(p, motion());
    let before = sim.motion(0);
    for forward in [-1.0, -0.5, -1e-9, 0.0, 1e-9, 0.5, 1.0] {
        for turn in [-1.0, 0.0, 1.0] {
            let target = LabController::new(LabControllerKind::Advanced).targets(
                &sim,
                0,
                p,
                Environment::default(),
                MotionIntent { forward, turn },
            );
            assert!(target.sheet_target_angle_rad.abs() <= p.max_sail_angle_rad);
            assert!(target.rudder_target_angle_rad.abs() <= p.max_rudder_angle_rad);
            assert_eq!(
                target.sail_target_angle_rad,
                (forward < 0.0).then_some(target.sheet_target_angle_rad)
            );
            assert_eq!(sim.motion(0), before);
        }
    }
}

#[test]
fn advanced_sailor_keeps_working_boom_side_for_small_downwind_corrections() {
    let p = dinghy_parameters();
    for side in [-1.0, 1.0] {
        let environment = Environment {
            true_wind_velocity_m_s: Vector::new(6.0, 0.01 * side),
            ..Environment::default()
        };
        for yaw in [-0.05, 0.0, 0.05] {
            let state = BoatMotion {
                velocity_m_s: Vector::new(2.0, 0.0),
                angular_velocity_rad_s: yaw,
                sail_angle_rad: 1.4 * side,
                sail_angular_velocity_rad_s: 0.0,
                ..motion()
            };
            let mut sim = SharedSimulation::new();
            let boat = sim.add_boat(p, state);
            for turn in [-0.02, 0.0, 0.02] {
                let target = LabController::new(LabControllerKind::Advanced).targets(
                    &sim,
                    boat,
                    p,
                    environment,
                    MotionIntent { forward: 1.0, turn },
                );
                assert!(
                    target.sheet_target_angle_rad * side >= 0.0,
                    "side={side}, yaw={yaw}, turn={turn}, target={target:?}"
                );
            }
        }
    }
}

#[test]
fn advanced_sailor_targets_reflect_and_rotate_with_state_flow_and_intent() {
    let p = dinghy_parameters();
    let state = motion();
    let environment = Environment {
        true_wind_velocity_m_s: Vector::new(-3.0, 5.0),
        true_water_velocity_m_s: Vector::new(0.3, -0.2),
        ..Environment::default()
    };
    let target = |state, environment, forward, turn| {
        guide_target(
            &mut LabController::new(LabControllerKind::Advanced),
            p,
            state,
            environment,
            MotionIntent { forward, turn },
        )
    };
    for (state, environment) in [
        (state, environment),
        (
            BoatMotion {
                velocity_m_s: Vector::new(0.2, 0.0),
                angular_velocity_rad_s: 0.0,
                sail_angle_rad: 0.4,
                ..state
            },
            Environment {
                true_wind_velocity_m_s: Vector::new(2_f64.cos(), 2_f64.sin()) * 6.0,
                ..Environment::default()
            },
        ),
    ] {
        let reflection = |v: Vector| Vector::new(v.x, -v.y);
        let reflected = BoatMotion {
            position_m: reflection(state.position_m),
            velocity_m_s: reflection(state.velocity_m_s),
            angle_rad: -state.angle_rad,
            angular_velocity_rad_s: -state.angular_velocity_rad_s,
            sail_angle_rad: -state.sail_angle_rad,
            sail_angular_velocity_rad_s: -state.sail_angular_velocity_rad_s,
            rudder_angle_rad: -state.rudder_angle_rad,
            rudder_angular_velocity_rad_s: -state.rudder_angular_velocity_rad_s,
        };
        let rotation = Rotation::new(0.734);
        let rotated = BoatMotion {
            position_m: rotation.transform_vector(state.position_m),
            velocity_m_s: rotation.transform_vector(state.velocity_m_s),
            angle_rad: state.angle_rad + 0.734,
            ..state
        };
        for forward in [-1.0, -0.5, -1e-9, 0.0, 1e-9, 0.5, 1.0] {
            for turn in [-0.5, 0.0, 0.5] {
                let a = target(state, environment, forward, turn);
                let b = target(
                    reflected,
                    Environment {
                        true_wind_velocity_m_s: reflection(environment.true_wind_velocity_m_s),
                        true_water_velocity_m_s: reflection(environment.true_water_velocity_m_s),
                        ..environment
                    },
                    forward,
                    -turn,
                );
                let c = target(
                    rotated,
                    Environment {
                        true_wind_velocity_m_s: rotation
                            .transform_vector(environment.true_wind_velocity_m_s),
                        true_water_velocity_m_s: rotation
                            .transform_vector(environment.true_water_velocity_m_s),
                        ..environment
                    },
                    forward,
                    turn,
                );
                assert!(
                    (a.sheet_target_angle_rad + b.sheet_target_angle_rad).abs() < 1e-10,
                    "forward={forward}, turn={turn}, a={a:?}, b={b:?}"
                );
                assert!((a.rudder_target_angle_rad + b.rudder_target_angle_rad).abs() < 1e-10);
                assert!((a.sheet_target_angle_rad - c.sheet_target_angle_rad).abs() < 1e-10);
                assert!((a.rudder_target_angle_rad - c.rudder_target_angle_rad).abs() < 1e-10);
            }
        }
    }
}
