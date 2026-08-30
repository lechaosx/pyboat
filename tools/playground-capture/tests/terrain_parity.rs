use std::{fs, io::BufReader, process::Command};

use game::{
    celestial_lighting, procedural_solid_surface_elevation_m, procedural_terrain_elevation_m,
    procedural_world_environment, world_celestial_conditions,
};
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
                "--diagnostic",
                "terrain_elevation",
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
    for (diagnostic, expected) in [
        (
            "surface_air_temperature",
            signed_debug_pixel(sample.weather.surface_air_temperature_c / 40.0),
        ),
        (
            "water_tide_height",
            signed_debug_pixel(sample.water_height_m / 2.0),
        ),
    ] {
        let actual = render_diagnostic_pixel(world, position_m, diagnostic);
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 8,
                "{diagnostic} at {position_m:?}: {actual:?} != {expected:?}",
            );
        }
    }
}

#[test]
fn rendered_solar_eclipse_matches_authoritative_celestial_geometry() {
    let world = WorldGeometry::default();
    let world_time_seconds = 77.0 * 86_400.0;
    let position_m = Vector::new(-11_022.222, -17_955.555);
    let lighting = celestial_lighting(world_celestial_conditions(
        world,
        world_time_seconds,
        position_m,
    ));
    assert!(lighting.solar_eclipse_fraction > 0.5);
    let actual = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_solar_eclipse_coverage",
        1,
        1,
    );
    let expected = scalar_debug_pixel(lighting.solar_eclipse_fraction);

    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 4,
            "{actual:?} != {expected:?} for {:?}",
            lighting.solar_eclipse_fraction,
        );
    }
}

#[test]
fn rendered_sea_ice_matches_the_authoritative_solid_surface() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let season_phase = 0.09;
    let world_time_seconds = world_time_for_season_phase(season_phase);
    let tick = (world_time_seconds / f64::from(simulation::DT_SECONDS)) as u64;
    let position_m = (0..512)
        .map(|index| {
            Vector::new(
                world.width_m() * (index as f32 / 512.0 - 0.5),
                0.4 * world.height_m(),
            )
        })
        .find(|position_m| {
            let bedrock = procedural_terrain_elevation_m(world, SEED, *position_m);
            let sample = procedural_world_environment(world, SEED, tick, *position_m);
            bedrock < 0.0 && sample.surface_climate.polar_ice_closure > 0.5
        })
        .expect("winter polar region should contain sea ice");
    let sample = procedural_world_environment(world, SEED, tick, position_m);
    let solid_surface = procedural_solid_surface_elevation_m(
        world,
        SEED,
        position_m,
        sample.climate.annual_mean_air_temperature_c,
        sample.surface_climate.water_temperature_c,
    );
    let actual = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "terrain_elevation",
        1,
        1,
    );
    let expected = signed_debug_pixel(solid_surface / 12.0);

    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 3,
            "at {position_m:?}: {actual:?} != {expected:?} for solid surface {solid_surface}",
        );
    }
}

#[test]
fn whole_world_capture_contains_spatial_seasons_ice_and_solar_time() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let climate = render_diagnostic_image(world, "climate_composition", 32, 16);
    let solar = render_diagnostic_image(world, "lighting_solar_elevation", 32, 16);

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
fn whole_world_winter_snow_and_sea_ice_swap_hemispheres() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let northern_summer = render_diagnostic_image_at(
        world,
        Vector::ZERO,
        world_time_for_season_phase(0.6336),
        "climate_composition",
        32,
        16,
    );
    let northern_winter = render_diagnostic_image_at(
        world,
        Vector::ZERO,
        world_time_for_season_phase(0.09),
        "climate_composition",
        32,
        16,
    );

    let summer_north_snow = mean_channel(&northern_summer, 32, 0..8, 1);
    let winter_north_snow = mean_channel(&northern_winter, 32, 0..8, 1);
    let summer_south_snow = mean_channel(&northern_summer, 32, 8..16, 1);
    let winter_south_snow = mean_channel(&northern_winter, 32, 8..16, 1);
    assert!(winter_north_snow > summer_north_snow + 0.08);
    assert!(summer_south_snow > winter_south_snow + 0.08);

    let summer_north_ice = mean_channel(&northern_summer, 32, 0..8, 2);
    let winter_north_ice = mean_channel(&northern_winter, 32, 0..8, 2);
    let summer_south_ice = mean_channel(&northern_summer, 32, 8..16, 2);
    let winter_south_ice = mean_channel(&northern_winter, 32, 8..16, 2);
    assert!(winter_north_ice > summer_north_ice + 0.05);
    assert!(summer_south_ice > winter_south_ice + 0.05);
}

#[test]
fn low_cloud_optical_depth_is_independent_of_pixel_footprint() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(0.0, -10_000.0);
    let near = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        1.0,
        "cloud_low_optical_depth",
        1,
        1,
    );
    let far = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        200.0,
        "cloud_low_optical_depth",
        1,
        1,
    );

    assert_ne!(&near[..3], &[33, 44, 53]);
    assert_eq!(&near[..3], &far[..3]);
}

#[test]
fn ambient_sky_occlusion_is_lower_frequency_than_canopy_optical_depth() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(-12_000.0, -5_000.0);
    let ambient = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        4_000.0,
        "lighting_ambient_sky_occlusion",
        256,
        64,
    );
    let canopy = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        4_000.0,
        "cloud_weather_canopy_optical_depth",
        256,
        64,
    );
    let ambient_detail = mean_neighbor_luminance_difference(&ambient, 256, 64);
    let canopy_detail = mean_neighbor_luminance_difference(&canopy, 256, 64);

    assert!(canopy_detail > 0.0005, "{canopy_detail}");
    assert!(
        ambient_detail < 0.5 * canopy_detail,
        "ambient {ambient_detail}, canopy {canopy_detail}",
    );
}

#[test]
fn ambient_sky_occlusion_follows_existing_weather_conditions() {
    let world = WorldGeometry::default();
    let width = 32usize;
    let height = 32usize;
    let ambient = render_diagnostic_image(
        world,
        "lighting_ambient_sky_occlusion",
        width as u32,
        height as u32,
    );
    let mut occlusion = Vec::with_capacity(width * height);
    let mut cloud_condition = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let offset = 4 * (row * width + column);
            occlusion.push(
                (ambient[offset] as f32 + ambient[offset + 1] as f32 + ambient[offset + 2] as f32)
                    / (3.0 * 255.0),
            );
            let position_m = Vector::new(
                (column as f32 + 0.5) / width as f32 * world.width_m() - 0.5 * world.width_m(),
                (0.5 - (row as f32 + 0.5) / height as f32) * world.height_m(),
            );
            let weather = procedural_world_environment(world, SEED, 0, position_m).weather;
            cloud_condition.push(
                weather
                    .upper_cloudiness
                    .max(0.5 * weather.cloudiness + 0.5 * weather.rain_intensity),
            );
        }
    }

    let correlation = pearson_correlation(&occlusion, &cloud_condition);
    assert!(correlation > 0.20, "{correlation}");
}

#[test]
fn visual_cloud_fields_are_independent_of_view_center_weather() {
    let world = WorldGeometry::default();
    let width = 64usize;
    let height = 16usize;
    let mut differences = Vec::new();
    for diagnostic in ["cloud_low_amount", "cloud_weather_canopy_optical_depth"] {
        let storm_centered = render_diagnostic_image_with_view_at(
            world,
            Vector::new(-4_000.0, 0.0),
            0.0,
            8_000.0,
            diagnostic,
            width as u32,
            height as u32,
        );
        let clear_centered = render_diagnostic_image_with_view_at(
            world,
            Vector::ZERO,
            0.0,
            8_000.0,
            diagnostic,
            width as u32,
            height as u32,
        );
        let mut maximum_difference = 0u8;
        for row in 0..height {
            for storm_column in 8..width {
                let clear_column = storm_column - 8;
                for channel in 0..3 {
                    let storm = storm_centered[4 * (row * width + storm_column) + channel];
                    let clear = clear_centered[4 * (row * width + clear_column) + channel];
                    maximum_difference = maximum_difference.max(storm.abs_diff(clear));
                }
            }
        }
        differences.push((diagnostic, maximum_difference));
    }
    assert!(
        differences.iter().all(|(_, difference)| *difference <= 1),
        "{differences:?}",
    );
}

#[test]
fn canopy_weather_envelope_stays_in_world_coordinates() {
    let world = WorldGeometry::default();
    let width = 32usize;
    let height = 32usize;
    let cloud_time_seconds = 1_800.0;
    let canopy = render_diagnostic_image_with_cloud_time(
        world,
        Vector::ZERO,
        0.0,
        cloud_time_seconds,
        world.height_m(),
        "cloud_weather_canopy_optical_depth",
        width as u32,
        height as u32,
    );
    let tick = (cloud_time_seconds / simulation::DT_SECONDS) as u64;
    let mut optical_depth = Vec::with_capacity(width * height);
    let mut cloud_condition = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let offset = 4 * (row * width + column);
            optical_depth.push(
                (canopy[offset] as f32 + canopy[offset + 1] as f32 + canopy[offset + 2] as f32)
                    / (3.0 * 255.0),
            );
            let position_m = Vector::new(
                (column as f32 + 0.5) / width as f32 * world.width_m() - 0.5 * world.width_m(),
                (0.5 - (row as f32 + 0.5) / height as f32) * world.height_m(),
            );
            let weather = procedural_world_environment(world, SEED, tick, position_m).weather;
            cloud_condition.push(weather.upper_cloudiness.max(0.30 * weather.cloudiness));
        }
    }

    let correlation = pearson_correlation(&optical_depth, &cloud_condition);
    assert!(correlation > 0.45, "{correlation}");
}

#[test]
fn projected_upper_shadow_fades_continuously_at_solar_horizon() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(0.0, 6_000.0);
    let sunset_seconds = 16_216.388_892_196_119;
    let before_sunset = render_diagnostic_image_with_view_at(
        world,
        position_m,
        sunset_seconds - 3.0,
        1_000.0,
        "lighting_projected_upper_shadow",
        64,
        64,
    );
    let after_sunset = render_diagnostic_image_with_view_at(
        world,
        position_m,
        sunset_seconds + 3.0,
        1_000.0,
        "lighting_projected_upper_shadow",
        64,
        64,
    );

    let mean_difference = before_sunset
        .chunks_exact(4)
        .zip(after_sunset.chunks_exact(4))
        .map(|(before, after)| {
            (0..3)
                .map(|channel| before[channel].abs_diff(after[channel]) as f32 / 255.0)
                .sum::<f32>()
                / 3.0
        })
        .sum::<f32>()
        / (64.0 * 64.0);
    assert!(
        mean_difference < 0.005,
        "{mean_difference} at {position_m:?}, sunset {sunset_seconds}",
    );
}

#[test]
fn rendered_pressure_is_spatial_across_world() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let pressure = render_diagnostic_image(world, "pressure_full_anomaly", 32, 16);
    let red_range = channel_range(&pressure, 0);
    let blue_range = channel_range(&pressure, 2);

    assert!(
        red_range > 0.02 && blue_range > 0.02,
        "red range {red_range}, blue range {blue_range}",
    );
    assert!(
        pressure
            .chunks_exact(4)
            .any(|pixel| pixel[0] > pixel[2].saturating_add(8)),
        "pressure diagnostic contains no high-pressure red"
    );
    assert!(
        pressure
            .chunks_exact(4)
            .any(|pixel| pixel[2] > pixel[0].saturating_add(8)),
        "pressure diagnostic contains no low-pressure blue"
    );
}

#[test]
fn rendered_temperature_gradient_is_spatial_across_world() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let gradient = render_diagnostic_image(world, "temperature_gradient", 32, 16);

    assert!(channel_range(&gradient, 0) > 0.08);
    assert!(channel_range(&gradient, 1) > 0.08);
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

fn render_diagnostic_pixel(world: WorldGeometry, position_m: Vector, diagnostic: &str) -> [u8; 4] {
    render_diagnostic_image_at(world, position_m, 0.0, diagnostic, 1, 1)
        .try_into()
        .unwrap()
}

fn render_diagnostic_image(
    world: WorldGeometry,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    render_diagnostic_image_at(world, Vector::ZERO, 0.0, diagnostic, width, height)
}

fn world_time_for_season_phase(season_phase: f64) -> f64 {
    (season_phase - 0.5021).rem_euclid(1.0) * 86_400.0 * 336.0
}

fn render_diagnostic_image_at(
    world: WorldGeometry,
    position_m: Vector,
    world_time_seconds: f64,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        world.height_m(),
        diagnostic,
        width,
        height,
    )
}

fn render_diagnostic_image_with_view_at(
    world: WorldGeometry,
    position_m: Vector,
    world_time_seconds: f64,
    view_height_m: f32,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    render_diagnostic_image_with_cloud_time(
        world,
        position_m,
        world_time_seconds,
        0.0,
        view_height_m,
        diagnostic,
        width,
        height,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_diagnostic_image_with_cloud_time(
    world: WorldGeometry,
    position_m: Vector,
    world_time_seconds: f64,
    cloud_time_seconds: f32,
    view_height_m: f32,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-spatial-surface-{}-{:016x}-{:08x}-{:08x}-{:08x}-{diagnostic}-{width}x{height}.png",
        std::process::id(),
        world_time_seconds.to_bits(),
        cloud_time_seconds.to_bits(),
        position_m.x.to_bits(),
        position_m.y.to_bits(),
    ));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
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
            &view_height_m.to_string(),
            "--seed",
            &SEED.to_string(),
            "--cloud-time",
            &cloud_time_seconds.to_string(),
            "--world-time",
            &world_time_seconds.to_string(),
            "--diagnostic",
            diagnostic,
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

fn mean_neighbor_luminance_difference(bytes: &[u8], width: usize, height: usize) -> f32 {
    let luminance = |offset: usize| {
        (bytes[offset] as f32 + bytes[offset + 1] as f32 + bytes[offset + 2] as f32) / (3.0 * 255.0)
    };
    let mut difference = 0.0;
    let mut count = 0;
    for row in 0..height {
        for column in 1..width {
            let offset = 4 * (row * width + column);
            difference += (luminance(offset) - luminance(offset - 4)).abs();
            count += 1;
        }
    }
    difference / count as f32
}

fn pearson_correlation(left: &[f32], right: &[f32]) -> f32 {
    let left_mean = left.iter().sum::<f32>() / left.len() as f32;
    let right_mean = right.iter().sum::<f32>() / right.len() as f32;
    let mut covariance = 0.0;
    let mut left_variance = 0.0;
    let mut right_variance = 0.0;
    for (&left, &right) in left.iter().zip(right) {
        let centered_left = left - left_mean;
        let centered_right = right - right_mean;
        covariance += centered_left * centered_right;
        left_variance += centered_left * centered_left;
        right_variance += centered_right * centered_right;
    }
    covariance / (left_variance * right_variance).sqrt()
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

fn scalar_debug_pixel(value: f32) -> [u8; 4] {
    let amount = value.clamp(0.0, 1.0);
    let dark = [0.015, 0.025, 0.035];
    let target = [0.92, 0.96, 0.94];
    let mut pixel = [0, 0, 0, 255];
    for channel in 0..3 {
        let linear = dark[channel] + amount * (target[channel] - dark[channel]);
        let srgb = if linear <= 0.003_130_8 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        pixel[channel] = (255.0 * srgb).round() as u8;
    }
    pixel
}
