use simulation::{ControlInput, MotionIntent, SurfaceEffect, continuous_yaw_rate_move_control};

fn target(samples: &[(f64, f64)], turn: f64) -> f64 {
    let input = ControlInput {
        boat_angular_velocity_rad_s: 0.0,
        free_sail_angle_rad: 0.0,
        sail_angle_rad: 0.0,
        apparent_wind_angle_from_boat_forward_rad: 0.0,
        sail_side_hysteresis_angle_rad: 0.0,
        max_rudder_angle_rad: 1.0,
        sail_effects: vec![SurfaceEffect {
            angle_rad: 0.0,
            drive_force_n: 0.0,
            yaw_moment_n_m: 0.0,
        }],
        rudder_effects: samples
            .iter()
            .map(|&(angle_rad, yaw_moment_n_m)| SurfaceEffect {
                angle_rad,
                yaw_moment_n_m,
                drive_force_n: 0.0,
            })
            .collect(),
    };
    continuous_yaw_rate_move_control(&input, MotionIntent { forward: 0.0, turn }, 1.0)
        .rudder_target_angle_rad
}

#[test]
fn fractional_turn_inverts_between_sampled_moments() {
    assert!((target(&[(-1.0, -10.0), (0.0, 0.0), (1.0, 10.0)], 0.43) - 0.43).abs() < 1e-12);
    assert!((target(&[(-1.0, -10.0), (0.0, 0.0), (1.0, 10.0)], -0.43) + 0.43).abs() < 1e-12);
}

#[test]
fn unreachable_moment_uses_closest_endpoint() {
    assert_eq!(target(&[(-1.0, 2.0), (0.2, 3.0), (1.0, 4.0)], 0.0), -1.0);
    assert_eq!(target(&[(-1.0, -2.0), (1.0, 2.0)], 1.0), 1.0);
}

#[test]
fn multiple_roots_choose_smallest_absolute_angle() {
    assert!(
        (target(&[(-1.0, -2.0), (-0.5, 2.0), (0.0, -2.0), (1.0, 2.0)], 0.0) + 0.25).abs() < 1e-12
    );
}

#[test]
fn flat_segment_chooses_nearest_zero_in_its_interval() {
    assert_eq!(target(&[(-0.5, 0.0), (0.7, 0.0)], 0.0), 0.0);
    assert_eq!(target(&[(0.2, 0.0), (0.7, 0.0)], 0.0), 0.2);
}

#[test]
fn tiny_moment_differences_retain_existing_tie_tolerance() {
    assert_eq!(target(&[(-1.0, -1e-8), (0.0, 1e-8), (1.0, 2e-8)], 0.0), 0.0);
}
