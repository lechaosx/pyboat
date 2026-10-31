use std::f32::consts::PI;

use browser::{
    Demo, PointOfSail, boat_scene, bounded_canvas_dimensions, point_of_sail, scale_bar,
    view_width_m, viewport_interest_radius_m, windsock_extension, zoom_view_height_m,
};
use game::dinghy_parameters;
use rapier2d::prelude::Vector;
use simulation::{BoatMotion, DT_SECONDS, MotionIntent};

const SCENE_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/scene-production.wgsl"));

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

    let parameters = dinghy_parameters();
    let scene = boat_scene(parameters, motion);

    assert_vector_close(
        scene.hull_m[0],
        Vector::new(3.0, 4.0 + parameters.hull.length_m / 2.0),
    );
    assert_vector_close(scene.sail_m[0], Vector::new(3.0, 4.4));
    assert_vector_close(scene.sail_m[1], Vector::new(5.0, 4.4));
    assert_vector_close(
        scene.rudder_m[0],
        Vector::new(3.0, 4.0 + parameters.rudder_position_m.x),
    );
    assert_vector_close(
        scene.rudder_m[1],
        Vector::new(
            3.0,
            4.0 + parameters.rudder_position_m.x - parameters.rudder_draw_length_m,
        ),
    );
}

#[test]
fn viewport_interest_covers_the_visible_rectangle_and_boat_margin() {
    assert!((viewport_interest_radius_m(4.0 / 3.0, 15.0) - 15.0).abs() < 1.0e-5);
}

#[test]
fn viewport_interest_remains_finite_at_extreme_zoom() {
    assert!(viewport_interest_radius_m(4.0, f32::MAX).is_finite());
    assert!(view_width_m(4.0, f32::MAX).is_finite());
    let scale = scale_bar(f32::MAX, 1.0);
    assert!(scale.distance_m.is_finite());
    assert!(scale.width_px.is_finite());
}

#[test]
fn canvas_dimensions_preserve_aspect_ratio_within_the_device_limit() {
    assert_eq!(
        bounded_canvas_dimensions(800, 600, 2.0, 8_192),
        (1_600, 1_200)
    );
    assert_eq!(
        bounded_canvas_dimensions(10_000, 5_000, 2.0, 8_192),
        (8_192, 4_096),
    );
}

#[test]
fn wheel_zoom_scales_without_an_artificial_view_limit() {
    let zoomed_out = zoom_view_height_m(15.0, 1_000.0);
    let restored = zoom_view_height_m(zoomed_out, -1_000.0);

    assert!(zoomed_out > 15.0);
    assert!((restored - 15.0).abs() < 1.0e-4);
}

#[test]
fn scale_bar_uses_real_world_distance_at_every_zoom() {
    let normal = scale_bar(15.0, 900.0);
    assert_eq!(normal.distance_m, 2.0);
    assert_eq!(normal.label, "2 m");
    assert!((normal.width_px - 120.0).abs() < 1.0e-5);

    let far = scale_bar(15_000.0, 900.0);
    assert_eq!(far.distance_m, 2_000.0);
    assert_eq!(far.label, "2 km");
    assert!((far.width_px - 120.0).abs() < 1.0e-5);

    let close = scale_bar(0.15, 900.0);
    assert_eq!(close.distance_m, 0.02);
    assert_eq!(close.label, "2 cm");
    assert!((close.width_px - 120.0).abs() < 1.0e-4);
}

#[test]
fn windsock_reveals_one_band_per_three_knots() {
    const THREE_KNOTS_M_S: f32 = 1.543_333_4;

    assert_eq!(windsock_extension(0.0), 0.0);
    assert!((windsock_extension(THREE_KNOTS_M_S) - 0.2).abs() < 1.0e-5);
    assert!((windsock_extension(2.0 * THREE_KNOTS_M_S) - 0.4).abs() < 1.0e-5);
    assert_eq!(windsock_extension(5.0 * THREE_KNOTS_M_S), 1.0);
    assert_eq!(windsock_extension(20.0), 1.0);
}

#[test]
fn scene_shader_satisfies_webgpu_validation() {
    let module =
        naga::front::wgsl::parse_str(SCENE_SHADER).expect("scene shader should be valid WGSL");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("scene shader should satisfy WebGPU validation");
}

#[test]
fn boat_sprite_shader_satisfies_webgpu_validation() {
    let shader = include_str!(concat!(env!("OUT_DIR"), "/boat-sprite.wgsl"));
    let module = naga::front::wgsl::parse_str(shader).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
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
    let mut straight = Demo::new();
    let mut controlled = Demo::new();
    straight.set_motion_intent(1.0, 0.0);
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
        straight.advance(DT_SECONDS);
        controlled.advance(DT_SECONDS);
    }

    let idle = idle.motion();
    let controlled = controlled.motion();
    assert!((controlled.position_m - idle.position_m).length() > 0.5);
    assert!((controlled.angle_rad - straight.motion().angle_rad).abs() > 0.1);
}

#[test]
fn demo_maps_keyboard_and_world_relative_motion() {
    let mut demo = Demo::new();

    demo.set_player_input(1.0, -1.0, 0.0, 1.0);
    let gamepad = demo.motion_intent();
    assert!((gamepad.forward - 1.0).abs() < 1.0e-5);
    assert!((gamepad.turn + 1.0).abs() < 1.0e-5);

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
fn point_of_sail_matches_the_feedback_boundaries() {
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

    let collided = (0..30 * 60).any(|_| {
        let scenes = demo.scenes();
        let toward_practice = (scenes[1].sail_m[0] - scenes[0].sail_m[0]).normalize();
        demo.set_player_input(0.0, 0.0, toward_practice.x, toward_practice.y);
        demo.advance(DT_SECONDS);
        !demo.contacts().is_empty()
    });

    assert!(collided);
}
