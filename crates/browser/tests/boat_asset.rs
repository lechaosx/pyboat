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
fn hull_sprite_follows_the_boat_without_changing_its_texture_coordinates() {
    let original = scene(Vector::ZERO);
    let rotation = Rotation::new(1.2);
    let translation = Vector::new(25.0, -13.0);
    let mut transformed = original;
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
fn filled_sail_has_more_draft_forward_than_at_the_mirrored_aft_section() {
    let scene = scene(Vector::new(0.0, 8.0));
    let boom = scene.sail_m[1] - scene.sail_m[0];
    let section = |fraction: f32| {
        (0..4)
            .map(|phase| {
                let time = phase as f64 * std::f64::consts::FRAC_PI_2 / 13.0;
                sail_mesh(&scene, time)
                    .iter()
                    .filter(|v| {
                        let along =
                            (v.position_m - scene.sail_m[0]).dot(boom) / boom.length_squared();
                        (along - fraction).abs() < 1.0e-6
                    })
                    .map(|v| v.position_m.y)
                    .fold(0.0, f32::max)
            })
            .sum::<f32>()
            / 4.0
    };
    assert!(section(0.25) > section(0.75) + 1.0e-5);
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
fn partial_windsock_crops_the_texture_instead_of_rescaling_all_stripes() {
    let mesh = |speed| {
        browser::windsock_mesh(
            browser::Windsock {
                anchor_m: Vector::ZERO,
                apparent_wind_velocity_m_s: Vector::new(speed, 0.0),
            },
            0.0,
        )
    };
    let uv_span = |vertices: &[browser::BoatSpriteVertex]| {
        let bounds = vertices
            .iter()
            .fold([f32::INFINITY, f32::NEG_INFINITY], |b, v| {
                [b[0].min(v.uv[0]), b[1].max(v.uv[0])]
            });
        bounds[1] - bounds[0]
    };
    let partial = mesh(3.0);
    let full = mesh(8.0);
    assert!((uv_span(&partial) / uv_span(&full) - browser::windsock_extension(3.0)).abs() < 1.0e-6);
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
    let mut transformed = original;
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
