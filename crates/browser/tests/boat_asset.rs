use browser::{BoatScene, boat_scene, hull_sprite_mesh, sail_mesh};
use game::dinghy_parameters;
use rapier2d::prelude::{Rotation, Vector};
use simulation::BoatMotion;

#[test]
fn odd_sized_sprite_minification_preserves_coverage() {
    let mut rgba = [0; 20];
    rgba[16..].copy_from_slice(&[255, 255, 255, 255]);
    let mips = browser::sprite_mip_chain(5, 1, &rgba);
    assert_eq!(mips.last().unwrap().rgba, [124, 124, 124, 51]);
}

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

#[test]
fn wind_step_cannot_teleport_cloth_before_any_time_has_elapsed() {
    let mut scene = scene(Vector::new(0.0, 8.0));
    let mut motion = browser::ClothMotion::new(&scene);
    let before = browser::sail_mesh_with_cloth(&scene, 0.4, motion.pose());
    let sock_before = browser::windsock_mesh_with_cloth(
        browser::Windsock {
            anchor_m: scene.hull_m[0],
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        },
        0.4,
        motion.pose(),
    );
    scene.apparent_wind_velocity_m_s = Vector::new(0.0, -8.0);
    motion.advance(&scene, 0.0);
    let after = browser::sail_mesh_with_cloth(&scene, 0.4, motion.pose());
    let jump = before
        .iter()
        .zip(after)
        .map(|(a, b)| (a.position_m - b.position_m).length())
        .fold(0.0, f32::max);
    assert!(
        jump < 1.0e-6,
        "wind step moved cloth {jump} m without elapsed time"
    );
    let sock_after = browser::windsock_mesh_with_cloth(
        browser::Windsock {
            anchor_m: scene.hull_m[0],
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        },
        0.4,
        motion.pose(),
    );
    assert_eq!(sock_before, sock_after);
}

#[test]
fn hull_sprite_follows_the_boat_without_changing_its_texture_coordinates() {
    let original = scene(Vector::ZERO);
    let rotation = Rotation::new(1.2);
    let translation = Vector::new(25.0, -13.0);
    let mut transformed = original.clone();
    transformed.hull_m = original.hull_m.map(|p| rotation * p + translation);
    for (before, after) in hull_sprite_mesh(&original)
        .iter()
        .zip(hull_sprite_mesh(&transformed))
    {
        assert_eq!(before.uv, after.uv);
        assert!((rotation * before.position_m + translation - after.position_m).length() < 1.0e-5);
    }
}

#[test]
fn cloth_moves_in_wind_and_is_still_in_calm_air() {
    let calm = scene(Vector::ZERO);
    assert_eq!(sail_mesh(&calm, 0.0), sail_mesh(&calm, 0.17));
    let windy = scene(Vector::new(0.0, 8.0));
    assert_ne!(sail_mesh(&windy, 0.0), sail_mesh(&windy, 0.17));
}

#[test]
fn increasing_cross_sail_pressure_reduces_flutter() {
    let motion = |wind_speed| {
        let scene = scene(Vector::new(0.0, wind_speed));
        sail_mesh(&scene, 0.0)
            .iter()
            .zip(sail_mesh(&scene, 0.17))
            .map(|(a, b)| (a.position_m - b.position_m).length())
            .fold(0.0, f32::max)
    };
    assert!(motion(16.0) < motion(8.0));
    assert!(motion(32.0) < motion(16.0));
}

#[test]
fn hull_texture_coordinates_keep_the_artwork_affine() {
    let scene = scene(Vector::ZERO);
    let mesh = hull_sprite_mesh(&scene);
    // Any affine image mapping has one constant UV-to-world derivative.
    let a = mesh[0];
    let b = mesh[1];
    let c = mesh[2];
    let ab = Vector::from(b.uv) - Vector::from(a.uv);
    let ac = Vector::from(c.uv) - Vector::from(a.uv);
    let determinant = ab.perp_dot(ac);
    for vertex in mesh {
        let uv = Vector::from(vertex.uv) - Vector::from(a.uv);
        let predicted = a.position_m
            + (b.position_m - a.position_m) * uv.perp_dot(ac) / determinant
            + (c.position_m - a.position_m) * ab.perp_dot(uv) / determinant;
        assert!((predicted - vertex.position_m).length() < 1.0e-5);
    }
}

#[test]
fn sprite_minification_filters_linear_premultiplied_color_to_one_texel() {
    let mips = browser::sprite_mip_chain(
        2,
        2,
        &[
            255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ],
    );
    assert_eq!(mips.len(), 2);
    assert_eq!((mips[1].width, mips[1].height), (1, 1));
    assert_eq!(mips[1].rgba, [188, 188, 188, 255]);
    let transparent = browser::sprite_mip_chain(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0]);
    assert_eq!(transparent[1].rgba, [188, 0, 0, 128]);
}

#[test]
fn slack_rope_moves_between_fixed_attachments_and_taut_rope_stays_straight() {
    let start = Vector::new(0.3, 0.5);
    let end = Vector::new(1.7, -0.3);
    let wind = Vector::new(0.0, 8.0);
    let a = browser::rope_curve(start, end, 0.08, wind, 0.0);
    let b = browser::rope_curve(start, end, 0.08, wind, 0.5);
    assert_eq!(a[0], start);
    assert!((a[12] - end).length() < 1.0e-6);
    assert_eq!(a[0], b[0]);
    assert_eq!(a[12], b[12]);
    assert_ne!(a[6], b[6]);
    let taut = browser::rope_curve(start, end, 0.0, wind, 0.0);
    assert_eq!(taut, browser::rope_curve(start, end, 0.0, wind, 0.5));
    for p in taut {
        assert!((p - start).perp_dot(end - start).abs() < 1.0e-6);
    }
}

#[test]
fn filled_windsock_flutters_along_and_across_the_wind() {
    let windsock = browser::Windsock {
        anchor_m: Vector::ZERO,
        apparent_wind_velocity_m_s: Vector::new(8.0, 0.0),
    };
    let a = browser::windsock_mesh(windsock, 0.0);
    let b = browser::windsock_mesh(windsock, 0.17);
    assert!(
        a.iter()
            .zip(&b)
            .any(|(a, b)| (a.position_m.x - b.position_m.x).abs() > 1.0e-6)
    );
    assert!(
        a.iter()
            .zip(&b)
            .any(|(a, b)| (a.position_m.y - b.position_m.y).abs() > 1.0e-6)
    );
    for (a, b) in a
        .iter()
        .zip(&b)
        .filter(|(a, _)| a.position_m.x.abs() < 1.0e-6)
    {
        assert_eq!(a.position_m, b.position_m);
    }
}

#[test]
fn partial_windsock_keeps_all_cloth_and_the_exposed_stripe_scale() {
    let mesh = |speed| {
        browser::windsock_mesh(
            browser::Windsock {
                anchor_m: Vector::ZERO,
                apparent_wind_velocity_m_s: Vector::new(speed, 0.0),
            },
            0.0,
        )
    };
    let bounds = |vertices: &[browser::BoatSpriteVertex]| {
        vertices
            .iter()
            .fold([f32::INFINITY, f32::NEG_INFINITY], |b, v| {
                [b[0].min(v.uv[0]), b[1].max(v.uv[0])]
            })
    };
    let partial = mesh(3.0);
    let full = mesh(8.0);
    assert_eq!(
        bounds(&partial),
        bounds(&full),
        "folded cloth must remain in the mesh"
    );
    let [start, end] = bounds(&full);
    for vertices in [&partial, &full] {
        for vertex in vertices
            .iter()
            .filter(|v| (v.uv[0] - start) / (end - start) < browser::windsock_extension(3.0))
        {
            let u = (vertex.uv[0] - start) / (end - start);
            assert!((vertex.position_m.x - 1.2 * u).abs() <= 0.025 * u * u + 1.0e-6);
        }
    }
}

#[test]
fn calm_windsock_keeps_finite_folded_cloth_at_the_anchor() {
    let anchor = Vector::new(2.0, -1.0);
    let mesh = browser::windsock_mesh(
        browser::Windsock {
            anchor_m: anchor,
            apparent_wind_velocity_m_s: Vector::ZERO,
        },
        1.0,
    );
    assert!(!mesh.is_empty());
    assert!(
        mesh.iter()
            .all(|v| v.position_m.x.is_finite() && v.position_m.y.is_finite())
    );
    let mouth_u = mesh.iter().map(|v| v.uv[0]).fold(f32::INFINITY, f32::min);
    let mouth: Vec<_> = mesh.iter().filter(|v| v.uv[0] == mouth_u).collect();
    let top = mouth
        .iter()
        .map(|v| v.position_m.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = mouth
        .iter()
        .map(|v| v.position_m.y)
        .fold(f32::INFINITY, f32::min);
    assert!(mouth.iter().all(|v| v.position_m.x == anchor.x));
    assert!(((top + bottom) * 0.5 - anchor.y).abs() < 1.0e-6);
    let bottom_v = mesh
        .iter()
        .map(|v| v.uv[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let top_v = mesh.iter().map(|v| v.uv[1]).fold(f32::INFINITY, f32::min);
    assert!(
        mesh.iter().all(|v| {
            let across = (bottom_v - v.uv[1]) / (bottom_v - top_v);
            (v.position_m.y - (bottom + across * (top - bottom))).abs() < 1.0e-6
        }),
        "downward sag projects along the sock axis without narrowing its cross-section"
    );
    let tip_u = mesh
        .iter()
        .map(|v| v.uv[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        mesh.iter()
            .filter(|v| v.uv[0] == tip_u)
            .all(|v| v.position_m.x > anchor.x),
        "the compressed tail still projects forward from the fold"
    );
}

#[test]
fn cloth_billows_to_leeward_and_keeps_the_mast_and_clew_attached() {
    for side in [-1.0, 1.0] {
        let scene = scene(Vector::new(0.0, side * 8.0));
        let mesh = sail_mesh(&scene, 0.3);
        assert!(mesh.iter().all(|v| v.position_m.y * side >= -1.0e-6));
        assert!(mesh.iter().any(|v| v.position_m.y * side > 0.0));
        for anchor in scene.sail_m {
            assert!(
                mesh.iter()
                    .any(|v| (v.position_m - anchor).length() < 1.0e-6)
            );
        }
    }
}

#[test]
fn cloth_follows_the_boat_pose_and_wind_in_world_coordinates() {
    let original = scene(Vector::new(0.0, 8.0));
    let rotation = Rotation::new(1.2);
    let translation = Vector::new(25.0, -13.0);
    let mut transformed = original.clone();
    transformed.sail_m = original.sail_m.map(|p| rotation * p + translation);
    transformed.apparent_wind_velocity_m_s = rotation * original.apparent_wind_velocity_m_s;
    for (before, after) in sail_mesh(&original, 0.7)
        .iter()
        .zip(sail_mesh(&transformed, 0.7))
    {
        assert!((rotation * before.position_m + translation - after.position_m).length() < 1.0e-5);
        assert!(after.shade.is_finite() && (0.0..=1.0).contains(&after.shade));
    }
}

#[test]
fn tiller_follows_the_shared_length_hinge_and_articulated_rudder_pose() {
    let mut parameters = dinghy_parameters();
    parameters.tiller_length_m = 0.93;
    let motion = BoatMotion {
        position_m: Vector::new(17.0, -8.0),
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.8,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: -0.4,
        rudder_angular_velocity_rad_s: 0.0,
    };
    let scene = boat_scene(parameters, motion);
    let axis = (scene.rudder_m[0] - scene.rudder_m[1]).normalize();
    assert!(
        (scene.tiller_end_m - scene.rudder_m[0] - axis * parameters.tiller_length_m).length()
            < 1.0e-5
    );
}

#[test]
fn triangular_sail_material_keeps_its_foot_on_the_boom_and_luff_at_the_mast() {
    let scene = scene(Vector::new(0.0, 8.0));
    let mesh = sail_mesh(&scene, 0.3);
    let uv_bounds = mesh.iter().fold(
        [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, v| {
            [
                b[0].min(v.uv[0]),
                b[1].max(v.uv[0]),
                b[2].min(v.uv[1]),
                b[3].max(v.uv[1]),
            ]
        },
    );
    let boom = scene.sail_m[1] - scene.sail_m[0];
    for vertex in mesh {
        let foot_fraction = (vertex.uv[0] - uv_bounds[0]) / (uv_bounds[1] - uv_bounds[0]);
        let height_fraction = (uv_bounds[3] - vertex.uv[1]) / (uv_bounds[3] - uv_bounds[2]);
        assert!(
            foot_fraction + height_fraction <= 1.0 + 1.0e-5,
            "cloth material must narrow toward the head of a triangular main"
        );
        if height_fraction < 1.0e-5 {
            assert!((vertex.position_m - scene.sail_m[0] - boom * foot_fraction).length() < 1.0e-5);
        }
        if foot_fraction < 1.0e-5 {
            assert!(
                (vertex.position_m - scene.sail_m[0]).length() < 1.0e-5,
                "the entire vertical luff projects onto the mast from overhead"
            );
        }
    }
}

#[test]
fn sail_foot_samples_opaque_canvas_at_its_boom_attachment() {
    let mut reader = png::Decoder::new(std::io::Cursor::new(browser::RIG_IMAGE))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let mesh = sail_mesh(&scene(Vector::new(0.0, 8.0)), 0.3);
    let foot_v = mesh
        .iter()
        .map(|v| v.uv[1])
        .fold(f32::NEG_INFINITY, f32::max);
    for vertex in mesh.iter().filter(|v| v.uv[1] == foot_v) {
        let x = (vertex.uv[0] * info.width as f32).floor() as usize;
        let y = (vertex.uv[1] * info.height as f32).floor() as usize;
        assert!(
            bytes[4 * (y * info.width as usize + x) + 3] >= 250,
            "transparent atlas padding must not separate the sail foot from its boom"
        );
    }
}
