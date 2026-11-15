use game::{dinghy_parameters, sailor_targets};
use physics::prelude::{Rotation, Vector};
use simulation::{BoatMotion, Environment, MotionIntent};

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
fn power_regimes_use_fractional_sheet_and_direct_helm_targets() {
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
        for (power, expected_degrees) in [
            (0.0, 90.0),
            (1e-6, 90.0 - 45e-6),
            (0.5, 67.5),
            (1.0, 45.0),
            (-1e-6, -45e-6),
            (-0.5, -22.5),
            (-1.0, -45.0),
        ] {
            let targets = sailor_targets(
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
        let neutral = sailor_targets(p, state, environment, MotionIntent::default());
        assert_eq!(neutral.rudder_target_angle_rad, 0.0);
    }
}

#[test]
fn direct_helm_targets_ignore_flow_direction_and_history() {
    let mut p = dinghy_parameters();
    p.rudder_position_m.y = 0.4;
    for (velocity, current, angular_velocity) in [
        (2.0, 0.0, 0.0),
        (0.0, 0.0, 0.0),
        (-2.0, 0.0, 0.0),
        (0.0, 0.05, 0.0),
        (0.0, -0.05, 0.0),
        (0.0, -2.0, 0.0),
        (0.0, 2.0, 0.0),
        (0.0, 0.0, 1.0),
        (0.0, 0.0, -1.0),
        (0.0, 0.0, 0.0),
    ] {
        let state = BoatMotion {
            velocity_m_s: Vector::new(velocity, 0.0),
            angular_velocity_rad_s: angular_velocity,
            ..motion()
        };
        let environment = Environment {
            true_water_velocity_m_s: Vector::new(current, 0.0),
            ..Environment::default()
        };
        for turn in [-2.0_f64, -1.0, -0.5, 0.0, 0.25, 1.0, 2.0] {
            for forward in [-1.0, 0.0, 1.0] {
                let target = sailor_targets(p, state, environment, MotionIntent { forward, turn });
                assert_eq!(
                    target.rudder_target_angle_rad,
                    -turn.clamp(-1.0, 1.0) * p.max_rudder_angle_rad,
                    "velocity={velocity}, current={current}, angular_velocity={angular_velocity}, turn={turn}, forward={forward}"
                );
            }
        }
    }
}

#[test]
fn backing_uses_raw_wind_side_even_when_drive_retains_boom_side() {
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
    let drive = sailor_targets(
        p,
        state,
        environment,
        MotionIntent {
            forward: 1.0,
            turn: 0.0,
        },
    );
    assert!((drive.sheet_target_angle_rad.to_degrees() + 87.5).abs() < 1e-10);
    let back = sailor_targets(
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
fn targets_reflect_and_rotate_with_the_boat_and_flows() {
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
    let target = sailor_targets(p, state, environment, intent);
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
    let mirrored = sailor_targets(
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
    let turned = sailor_targets(p, rotated, rotated_environment, intent);
    assert!((target.sheet_target_angle_rad - turned.sheet_target_angle_rad).abs() < 1e-12);
    assert_eq!(
        target.rudder_target_angle_rad,
        turned.rudder_target_angle_rad
    );
}
