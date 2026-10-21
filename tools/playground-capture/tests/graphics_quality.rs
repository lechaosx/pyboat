//! Lower graphics tiers keep what players navigate by: shorelines, shallow
//! water, the sea-ice edge, and night and storm darkening match the high tier.
//! Each check also runs on a gameplay-affecting change that it must catch.

use std::{fs, io::BufReader, process::Command};

use game::{
    celestial_lighting, procedural_terrain_elevation_m, procedural_world_environment,
    world_celestial_conditions,
};
use rapier2d::prelude::Vector;
use simulation::{TICKS_PER_SECOND, WorldGeometry};

const SEED: u64 = 0x5eed;
const WIDTH: usize = 640;
const HEIGHT: usize = 360;
const LOWER_TIERS: [&str; 2] = ["medium", "low"];
/// Pixels whose water classification may differ, 0.01% of the image.
const MAXIMUM_RECLASSIFIED_PIXELS: usize = WIDTH * HEIGHT / 10_000;
/// 99th percentile of 8×8-pixel mean water color changes, in 8-bit levels.
/// Appearance detail averages out over a block; moved shallows do not.
const MAXIMUM_SHALLOW_TINT_LEVELS: f64 = 2.0;
const MAXIMUM_MEAN_LUMINANCE_CHANGE: f64 = 0.01;
/// 99th percentile of relative 16×16-pixel mean luminance changes.
const MAXIMUM_REGIONAL_LUMINANCE_CHANGE: f64 = 0.05;

/// Removes every material color except water and the shallow-water bottom
/// tint, leaving land and ice black.
const WATER_ONLY: [&str; 4] = [
    "--land-material-colors",
    "0,0,0,0",
    "--solid-material-colors",
    "0,0,0,0",
];

#[derive(Clone, Copy)]
struct View {
    camera_m: [f32; 2],
    view_height_m: f32,
    world_time_seconds: f64,
}

fn reef_coast() -> View {
    let coast_m = rugged_coast_m();
    View {
        camera_m: [coast_m.x, coast_m.y],
        view_height_m: 240.0,
        world_time_seconds: 37.0,
    }
}

/// The first shoreline crossing along the reef coast's middle row.
fn rocky_shore() -> View {
    let coast_m = rugged_coast_m();
    let is_land =
        |x_m| procedural_terrain_elevation_m(world(), SEED, Vector::new(x_m, coast_m.y)) >= 0.0;
    let shore_x_m = (0..1_000)
        .map(|step| coast_m.x - 240.0 + 0.5 * step as f32)
        .find(|&x_m| is_land(x_m) != is_land(x_m + 0.5))
        .expect("the reef coast's middle row should cross the shoreline");
    View {
        camera_m: [shore_x_m, coast_m.y],
        view_height_m: 60.0,
        world_time_seconds: 37.0,
    }
}

fn moonlit_coast() -> View {
    moonlit(View {
        view_height_m: 400.0,
        ..reef_coast()
    })
}

/// The view at the first hour whose night has a high, bright Moon.
fn moonlit(view: View) -> View {
    let position_m = Vector::new(view.camera_m[0], view.camera_m[1]);
    let world_time_seconds = (0..28 * 24)
        .map(|hour| f64::from(hour) * 3_600.0)
        .find(|&world_time_seconds| {
            let light = celestial_lighting(world_celestial_conditions(
                world(),
                world_time_seconds,
                position_m,
            ));
            light.sun_direction[2] < -0.2
                && light.moon_direction[2] > 0.7
                && light.moon_intensity > 0.08
        })
        .expect("a lunar month should have a moonlit night");
    View {
        world_time_seconds,
        ..view
    }
}

/// A northern summer ice edge across open water beside an island, found by
/// scanning meridians 500 m apart for where polar sea ice closes.
fn summer_ice_edge() -> View {
    let world_time_seconds = 15_595_200.0;
    let tick = (world_time_seconds * TICKS_PER_SECOND as f64) as u64;
    let is_land = |position_m| procedural_terrain_elevation_m(world(), SEED, position_m) >= 0.0;
    let is_sea_ice = |position_m| {
        !is_land(position_m)
            && procedural_world_environment(world(), SEED, tick, position_m)
                .surface_climate
                .polar_ice_closure
                > 0.5
    };
    let (half_width_m, half_height_m) = (2_133.0, 1_200.0);
    let camera_m = (0..128)
        .filter_map(|meridian| {
            let x_m = 500.0 * meridian as f32 - 0.5 * world().width_m();
            let edge_y_m = (0..128)
                .map(|step| 0.3 * world().height_m() + 100.0 * step as f32)
                .find(|&y_m| is_sea_ice(Vector::new(x_m, y_m)))?;
            Some(Vector::new(x_m, edge_y_m - 0.6 * half_height_m))
        })
        .find(|&center_m| {
            let land = (0..43 * 24)
                .filter(|index| {
                    is_land(
                        center_m
                            + Vector::new(
                                100.0 * (index % 43) as f32 - half_width_m,
                                100.0 * (index / 43) as f32 - half_height_m,
                            ),
                    )
                })
                .count();
            (10..=200).contains(&land)
        })
        .expect("a summer ice edge should pass an island");
    View {
        camera_m: [camera_m.x, camera_m.y],
        view_height_m: 2400.0,
        world_time_seconds,
    }
}

/// Heavy daytime rain over deep water, scanning 500 m apart outward from the
/// origin.
fn storm() -> View {
    let world_time_seconds = 63_037.0;
    let tick = (world_time_seconds * TICKS_PER_SECOND as f64) as u64;
    let camera_m = rings(500.0)
        .find(|&position_m| {
            procedural_terrain_elevation_m(world(), SEED, position_m) < -20.0
                && celestial_lighting(world_celestial_conditions(
                    world(),
                    world_time_seconds,
                    position_m,
                ))
                .sun_direction[2]
                    > 0.3
                && procedural_world_environment(world(), SEED, tick, position_m)
                    .weather
                    .rain_intensity
                    > 0.6
        })
        .expect("the world should have a daytime storm over water");
    View {
        camera_m: [camera_m.x, camera_m.y],
        view_height_m: 40.0,
        world_time_seconds,
    }
}

/// The first view centre, scanning 500 m apart outward from the origin, that is
/// daylit at 37 s and whose 240 m-high view has a shoreline that reefs and rocks
/// break up: its middle row crosses the shoreline at least twelve times at 2 m
/// spacing.
fn rugged_coast_m() -> Vector {
    let half_width_m = 0.5 * 240.0 * 16.0 / 9.0;
    let crossings = |center_m: Vector, step_m: f32| {
        let count = (2.0 * half_width_m / step_m) as usize;
        (0..=count)
            .map(|index| {
                let x_m = center_m.x - half_width_m + step_m * index as f32;
                procedural_terrain_elevation_m(world(), SEED, Vector::new(x_m, center_m.y)) >= 0.0
            })
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
    };
    rings(500.0)
        .find(|&center_m| {
            celestial_lighting(world_celestial_conditions(world(), 37.0, center_m)).sun_direction[2]
                > 0.5
                && crossings(center_m, 36.0) > 0
                && crossings(center_m, 2.0) >= 12
        })
        .expect("the world should have a rugged coast")
}

/// Grid points `spacing_m` apart in square rings around the origin, within
/// 16 km.
fn rings(spacing_m: f32) -> impl Iterator<Item = Vector> {
    let ring_count = (16_000.0 / spacing_m) as i32;
    (0..ring_count).flat_map(move |ring| {
        (-ring..=ring).flat_map(move |y_index| {
            (-ring..=ring)
                .filter(move |x_index| x_index.abs() == ring || y_index.abs() == ring)
                .map(move |x_index| spacing_m * Vector::new(x_index as f32, y_index as f32))
        })
    })
}

fn world() -> WorldGeometry {
    WorldGeometry::default()
}

#[derive(Debug)]
struct WaterComparison {
    reclassified_pixels: usize,
    shallow_tint_p99_levels: f64,
}

impl WaterComparison {
    fn matches(&self) -> bool {
        self.reclassified_pixels <= MAXIMUM_RECLASSIFIED_PIXELS
            && self.shallow_tint_p99_levels <= MAXIMUM_SHALLOW_TINT_LEVELS
    }
}

#[derive(Debug)]
struct LightComparison {
    mean_luminance_change: f64,
    regional_luminance_change_p99: f64,
}

impl LightComparison {
    fn matches(&self) -> bool {
        self.mean_luminance_change <= MAXIMUM_MEAN_LUMINANCE_CHANGE
            && self.regional_luminance_change_p99 <= MAXIMUM_REGIONAL_LUMINANCE_CHANGE
    }
}

#[test]
fn lower_tiers_keep_shorelines_shallow_water_and_the_sea_ice_edge() {
    let summer_ice_edge = summer_ice_edge();
    for view in [
        reef_coast(),
        rocky_shore(),
        summer_ice_edge,
        moonlit_coast(),
    ] {
        let high = capture(&view, &WATER_ONLY);
        for quality in LOWER_TIERS {
            let comparison = compare_water(
                &high,
                &capture(&view, &[&WATER_ONLY[..], &["--quality", quality]].concat()),
            );
            assert!(
                comparison.matches(),
                "{quality} at {:?}: {comparison:?}",
                view.camera_m
            );
        }
        // Coastal relief carries reefs, rocks, and fine shorelines, as dropped
        // terrain octaves would remove them.
        let without_coastal_relief = compare_water(
            &high,
            &capture(
                &view,
                &[&WATER_ONLY[..], &["--landscape-visibility", "0,1,1,1"]].concat(),
            ),
        );
        assert!(
            !without_coastal_relief.matches(),
            "control at {:?}: {without_coastal_relief:?}",
            view.camera_m
        );
    }
    // A week of summer retreat moves the ice edge across open water.
    let a_week_later = View {
        world_time_seconds: summer_ice_edge.world_time_seconds + 7.0 * 86_400.0,
        ..summer_ice_edge
    };
    let moved_ice_edge = compare_water(
        &capture(&summer_ice_edge, &WATER_ONLY),
        &capture(&a_week_later, &WATER_ONLY),
    );
    assert!(!moved_ice_edge.matches(), "{moved_ice_edge:?}");
}

#[test]
fn lower_tiers_keep_night_and_storm_darkening() {
    for view in [moonlit_coast(), moonlit(rocky_shore()), storm()] {
        let high = capture(&view, &[]);
        for quality in LOWER_TIERS {
            let comparison = compare_light(&high, &capture(&view, &["--quality", quality]));
            assert!(
                comparison.matches(),
                "{quality} at {:?}: {comparison:?}",
                view.camera_m
            );
        }
        let brighter = compare_light(
            &high,
            &capture(&view, &["--light-balance", "1.1,1.1,1.1,1.1"]),
        );
        assert!(
            !brighter.matches(),
            "control at {:?}: {brighter:?}",
            view.camera_m
        );
    }
}

fn compare_water(high: &[u8], other: &[u8]) -> WaterComparison {
    let is_water = |pixel: &[u8; 4]| pixel[..3].iter().any(|channel| *channel > 0);
    let reclassified_pixels = high
        .as_chunks::<4>()
        .0
        .iter()
        .zip(other.as_chunks::<4>().0)
        .filter(|(high, other)| is_water(high) != is_water(other))
        .count();
    let channel_blocks = |image: &[u8], channel: usize| {
        block_means(
            &image
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| f64::from(pixel[channel]))
                .collect::<Vec<_>>(),
            8,
        )
    };
    let mut tint_changes = vec![0.0_f64; (WIDTH / 8) * (HEIGHT / 8)];
    for channel in 0..3 {
        for ((change, high), other) in tint_changes
            .iter_mut()
            .zip(channel_blocks(high, channel))
            .zip(channel_blocks(other, channel))
        {
            *change = change.max((high - other).abs());
        }
    }
    WaterComparison {
        reclassified_pixels,
        shallow_tint_p99_levels: percentile_99(tint_changes),
    }
}

fn compare_light(high: &[u8], other: &[u8]) -> LightComparison {
    let high = luminance(high);
    let other = luminance(other);
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    let regional_changes = block_means(&high, 16)
        .into_iter()
        .zip(block_means(&other, 16))
        .map(|(high, other)| (other - high).abs() / high.max(1.0e-4))
        .collect();
    LightComparison {
        mean_luminance_change: (mean(&other) - mean(&high)).abs() / mean(&high),
        regional_luminance_change_p99: percentile_99(regional_changes),
    }
}

fn luminance(image: &[u8]) -> Vec<f64> {
    let linear = |encoded: u8| {
        let encoded = f64::from(encoded) / 255.0;
        if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    };
    image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2])
        })
        .collect()
}

fn block_means(values: &[f64], block: usize) -> Vec<f64> {
    let (columns, rows) = (WIDTH / block, HEIGHT / block);
    let mut means = vec![0.0; columns * rows];
    for (index, value) in values.iter().enumerate() {
        let (x, y) = (index % WIDTH / block, index / WIDTH / block);
        if x < columns && y < rows {
            means[y * columns + x] += value / (block * block) as f64;
        }
    }
    means
}

fn percentile_99(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() - 1) * 99 / 100]
}

fn capture(view: &View, arguments: &[&str]) -> Vec<u8> {
    let camera_x = view.camera_m[0].to_string();
    let camera_y = view.camera_m[1].to_string();
    let view_height = view.view_height_m.to_string();
    let world_time = view.world_time_seconds.to_string();
    let view_arguments = [
        "--camera-x",
        &camera_x,
        "--camera-y",
        &camera_y,
        "--view-height",
        &view_height,
        "--world-time",
        &world_time,
    ];
    let output = std::env::temp_dir().join(format!(
        "pyboat-graphics-quality-{}-{:016x}.png",
        std::process::id(),
        [&view_arguments[..], arguments]
            .concat()
            .join(" ")
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            }),
    ));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
            "--width",
            &WIDTH.to_string(),
            "--height",
            &HEIGHT.to_string(),
            "--seed",
            "0x5eed",
            "--output",
            output.to_str().unwrap(),
        ])
        .args(view_arguments)
        .args(arguments)
        .output()
        .expect("headless capture should start");
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr),
    );
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(&output).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    fs::remove_file(&output).unwrap();
    fs::remove_file(output.with_extension("json")).unwrap();
    bytes
}
