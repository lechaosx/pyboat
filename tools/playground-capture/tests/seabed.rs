//! The seabed's mobility follows the wind climate's waves, and the sediment
//! they stir from it follows the bed's cover, the waves' stress on it and
//! what it holds.

use std::{
    fs,
    io::BufReader,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::WorldGeometry;

fn capture_rgba(name: &str, arguments: &[&str]) -> Vec<u8> {
    // Tests running in parallel may capture the same view.
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let output = std::env::temp_dir().join(format!(
        "pyboat-seabed-{}-{}-{name}.png",
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

/// Each pixel of a diagnostic that writes a 16-bit number, its high byte in
/// red and its low in green, by rows.
fn sixteen_bit_pixels(diagnostic: &str, width: usize, height: usize) -> Vec<Vec<f64>> {
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
                    let pixel = pixels[row * width + column];
                    f64::from(u32::from(pixel[0]) * 256 + u32::from(pixel[1]))
                })
                .collect()
        })
        .collect()
}

const MOBILITY_COLUMNS: usize = 161;

fn seabed_mobility() -> Vec<Vec<f64>> {
    sixteen_bit_pixels("lighting_seabed_mobility", MOBILITY_COLUMNS, 5)
}

/// The table of the seabed's mobility holds what the wave functions give over
/// the wind climate: within 0.002 at its depths, the integration's and
/// precision's spread, and within 0.01 between them. On failure it prints the
/// table afresh.
#[test]
fn the_seabed_mobility_table_follows_the_wave_climate() {
    let rows = seabed_mobility();
    let share = |row: usize, column: usize| rows[row][column] / 65535.0;
    let mut worst = 0.0_f64;
    let mut failures = Vec::new();
    for (grain, table_row) in [("sand", 0), ("fine sediment", 2)] {
        for column in 0..MOBILITY_COLUMNS {
            let (table, integrated) = (share(table_row, column), share(table_row + 1, column));
            let bound = if column % 2 == 0 { 0.002 } else { 0.01 };
            worst = worst.max((table - integrated).abs() / bound);
            if (table - integrated).abs() > bound {
                let depth_m = 2.0_f64.powf(column as f64 / 16.0 - 2.0);
                failures.push(format!(
                    "{grain} at {depth_m:.3} m: table {table:.5}, integrated {integrated:.5}"
                ));
            }
        }
    }
    let table: Vec<String> = (0..MOBILITY_COLUMNS)
        .step_by(2)
        .map(|column| {
            format!(
                "vec2<f32>({:.5}, {:.5}),",
                share(1, column),
                share(3, column)
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{}\n\nregenerated SEABED_MOBILITY:\n{}",
        failures.join("\n"),
        table
            .chunks(3)
            .map(|line| format!("    {}", line.join(" ")))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    assert!(worst <= 1.0);
}

/// The wavenumber times depth of waves whose deep-water wavenumber is `k0`
/// over water `depth_m` deep, solving the dispersion relation of linear wave
/// theory, kh tanh(kh) = k0 h, by Newton's method.
fn shallow_wavenumber_depth(k0: f64, depth_m: f64) -> f64 {
    let deep = k0 * depth_m;
    let mut kh = deep.max(deep.sqrt());
    for _ in 0..50 {
        let tanh = kh.tanh();
        kh -= (kh * tanh - deep) / (tanh + kh / kh.cosh().powi(2));
    }
    kh
}

/// Komar and Miller's threshold moves 0.25 mm quartz sand at 0.176 m/s under
/// 6 s waves. The orbital speed at the bed follows linear wave theory under a
/// 4 m sea peaking at Pierson and Moskowitz's wavelength for 15 m/s, 2π U² /
/// (0.769 g), within the 7% that Eckart's 5% in kh gives sinh(kh) there.
#[test]
fn waves_move_the_bed_as_linear_wave_theory_and_komar_and_miller_give() {
    let row = &seabed_mobility()[4];
    let threshold_m_s = row[0] / 1e4;
    assert!(
        (threshold_m_s - 0.1762).abs() <= 5e-4,
        "sand threshold {threshold_m_s} m/s at 6 s"
    );
    let gravity = 9.81;
    let k0 = 0.769 * gravity / (15.0_f64 * 15.0);
    let period_s = std::f64::consts::TAU / (k0 * gravity).sqrt();
    for (depth_m, rendered) in [(20.0, row[1] / 1e4), (30.0, row[2] / 1e4)] {
        let expected =
            std::f64::consts::PI * 4.0 / (period_s * shallow_wavenumber_depth(k0, depth_m).sinh());
        assert!(
            (rendered / expected - 1.0).abs() <= 0.07,
            "{depth_m} m down: {rendered} m/s, linear wave theory {expected} m/s",
        );
    }
}

/// The wind climate's median, 90th and 99th percentiles lie within 5% of
/// those of the game's resolved surface wind, sampled at 20 000 places and
/// times over a year.
#[test]
fn the_wind_climate_matches_the_games_wind() {
    let world = WorldGeometry::default();
    let (width_m, height_m) = (world.width_m(), world.height_m());
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut unit = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1_u64 << 53) as f64
    };
    let mut speeds: Vec<f32> = (0..20_000)
        .map(|_| {
            let position_m = Vector::new(
                (unit() * width_m) as f32,
                ((unit() - 0.5) * height_m) as f32,
            );
            let tick = (unit() * 365.0 * 86_400.0 * 60.0) as u64;
            game::procedural_weather(world, 0x5eed, tick, position_m)
                .wind_velocity_m_s
                .length()
        })
        .collect();
    speeds.sort_by(f32::total_cmp);
    let climate = &seabed_mobility()[4][3..6];
    for (quantile, shipped) in [0.5, 0.9, 0.99].into_iter().zip(climate) {
        let sampled = f64::from(speeds[(quantile * (speeds.len() - 1) as f64) as usize]);
        let shipped = shipped / 1e3;
        assert!(
            (shipped / sampled - 1.0).abs() <= 0.05,
            "quantile {quantile}: climate {shipped} m/s, game {sampled} m/s",
        );
    }
}

fn srgb_channel_to_linear(channel: u8) -> f64 {
    let srgb = f64::from(channel) / 255.0;
    if srgb <= 0.040_45 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

/// Sand cover by `lighting_seabed_sand_cover`, by rows of `width` columns.
fn sand_cover(width: usize, height: usize) -> Vec<f64> {
    let rgba = capture_rgba(
        "sand-cover",
        &[
            "--diagnostic",
            "lighting_seabed_sand_cover",
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
        ],
    );
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| srgb_channel_to_linear(pixel[0]))
        .collect()
}

/// Below the beach, sand covers the bed in proportion to the beach's
/// sediment against the share of time the waves move it: level ground
/// without coastal rock, from 0.1 m above mean sea level to 40 m below, keeps
/// at least the beach's sand all the way down, gains more with depth as the
/// waves move it less often, unless the beach is all sand, and holds none
/// where the coastal geology holds no beach sediment. The bounds allow the
/// diagnostic's eight-bit steps.
#[test]
fn sand_covers_the_bed_below_the_beach_as_the_waves_leave_it() {
    let (width, height) = (401, 4);
    let pixels = sand_cover(width, height);
    let step = 2.0 / 255.0;
    for (row, amount) in [0.05, 0.2, 0.3, 0.5].into_iter().enumerate() {
        let cover: Vec<f64> = (0..width)
            .map(|column| pixels[row * width + column])
            .collect();
        let elevation_m = |column: usize| 0.1 - 0.1 * column as f64;
        if amount < 0.08 {
            assert!(
                cover.iter().all(|&share| share == 0.0),
                "coastal sediment {amount}: sand cover {cover:?}"
            );
            continue;
        }
        // Half a metre below mean sea level lies within each beach.
        let beach_column = 6;
        let beach = cover[beach_column];
        for column in beach_column + 1..width {
            assert!(
                cover[column] >= cover[column - 1] - step,
                "coastal sediment {amount}: sand cover {} at {} m above {} at {} m",
                cover[column - 1],
                elevation_m(column - 1),
                cover[column],
                elevation_m(column),
            );
        }
        assert!(
            cover[width - 1] > (beach + step).min(1.0 - step),
            "coastal sediment {amount}: sand cover {} 40 m down, the beach's {beach}",
            cover[width - 1],
        );
    }
}

/// Sand rests on the bed below the beach up to its angle of rest: on a
/// submerged slope of 0.3 it covers the bed as on level ground, and on a
/// slope of 0.75, steeper than the 30-35° sands rest at, none of it, from
/// 5 m down to 40 m. The bound allows the diagnostic's eight-bit steps.
#[test]
fn sand_rests_on_the_bed_up_to_its_angle_of_rest() {
    let width = 401;
    let cover = sand_cover(width, 6);
    for column in 51..width {
        let depth_m = 0.1 * column as f64 - 0.1;
        let (level, gentle, steep) = (
            cover[2 * width + column],
            cover[4 * width + column],
            cover[5 * width + column],
        );
        assert!(
            (gentle - level).abs() <= 2.0 / 255.0,
            "{depth_m} m down: sand cover {gentle} on a slope of 0.3, {level} level",
        );
        assert_eq!(
            steep, 0.0,
            "{depth_m} m down: sand cover on a slope of 0.75"
        );
    }
}

/// Fine sediment by `lighting_stirred_sediment`: its named cases in g/m³,
/// then what a bed of fine sediment alone gives the column above it in g/m²,
/// by wind and depth.
fn stirred_sediment() -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut rows = sixteen_bit_pixels("lighting_stirred_sediment", 16, 9);
    let cases = rows
        .remove(0)
        .into_iter()
        .map(|units| units / 100.0)
        .collect();
    let columns = rows
        .into_iter()
        .map(|row| row.into_iter().map(|units| units / 10.0).collect())
        .collect();
    (cases, columns)
}

/// Clear sandy coasts hold little: under the wind climate's median wind a
/// bed all sand 5 m deep holds 1-2 g/m³, as the fringing reefs of Magnetic
/// Island do (Waterhouse et al. 2021), and beds 2 to 10 m deep hold some but
/// no more than Cleveland Bay's fair-weather background of under 5 g/m³
/// (Larcombe et al. 1995), as Jones et al. (2021, Front. Mar. Sci. 8, 727206)
/// cite them. The ceiling takes a 10% margin, 5.5 g/m³: the availability
/// calibrated at 5 m gives a bed 10 m deep 5.24 g/m³. Under 2.9 m/s they hold
/// no more than 2 g/m³.
#[test]
fn sandy_coasts_hold_what_clear_coasts_measure() {
    let (cases, _) = stirred_sediment();
    let median = &cases[0..5];
    assert!(
        (1.0..=2.0).contains(&median[2]),
        "5 m of sand under the median wind: {} g/m³",
        median[2]
    );
    for (depth_m, &load) in [2, 3, 5, 7, 10].iter().zip(median) {
        assert!(
            load > 0.0 && load <= 5.5,
            "{depth_m} m of sand under the median wind: {load} g/m³"
        );
    }
    for (depth_m, &load) in [2, 3, 5, 7, 10].iter().zip(&cases[5..10]) {
        assert!(
            load <= 2.0,
            "{depth_m} m of sand under 2.9 m/s: {load} g/m³"
        );
    }
}

/// Waves stressing a bed of fine sediment alone 3 m down by 0.3 Pa hold
/// 10-100 g/m³, as Tampa Bay's storms raise it from about 10 to over 40 g/m³
/// (Sheng et al. 1995, as Green and Coco 2014 cite it).
#[test]
fn a_muddy_bed_holds_what_shallow_bays_measure() {
    let (cases, _) = stirred_sediment();
    assert!(
        (10.0..=100.0).contains(&cases[10]),
        "fine sediment 3 m down under 0.3 Pa: {} g/m³",
        cases[10]
    );
}

/// A storm stirs tens of g/m³ from a bed of fine sediment alone 20 m down, as
/// a storm raised the Gulf of Lion's inner shelf, 26 m deep, to 70 g/m³ half
/// a metre above the bed (Ferré et al. 2005, Cont. Shelf Res. 25); the median
/// wind's waves leave it be. Open ocean and bare rock hold none even in a
/// storm.
#[test]
fn storms_stir_muddy_shelves_but_not_open_ocean_or_rock() {
    let (cases, _) = stirred_sediment();
    assert!(
        cases[11] >= 10.0,
        "fine sediment 20 m down in a storm: {} g/m³",
        cases[11]
    );
    assert_eq!(
        cases[12], 0.0,
        "fine sediment 20 m down under the median wind"
    );
    assert_eq!(cases[13], 0.0, "sand 10 km down in a storm");
    assert_eq!(cases[14], 0.0, "bare rock 5 m down in a storm");
}

/// Waves lift no more than the bed holds, 500 g/m² of fine sediment, the
/// York River's at the 0.6 Pa top of its measured stresses (Dickhudt et al.
/// 2009), at any wind or depth; and none where they leave the bed below its
/// threshold, as calm water 2 m deep and more.
#[test]
fn waves_lift_no_more_than_the_bed_holds() {
    let (_, columns) = stirred_sediment();
    for (wind_m_s, row) in [0, 3, 6, 9, 12, 15, 20, 25].iter().zip(&columns) {
        for (column, &lifted) in row.iter().enumerate() {
            let depth_m = 2.0_f64.powf(column as f64 / 2.0 - 1.0);
            assert!(
                lifted <= 500.05,
                "{lifted} g/m² from {depth_m} m down under {wind_m_s} m/s"
            );
            if *wind_m_s == 0 && depth_m >= 2.0 {
                assert_eq!(lifted, 0.0, "calm water {depth_m} m deep");
            }
        }
    }
}

/// Under the median wind, sandy shallows 2-5 m deep stay clear enough to
/// show their bed: the sediment the waves stir there leaves at least half of
/// the light a bright sand bed adds through clear seawater. Shallows milky
/// enough to hide their bed would fail it.
#[test]
fn the_median_wind_leaves_sandy_shallows_showing_their_bed() {
    let rgba = capture_rgba(
        "stirred-shallows",
        &[
            "--diagnostic",
            "lighting_stirred_shallows",
            "--width",
            "3",
            "--height",
            "1",
        ],
    );
    let pixels = rgba.as_chunks::<4>().0;
    for (column, depth_m) in [2.0, 3.0, 5.0].into_iter().enumerate() {
        let share = srgb_channel_to_linear(pixels[column][0]);
        assert!(
            share >= 0.5,
            "sand {depth_m} m deep keeps {share} of the light it adds through clear water",
        );
    }
}

/// The wavelength each band takes its optics at.
const BAND_WAVELENGTHS_NM: [f64; 8] = [437.8, 478.0, 501.8, 531.2, 565.9, 589.8, 615.8, 665.3];

/// Stirred sediment absorbs as the colour of its bed implies, by
/// Kubelka and Munk's a / b_b = (1 - w)² / 2w: a 30% grey bed's sediment
/// absorbs alike in every band, within the diagnostic's 3%; and a bed of
/// Babin et al.'s mineral colour gives their mineral sediment's absorption
/// per unit of backscattering, 0.041 e^(-0.0123 (λ - 443 nm)) over
/// 0.010 (555 nm / λ)^0.5, within 30% in each band. The 30% covers two
/// modelled steps between the colour and the bands, projecting three colour
/// channels onto eight bands and immersing the bed, which together leave the
/// red bands up to 27% low.
#[test]
fn stirred_sediment_absorbs_as_the_bed_it_came_from() {
    let rgba = capture_rgba(
        "sediment-optics",
        &[
            "--diagnostic",
            "lighting_sediment_optics",
            "--width",
            "8",
            "--height",
            "2",
        ],
    );
    let pixels = rgba.as_chunks::<4>().0;
    let ratio = |index: usize| {
        let encoded = srgb_channel_to_linear(pixels[index][0]);
        encoded / (1.0 - encoded)
    };
    let grey: Vec<f64> = (8..16).map(ratio).collect();
    for &band in &grey {
        assert!(
            (band / grey[0] - 1.0).abs() <= 0.03,
            "grey bed's sediment by band {grey:?}"
        );
    }
    for (band, wavelength_nm) in BAND_WAVELENGTHS_NM.into_iter().enumerate() {
        let babin = 0.041 * (-0.0123 * (wavelength_nm - 443.0)).exp()
            / (0.010 * (555.0 / wavelength_nm).sqrt());
        let rendered = ratio(band);
        assert!(
            (rendered / babin - 1.0).abs() <= 0.3,
            "band at {wavelength_nm} nm: {rendered}, Babin {babin}"
        );
    }
}
