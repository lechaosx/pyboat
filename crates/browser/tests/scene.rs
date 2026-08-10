use std::f32::consts::PI;

use pyboat_browser::{Demo, PointOfSail, boat_scene, point_of_sail};
use pyboat_simulation::{BoatMotion, BoatParameters, DT_SECONDS, HullParameters, MotionIntent};
use rapier2d::prelude::Vector;

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
    }
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert!(
        (actual - expected).length() < 1.0e-5,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn boat_scene_follows_hull_and_surface_motion() {
    let motion = BoatMotion {
        position_m: Vector::new(3.0, 4.0),
        velocity_m_s: Vector::ZERO,
        angle_rad: PI / 2.0,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: PI / 2.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    };

    let scene = boat_scene(parameters(), motion);

    assert_vector_close(scene.hull_m[0], Vector::new(3.0, 6.0));
    assert_vector_close(scene.sail_m[0], Vector::new(3.0, 4.4));
    assert_vector_close(scene.sail_m[1], Vector::new(5.0, 4.4));
    assert_vector_close(scene.rudder_m[0], Vector::new(3.0, 2.2));
    assert_vector_close(scene.rudder_m[1], Vector::new(3.0, 1.7));
}

#[test]
fn demo_advances_the_real_simulation_at_fixed_ticks() {
    let mut demo = Demo::new();
    let initial = demo.motion();
    demo.set_motion_intent(1.0, 0.0);

    for _ in 0..360 {
        demo.advance(DT_SECONDS);
    }

    let advanced = demo.motion();
    assert!((advanced.position_m - initial.position_m).length() > 0.5);
    assert!(advanced.sail_angle_rad.abs() > 0.1);
}

#[test]
fn demo_clamps_and_applies_player_motion_intent() {
    let mut idle = Demo::new();
    let mut controlled = Demo::new();
    controlled.set_motion_intent(2.0, -2.0);

    assert_eq!(
        controlled.motion_intent(),
        MotionIntent {
            forward: 1.0,
            turn: -1.0,
        }
    );
    for _ in 0..360 {
        idle.advance(DT_SECONDS);
        controlled.advance(DT_SECONDS);
    }

    let idle = idle.motion();
    let controlled = controlled.motion();
    assert!((controlled.position_m - idle.position_m).length() > 0.5);
    assert!((controlled.angle_rad - idle.angle_rad).abs() > 0.1);
}

#[test]
fn demo_maps_keyboard_and_world_relative_gamepad_motion() {
    let mut demo = Demo::new();

    demo.set_player_input(1.0, -1.0, 0.0, -1.0);
    let gamepad = demo.motion_intent();
    assert!((gamepad.forward + 1.0).abs() < 1.0e-5);
    assert!((gamepad.turn - 1.0).abs() < 1.0e-5);

    demo.set_player_input(1.0, -1.0, 0.0, 0.0);
    assert_eq!(
        demo.motion_intent(),
        MotionIntent {
            forward: 1.0,
            turn: -1.0,
        }
    );

    demo.set_player_input(1.0, -1.0, 0.1, 0.1);
    let small_gamepad = demo.motion_intent();
    let mapped_magnitude = small_gamepad.forward.abs().max(small_gamepad.turn.abs());
    assert!((mapped_magnitude - 0.02_f32.sqrt()).abs() < 1.0e-5);
}

#[test]
fn point_of_sail_matches_the_python_feedback_boundaries() {
    assert_eq!(point_of_sail(0.0), PointOfSail::InIrons);
    assert_eq!(
        point_of_sail(25.0_f32.to_radians()),
        PointOfSail::CloseHauled
    );
    assert_eq!(point_of_sail(65.0_f32.to_radians()), PointOfSail::BeamReach);
    assert_eq!(
        point_of_sail(120.0_f32.to_radians()),
        PointOfSail::BroadReach
    );
    assert_eq!(point_of_sail(155.0_f32.to_radians()), PointOfSail::Run);
    assert_eq!(point_of_sail(PI), PointOfSail::Run);
}

#[test]
fn demo_view_reports_sailing_feedback() {
    let mut demo = Demo::new();
    let initial = demo.view();

    assert_eq!(initial.speed_m_s, 0.0);
    assert_eq!(initial.apparent_wind_speed_m_s, 6.0);
    assert_eq!(initial.point_of_sail, PointOfSail::CloseHauled);

    demo.set_motion_intent(1.0, 0.0);
    for _ in 0..360 {
        demo.advance(DT_SECONDS);
    }

    let underway = demo.view();
    assert!(underway.speed_m_s > 0.5);
    assert!(underway.apparent_wind_speed_m_s.is_finite());
}
