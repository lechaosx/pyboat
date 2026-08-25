use std::{fs, io::BufReader, process::Command};

use game::{procedural_terrain_elevation_m, procedural_world_environment};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed;

#[test]
fn rendered_terrain_matches_authoritative_rust_across_rectangles() {
    for (index, world) in [
        WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
        WorldGeometry::new(80_000.0, 40_000.0).unwrap(),
        WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
    ]
    .into_iter()
    .enumerate()
    {
        let position_m = representative_shore(world);
        let output = std::env::temp_dir().join(format!(
            "pyboat-terrain-parity-{}-{index}.png",
            std::process::id(),
        ));
        let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
            .args([
                "--scenario",
                "clear-high",
                "--width",
                "1",
                "--height",
                "1",
                "--world-width",
                &world.width_m().to_string(),
                "--world-height",
                &world.height_m().to_string(),
                "--camera-x",
                &position_m.x.to_string(),
                "--camera-y",
                &position_m.y.to_string(),
                "--view-height",
                "1",
                "--seed",
                &SEED.to_string(),
                "--debug",
                "32",
                "--output",
                output.to_str().unwrap(),
            ])
            .output()
            .expect("headless terrain parity capture should start");
        assert!(
            capture.status.success(),
            "{}",
            String::from_utf8_lossy(&capture.stderr),
        );

        let actual = read_pixel(&output);
        let elevation_m = procedural_terrain_elevation_m(world, SEED, position_m);
        let expected = signed_debug_pixel(elevation_m / 12.0);
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 3,
                "{world:?} at {position_m:?}: {actual:?} != {expected:?} for elevation {elevation_m}",
            );
        }

        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("json")).unwrap();
    }
}

#[test]
fn rendered_surface_conditions_match_authoritative_rust_at_world_positions() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let tick = 0;
    let position_m = Vector::new(1_234.5, -6_789.25);
    let sample = procedural_world_environment(world, SEED, tick, position_m);
    for (mode, expected) in [
        (
            52,
            signed_debug_pixel(sample.weather.surface_air_temperature_c / 40.0),
        ),
        (55, signed_debug_pixel(sample.water_height_m / 2.0)),
    ] {
        let actual = render_debug_pixel(world, position_m, mode);
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 8,
                "mode {mode} at {position_m:?}: {actual:?} != {expected:?}",
            );
        }
    }
}

#[test]
fn whole_world_capture_contains_spatial_seasons_ice_and_solar_time() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let climate = render_debug_image(world, 57, 32, 16);
    let solar = render_debug_image(world, 56, 32, 16);

    let climate_equator = mean_channel(&climate, 32, 7..9, 0);
    let climate_poles =
        0.5 * (mean_channel(&climate, 32, 0..2, 0) + mean_channel(&climate, 32, 14..16, 0));
    assert!(climate_equator > climate_poles + 0.25);

    let northern_snow = mean_channel(&climate, 32, 0..8, 1);
    let southern_snow = mean_channel(&climate, 32, 8..16, 1);
    assert!((northern_snow - southern_snow).abs() > 0.08);

    let ice_equator = mean_channel(&climate, 32, 7..9, 2);
    let ice_poles =
        0.5 * (mean_channel(&climate, 32, 0..2, 2) + mean_channel(&climate, 32, 14..16, 2));
    assert!(ice_poles > ice_equator + 0.25);

    let solar_range = column_luminance_range(&solar, 32, 16);
    assert!(solar_range > 0.30);
}

#[test]
fn cloud_formation_is_spatial_across_world() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let formation = render_debug_image(world, 58, 16, 8);

    assert_grayscale(&formation);
    assert!(channel_range(&formation, 0) > 0.20);
}

fn representative_shore(world: WorldGeometry) -> Vector {
    for x_index in -32..=32 {
        for y_index in -32..=32 {
            let position_m = Vector::new(
                x_index as f32 * world.width_m() / 128.0,
                y_index as f32 * world.height_m() / 128.0,
            );
            let elevation_m = procedural_terrain_elevation_m(world, SEED, position_m);
            if (-8.0..=8.0).contains(&elevation_m) {
                return position_m;
            }
        }
    }
    panic!("{world:?} should contain a terrain sample within the diagnostic range");
}

fn read_pixel(path: &std::path::Path) -> [u8; 4] {
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes[..info.buffer_size()].try_into().unwrap()
}

fn render_debug_pixel(world: WorldGeometry, position_m: Vector, mode: u32) -> [u8; 4] {
    render_debug_image_at(world, position_m, mode, 1, 1)
        .try_into()
        .unwrap()
}

fn render_debug_image(world: WorldGeometry, mode: u32, width: u32, height: u32) -> Vec<u8> {
    render_debug_image_at(world, Vector::ZERO, mode, width, height)
}

fn render_debug_image_at(
    world: WorldGeometry,
    position_m: Vector,
    mode: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-spatial-surface-{}-{mode}-{width}x{height}.png",
        std::process::id(),
    ));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
            "--scenario",
            "fair-cumulus",
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
            "--world-width",
            &world.width_m().to_string(),
            "--world-height",
            &world.height_m().to_string(),
            "--camera-x",
            &position_m.x.to_string(),
            "--camera-y",
            &position_m.y.to_string(),
            "--view-height",
            &world.height_m().to_string(),
            "--seed",
            &SEED.to_string(),
            "--time",
            "0",
            "--debug",
            &mode.to_string(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("headless spatial surface capture should start");
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr),
    );
    let bytes = read_image(&output);
    fs::remove_file(&output).unwrap();
    fs::remove_file(output.with_extension("json")).unwrap();
    bytes
}

fn read_image(path: &std::path::Path) -> Vec<u8> {
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    bytes
}

fn mean_channel(bytes: &[u8], width: usize, rows: std::ops::Range<usize>, channel: usize) -> f32 {
    let mut sum = 0.0;
    let mut count = 0;
    for row in rows {
        for column in 0..width {
            let offset = 4 * (row * width + column);
            sum += bytes[offset + channel] as f32 / 255.0;
            count += 1;
        }
    }
    sum / count as f32
}

fn column_luminance_range(bytes: &[u8], width: usize, height: usize) -> f32 {
    let columns: Vec<_> = (0..width)
        .map(|column| {
            (0..height)
                .map(|row| {
                    let offset = 4 * (row * width + column);
                    (bytes[offset] as f32 + bytes[offset + 1] as f32 + bytes[offset + 2] as f32)
                        / (3.0 * 255.0)
                })
                .sum::<f32>()
                / height as f32
        })
        .collect();
    columns.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - columns.iter().copied().fold(f32::INFINITY, f32::min)
}

fn assert_grayscale(bytes: &[u8]) {
    for pixel in bytes.chunks_exact(4) {
        assert!(pixel[0].abs_diff(pixel[1]) <= 1, "{pixel:?}");
        assert!(pixel[1].abs_diff(pixel[2]) <= 1, "{pixel:?}");
    }
}

fn channel_range(bytes: &[u8], channel: usize) -> f32 {
    let minimum = bytes
        .chunks_exact(4)
        .map(|pixel| pixel[channel])
        .min()
        .unwrap();
    let maximum = bytes
        .chunks_exact(4)
        .map(|pixel| pixel[channel])
        .max()
        .unwrap();
    f32::from(maximum - minimum) / 255.0
}

fn signed_debug_pixel(value: f32) -> [u8; 4] {
    let amount = value.clamp(-1.0, 1.0);
    let dark = [0.07, 0.10, 0.14];
    let target = if amount >= 0.0 {
        [0.92, 0.52, 0.16]
    } else {
        [0.15, 0.52, 0.86]
    };
    let magnitude = amount.abs();
    let mut pixel = [0, 0, 0, 255];
    for channel in 0..3 {
        let linear = dark[channel] + magnitude * (target[channel] - dark[channel]);
        let srgb = if linear <= 0.003_130_8 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        pixel[channel] = (255.0 * srgb).round() as u8;
    }
    pixel
}
