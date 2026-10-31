use browser::{
    BoatScene, ClothMotion, ClothPose, ClothPresentation, Windsock, boat_scene,
    sail_mesh_with_cloth, windsock_mesh_with_cloth,
};
use game::dinghy_parameters;
use rapier2d::prelude::{Rotation, Vector};
use simulation::BoatMotion;

fn scene(wind: Vector) -> BoatScene {
    let mut scene = boat_scene(
        dinghy_parameters(),
        BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::ZERO,
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        },
    );
    scene.apparent_wind_velocity_m_s = wind;
    scene
}

fn coordinates(pose: ClothPose) -> [f32; 5] {
    [
        pose.sail_angle_rad,
        pose.sail_load,
        pose.flutter_response,
        pose.sock_angle_rad,
        pose.sock_extension,
    ]
}

#[test]
fn held_wind_response_is_independent_of_render_frame_partition() {
    let initial = scene(Vector::new(2.0, -2.0));
    let changed = scene(Vector::new(4.0, 3.0));
    let mut one_frame = ClothMotion::new(&initial);
    let mut many_frames = ClothMotion::new(&initial);
    one_frame.advance(&changed, 1.2);
    for _ in 0..120 {
        many_frames.advance(&changed, 0.01);
    }
    for (one, many) in coordinates(one_frame.pose())
        .into_iter()
        .zip(coordinates(many_frames.pose()))
    {
        assert!((one - many).abs() < 1.0e-5, "{one} != {many}");
    }
}

#[test]
fn reversing_wind_keeps_cloth_momentum_before_it_turns_back() {
    let initial = scene(Vector::new(4.0, 0.0));
    let forward = scene(Vector::new(0.0, 4.0));
    let reverse = scene(Vector::new(0.0, -4.0));
    let mut motion = ClothMotion::new(&initial);
    motion.advance(&forward, 0.08);
    let before = motion.pose();
    motion.advance(&reverse, 0.001);
    let after = motion.pose();
    assert!(after.sock_angle_rad > before.sock_angle_rad);
    assert!(after.sail_load < before.sail_load);
    motion.advance(&reverse, 4.0);
    let settled = ClothMotion::new(&reverse).pose();
    assert!((motion.pose().sock_angle_rad - settled.sock_angle_rad).abs() < 1.0e-5);
    assert!((motion.pose().sail_load - settled.sail_load).abs() < 1.0e-5);
}

#[test]
fn cloth_has_zero_initial_velocity_and_accelerates_under_new_load() {
    let initial = scene(Vector::ZERO);
    let changed = scene(Vector::new(0.0, 4.0));
    let mut short = ClothMotion::new(&initial);
    let mut long = ClothMotion::new(&initial);
    short.advance(&changed, 0.001);
    long.advance(&changed, 0.002);
    let ratio = long.pose().sail_load / short.pose().sail_load;
    assert!(
        (3.8..4.2).contains(&ratio),
        "initial displacement should grow with time squared, got ratio {ratio}"
    );
}

#[test]
fn stopped_wind_retains_then_damps_cloth_motion_without_resetting_heading() {
    let windy = scene(Vector::new(0.0, 6.0));
    let calm = scene(Vector::ZERO);
    let mut motion = ClothMotion::new(&windy);
    let before = motion.pose();
    motion.advance(&calm, 0.0);
    assert_eq!(before, motion.pose());
    motion.advance(&calm, 0.05);
    assert!(motion.pose().flutter_response > 0.0);
    assert!(motion.pose().sock_extension > 0.0);
    motion.advance(&calm, 10.0);
    assert!(motion.pose().flutter_response < 1.0e-5);
    assert!(motion.pose().sock_extension < 1.0e-5);
    assert!((motion.pose().sock_angle_rad - before.sock_angle_rad).abs() < 1.0e-5);
}

#[test]
fn angle_wrap_takes_the_short_turn_and_long_pauses_remain_finite() {
    let wind = |angle: f32| Rotation::new(angle.to_radians()) * Vector::new(4.0, 0.0);
    let initial = scene(wind(179.0));
    let changed = scene(wind(-179.0));
    let mut motion = ClothMotion::new(&initial);
    motion.advance(&changed, 0.2);
    assert!((motion.pose().sock_angle_rad - 179.0_f32.to_radians()).abs() < 2.0_f32.to_radians());
    let before = motion.pose();
    for dt in [0.0, -1.0, f64::NAN] {
        motion.advance(&changed, dt);
    }
    assert_eq!(before, motion.pose());
    motion.advance(&changed, 86_400.0);
    assert!(coordinates(motion.pose()).into_iter().all(f32::is_finite));
}

#[test]
fn sail_and_sock_stay_attached_when_their_support_moves_before_the_cloth_settles() {
    let mut scene = scene(Vector::new(0.0, 5.0));
    let mut motion = ClothMotion::new(&scene);
    let rotation = Rotation::new(0.8);
    let translation = Vector::new(8_000.0, -3.0);
    scene.sail_m = scene.sail_m.map(|point| rotation * point + translation);
    motion.advance(&scene, 0.02);
    let mesh = sail_mesh_with_cloth(&scene, 0.4, motion.pose());
    let foot_v = mesh
        .iter()
        .map(|v| v.uv[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let boom = scene.sail_m[1] - scene.sail_m[0];
    for vertex in mesh.iter().filter(|v| v.uv[1] == foot_v) {
        assert!((vertex.position_m - scene.sail_m[0]).perp_dot(boom).abs() < 0.003);
    }
    for anchor in scene.sail_m {
        assert!(
            mesh.iter()
                .any(|v| (v.position_m - anchor).length() < 0.001)
        );
    }
    let sock = windsock_mesh_with_cloth(
        Windsock {
            anchor_m: translation,
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        },
        0.4,
        motion.pose(),
    );
    let mouth_u = sock.iter().map(|v| v.uv[0]).fold(f32::INFINITY, f32::min);
    let mouth = sock
        .iter()
        .filter(|v| v.uv[0] == mouth_u)
        .map(|v| v.position_m)
        .collect::<Vec<_>>();
    let normal = Rotation::new(motion.pose().sock_angle_rad) * Vector::Y;
    let lower = mouth
        .iter()
        .min_by(|a, b| a.dot(normal).total_cmp(&b.dot(normal)))
        .unwrap();
    let upper = mouth
        .iter()
        .max_by(|a, b| a.dot(normal).total_cmp(&b.dot(normal)))
        .unwrap();
    let center = (*lower + *upper) * 0.5;
    assert!((center - translation).length() < 0.001);
}

#[test]
fn presentation_follows_identity_across_reordering_and_resets_after_removal() {
    let mut a = scene(Vector::new(4.0, 0.0));
    a.id = Some("a".into());
    let mut b = scene(Vector::new(0.0, 4.0));
    b.id = Some("b".into());
    let mut presentation = ClothPresentation::default();
    let initial = presentation.advance(&[a.clone(), b.clone()], 0.0);
    a.apparent_wind_velocity_m_s = Vector::new(0.0, -4.0);
    let reordered = presentation.advance(&[b.clone(), a.clone()], 0.0);
    assert_eq!(reordered, [initial[1], initial[0]]);
    presentation.advance(&[b], 0.1);
    assert_eq!(
        presentation.advance(&[a.clone()], 0.0),
        [ClothMotion::new(&a).pose()]
    );
}
