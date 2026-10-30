//! The eye adapts to the light falling on the view, pooled over its adapting
//! field, and HDR output only adds headroom above the SDR tone curve.

use std::{fs, io::BufReader, process::Command};

use game::{REFERENCE_LINKE_TURBIDITY, clear_sky_irradiance};
use serde_json::Value;

/// The tone curve's shoulder in linear light, as display.wgsl's.
const SHOULDER: f32 = 0.8;
/// The game's HDR output peak in diffuse whites.
const HDR_PEAK: &str = "4";

struct Capture {
    rgba: Vec<u8>,
    derived: Value,
}

fn capture(name: &str, arguments: &[&str]) -> Capture {
    let output = std::env::temp_dir().join(format!("pyboat-eye-{}-{name}.png", std::process::id()));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args(arguments)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("headless capture should start");
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr),
    );
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(&output).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    let sidecar = output.with_extension("json");
    let report: Value = serde_json::from_str(&fs::read_to_string(&sidecar).unwrap()).unwrap();
    fs::remove_file(&output).unwrap();
    fs::remove_file(sidecar).unwrap();
    Capture {
        rgba,
        derived: report["derived"].clone(),
    }
}

/// A view at `camera` (x, y, view height in metres) and `world_time`.
fn view<'a>(
    diagnostic: &'a str,
    size: [&'a str; 2],
    camera: [&'a str; 3],
    world_time: &'a str,
) -> Vec<&'a str> {
    vec![
        "--diagnostic",
        diagnostic,
        "--width",
        size[0],
        "--height",
        size[1],
        "--camera-x",
        camera[0],
        "--camera-y",
        camera[1],
        "--view-height",
        camera[2],
        "--world-time",
        world_time,
    ]
}

fn with<'a>(mut arguments: Vec<&'a str>, extra: &[&'a str]) -> Vec<&'a str> {
    arguments.extend_from_slice(extra);
    arguments
}

fn srgb_channel_to_linear(channel: u8) -> f32 {
    let srgb = f32::from(channel) / 255.0;
    if srgb <= 0.040_45 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_channel_byte(linear: f32) -> u8 {
    let srgb = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (255.0 * srgb.clamp(0.0, 1.0)).round() as u8
}

fn luminance(pixel: &[u8]) -> f32 {
    0.2126 * srgb_channel_to_linear(pixel[0])
        + 0.7152 * srgb_channel_to_linear(pixel[1])
        + 0.0722 * srgb_channel_to_linear(pixel[2])
}

fn luminances(rgba: &[u8]) -> Vec<f32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| luminance(pixel))
        .collect()
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

fn percentile(values: &[f32], share: f32) -> f32 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * share).round() as usize]
}

/// The light a view's centre is adapted to, from the exposure its sidecar
/// records, by the inverse of exposure.rs's `exposure_target`.
fn adapting_light(capture: &Capture) -> f32 {
    let exposure = capture.derived["settledExposureMultiplier"]
        .as_f64()
        .unwrap() as f32;
    exposure.powf(1.0 / (browser::ADAPTED_BRIGHTNESS_EXPONENT - 1.0))
}

/// Image columns of a view's left and right halves.
fn half_mean(luminance: &[f32], width: usize, right: bool) -> f32 {
    let half: Vec<f32> = luminance
        .chunks_exact(width)
        .flat_map(|row| {
            if right {
                &row[width / 2..]
            } else {
                &row[..width / 2]
            }
        })
        .copied()
        .collect();
    mean(&half)
}

/// HDR output takes the SDR tone curve's values below its shoulder and only
/// adds headroom above it, for a sunlit snowfield and a moonless night alike:
/// the eye's adaptation, rod shift, and vision noise do not see the output.
#[test]
fn hdr_output_matches_sdr_below_the_shoulder_and_only_adds_headroom() {
    let shoulder_byte = linear_channel_byte(SHOULDER);
    for (name, camera, world_time, has_highlights) in [
        ("snow", ["7550", "20670", "3000"], "5253000", true),
        ("moonless-night", ["800", "0", "3000"], "4253400", false),
    ] {
        let arguments = view("scene_production", ["320", "200"], camera, world_time);
        let sdr = capture(&format!("{name}-sdr"), &arguments);
        let hdr = capture(
            &format!("{name}-hdr"),
            &with(arguments.clone(), &["--output-peak", HDR_PEAK]),
        );
        let mut highlights = 0;
        for (sdr, hdr) in sdr
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(hdr.rgba.as_chunks::<4>().0)
        {
            if sdr[..3].iter().all(|&channel| channel < shoulder_byte) {
                assert_eq!(sdr, hdr, "{name}: below the shoulder");
            } else {
                highlights += 1;
            }
            for channel in 0..3 {
                assert!(
                    hdr[channel] >= sdr[channel],
                    "{name}: {hdr:?} below {sdr:?}"
                );
            }
        }
        assert_eq!(
            highlights > 0,
            has_highlights,
            "{name}: {highlights} highlights"
        );
    }
}

/// Along a ramp of one hue from 1/64 to 64 diffuse whites, both outputs rise
/// monotonically and keep the hue, SDR and HDR agree below the shoulder, and
/// only HDR approaches its peak.
#[test]
fn display_response_is_monotone_hue_preserving_and_shares_sdr_below_the_shoulder() {
    // The regression diagnostic's default vision colour and ramp.
    let color = [0.6_f32, 0.25, 0.1];
    let width = 256;
    let level = |column: usize| ((column as f32 + 0.5) / width as f32 * 12.0 - 6.0).exp2();
    let ramp = |peak: &str| {
        capture(
            &format!("ramp-{peak}"),
            &[
                "--diagnostic",
                "lighting_display_response",
                "--width",
                &width.to_string(),
                "--height",
                "1",
                "--output-peak",
                peak,
            ],
        )
        .rgba
    };
    let sdr = ramp("1");
    let hdr = ramp(HDR_PEAK);
    for (output, ramp) in [("sdr", &sdr), ("hdr", &hdr)] {
        let pixels = ramp.as_chunks::<4>().0;
        for (column, pair) in pixels.windows(2).enumerate() {
            assert!(
                (0..3).all(|channel| pair[1][channel] >= pair[0][channel]),
                "{output} falls after column {column}",
            );
        }
        for pixel in pixels {
            // Above 1% the 8-bit red channel resolves the hue to one level.
            let red = srgb_channel_to_linear(pixel[0]);
            if red > 0.01 {
                for channel in 1..3 {
                    let expected = linear_channel_byte(red * color[channel] / color[0]);
                    assert!(
                        pixel[channel].abs_diff(expected) <= 1,
                        "{output} hue {pixel:?}",
                    );
                }
            }
        }
    }
    let (sdr, hdr) = (sdr.as_chunks::<4>().0, hdr.as_chunks::<4>().0);
    for column in 0..width {
        let exposed = color[0] * level(column);
        if exposed < SHOULDER {
            assert_eq!(sdr[column], hdr[column], "column {column}");
        }
        assert!(
            sdr[column][..3]
                .iter()
                .zip(&hdr[column][..3])
                .all(|(s, h)| h >= s)
        );
    }
    // The diagnostic shows a quarter of the light, so SDR stays at or below a
    // quarter of white and HDR reaches nearly all of it.
    let quarter_white = linear_channel_byte(0.25);
    assert!(sdr.iter().all(|pixel| pixel[0] <= quarter_white));
    assert!(hdr[width - 1][0] >= linear_channel_byte(0.95));
}

/// A deck filling the view darkens the light falling on it about sixfold, and
/// the eye adapts to that light. The lab's cloud-shadow switch clears the
/// deck's shadows from the field grids the eye adapts through, so the view
/// with them shows CIECAM02's brightness of the deck's light beside the view
/// without: a little dimmer, not sixfold dimmer.
#[test]
fn a_view_under_a_deck_adapts_to_its_light_by_the_brightness_law() {
    let arguments = view("scene", ["160", "100"], ["13500", "-14800", "480"], "63037");
    let shaded = capture("deck-shaded", &arguments);
    let unshaded = capture(
        "deck-unshaded",
        &with(arguments.clone(), &["--cloud-shadows", "0"]),
    );
    let light_ratio = adapting_light(&shaded) / adapting_light(&unshaded);
    assert!(
        light_ratio < 0.3,
        "the eye is adapted to {light_ratio} of the light without the deck's shadows",
    );
    let displayed_ratio = mean(&luminances(&shaded.rgba)) / mean(&luminances(&unshaded.rgba));
    let brightness_ratio = light_ratio.powf(browser::ADAPTED_BRIGHTNESS_EXPONENT);
    assert!(
        displayed_ratio < 1.0 && (displayed_ratio / brightness_ratio).log2().abs() < 0.1,
        "displayed {displayed_ratio}, brightness law {brightness_ratio}",
    );
}

/// Adaptation follows the light falling on the view, not what its surfaces
/// reflect: hiding every land and ice colour leaves the rest of a coastal
/// view, and the light it is adapted to, unchanged.
#[test]
fn surface_colors_leave_adaptation_unchanged() {
    let arguments = view(
        "scene",
        ["320", "200"],
        ["-8800", "1700", "1500"],
        "1473600",
    );
    let colored = capture("coast-colored", &arguments);
    let hidden = capture(
        "coast-hidden",
        &with(
            arguments.clone(),
            &[
                "--land-material-colors",
                "0,0,0,0",
                "--solid-material-colors",
                "0,0,0,0",
            ],
        ),
    );
    assert_eq!(
        colored.derived["settledExposureMultiplier"],
        hidden.derived["settledExposureMultiplier"],
    );
    let mut hidden_pixels = 0;
    for (colored, hidden) in colored
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(hidden.rgba.as_chunks::<4>().0)
    {
        if hidden[..3] == [0, 0, 0] {
            hidden_pixels += 1;
        } else {
            assert_eq!(colored, hidden);
        }
    }
    // Land covers a large share of the view, so its colour would move any
    // adaptation to reflected light.
    assert!(hidden_pixels > 320 * 200 / 4, "{hidden_pixels}");
}

/// Cumulus shadows are small beside the adapting field, so their cores keep
/// most of the contrast of the diffuse light alone lighting them: at least
/// half of it in stops, where adapting to each shadow would keep a twelfth.
#[test]
fn small_cloud_shadows_keep_most_of_their_contrast() {
    let arguments = view("scene", ["320", "200"], ["-8950", "800", "3000"], "1123237");
    let shaded = capture("cumulus-shaded", &arguments);
    let unshaded = capture(
        "cumulus-unshaded",
        &with(arguments.clone(), &["--cloud-shadows", "0"]),
    );
    let ratios: Vec<f32> = luminances(&shaded.rgba)
        .into_iter()
        .zip(luminances(&unshaded.rgba))
        .filter(|&(shaded, unshaded)| shaded > 1e-3 && unshaded > 1e-3)
        .map(|(shaded, unshaded)| shaded / unshaded)
        .collect();
    let contrast = percentile(&ratios, 0.02) / percentile(&ratios, 0.98);
    let sun_height = shaded.derived["sunDirection"][2].as_f64().unwrap();
    let [beam, diffuse] = clear_sky_irradiance(sun_height, REFERENCE_LINKE_TURBIDITY, 1.0);
    let diffuse_share = (diffuse / (sun_height * beam + diffuse)) as f32;
    assert!(
        contrast.log2() < 0.5 * diffuse_share.log2(),
        "shadow cores at {contrast} of sunlit ground, diffuse share {diffuse_share}",
    );
}

/// A view across the terminator partly adapts each side to the other: its
/// night side shows darker than in a view of the night alone, and its
/// twilight side brighter than in a view reaching into daylight.
#[test]
fn a_view_across_the_terminator_partly_adapts_each_side() {
    let (width, height) = (320, 200);
    let view_height_m = 8000.0_f32;
    let half_width_m = 0.5 * view_height_m * width as f32 / height as f32;
    let center_x_m = 16_000.0_f32;
    let render = |name: &str, x_m: f32| {
        let x = x_m.to_string();
        let view_height = view_height_m.to_string();
        let width = width.to_string();
        let height = height.to_string();
        luminances(
            &capture(
                name,
                &view("scene", [&width, &height], [&x, "0", &view_height], "0"),
            )
            .rgba,
        )
    };
    let across = render("terminator-across", center_x_m);
    let night = render("terminator-night", center_x_m + half_width_m);
    let day = render("terminator-day", center_x_m - half_width_m);
    let night_beside_day = half_mean(&across, width, true);
    let night_alone = half_mean(&night, width, false);
    let twilight_beside_night = half_mean(&across, width, false);
    let twilight_beside_day = half_mean(&day, width, true);
    assert!(
        night_beside_day < 0.9 * night_alone,
        "night {night_beside_day} beside day, {night_alone} alone",
    );
    assert!(
        twilight_beside_night > 1.5 * twilight_beside_day,
        "twilight {twilight_beside_night} beside night, {twilight_beside_day} beside day",
    );
}

fn adapting_field_stops(capture: &Capture) -> Vec<f32> {
    capture.derived["adaptingFieldStops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stops| stops.as_f64().unwrap() as f32)
        .collect()
}

/// Adaptation is retinal: each part of the view eases in stops toward its own
/// adapting light, taking 3 s to darkening light and 1 s to brightening light,
/// and keeps its place on the screen as the view pans. Panning from daylight
/// to dusk leaves the part still in daylight nearly as adapted as it settles,
/// while the part now at dusk lags.
#[test]
fn each_part_of_the_view_adapts_in_place_as_the_view_pans() {
    let day = ["-400", "0", "8000"];
    let dusk = ["6000", "0", "8000"];
    let field = |name: &str, camera, extra: &[&str]| {
        adapting_field_stops(&capture(
            name,
            &with(view("scene_production", ["320", "200"], camera, "0"), extra),
        ))
    };
    let pan = |name: &str, from: [&str; 3], to, seconds| {
        field(
            name,
            to,
            &[
                "--prior-view",
                &from.join(","),
                "--adaptation-seconds",
                seconds,
            ],
        )
    };
    let settled_day = field("pan-day", day, &[]);
    let settled_dusk = field("pan-dusk", dusk, &[]);
    let to_dusk = pan("pan-to-dusk", day, dusk, "3");
    let to_day = pan("pan-to-day", dusk, day, "1");
    // Each pan lasts one time constant, leaving 1/e of each texel's change.
    let remaining = (-1.0_f32).exp();
    for (name, panned, from, to) in [
        ("to dusk", &to_dusk, &settled_day, &settled_dusk),
        ("to day", &to_day, &settled_dusk, &settled_day),
    ] {
        for texel in 0..panned.len() {
            let expected = to[texel] + (from[texel] - to[texel]) * remaining;
            assert!(
                (panned[texel] - expected).abs() < 0.01,
                "{name}, texel {texel}: {} stops, expected {expected}",
                panned[texel],
            );
        }
    }
    let texels = browser::ADAPTING_FIELD_TEXELS as usize;
    let lag = |texel: usize| (to_dusk[texel] - settled_dusk[texel]).abs();
    let left_quarter_lag = (0..to_dusk.len())
        .filter(|texel| texel % texels < texels / 4)
        .map(lag)
        .fold(0.0, f32::max);
    let right_quarter_lag = (0..to_dusk.len())
        .filter(|texel| texel % texels >= texels * 3 / 4)
        .map(lag)
        .fold(f32::INFINITY, f32::min);
    assert!(
        left_quarter_lag < 0.3 && right_quarter_lag > 0.75,
        "daylit quarter lags up to {left_quarter_lag} stops, dusk quarter at least {right_quarter_lag}",
    );
}

/// The eye adapts to the mean light over its adapting field, as CIE defines
/// adapting luminance, so a field half in daylight adapts to about half of
/// daylight. In a whole-world view across the terminator, the day side shows
/// within a small factor of a view of daylight alone, and the night side no
/// brighter than a view of the night alone.
#[test]
fn a_view_of_day_and_night_adapts_to_their_mean_light() {
    let (width, height) = (320, 200);
    // Mean luminance over 6 km of world east to west around `center_x_m`,
    // and 6 km about the equator, in a view centred on the equator.
    let region = |name: &str, camera_x_m: f32, view_height_m: f32, center_x_m: f32| {
        let (camera_x, view_height) = (camera_x_m.to_string(), view_height_m.to_string());
        let size = [width.to_string(), height.to_string()];
        let capture = capture(
            name,
            &view(
                "scene_production",
                [&size[0], &size[1]],
                [&camera_x, "0", &view_height],
                "0",
            ),
        );
        let metres_per_pixel = view_height_m / height as f32;
        let luminance = luminances(&capture.rgba);
        let pixels: Vec<f32> = (0..width * height)
            .filter(|pixel| {
                let x_m = camera_x_m
                    + ((pixel % width) as f32 + 0.5 - 0.5 * width as f32) * metres_per_pixel;
                let y_m = ((pixel / width) as f32 + 0.5 - 0.5 * height as f32) * metres_per_pixel;
                (x_m - center_x_m).abs() < 3000.0 && y_m.abs() < 3000.0
            })
            .map(|pixel| luminance[pixel])
            .collect();
        mean(&pixels)
    };
    let (day_x_m, night_x_m) = (-5000.0, 30_000.0);
    let day_beside_night = region("whole-world-day", 10_600.0, 32_000.0, day_x_m);
    let night_beside_day = region("whole-world-night", 10_600.0, 32_000.0, night_x_m);
    let day_alone = region("day-alone", day_x_m, 8000.0, day_x_m);
    let night_alone = region("night-alone", night_x_m, 8000.0, night_x_m);
    let day_ratio = day_beside_night / day_alone;
    assert!(
        (1.0 / 1.6..=1.6).contains(&day_ratio),
        "day beside night at {day_ratio} of day alone",
    );
    assert!(
        night_beside_day <= night_alone,
        "night beside day {night_beside_day}, alone {night_alone}",
    );
}
