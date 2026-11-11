//! Ground wets where water covers it and where the tide leaves it, and dries
//! by the clear-sky sunlight it receives; wet ground returns light as the bed
//! beneath water of no depth.

use std::{
    fs,
    io::BufReader,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use game::{celestial_lighting, procedural_water_height_m, world_celestial_conditions};
use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed;
/// The bench's reef coast.
const COAST_M: [f32; 2] = [-8_500.0, 0.0];

fn capture_rgba(name: &str, arguments: &[&str]) -> Vec<u8> {
    // Tests running in parallel may capture the same view.
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let output = std::env::temp_dir().join(format!(
        "pyboat-wetness-{}-{}-{name}.png",
        std::process::id(),
        CAPTURES.fetch_add(1, Ordering::Relaxed),
    ));
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
    fs::remove_file(&output).unwrap();
    fs::remove_file(output.with_extension("json")).unwrap();
    rgba
}

fn srgb_channel_to_linear(channel: u8) -> f64 {
    let srgb = f64::from(channel) / 255.0;
    if srgb <= 0.040_45 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

/// Each pixel of a diagnostic decoded to linear sRGB, by rows.
fn linear_pixels(diagnostic: &str, width: usize, height: usize) -> Vec<Vec<[f64; 3]>> {
    let rgba = capture_rgba(
        diagnostic,
        &[
            "--diagnostic",
            diagnostic,
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
        ],
    );
    let pixels = rgba.as_chunks::<4>().0;
    (0..height)
        .map(|row| {
            (0..width)
                .map(|column| {
                    [0, 1, 2].map(|channel| {
                        srgb_channel_to_linear(pixels[row * width + column][channel])
                    })
                })
                .collect()
        })
        .collect()
}

/// Wet level ground returns each light as the body of water over it does as
/// its depth goes to zero, so level ground keeps its brightness as the tide
/// covers it: bright sand, rock and grey alike, for diffuse light, the Sun and
/// the Moon.
#[test]
fn wet_level_ground_matches_the_water_column_at_no_depth() {
    let rows = linear_pixels("lighting_wet_ground_waterline", 3, 3);
    for (light, row) in ["diffuse", "Sun", "Moon"].iter().zip(&rows) {
        for (ground, pixel) in ["sand", "rock", "grey"].iter().zip(row) {
            for ratio in pixel.map(|encoded| 1.0 + (encoded - 0.5) / 50.0) {
                assert!(
                    (ratio - 1.0).abs() <= 1e-3,
                    "{ground} in {light} light: wet ground per unit of water 0.1 mm deep {ratio}",
                );
            }
        }
    }
}

/// Ångström (1925) measured dry sand of albedo 0.182 at 0.091 wet, and two
/// more wettings at 0.081, as Lekner and Dorf (1988, Appl. Opt. 27) list
/// them. Wet sand lies within that spread or up to 15% above it. Wet grey
/// stays grey and never brightens, up to 0.8, beyond the brightest wettable
/// cover.
#[test]
fn wet_sand_darkens_as_angstrom_measured() {
    let rows = linear_pixels("lighting_wet_ground_albedo", 5, 2);
    let decode = |encoded: f64| 0.25 * encoded / (1.0 - encoded);
    let sand = decode(rows[0][0][1]);
    assert!(
        (0.081..=0.105).contains(&sand),
        "wet sand of dry albedo 0.182: {sand}"
    );
    for (wet, dry) in rows[0].iter().zip(&rows[1]) {
        let [red, green, blue] = wet.map(decode);
        assert!(
            (red - green).abs() < 0.01 * green && (blue - green).abs() < 0.01 * green,
            "wet grey {wet:?} takes a hue",
        );
        assert!(
            green <= decode(dry[1]),
            "wet grey {green} brighter than dry {}",
            decode(dry[1])
        );
    }
}

/// The closed form of clear sky's sunlit seconds matches numerical
/// integration within a thousandth of a day of zenith sunlight, across
/// latitudes past the pole, the solstices and equinox, spans across noon and
/// across a night, and polar day and night.
#[test]
fn clear_sky_sunlight_matches_numerical_integration() {
    let rows = linear_pixels("lighting_clear_sky_sunlight", 12, 12);
    for (row, pixels) in rows.iter().enumerate() {
        for (column, pixel) in pixels.iter().enumerate() {
            let error = (pixel[0] - 0.5) / 200.0;
            assert!(
                error.abs() <= 1e-3,
                "row {row}, column {column}: {error} days"
            );
        }
    }
}

/// The tide's wetness along `lighting_tidal_wetness` at the coast at a world
/// time: per elevation step, the wet share of the ground in `row` and whether
/// water covers it.
fn tidal_wetness_row(world_time_seconds: f64, row: usize) -> Vec<(f64, bool)> {
    let rgba = capture_rgba(
        "tidal-wetness",
        &[
            "--diagnostic",
            "lighting_tidal_wetness",
            "--camera-x",
            &COAST_M[0].to_string(),
            "--camera-y",
            &COAST_M[1].to_string(),
            "--view-height",
            "0.01",
            "--world-time",
            &world_time_seconds.to_string(),
            "--width",
            "200",
            "--height",
            "3",
        ],
    );
    rgba.as_chunks::<4>().0[200 * row..200 * (row + 1)]
        .iter()
        .map(|pixel| (srgb_channel_to_linear(pixel[0]), pixel[1] > 127))
        .collect()
}

/// The sand's wetness by `tidal_wetness_row`.
fn tidal_wetness(world_time_seconds: f64) -> Vec<(f64, bool)> {
    tidal_wetness_row(world_time_seconds, 0)
}

/// The elevation of the step `lighting_tidal_wetness` puts in `column`.
fn step_elevation_m(column: usize) -> f64 {
    -2.0 + 0.03 * column as f64
}

fn water_height_m(world_time_seconds: f64) -> f64 {
    let world = WorldGeometry::default();
    f64::from(procedural_water_height_m(
        world,
        SEED,
        world_time_seconds,
        Vector::new(COAST_M[0], COAST_M[1]),
    ))
}

fn sun_height(world_time_seconds: f64) -> f64 {
    let world = WorldGeometry::default();
    let conditions = world_celestial_conditions(
        world,
        world_time_seconds,
        Vector::new(COAST_M[0], COAST_M[1]),
    );
    f64::from(celestial_lighting(conditions).sun_direction[2])
}

/// World times of the coast's low waters, then high waters, over `days`, to
/// five minutes.
fn tide_turns(days: f64) -> (Vec<f64>, Vec<f64>) {
    let times: Vec<f64> = (0..(days * 288.0) as usize)
        .map(|step| 300.0 * step as f64)
        .collect();
    let heights: Vec<f64> = times.iter().map(|&time| water_height_m(time)).collect();
    let mut lows = Vec::new();
    let mut highs = Vec::new();
    for index in 1..times.len() - 1 {
        if heights[index] < heights[index - 1] && heights[index] <= heights[index + 1] {
            lows.push(times[index]);
        }
        if heights[index] > heights[index - 1] && heights[index] >= heights[index + 1] {
            highs.push(times[index]);
        }
    }
    (lows, highs)
}

/// The height above the water of the highest wet step above it.
fn wet_band_m(steps: &[(f64, bool)], water_m: f64) -> f64 {
    steps
        .iter()
        .enumerate()
        .filter(|(_, (wet, covered))| !covered && *wet > 0.5)
        .map(|(column, _)| step_elevation_m(column) - water_m)
        .fold(0.0, f64::max)
}

/// Two hours either side of a morning low water, the water stands at about
/// the same height, but ground the falling tide has just left under the
/// early Sun is still wet, while ground the rising tide approaches has dried
/// in the hours of sunlight since the water left it.
#[test]
fn a_falling_tide_leaves_a_wider_wet_band_than_a_rising_tide() {
    let (lows, _) = tide_turns(30.0);
    let low = *lows
        .iter()
        .find(|&&low| {
            let (falling, rising) = (sun_height(low - 7_200.0), sun_height(low + 7_200.0));
            low > 7_200.0 && falling > 0.05 && falling < 0.3 && rising > 0.5
        })
        .expect("a morning low water");
    let (falling, rising) = (low - 7_200.0, low + 7_200.0);
    let falling_band = wet_band_m(&tidal_wetness(falling), water_height_m(falling));
    let rising_band = wet_band_m(&tidal_wetness(rising), water_height_m(rising));
    assert!(
        falling_band > rising_band + 0.1,
        "wet band above the water on the falling tide {falling_band} m, rising {rising_band} m",
    );
}

/// From a high water at dusk down to the next low water, ground never wets
/// again without water, and ground the tide left in the first hour after dusk
/// stays wet until that low water before sunrise.
#[test]
fn ground_the_tide_leaves_at_dusk_stays_wet_through_the_night() {
    let (lows, highs) = tide_turns(30.0);
    let high = *highs
        .iter()
        .find(|&&high| {
            let low = lows.iter().find(|&&low| low > high).unwrap();
            sun_height(high) < 0.05
                && sun_height(high) > -0.05
                && sun_height(high + 600.0) < sun_height(high)
                && sun_height(*low) < -0.1
        })
        .expect("a high water at dusk followed by a low water at night");
    let low = *lows.iter().find(|&&low| low > high).unwrap();
    let clocks: Vec<f64> = (0..=12)
        .map(|step| high + (low - high) * f64::from(step) / 12.0)
        .collect();
    let profiles: Vec<Vec<(f64, bool)>> =
        clocks.iter().map(|&clock| tidal_wetness(clock)).collect();
    for (earlier, later) in profiles.iter().zip(&profiles[1..]) {
        for (column, ((before, _), (after, _))) in earlier.iter().zip(later).enumerate() {
            assert!(
                *after <= before + 0.01,
                "ground {} m wets from {before} to {after} without water",
                step_elevation_m(column),
            );
        }
    }
    let (dusk_m, an_hour_later_m) = (water_height_m(high), water_height_m(high + 3_600.0));
    let night = profiles.last().unwrap();
    for (column, (wet, _)) in night.iter().enumerate() {
        let elevation_m = step_elevation_m(column);
        if elevation_m > an_hour_later_m && elevation_m < dusk_m {
            assert!(
                *wet > 0.99,
                "ground {elevation_m} m left at dusk is {wet} wet at night"
            );
        }
    }
}

/// Stockdon et al.'s (2006, Coastal Eng. 53) 2% run-up under waves of 4 m and
/// 11 s: 2.542 m on a slope of 0.1, py-wave-runup's worked `Stockdon2006`
/// value. On a dissipative slope of 0.01, of Iribarren number 0.07, the
/// general fit stays within the fits' 0.38 m root-mean-square error of
/// Stockdon's dissipative fit, 0.043 √(H₀ L₀) = 1.182 m, with L₀ = g T² / 2π.
#[test]
fn run_up_follows_stockdon() {
    let rgba = capture_rgba(
        "run-up",
        &[
            "--diagnostic",
            "lighting_wave_run_up",
            "--width",
            "2",
            "--height",
            "1",
        ],
    );
    let run_up_m: Vec<f64> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| f64::from(u32::from(pixel[0]) * 256 + u32::from(pixel[1])) / 1000.0)
        .collect();
    assert!(
        (run_up_m[0] - 2.542_036).abs() <= 1e-3,
        "run-up {} m",
        run_up_m[0]
    );
    assert!(
        (run_up_m[1] - 1.182_050).abs() <= 0.38,
        "dissipative run-up {} m",
        run_up_m[1],
    );
}

/// At high water on a surf beach, waves run up the sand and wet it to their
/// run-up above the water, beyond the sand's capillary rise.
#[test]
fn surf_wets_the_beach_to_its_run_up() {
    let (_, highs) = tide_turns(3.0);
    let high = highs[1];
    let water_m = water_height_m(high);
    let band_m = wet_band_m(&tidal_wetness_row(high, 2), water_m);
    assert!(
        band_m >= 0.33 + 2.542 - 0.04,
        "surf of 4 m wets {band_m} m above the water at high water",
    );
}

/// The world's clocks that sample its weather: eight, 3.37 days apart from
/// 37 s, so their local times of day spread.
fn weather_clocks() -> impl Iterator<Item = f64> {
    (0..8).map(|step| 37.0 + (f64::from(step) * 3.37 * 86_400.0).round())
}

/// A diagnostic over the whole default world, 128 by 128 points, at a clock.
fn whole_world(diagnostic: &str, world_time_seconds: f64) -> Vec<[u8; 4]> {
    capture_rgba(
        diagnostic,
        &[
            "--diagnostic",
            diagnostic,
            "--camera-x",
            "0",
            "--camera-y",
            "0",
            "--view-height",
            "64000",
            "--width",
            "128",
            "--height",
            "128",
            "--world-time",
            &world_time_seconds.to_string(),
        ],
    )
    .as_chunks::<4>()
    .0
    .to_vec()
}

/// The canopy's rain-capable share of the world's weather, over space and
/// time, matches the share where the cloud scheme rains, as its calibration
/// sets it.
#[test]
fn the_rain_capable_canopy_covers_as_much_as_rain_falls() {
    let (mut rain_capable, mut raining, mut count) = (0.0, 0.0, 0.0);
    for clock in weather_clocks() {
        let weather = whole_world("lighting_weather_wetness", clock);
        let calibration = whole_world("lighting_weather_wetness_calibration", clock);
        for (wetness, rain) in weather.iter().zip(&calibration) {
            rain_capable += srgb_channel_to_linear(wetness[1]);
            raining += if rain[2] > 127 { 1.0 } else { 0.0 };
            count += 1.0;
        }
    }
    let (rain_capable, raining) = (rain_capable / count, raining / count);
    assert!(
        (rain_capable - raining).abs() <= 0.1 * raining,
        "rain-capable share {rain_capable}, raining share {raining}",
    );
}

/// The present weather wets bare ground in fog and under a thick deck, but
/// not under clear dry air nor below freezing.
#[test]
fn fog_and_thick_decks_wet_ground_while_dry_air_and_frost_do_not() {
    let rgba = capture_rgba(
        "weather-cases",
        &[
            "--diagnostic",
            "lighting_weather_wetness_cases",
            "--width",
            "6",
            "--height",
            "1",
        ],
    );
    let wet: Vec<f64> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| srgb_channel_to_linear(pixel[0]))
        .collect();
    let (clear, fog, deck, frozen) = (wet[0], wet[1], wet[2], wet[3]);
    assert!(fog > 0.9 && deck > 0.9, "fog {fog}, thick deck {deck}");
    assert!(clear < 0.01, "clear dry air {clear}");
    assert!(frozen < 0.01, "fog under a deck at -5 °C {frozen}");
}

/// Weather's wetness follows its slow causes, so it changes far less from
/// one minute to the next than over half an hour.
#[test]
fn weather_wetness_does_not_flicker() {
    let clock = 37.0 + 2.0 * 86_400.0;
    let wetness = |offset_s: f64| -> Vec<f64> {
        whole_world("lighting_weather_wetness", clock + offset_s)
            .iter()
            .map(|pixel| srgb_channel_to_linear(pixel[0]))
            .collect()
    };
    let (now, a_minute, half_an_hour) = (wetness(0.0), wetness(60.0), wetness(1_800.0));
    let change = |later: &[f64]| {
        now.iter()
            .zip(later)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / now.len() as f64
    };
    let (minute, half_hour) = (change(&a_minute), change(&half_an_hour));
    assert!(
        half_hour > 0.0 && minute < 0.2 * half_hour,
        "mean change over a minute {minute}, over half an hour {half_hour}",
    );
}

/// The weather's wetness of `lighting_weather_wetness_cases`' humid air,
/// nine tenths saturated, at a view centre and world time: under clear sky,
/// then under full overcast.
fn humid_air_wetness(camera_m: [f32; 2], world_time_seconds: f64) -> [f64; 2] {
    let rgba = capture_rgba(
        "dew",
        &[
            "--diagnostic",
            "lighting_weather_wetness_cases",
            "--camera-x",
            &camera_m[0].to_string(),
            "--camera-y",
            &camera_m[1].to_string(),
            "--view-height",
            "0.01",
            "--width",
            "6",
            "--height",
            "1",
            "--world-time",
            &world_time_seconds.to_string(),
        ],
    );
    let pixels = rgba.as_chunks::<4>().0;
    [4, 5].map(|column| srgb_channel_to_linear(pixels[column][0]))
}

/// Humid clear air wets ground with dew through a night until the morning's
/// sunlight evaporates it, but not under full overcast, which keeps the
/// ground as warm as the air. At the coast near the equator, a world time of
/// 0 is 10:49 local time.
#[test]
fn dew_forms_at_night_and_dries_in_the_morning() {
    let local_hour = |hour: f64| 86_400.0 + (hour - 10.812) * 3_600.0;
    let [night, night_overcast] = humid_air_wetness(COAST_M, local_hour(3.0));
    assert!(night > 0.9, "humid clear night {night}");
    assert!(
        night_overcast < 0.01,
        "humid overcast night {night_overcast}"
    );
    let [noon, _] = humid_air_wetness(COAST_M, local_hour(12.0));
    assert!(noon < 0.01, "humid clear noon {noon}");
}

/// The view centre at a latitude on the world's meridian x = 0.
fn latitude_m(world: WorldGeometry, degrees: f64) -> [f32; 2] {
    [0.0, (degrees / 180.0 * world.height_m()) as f32]
}

/// Under the midnight Sun no night cools the ground to dew.
#[test]
fn polar_day_holds_no_dew() {
    let world = WorldGeometry::default();
    // 80° north at the June solstice, 181.7 days from the world's start.
    for hour in [0.0, 6.0, 12.0, 18.0] {
        let clock = 181.7 * 86_400.0 + hour * 3_600.0;
        let [dew, _] = humid_air_wetness(latitude_m(world, 80.0), clock);
        assert!(dew < 0.01, "dew {dew} at {hour} h into the solstice");
    }
}

/// The world time, to a second, at which the Sun's centre crosses the
/// horizon at `camera_m`, rising if `rising`, within the day from `start`.
fn horizon_crossing(camera_m: [f32; 2], start: f64, rising: bool) -> f64 {
    let world = WorldGeometry::default();
    let height = |time: f64| {
        let conditions =
            world_celestial_conditions(world, time, Vector::new(camera_m[0], camera_m[1]));
        f64::from(celestial_lighting(conditions).sun_direction[2])
    };
    let (mut low, mut high) = (0..288)
        .map(|step| {
            (
                start + 300.0 * f64::from(step),
                start + 300.0 * f64::from(step + 1),
            )
        })
        .find(|&(before, after)| {
            (height(before) < 0.0) == rising && (height(after) < 0.0) != rising
        })
        .expect("a horizon crossing within the day");
    while high - low > 1.0 {
        let middle = 0.5 * (low + high);
        if (height(middle) < 0.0) == rising {
            low = middle;
        } else {
            high = middle;
        }
    }
    high.round()
}

/// Dew changes continuously in time where days or nights are short: across
/// a sunset ending a short winter day and a sunrise ending a short summer
/// night, both at 66° north, and local noon in polar night at 80° north. Forming and drying change it by at most a few hundredths a
/// minute.
#[test]
fn dew_stays_continuous_through_short_days_and_nights() {
    let world = WorldGeometry::default();
    let winter = 86_400.0;
    let summer = 181.7 * 86_400.0;
    let polar_noon = {
        let camera_m = latitude_m(world, 80.0);
        // Local noon at x = 0 falls where the day phase, 0.5833 at time 0,
        // reaches one half.
        (camera_m, winter + (1.0 - 0.5833 + 0.5) * 86_400.0)
    };
    for (label, (camera_m, event)) in [
        (
            "short winter day's sunset",
            (
                latitude_m(world, 66.0),
                horizon_crossing(latitude_m(world, 66.0), winter, false),
            ),
        ),
        (
            "short summer night's sunrise",
            (
                latitude_m(world, 66.0),
                horizon_crossing(latitude_m(world, 66.0), summer, true),
            ),
        ),
        ("polar night's noon", polar_noon),
    ] {
        let dew: Vec<f64> = (-5..=5)
            .map(|minute| humid_air_wetness(camera_m, event + 60.0 * f64::from(minute))[0])
            .collect();
        for (step, pair) in dew.windows(2).enumerate() {
            assert!(
                (pair[1] - pair[0]).abs() <= 0.05,
                "{label}: dew {} then {} a minute later ({dew:?}, step {step})",
                pair[0],
                pair[1],
            );
        }
    }
}
