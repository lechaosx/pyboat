use std::f32::consts::{PI, TAU};

use rapier2d::prelude::{Rotation, Vector};

use crate::{BoatScene, Windsock, windsock_extension};

pub const HULL_IMAGE: &[u8] = include_bytes!("../assets/wooden-dinghy-painted.png");
pub const RIG_IMAGE: &[u8] = include_bytes!("../assets/boat-rig-painted.png");
const BOOM_IMAGE: &[u8] = include_bytes!("../assets/boat-boom-painted.png");
pub const BOAT_IMAGES: [&[u8]; 3] = [HULL_IMAGE, RIG_IMAGE, BOOM_IMAGE];

const RUDDER: [f32; 4] = [16.0, 192.0, 627.0, 335.0];
const MAST: [f32; 4] = [634.0, 94.0, 931.0, 394.0];
const BLOCK: [f32; 4] = [110.0, 511.0, 400.0, 665.0];
const ROPE: [f32; 4] = [530.0, 691.0, 982.0, 795.0];
// Matches the bow coil's roughly 10-pixel strands on the 1565-pixel, 4.05 m hull.
const ROPE_RADIUS_M: f32 = 0.014;
const CLOTH: [f32; 4] = [48.0, 985.0, 473.0, 1404.0];
const SOCK: [f32; 4] = [509.0, 1085.0, 1017.0, 1338.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatSpriteVertex {
    pub image: usize,
    pub position_m: Vector,
    pub uv: [f32; 2],
    pub shade: f32,
    pub opacity: f32,
}

fn textured(position_m: Vector, rect: [f32; 4], u: f32, v: f32) -> BoatSpriteVertex {
    BoatSpriteVertex {
        image: 1,
        position_m,
        uv: [
            (rect[0] + (rect[2] - rect[0]) * u) / 1024.0,
            (rect[1] + (rect[3] - rect[1]) * v) / 1536.0,
        ],
        shade: 1.0,
        opacity: 1.0,
    }
}

pub fn hull_sprite_mesh(scene: &BoatScene) -> Vec<BoatSpriteVertex> {
    let stern = (scene.hull_m[7] + scene.hull_m[8]) * 0.5;
    let forward = (scene.hull_m[0] - stern).normalize();
    let port = Vector::new(-forward.y, forward.x);
    let center = (scene.hull_m[0] + stern) * 0.5;
    let length = (scene.hull_m[0] - stern).length();
    let beam = scene
        .hull_m
        .iter()
        .map(|p| (p - center).dot(port).abs())
        .fold(0.0, f32::max)
        * 2.0;
    let size = [1671.0, 941.0];
    let [left, top, right, bottom] = [65.0, 180.0, 1630.0, 760.0];
    // One affine map preserves straight planks and benches.
    [0, 1, 2, 0, 2, 3]
        .map(|i| {
            let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]][i];
            let x = (uv[0] * size[0] - (left + right) * 0.5) / (right - left);
            let y = ((top + bottom) * 0.5 - uv[1] * size[1]) / (bottom - top);
            BoatSpriteVertex {
                image: 0,
                position_m: center + forward * (x * length) + port * (y * beam),
                uv,
                shade: 1.0,
                opacity: 1.0,
            }
        })
        .to_vec()
}

fn sail_load(scene: &BoatScene) -> (f32, f32) {
    let boom = scene.sail_m[1] - scene.sail_m[0];
    let normal = Vector::new(-boom.y, boom.x).normalize();
    let across = scene.apparent_wind_velocity_m_s.dot(normal);
    let load = across / (across * across + 4.0).sqrt();
    let speed = scene.apparent_wind_velocity_m_s.length();
    // Pressure takes up membrane slack; a flutter floor would keep loaded cloth waving.
    let flutter = 0.12 * speed / (speed + 2.0) * (1.0 - load * load).powi(2);
    (load, flutter)
}

pub fn sail_mesh(scene: &BoatScene, time: f64) -> Vec<BoatSpriteVertex> {
    let boom = scene.sail_m[1] - scene.sail_m[0];
    let length = boom.length();
    let normal = Vector::new(-boom.y, boom.x) / length;
    let (load, flutter) = sail_load(scene);
    let mut vertices = Vec::new();
    const LENGTH_STEPS: usize = 24;
    const HEIGHT_STEPS: usize = 12;
    // A triangular main has a vertical luff at the mast and a foot on the boom.
    // Only camber and twist give its upright surface width in overhead projection.
    for y in 0..HEIGHT_STEPS {
        for x in 0..LENGTH_STEPS {
            let a = x as f32 / LENGTH_STEPS as f32;
            let b = (x + 1) as f32 / LENGTH_STEPS as f32;
            let c = y as f32 / HEIGHT_STEPS as f32;
            let d = (y + 1) as f32 / HEIGHT_STEPS as f32;
            for (section, height) in [(a, c), (b, c), (b, d), (a, c), (b, d), (a, d)] {
                let draft = 0.4;
                let envelope = if section <= draft {
                    1.0 - ((section - draft) / draft).powi(2)
                } else {
                    1.0 - ((section - draft) / (1.0 - draft)).powi(2)
                };
                let chord = 1.0 - height;
                let along = section * chord;
                let ripple = (time * 13.0 - f64::from(along * TAU * 1.7)).sin() as f32;
                let twist = (0.25 * load + flutter * ripple) * height;
                let point = scene.sail_m[0]
                    + Rotation::new(twist) * boom * along
                    + normal
                        * (length
                            * chord
                            * envelope
                            * (PI * height).sin()
                            * (0.18 * load + flutter * ripple * height));
                // UVs use material distance, so sewn panels narrow with the triangle.
                let mut vertex = textured(point, CLOTH, along, 1.0 - height);
                vertex.shade = (0.76
                    + 0.24 * (PI * height * 0.8).sin()
                    + (along * TAU * 3.0 + height * 2.0).cos() * flutter)
                    .clamp(0.0, 1.0);
                if scene.wreck {
                    vertex.shade *= 0.65;
                }
                vertices.push(vertex);
            }
        }
    }
    vertices
}

pub fn rope_curve(
    start: Vector,
    end: Vector,
    extra_length_m: f32,
    wind: Vector,
    time: f64,
) -> [Vector; 13] {
    let delta = end - start;
    let length = delta.length();
    let normal = if length > 0.0 {
        Vector::new(-delta.y, delta.x) / length
    } else {
        Vector::ZERO
    };
    // Small-sag cable geometry: excess length = 8*sag²/(3*span). Only its
    // horizontal wind deflection is visible in a top-down view.
    let sag = (3.0 * length * extra_length_m / 8.0).sqrt();
    let sway = wind.dot(normal) / (wind.length() + 1.0) * (0.7 + 0.3 * (time * 2.3).sin() as f32);
    std::array::from_fn(|i| {
        let t = i as f32 / 12.0;
        start + delta * t + normal * (sag * 4.0 * t * (1.0 - t) * sway)
    })
}

pub fn boat_mesh(scene: &BoatScene, time: f64, pixel_footprint_m: f32) -> Vec<BoatSpriteVertex> {
    let mut vertices = Vec::new();
    let stern = (scene.hull_m[7] + scene.hull_m[8]) * 0.5;
    let forward = (scene.hull_m[0] - stern).normalize();
    let port = Vector::new(-forward.y, forward.x);
    let mast = scene.sail_m[0];
    let boom = scene.sail_m[1] - mast;
    let center = (scene.hull_m[0] + stern) * 0.5;
    let (load, _) = sail_load(scene);
    let wind = scene.apparent_wind_velocity_m_s;

    let rudder_axis = (scene.rudder_m[0] - scene.rudder_m[1]).normalize();
    for (rect, scale) in [
        (
            [RUDDER[0], RUDDER[1], 230.0, RUDDER[3]],
            [
                (scene.rudder_m[1] - scene.rudder_m[0]).length() / 205.0,
                0.0011,
            ],
        ),
        (
            [230.0, RUDDER[1], RUDDER[2], RUDDER[3]],
            [
                (scene.tiller_end_m - scene.rudder_m[0]).length() / 387.0,
                0.0011,
            ],
        ),
    ] {
        stamp(
            &mut vertices,
            scene.rudder_m[0],
            rudder_axis,
            scale,
            rect,
            [230.0, 263.0],
        );
    }

    let length = (scene.hull_m[0] - stern).length();
    let pixel_m = length / 1565.0;
    // Rail bolts at source (930,211)/(930,729), and the bow's bronze eye.
    // Mirror the fittings around the structural centerline rather than the
    // slightly asymmetric painted highlights.
    for anchor in [
        center + forward * (770.5 * pixel_m),
        center + forward * (82.5 * pixel_m) + port * (259.0 * pixel_m),
        center + forward * (82.5 * pixel_m) - port * (259.0 * pixel_m),
    ] {
        rope(
            &mut vertices,
            mast,
            anchor,
            0.0,
            wind,
            time,
            ROPE_RADIUS_M,
            pixel_footprint_m,
        );
    }

    // The bridle and sheet lie ahead of the tiller's entire sweep. All rope
    // geometry is below the cloth in this top-down projection.
    let sheet_deck = center - forward * (327.5 * pixel_m);
    let sheet_boom = mast + boom * 0.45;
    let sheet_direction = (sheet_boom - sheet_deck).normalize();
    let lower_sheave = sheet_deck + sheet_direction * 0.056;
    let upper_sheave = sheet_boom - sheet_direction * 0.056;
    for side in [-1.0, 1.0] {
        let bridle = sheet_deck + port * (side * 259.0 * pixel_m);
        rope(
            &mut vertices,
            bridle,
            sheet_deck,
            0.0,
            wind,
            time,
            ROPE_RADIUS_M,
            pixel_footprint_m,
        );
    }
    for side in [-1.0, 1.0] {
        let offset = port * (side * 0.015);
        rope(
            &mut vertices,
            lower_sheave + offset,
            upper_sheave + offset,
            0.04 * (1.0 - load * load),
            wind,
            time,
            ROPE_RADIUS_M,
            pixel_footprint_m,
        );
    }
    stamp(
        &mut vertices,
        sheet_deck,
        -sheet_direction,
        [0.00055; 2],
        BLOCK,
        [357.0, 589.0],
    );

    let boom_start = vertices.len();
    let scale = boom.length() / (2139.0 - 64.0);
    stamp(
        &mut vertices,
        mast,
        boom.normalize(),
        [scale; 2],
        [28.0, 310.0, 2143.0, 398.0],
        [64.0, 354.0],
    );
    for vertex in &mut vertices[boom_start..] {
        vertex.image = 2;
        vertex.uv[0] *= 1024.0 / 2172.0;
        vertex.uv[1] *= 1536.0 / 724.0;
    }
    stamp(
        &mut vertices,
        sheet_boom,
        sheet_direction,
        [0.00055; 2],
        BLOCK,
        [357.0, 589.0],
    );
    vertices.extend(sail_mesh(scene, time));
    stamp(&mut vertices, mast, port, [0.0004; 2], MAST, [783.0, 260.0]);
    vertices
}

pub fn windsock_mesh(windsock: Windsock, time: f64) -> Vec<BoatSpriteVertex> {
    let mut vertices = Vec::new();
    let speed = windsock.apparent_wind_velocity_m_s.length();
    let direction = if speed > f32::EPSILON {
        windsock.apparent_wind_velocity_m_s / speed
    } else {
        Vector::X
    };
    let normal = Vector::new(-direction.y, direction.x);
    let extension = windsock_extension(speed);
    let response = speed / (speed + 2.0);
    // Approximate downward sag with thirty percent of the remaining axial length;
    // exposed stripes retain their authored scale and width.
    for band in 0..6 {
        let left = -1.0 + 2.0 * band as f32 / 6.0;
        let right = -1.0 + 2.0 * (band + 1) as f32 / 6.0;
        for tail in [false, true] {
            if (!tail && extension <= f32::EPSILON) || (tail && 1.0 - extension <= f32::EPSILON) {
                continue;
            }
            for step in 0..24 {
                let a = step as f32 / 24.0;
                let b = (step + 1) as f32 / 24.0;
                for (t, side) in [
                    (a, left),
                    (b, left),
                    (b, right),
                    (a, left),
                    (b, right),
                    (a, right),
                ] {
                    let crease =
                        extension + 0.24 * extension * (1.0 - extension) * (1.0 - side * side);
                    let u = if tail {
                        crease + (1.0 - crease) * t
                    } else {
                        crease * t
                    };
                    let phase = time * 4.5 - f64::from(u) * 6.0;
                    let flutter = 0.025 * response * u * u;
                    let sway = flutter * phase.sin() as f32;
                    let axial = flutter * phase.cos() as f32;
                    let folded = (u - crease).max(0.0) / (1.0 - crease).max(f32::EPSILON);
                    let along = 1.2 * u.min(crease) + 0.36 * (u - crease).max(0.0);
                    let point = windsock.anchor_m
                        + direction * (along + axial)
                        + normal * (sway + side * 0.125);
                    let mut vertex = textured(point, SOCK, u, (1.0 - side) * 0.5);
                    vertex.shade = 1.0 - 0.3 * (PI * folded).sin();
                    vertices.push(vertex);
                }
            }
        }
    }

    vertices
}

fn stamp(
    vertices: &mut Vec<BoatSpriteVertex>,
    origin: Vector,
    axis: Vector,
    scale: [f32; 2],
    rect: [f32; 4],
    pivot: [f32; 2],
) {
    let normal = Vector::new(-axis.y, axis.x);
    for (u, v) in [
        (0.0, 0.0),
        (1.0, 0.0),
        (1.0, 1.0),
        (0.0, 0.0),
        (1.0, 1.0),
        (0.0, 1.0),
    ] {
        let x = rect[0] + (rect[2] - rect[0]) * u - pivot[0];
        let y = pivot[1] - rect[1] - (rect[3] - rect[1]) * v;
        vertices.push(textured(
            origin + axis * (x * scale[0]) + normal * (y * scale[1]),
            rect,
            u,
            v,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn rope(
    vertices: &mut Vec<BoatSpriteVertex>,
    start: Vector,
    end: Vector,
    slack: f32,
    wind: Vector,
    time: f64,
    radius: f32,
    pixel: f32,
) {
    let points = rope_curve(start, end, slack, wind, time);
    let mut distance = 0.0;
    for pair in points.windows(2) {
        let delta = pair[1] - pair[0];
        let length = delta.length();
        if length < 1.0e-6 {
            continue;
        }
        let normal = Vector::new(-delta.y, delta.x) / length;
        let outer = radius + pixel * 0.5;
        let core = (radius - pixel * 0.5).max(0.0);
        let coverage = (2.0 * radius / (core + outer)).min(1.0);
        let repeat_m = 2.0 * radius * (ROPE[2] - ROPE[0]) / (ROPE[3] - ROPE[1]);
        let first_tile = (distance / repeat_m).floor() as u32;
        let last_tile = ((distance + length) / repeat_m).floor() as u32;
        // Split at tile boundaries so interpolation never runs backwards through the atlas.
        for tile in first_tile..=last_tile {
            let start = (tile as f32 * repeat_m - distance).clamp(0.0, length);
            let end = ((tile + 1) as f32 * repeat_m - distance).clamp(0.0, length);
            if end <= start {
                continue;
            }
            for (a, b, alpha_a, alpha_b) in [
                (-outer, -core, 0.0, coverage),
                (-core, core, coverage, coverage),
                (core, outer, coverage, 0.0),
            ] {
                if a == b {
                    continue;
                }
                for (along, offset, opacity) in [
                    (start, a, alpha_a),
                    (end, a, alpha_a),
                    (end, b, alpha_b),
                    (start, a, alpha_a),
                    (end, b, alpha_b),
                    (start, b, alpha_b),
                ] {
                    let u = ((distance + along) / repeat_m - tile as f32).clamp(0.0, 1.0);
                    let v = (offset / radius).clamp(-1.0, 1.0) * 0.5 + 0.5;
                    let mut vertex = textured(
                        pair[0] + delta * (along / length) + normal * offset,
                        ROPE,
                        u,
                        v,
                    );
                    vertex.opacity = opacity;
                    vertices.push(vertex);
                }
            }
        }
        distance += length;
    }
}
