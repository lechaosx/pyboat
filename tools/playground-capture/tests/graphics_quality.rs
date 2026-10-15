//! Lower graphics tiers keep what players navigate by: shorelines, shallow
//! water, the sea-ice edge, and night and storm darkening match the high tier.
//! Each check also runs on a gameplay-affecting change that it must catch.

use std::{fs, io::BufReader, process::Command};

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
/// tint, leaving land, ice, and lower clouds black.
const WATER_ONLY: [&str; 6] = [
    "--land-material-colors",
    "0,0,0,0",
    "--solid-material-colors",
    "0,0,0,0",
    "--layer-material-colors",
    "0,0,0,0",
];

struct View {
    camera_m: [f32; 2],
    view_height_m: f32,
    world_time_seconds: f64,
}

const REEF_COAST: View = View {
    camera_m: [-5890.0, -2100.0],
    view_height_m: 240.0,
    world_time_seconds: 37.0,
};
const ROCKY_SHORE: View = View {
    camera_m: [-5770.0, -2100.0],
    view_height_m: 60.0,
    world_time_seconds: 37.0,
};
const SUMMER_ICE_EDGE: View = View {
    camera_m: [2300.0, 25300.0],
    view_height_m: 2400.0,
    world_time_seconds: 15_595_200.0,
};
/// A week of summer retreat moves the ice edge across open water.
const SUMMER_ICE_EDGE_A_WEEK_LATER: View = View {
    world_time_seconds: SUMMER_ICE_EDGE.world_time_seconds + 7.0 * 86_400.0,
    ..SUMMER_ICE_EDGE
};
const MOONLIT_COAST: View = View {
    camera_m: [-5420.0, -2700.0],
    view_height_m: 400.0,
    world_time_seconds: 209_700.0,
};
const MOONLIT_ROCKY_SHORE: View = View {
    world_time_seconds: MOONLIT_COAST.world_time_seconds,
    ..ROCKY_SHORE
};
const STORM: View = View {
    camera_m: [8000.0, -20000.0],
    view_height_m: 40.0,
    world_time_seconds: 63_037.0,
};

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
    for view in [REEF_COAST, ROCKY_SHORE, SUMMER_ICE_EDGE, MOONLIT_COAST] {
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
    let moved_ice_edge = compare_water(
        &capture(&SUMMER_ICE_EDGE, &WATER_ONLY),
        &capture(&SUMMER_ICE_EDGE_A_WEEK_LATER, &WATER_ONLY),
    );
    assert!(!moved_ice_edge.matches(), "{moved_ice_edge:?}");
}

#[test]
fn lower_tiers_keep_night_and_storm_darkening() {
    for view in [MOONLIT_COAST, MOONLIT_ROCKY_SHORE, STORM] {
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
