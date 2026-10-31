use std::f32::consts::{PI, TAU};

use rapier2d::prelude::Vector;

use crate::{BoatScene, Windsock, windsock_extension};

pub const HULL_IMAGE: &[u8] = include_bytes!("../assets/wooden-dinghy-painted.png");
pub const RIG_IMAGE: &[u8] = include_bytes!("../assets/boat-rig-painted.png");
pub const BOAT_IMAGES: [&[u8]; 2] = [HULL_IMAGE, RIG_IMAGE];

const RUDDER: [f32; 4] = [16.0, 192.0, 627.0, 335.0];
const MAST: [f32; 4] = [634.0, 94.0, 931.0, 394.0];
const BLOCK: [f32; 4] = [110.0, 511.0, 400.0, 665.0];
const CLEAT: [f32; 4] = [76.0, 681.0, 441.0, 810.0];
const BOOM: [f32; 4] = [10.0, 829.0, 566.0, 910.0];
const ROPE: [f32; 4] = [530.0, 691.0, 982.0, 795.0];
// Matches the bow coil's roughly 10-pixel strands on the 1565-pixel, 4 m hull.
const ROPE_RADIUS_M: f32 = 0.014;
const CLOTH: [f32; 4] = [30.0, 962.0, 489.0, 1419.0];
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
    const WIDTH_STEPS: usize = 6;
    for x in 0..LENGTH_STEPS {
        for y in 0..WIDTH_STEPS {
            let a = x as f32 / LENGTH_STEPS as f32;
            let b = (x + 1) as f32 / LENGTH_STEPS as f32;
            let c = y as f32 / WIDTH_STEPS as f32;
            let d = (y + 1) as f32 / WIDTH_STEPS as f32;
            for (along, across) in [(a, c), (b, c), (b, d), (a, c), (b, d), (a, d)] {
                // A forward draft with a flatter exit is a projected cloth approximation.
                let draft = 0.4;
                let envelope = if along <= draft {
                    1.0 - ((along - draft) / draft).powi(2)
                } else {
                    1.0 - ((along - draft) / (1.0 - draft)).powi(2)
                };
                let ripple = (time * 13.0 - f64::from(along * TAU * 1.7)).sin() as f32;
                let point = scene.sail_m[0]
                    + boom * along
                    + normal
                        * (length * envelope * across * (0.30 * load + flutter * ripple * across));
                let mut vertex = textured(point, CLOTH, along, 1.0 - across);
                vertex.shade = (0.76
                    + 0.24 * (PI * across * 0.8).sin()
                    + (along * TAU * 3.0 + across * 2.0).cos() * flutter)
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
            [0.55 / 387.0, 0.0011],
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

    for anchor in [
        scene.hull_m[0] - forward * 0.12,
        mast - forward * 0.35 + port * 0.67,
        mast - forward * 0.35 - port * 0.67,
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
        stamp(
            &mut vertices,
            anchor,
            forward,
            [0.00044; 2],
            CLEAT,
            [258.0, 746.0],
        );
    }

    // The bridle and sheet lie ahead of the tiller's entire sweep. All rope
    // geometry is below the cloth in this top-down projection.
    let sheet_deck = center - forward * 0.35;
    let sheet_boom = mast + boom * 0.45;
    let sheet_direction = (sheet_boom - sheet_deck).normalize();
    let lower_sheave = sheet_deck + sheet_direction * 0.056;
    let upper_sheave = sheet_boom - sheet_direction * 0.056;
    for side in [-1.0, 1.0] {
        let bridle = sheet_deck + port * (side * 0.67);
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
        stamp(
            &mut vertices,
            bridle,
            forward,
            [0.00044; 2],
            CLEAT,
            [258.0, 746.0],
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

    stamp(
        &mut vertices,
        mast,
        boom.normalize(),
        [boom.length() / 490.0, 0.0009],
        BOOM,
        [48.0, 869.0],
    );
    stamp(
        &mut vertices,
        sheet_boom,
        sheet_direction,
        [0.00055; 2],
        BLOCK,
        [357.0, 589.0],
    );
    vertices.extend(sail_mesh(scene, time));
    stamp(
        &mut vertices,
        mast,
        port,
        [0.00068; 2],
        MAST,
        [783.0, 260.0],
    );
    vertices
}

pub fn windsock_mesh(windsock: Windsock, time: f64) -> Vec<BoatSpriteVertex> {
    let mut vertices = Vec::new();
    let speed = windsock.apparent_wind_velocity_m_s.length();
    if speed <= f32::EPSILON {
        return vertices;
    }
    let direction = windsock.apparent_wind_velocity_m_s / speed;
    let normal = Vector::new(-direction.y, direction.x);
    let extension = windsock_extension(speed);
    let response = speed / (speed + 2.0);
    for step in 0..24 {
        let a = extension * step as f32 / 24.0;
        let b = extension * (step + 1) as f32 / 24.0;
        for (u, side) in [
            (a, -1.0),
            (b, -1.0),
            (b, 1.0),
            (a, -1.0),
            (b, 1.0),
            (a, 1.0),
        ] {
            let phase = time * 7.0 - f64::from(u) * 6.0;
            let flutter = 0.025 * response * u * u;
            let sway = flutter * phase.sin() as f32;
            let axial = flutter * phase.cos() as f32;
            let point =
                windsock.anchor_m + direction * (1.2 * u + axial) + normal * (sway + side * 0.125);
            vertices.push(textured(point, SOCK, u, (1.0 - side) * 0.5));
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
