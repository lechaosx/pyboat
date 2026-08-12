use std::f32::consts::PI;

use pyboat_browser::{
    Demo, PointOfSail, boat_scene, point_of_sail, viewport_interest_radius_m, windsock_extension,
    zoom_view_height_m,
};
use pyboat_game::dinghy_parameters;
use pyboat_simulation::{BoatMotion, DT_SECONDS, MotionIntent};
use rapier2d::prelude::Vector;

const SCENE_SHADER: &str = include_str!("../src/scene.wgsl");

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

    let scene = boat_scene(dinghy_parameters(), motion);

    assert_vector_close(scene.hull_m[0], Vector::new(3.0, 6.0));
    assert_vector_close(scene.sail_m[0], Vector::new(3.0, 4.4));
    assert_vector_close(scene.sail_m[1], Vector::new(5.0, 4.4));
    assert_vector_close(scene.rudder_m[0], Vector::new(3.0, 2.2));
    assert_vector_close(scene.rudder_m[1], Vector::new(3.0, 1.7));
}

#[test]
fn viewport_interest_covers_the_visible_rectangle_and_boat_margin() {
    assert!((viewport_interest_radius_m(4.0 / 3.0, 15.0) - 15.0).abs() < 1.0e-5);
}

#[test]
fn wheel_zoom_scales_without_an_artificial_view_limit() {
    let zoomed_out = zoom_view_height_m(15.0, 1_000.0);
    let restored = zoom_view_height_m(zoomed_out, -1_000.0);

    assert!(zoomed_out > 15.0);
    assert!((restored - 15.0).abs() < 1.0e-4);
}

#[test]
fn windsock_progressively_lifts_into_apparent_wind() {
    let calm = windsock_extension(0.0);
    let moderate = windsock_extension(4.0);
    let strong = windsock_extension(8.0);

    assert_eq!(calm, 0.0);
    assert!((moderate - 0.5_f32.sqrt()).abs() < 1.0e-5);
    assert!(moderate < strong);
    assert!(strong < 1.0);
}

#[test]
fn scene_shader_samples_seeded_terrain_per_fragment() {
    let module =
        naga::front::wgsl::parse_str(SCENE_SHADER).expect("scene shader should be valid WGSL");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("scene shader should satisfy WebGPU validation");
    assert!(SCENE_SHADER.contains("fn procedural_terrain_elevation_m"));
    assert!(SCENE_SHADER.contains("procedural_terrain_elevation_m(input.world_position_m)"));
    assert!(SCENE_SHADER.contains("match_seed: vec2<u32>"));
    assert!(SCENE_SHADER.contains("wavelength_m = 8192.0"));
    assert!(SCENE_SHADER.contains("wavelength_m = 1024.0"));
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

#[test]
fn demo_contains_a_distinct_practice_boat() {
    let demo = Demo::new();
    let scenes = demo.scenes();

    assert_eq!(scenes.len(), 2);
    assert!((scenes[1].rudder_m[0] - scenes[0].rudder_m[0]).length() > 4.0);
}

#[test]
fn demo_reports_a_collision_with_the_practice_boat() {
    let mut demo = Demo::new();
    demo.set_motion_intent(1.0, 0.0);

    let collided = (0..30 * 60).any(|_| {
        demo.advance(DT_SECONDS);
        !demo.contacts().is_empty()
    });

    assert!(collided);
}
