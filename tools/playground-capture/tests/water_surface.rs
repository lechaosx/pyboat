//! Water reflects each beam by Fresnel's law from facets whose slopes follow
//! Cox and Munk's statistics, seen from the drone camera, and reflects the
//! clear sky by a radiance that carries its diffuse light. Its body returns
//! light by its spectral optics over the bed beneath.

use std::{
    f64::consts::PI,
    fs,
    io::BufReader,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use browser::DroneCamera;
use game::procedural_weather;
use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::{TICKS_PER_SECOND, WorldGeometry};

/// Open ocean, the bench's open-ocean view centre, at the capture's default
/// world time.
const OPEN_OCEAN_M: [f64; 2] = [-14_900.0, -5_330.0];
const OPEN_OCEAN_WORLD_TIME_SECONDS: f64 = 37.0;
/// Open ocean where gusts blow at that time.
const GUSTY_OPEN_OCEAN_M: [f64; 2] = [-12_000.0, -21_000.0];
/// Water's refractive index, written out here so the oracle shares no code
/// with the shader.
const WATER_REFRACTIVE_INDEX: f64 = 1.34;

fn capture_rgba(name: &str, arguments: &[&str]) -> Vec<u8> {
    // Tests running in parallel may capture the same view.
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let output = std::env::temp_dir().join(format!(
        "pyboat-water-{}-{}-{name}.png",
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

/// One water pixel of `lighting_water_glint_*`.
struct GlintPixel {
    column: usize,
    row: usize,
    /// Sun glint per unit of the beam's light on level ground.
    glint: f64,
    wind_speed_m_s: f64,
    /// The wind's axis from +x.
    wind_axis_rad: f64,
}

struct GlintView {
    width: usize,
    height: usize,
    view_height_m: f64,
    camera_height_m: f64,
    sun_degrees: f64,
    camera_m: [f64; 2],
    world_time_seconds: f64,
}

impl GlintView {
    fn capture(&self, quality: &str) -> Vec<GlintPixel> {
        let (width, height) = (self.width.to_string(), self.height.to_string());
        let (camera_x, camera_y) = (self.camera_m[0].to_string(), self.camera_m[1].to_string());
        let world_time = self.world_time_seconds.to_string();
        let view_height = self.view_height_m.to_string();
        let camera_height = self.camera_height_m.to_string();
        let diagnostic = format!("lighting_water_glint_{}_{quality}", self.sun_degrees);
        let rgba = capture_rgba(
            &format!("{diagnostic}-{view_height}-{camera_x}-{world_time}"),
            &[
                "--diagnostic",
                &diagnostic,
                "--width",
                &width,
                "--height",
                &height,
                "--camera-x",
                &camera_x,
                "--camera-y",
                &camera_y,
                "--view-height",
                &view_height,
                "--camera-height",
                &camera_height,
                "--world-time",
                &world_time,
            ],
        );
        rgba.as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel[3] == 255)
            .map(|(index, pixel)| {
                let [glint, speed, axis] =
                    [0, 1, 2].map(|channel| srgb_channel_to_linear(pixel[channel]));
                // A saturated pixel takes the largest glint below saturation,
                // so it bounds rather than swamps a mean.
                let glint = glint.min(srgb_channel_to_linear(254));
                GlintPixel {
                    column: index % self.width,
                    row: index / self.width,
                    glint: 0.1 * glint / (1.0 - glint),
                    wind_speed_m_s: 10.0 * speed / (1.0 - speed),
                    wind_axis_rad: PI * axis,
                }
            })
            .collect()
    }

    /// A pixel's sea-level point relative to the view centre.
    fn offset_m(&self, column: usize, row: usize) -> [f64; 2] {
        let (width, height) = (self.width as f64, self.height as f64);
        [
            (column as f64 + 0.5 - 0.5 * width) / height * self.view_height_m,
            (0.5 * height - row as f64 - 0.5) / height * self.view_height_m,
        ]
    }

    /// The unit vector from a pixel's sea-level point to the camera.
    fn view_direction(&self, column: usize, row: usize) -> [f64; 3] {
        let offset_m = self.offset_m(column, row);
        normalize([-offset_m[0], -offset_m[1], self.camera_height_m])
    }

    fn sun_direction(&self) -> [f64; 3] {
        let height = self.sun_degrees.to_radians();
        [height.cos(), 0.0, height.sin()]
    }
}

fn normalize(vector: [f64; 3]) -> [f64; 3] {
    let length = vector
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt();
    vector.map(|component| component / length)
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}

/// Unpolarised Fresnel reflectance from air into water.
fn fresnel_reflectance(cos_incidence: f64) -> f64 {
    let n = WATER_REFRACTIVE_INDEX;
    let cos_transmitted = (1.0 - (1.0 - cos_incidence * cos_incidence) / (n * n)).sqrt();
    let s = (cos_incidence - n * cos_transmitted) / (cos_incidence + n * cos_transmitted);
    let p = (n * cos_incidence - cos_transmitted) / (n * cos_incidence + cos_transmitted);
    0.5 * (s * s + p * p)
}

/// π times the radiance a sea of Cox and Munk's (1954) clean-surface slopes
/// reflects from a beam toward `view`, per unit of the beam's light on a
/// surface facing it: π R p / (4 cos θ_v cos⁴ β) for the density p of the
/// facet slope that mirrors the beam toward the view, tilted β. Their fit's
/// intercept is shared equally between the axes, as the renderer shares it.
fn cox_munk_glint(view: [f64; 3], light: [f64; 3], wind_speed_m_s: f64, wind_axis_rad: f64) -> f64 {
    let facet = normalize([view[0] + light[0], view[1] + light[1], view[2] + light[2]]);
    let slope = [-facet[0] / facet[2], -facet[1] / facet[2]];
    let (sin_axis, cos_axis) = wind_axis_rad.sin_cos();
    let upwind = slope[0] * cos_axis + slope[1] * sin_axis;
    let crosswind = -slope[0] * sin_axis + slope[1] * cos_axis;
    let upwind_variance = 0.0015 + 3.16e-3 * wind_speed_m_s;
    let crosswind_variance = 0.0015 + 1.92e-3 * wind_speed_m_s;
    let density = (-0.5
        * (upwind * upwind / upwind_variance + crosswind * crosswind / crosswind_variance))
        .exp()
        / (2.0 * PI * (upwind_variance * crosswind_variance).sqrt());
    PI * fresnel_reflectance(dot(view, facet)) * density / (4.0 * view[2] * facet[2].powi(4))
}

/// Smith's Λ of a direction for Cox and Munk's (1954) clean-surface slopes:
/// (1/μ) ∫_μ^∞ (q − μ) p(q) dq for the slope q along the direction's azimuth
/// and μ the cotangent of its zenith angle (Bourlier et al. 2006, PIER 58,
/// eq. 14), integrated numerically over the normal density of q.
fn smith_lambda(direction: [f64; 3], wind_speed_m_s: f64, wind_axis_rad: f64) -> f64 {
    let horizontal = direction[0].hypot(direction[1]);
    if horizontal == 0.0 {
        return 0.0;
    }
    let azimuth = direction[1].atan2(direction[0]) - wind_axis_rad;
    let variance = (0.0015 + 3.16e-3 * wind_speed_m_s) * azimuth.cos().powi(2)
        + (0.0015 + 1.92e-3 * wind_speed_m_s) * azimuth.sin().powi(2);
    let deviation = variance.sqrt();
    let mu = direction[2] / horizontal;
    let steps = 4000;
    let step = 12.0 * deviation / f64::from(steps);
    let integral: f64 = (0..steps)
        .map(|index| {
            let q = mu + (f64::from(index) + 0.5) * step;
            (q - mu) * (-0.5 * q * q / variance).exp() / ((2.0 * PI).sqrt() * deviation) * step
        })
        .sum();
    integral / mu
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0), |(sum, count), value| (sum + value, count + 1));
    sum / f64::from(count)
}

/// Beyond 500 m pixels no wave mode or short-wave tile is resolved, so
/// every pixel takes the whole of Cox and Munk's slope distribution at its
/// wind and mean glint follows their closed form.
#[test]
fn wide_open_water_glints_as_cox_and_munk_slopes_mirror_the_sun() {
    let view = GlintView {
        width: 64,
        height: 40,
        view_height_m: 20_000.0,
        camera_height_m: 20_000.0,
        sun_degrees: 60.0,
        camera_m: OPEN_OCEAN_M,
        world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
    };
    let pixels = view.capture("high");
    assert!(
        pixels.len() > view.width * view.height / 2,
        "{} water pixels",
        pixels.len()
    );
    assert_cox_and_munk_mean_glint(&view, &pixels);
}

/// At boat zoom the wave modes and short-wave tiles resolve part of the
/// slopes and the unresolved facets carry the rest of Cox and Munk's
/// variance, so mean glint still follows their closed form.
#[test]
fn open_water_at_boat_zoom_keeps_cox_and_munks_mean_glint() {
    let view = GlintView {
        width: 960,
        height: 600,
        view_height_m: 240.0,
        camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
        sun_degrees: 60.0,
        camera_m: OPEN_OCEAN_M,
        world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
    };
    assert_cox_and_munk_mean_glint(&view, &view.capture("high"));
}

/// Gusts roughen the water: where the game's gusts raise Cox and Munk's mean
/// glint well beyond the tolerance, the rendered mean glint follows their
/// slopes at the game's gusting surface wind under each pixel.
#[test]
fn gusts_roughen_the_glint() {
    let view = GlintView {
        width: 960,
        height: 600,
        view_height_m: 240.0,
        camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
        sun_degrees: 60.0,
        camera_m: GUSTY_OPEN_OCEAN_M,
        world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
    };
    let pixels = view.capture("high");
    let sun = view.sun_direction();
    let tick = (view.world_time_seconds * TICKS_PER_SECOND as f64) as u64;
    let winds: Vec<_> = pixels
        .iter()
        .map(|pixel| {
            let offset_m = view.offset_m(pixel.column, pixel.row);
            let weather = procedural_weather(
                WorldGeometry::default(),
                0x5eed,
                tick,
                Vector::new(
                    (view.camera_m[0] + offset_m[0]) as f32,
                    (view.camera_m[1] + offset_m[1]) as f32,
                ),
            );
            (
                weather.wind_velocity_m_s,
                weather.wind_velocity_m_s - weather.surface_gust_velocity_m_s,
            )
        })
        .collect();
    let glint_at = |pick: fn(&(Vector, Vector)) -> Vector| {
        mean(pixels.iter().zip(&winds).map(|(pixel, wind)| {
            let wind = pick(wind).as_dvec2();
            cox_munk_glint(
                view.view_direction(pixel.column, pixel.row),
                sun,
                wind.length(),
                wind.y.atan2(wind.x),
            ) / sun[2]
        }))
    };
    let gusting = glint_at(|wind| wind.0);
    let sustained = glint_at(|wind| wind.1);
    assert!(
        (sustained / gusting - 1.0).abs() > 0.15,
        "Cox and Munk's mean glint {gusting} at the gusting wind, {sustained} at the sustained",
    );
    let rendered = mean(pixels.iter().map(|pixel| pixel.glint));
    assert!(
        (rendered / gusting - 1.0).abs() < 0.05,
        "mean glint {rendered}, Cox and Munk {gusting} at the gusting wind, {sustained} at the sustained",
    );
}

/// The scene reads the gusts over water from the environment grid as direct
/// evaluation computes them: lit by the Sun alone at the gusty view, grid
/// water's mean luminance matches direct water's within 3%. The grid's
/// bilinear reconstruction and cell means leave 1.3%; a grid without gusts
/// leaves 6.5%.
#[test]
fn grid_water_takes_the_gusts_direct_evaluation_takes() {
    let (camera_x, camera_y) = (
        GUSTY_OPEN_OCEAN_M[0].to_string(),
        GUSTY_OPEN_OCEAN_M[1].to_string(),
    );
    let world_time = OPEN_OCEAN_WORLD_TIME_SECONDS.to_string();
    let mean_light = |diagnostic| {
        let rgba = capture_rgba(
            &format!("gusty-{diagnostic}"),
            &[
                "--diagnostic",
                diagnostic,
                "--width",
                "480",
                "--height",
                "300",
                "--camera-x",
                &camera_x,
                "--camera-y",
                &camera_y,
                "--view-height",
                "240",
                "--world-time",
                &world_time,
                "--cloud-shadows",
                "0",
                "--light-contributions",
                "1,0,0,0",
            ],
        );
        mean(rgba.as_chunks::<4>().0.iter().map(|pixel| {
            0.2126 * srgb_channel_to_linear(pixel[0])
                + 0.7152 * srgb_channel_to_linear(pixel[1])
                + 0.0722 * srgb_channel_to_linear(pixel[2])
        }))
    };
    let (grid, direct) = (mean_light("scene"), mean_light("scene_direct"));
    assert!(
        (grid / direct - 1.0).abs() < 0.03,
        "mean luminance of grid water {grid}, direct {direct}",
    );
}

/// Where the wave modes resolve more slope variance than Cox and Munk's total
/// leaves beside the short waves, at 15 m/s, the surface is rougher than
/// theirs by that excess, so its mean glint is not theirs. The short waves no
/// footprint resolves still roughen every facet on both axes, so the sea
/// glints as a lobe rather than as a mirror's isolated sparks: no more pixels
/// glint near saturation than Cox and Munk's slopes would give.
#[test]
fn steep_wave_modes_leave_short_waves_roughening_the_glint() {
    let view = GlintView {
        width: 960,
        height: 600,
        view_height_m: 240.0,
        camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
        sun_degrees: 60.0,
        camera_m: [30_667.0, 8_444.0],
        world_time_seconds: 63_037.0,
    };
    let pixels = view.capture("high");
    let bright = srgb_channel_to_linear(250);
    let bright_glint = 0.1 * bright / (1.0 - bright);
    let rendered = pixels
        .iter()
        .filter(|pixel| pixel.glint >= bright_glint)
        .count();
    let expected = cox_munk_glints(&view, &pixels)
        .filter(|glint| *glint >= bright_glint)
        .count();
    assert!(
        rendered <= expected,
        "{rendered} pixels near saturation, Cox and Munk {expected}",
    );
}

/// Each pixel's glint by `cox_munk_glint` at its view and wind, per unit of
/// the beam's light on level ground.
fn cox_munk_glints(view: &GlintView, pixels: &[GlintPixel]) -> impl Iterator<Item = f64> {
    let sun = view.sun_direction();
    pixels.iter().map(move |pixel| {
        cox_munk_glint(
            view.view_direction(pixel.column, pixel.row),
            sun,
            pixel.wind_speed_m_s,
            pixel.wind_axis_rad,
        ) / sun[2]
    })
}

fn assert_cox_and_munk_mean_glint(view: &GlintView, pixels: &[GlintPixel]) {
    let rendered = mean(pixels.iter().map(|pixel| pixel.glint));
    let expected = mean(cox_munk_glints(view, pixels));
    assert!(
        (rendered / expected - 1.0).abs() < 0.05,
        "mean glint {rendered}, Cox and Munk {expected}",
    );
}

/// A lower tier resolves fewer short-wave tile layers and leaves their slopes
/// to the unresolved distribution, so its mean glint matches the high tier's:
/// on open ocean at 4 m/s, and in a 29 m/s storm whose wave modes leave the
/// tile layers none of Cox and Munk's total beyond the short waves'.
#[test]
fn graphics_tiers_share_mean_glint_at_boat_zoom() {
    for (camera_m, world_time_seconds) in [
        (OPEN_OCEAN_M, OPEN_OCEAN_WORLD_TIME_SECONDS),
        ([-24_444.0, 25_333.0], 21_637.0),
    ] {
        let view = GlintView {
            width: 960,
            height: 600,
            view_height_m: 240.0,
            camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
            sun_degrees: 60.0,
            camera_m,
            world_time_seconds,
        };
        let high = mean(view.capture("high").iter().map(|pixel| pixel.glint));
        for quality in ["medium", "low"] {
            let lower = mean(view.capture(quality).iter().map(|pixel| pixel.glint));
            assert!(
                (lower / high - 1.0).abs() < 0.05,
                "at {camera_m:?}: high {high}, {quality} {lower}",
            );
        }
    }
}

/// Under a Sun 75° high toward +x, the camera sees the Sun mirrored by level
/// water 268 m to the right of the view centre, so a 1 km view glints more on
/// its right half, where facets nearer level mirror the Sun.
#[test]
fn a_wide_view_glints_more_on_the_suns_side() {
    let view = GlintView {
        width: 320,
        height: 200,
        view_height_m: 1_000.0,
        camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
        sun_degrees: 75.0,
        camera_m: OPEN_OCEAN_M,
        world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
    };
    let pixels = view.capture("high");
    assert!(
        pixels.len() > view.width * view.height / 2,
        "{} water pixels",
        pixels.len()
    );
    let half = |right: bool| {
        mean(
            pixels
                .iter()
                .filter(|pixel| (pixel.column >= view.width / 2) == right)
                .map(|pixel| pixel.glint),
        )
    };
    let (left, right) = (half(false), half(true));
    assert!(right > 2.0 * left, "left {left}, right {right}");
}

/// Waves shadow a low Sun's glint: under a Sun 3° high, mean glint over a
/// 2 km view from 1 km, whose sunward edge looks 32° down onto the glint's
/// flank, follows Cox and Munk's closed form times Heitz's (2014, JCGT 3,
/// eq. 99) height-correlated shadowing and masking,
/// 1/(1 + Λ(view) + Λ(Sun)), well below their closed form alone.
#[test]
fn waves_shadow_a_low_suns_glint() {
    let view = GlintView {
        width: 320,
        height: 200,
        view_height_m: 2_000.0,
        camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
        sun_degrees: 3.0,
        camera_m: OPEN_OCEAN_M,
        world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
    };
    let pixels = view.capture("high");
    assert!(
        pixels.len() > view.width * view.height / 2,
        "{} water pixels",
        pixels.len()
    );
    let sun = view.sun_direction();
    let unshadowed = mean(cox_munk_glints(&view, &pixels));
    let shadowed = mean(pixels.iter().map(|pixel| {
        let view_direction = view.view_direction(pixel.column, pixel.row);
        let lambda = |direction| smith_lambda(direction, pixel.wind_speed_m_s, pixel.wind_axis_rad);
        cox_munk_glint(
            view_direction,
            sun,
            pixel.wind_speed_m_s,
            pixel.wind_axis_rad,
        ) / sun[2]
            / (1.0 + lambda(view_direction) + lambda(sun))
    }));
    assert!(
        shadowed < 0.8 * unshadowed,
        "shadowed {shadowed}, unshadowed {unshadowed}",
    );
    let rendered = mean(pixels.iter().map(|pixel| pixel.glint));
    assert!(
        (rendered / shadowed - 1.0).abs() < 0.05,
        "mean glint {rendered}, shadowed {shadowed}, unshadowed {unshadowed}",
    );
}

/// The clear sky's reflected radiance, integrated over the sky with the
/// cosine weight, carries exactly its diffuse light at every Sun height.
#[test]
fn reflected_clear_sky_radiance_carries_its_diffuse_light() {
    let rgba = capture_rgba(
        "clear-sky-integral",
        &[
            "--diagnostic",
            "lighting_clear_sky_radiance_integral",
            "--width",
            "19",
            "--height",
            "1",
        ],
    );
    for (column, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let integral = 2.0 * srgb_channel_to_linear(pixel[0]);
        assert!(
            (integral - 1.0).abs() < 0.02,
            "Sun {}°: {integral}",
            5 * column,
        );
    }
}

/// The reflected clear sky keeps its shape while the Sun's disc sets: the
/// directly lit air that shapes it hardly changes over the disc's width, so
/// level water mirrors the same sky just before and just after.
#[test]
fn a_setting_disc_leaves_the_reflected_sky_unchanged() {
    let rgba = capture_rgba(
        "sky-through-sunset",
        &[
            "--diagnostic",
            "lighting_water_sky_through_sunset",
            "--width",
            "2",
            "--height",
            "1",
        ],
    );
    let [up, set] = [0, 4].map(|offset| srgb_channel_to_linear(rgba[offset]));
    assert!((set / up - 1.0).abs() < 0.05, "disc up {up}, set {set}");
}

/// A lower tier draws the finest short-wave layers a portrait phone's 3.5 cm
/// pixels at boat zoom mostly resolve, not the longest, so the slopes it draws
/// decorrelate within a metre at least as fully as the high tier's: slope
/// differences across 1 m make up as large a share of those across 10 m,
/// on open ocean in a light and in a moderate wind.
#[test]
fn lower_tiers_draw_the_short_waves_a_phones_boat_zoom_resolves() {
    let (width, height) = (390, 844);
    let view_height_m = 29.4;
    let share_within_a_metre = |camera_m: [f64; 2], quality: &str| {
        let diagnostic = format!("water_visual_normals_{quality}");
        let (camera_x, camera_y) = (camera_m[0].to_string(), camera_m[1].to_string());
        let rgba = capture_rgba(
            &format!("{diagnostic}-{camera_x}"),
            &[
                "--diagnostic",
                &diagnostic,
                "--width",
                &width.to_string(),
                "--height",
                &height.to_string(),
                "--view-height",
                &view_height_m.to_string(),
                "--camera-x",
                &camera_x,
                "--camera-y",
                &camera_y,
                "--world-time",
                &OPEN_OCEAN_WORLD_TIME_SECONDS.to_string(),
            ],
        );
        let slopes: Vec<[f64; 2]> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let normal =
                    [0, 1, 2].map(|channel| 2.0 * srgb_channel_to_linear(pixel[channel]) - 1.0);
                [-normal[0] / normal[2], -normal[1] / normal[2]]
            })
            .collect();
        // The mean squared difference of slopes `distance_m` apart down the
        // view.
        let structure = |distance_m: f64| {
            let rows = (distance_m / view_height_m * height as f64).round() as usize;
            mean((0..(height - rows) * width).map(|index| {
                let (near, far) = (slopes[index], slopes[index + rows * width]);
                (near[0] - far[0]).powi(2) + (near[1] - far[1]).powi(2)
            }))
        };
        structure(1.0) / structure(10.0)
    };
    for camera_m in [OPEN_OCEAN_M, [0.0, -20_000.0]] {
        let high = share_within_a_metre(camera_m, "high");
        for quality in ["medium", "low"] {
            let lower = share_within_a_metre(camera_m, quality);
            assert!(
                lower >= high,
                "at {camera_m:?}: share within a metre {high} high, {lower} {quality}",
            );
        }
    }
}

/// At a calm point of the surface wind the short-wave tiles keep their
/// Phillips share of Cox and Munk's total there, spread down from the tiles'
/// longest wavelength, so the high tier still resolves more slope than the
/// low.
#[test]
fn a_calm_point_keeps_the_waves_its_tier_resolves() {
    let (width, height) = (960, 600);
    let resolved_variance = |quality: &str| {
        let diagnostic = format!("lighting_water_resolved_slope_{quality}");
        let rgba = capture_rgba(
            &diagnostic,
            &[
                "--diagnostic",
                &diagnostic,
                "--width",
                &width.to_string(),
                "--height",
                &height.to_string(),
                "--camera-x",
                "-13898.8",
                "--camera-y",
                "6164.2",
                "--view-height",
                "400",
            ],
        );
        // The calm point lies at the view centre.
        let pixels = rgba.as_chunks::<4>().0;
        mean((height / 2 - 10..height / 2 + 10).flat_map(|row| {
            (width / 2 - 10..width / 2 + 10).map(move |column| {
                let encoded = srgb_channel_to_linear(pixels[row * width + column][0]);
                0.001 * encoded / (1.0 - encoded)
            })
        }))
    };
    let (high, low) = (resolved_variance("high"), resolved_variance("low"));
    assert!(high > low, "high tier {high}, low tier {low}");
}

/// A view wider than the world repeats it, and each copy is seen from where
/// it lies on screen: the direction to the camera turns steadily across the
/// view, through the line half a lap from the camera where copies meet.
#[test]
fn repeated_copies_of_the_world_face_the_camera_from_where_they_lie() {
    let (width, height) = (125, 90);
    let rgba = capture_rgba(
        "view-direction",
        &[
            "--diagnostic",
            "lighting_view_direction",
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
            // 157 km wide on the default 64 km world.
            "--view-height",
            "113000",
            "--camera-height",
            "10000",
        ],
    );
    let row = &rgba.as_chunks::<4>().0[height / 2 * width..(height / 2 + 1) * width];
    for (column, pair) in row.windows(2).enumerate() {
        assert!(pair[1][0] <= pair[0][0], "turns back after column {column}");
    }
}

/// Water's body by `lighting_water_body_*`, per unit of the Sun's light on
/// level ground, in linear sRGB: a row over a black bed, then one over a grey
/// bed of albedo 0.12, each 0, 1, 2, 5, 20 and 10 000 m deep.
fn water_body(sun_degrees: u32, view_degrees: u32, sediment: &str) -> [[[f64; 3]; 6]; 2] {
    let diagnostic = format!("lighting_water_body_{sun_degrees}_{view_degrees}_{sediment}");
    let rgba = capture_rgba(
        &diagnostic,
        &["--diagnostic", &diagnostic, "--width", "6", "--height", "2"],
    );
    let pixels = rgba.as_chunks::<4>().0;
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            [0, 1, 2].map(|channel| {
                let encoded = srgb_channel_to_linear(pixels[row * 6 + column][channel]);
                0.05 * encoded / (1.0 - encoded)
            })
        })
    })
}

/// The columns of `water_body` 5 m and 10 000 m deep.
const FIVE_METRES: usize = 3;
const DEEP: usize = 5;

/// CIELAB of a linear sRGB colour against white.
fn lab(rgb: [f64; 3]) -> [f64; 3] {
    let xyz = [
        [0.412_456_4, 0.357_576_1, 0.180_437_5],
        [0.212_672_9, 0.715_152_2, 0.072_175_0],
        [0.019_333_9, 0.119_192_0, 0.950_304_1],
    ]
    .map(|row| {
        (0..3)
            .map(|channel| row[channel] * rgb[channel])
            .sum::<f64>()
    });
    let white = [0.950_47, 1.0, 1.088_83];
    let [x, y, z] = [0, 1, 2].map(|axis| {
        let t = xyz[axis] / white[axis];
        if t > 216.0 / 24_389.0 {
            t.cbrt()
        } else {
            (24_389.0 / 27.0 * t + 16.0) / 116.0
        }
    });
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

fn delta_e(left: [f64; 3], right: [f64; 3]) -> f64 {
    let (left, right) = (lab(left), lab(right));
    (0..3)
        .map(|axis| (left[axis] - right[axis]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// From a bare bed to deep water, clear and turbid, water's body matches a
/// 1 nm integration of the same optics within the eight bands' ΔE*ab 2.1 of
/// white. The references integrate Maritorena, Morel and Gentili's shallow
/// water on Gordon's deep reflectance, through Lee's relation above water,
/// with Pope and Fry's and Smith and Baker's absorption plus the faked
/// background, Morel's seawater backscattering plus Morel and Maritorena's
/// particle backscattering at 0.1 mg/m³ of chlorophyll, and Babin's sediment,
/// over the CIE 1931 colour-matching functions under a 5778 K blackbody, with
/// the Sun and view at the zenith; negative channels clip at zero.
#[test]
fn water_body_follows_its_spectral_optics_from_bare_bed_to_deep_water() {
    let clear = water_body(90, 0, "0");
    let turbid = water_body(90, 0, "5");
    for (label, rendered, reference) in [
        (
            "grey bed bare",
            clear[1][0],
            [0.068_485, 0.068_485, 0.068_485],
        ),
        (
            "grey bed 2 m deep",
            clear[1][2],
            [0.019_057, 0.055_295, 0.064_507],
        ),
        ("deep", clear[0][DEEP], [0.0, 0.004_919, 0.019_842]),
        (
            "grey bed 5 m deep, 5 g/m³",
            turbid[1][FIVE_METRES],
            [0.024_804, 0.067_609, 0.043_265],
        ),
        (
            "deep, 5 g/m³",
            turbid[0][DEEP],
            [0.023_175, 0.066_857, 0.041_675],
        ),
    ] {
        let difference = delta_e(rendered, reference);
        assert!(
            difference <= 2.1,
            "{label}: rendered {rendered:?}, reference {reference:?}, ΔE {difference}",
        );
    }
}

/// Fresnel transmittance into level water at `degrees` off the vertical.
fn transmittance(degrees: f64) -> f64 {
    1.0 - fresnel_reflectance(degrees.to_radians().cos())
}

/// Deep water hides its bed, so its body differs only by the Fresnel
/// transmittances its light passes: the view's out of the water, and the
/// Sun's into it. The 3% bound is twice the diagnostic's eight-bit
/// resolution of each value.
#[test]
fn deep_water_passes_light_by_fresnel_transmittance_both_ways() {
    let blue = |sun_degrees, view_degrees| water_body(sun_degrees, view_degrees, "0")[0][DEEP][2];
    let overhead = blue(90, 0);
    for (label, rendered, expected) in [
        (
            "view 80° off the vertical",
            blue(90, 80) / overhead,
            transmittance(80.0) / transmittance(0.0),
        ),
        (
            "Sun 20° high",
            blue(20, 0) / overhead,
            transmittance(70.0) / transmittance(0.0),
        ),
    ] {
        assert!(
            (rendered / expected - 1.0).abs() < 0.03,
            "{label}: rendered {rendered}, Fresnel {expected}",
        );
    }
}

/// The luminance a grey bed adds to water's body at 5 m.
fn bed_share(body: &[[[f64; 3]; 6]; 2]) -> f64 {
    let luminance = |rgb: [f64; 3]| 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    luminance(body[1][FIVE_METRES]) - luminance(body[0][FIVE_METRES])
}

/// A low Sun's light refracts along a longer path down to the bed, so the bed
/// adds less to the body even per unit of the light that enters: about 9%
/// less under a Sun 20° high than 70° high, here beyond the diagnostic's
/// eight-bit resolution of the bed's share, under 3%.
#[test]
fn a_low_sun_shows_less_of_the_bed() {
    let entering_bed_share = |sun_degrees: u32| {
        bed_share(&water_body(sun_degrees, 0, "0")) / transmittance(f64::from(90 - sun_degrees))
    };
    let (low, high) = (entering_bed_share(20), entering_bed_share(70));
    assert!(
        low < 0.95 * high,
        "bed share per unit of entering light under a Sun 20° high {low}, 70° high {high}",
    );
}

/// Sediment veils the bed.
#[test]
fn sediment_veils_the_bed() {
    let clear = bed_share(&water_body(90, 0, "0"));
    let turbid = bed_share(&water_body(90, 0, "5"));
    assert!(
        turbid < clear,
        "bed share clear {clear}, with 5 g/m³ {turbid}"
    );
}

/// At the game's 15 m boat zoom every short-wave tile layer a footprint of
/// 1.4 cm resolves draws, and the unresolved facets carry what those and the
/// wave modes leave of Cox and Munk's slopes, so mean glint over views that
/// together hold many of the longest tiles' waves follows their closed form.
/// The Sun stands high enough that each view holds its glint's core.
#[test]
fn boat_zoom_with_every_short_wave_layer_keeps_cox_and_munks_mean_glint() {
    let (mut rendered, mut expected) = (0.0, 0.0);
    for step in 0..8 {
        let view = GlintView {
            width: 1920,
            height: 1080,
            view_height_m: 15.0,
            camera_height_m: f64::from(DroneCamera::PRODUCTION_HEIGHT_M),
            sun_degrees: 89.6,
            camera_m: [
                OPEN_OCEAN_M[0] + 97.0 * f64::from(step),
                OPEN_OCEAN_M[1] + 61.0 * f64::from(step),
            ],
            world_time_seconds: OPEN_OCEAN_WORLD_TIME_SECONDS,
        };
        let pixels = view.capture("high");
        rendered += pixels.iter().map(|pixel| pixel.glint).sum::<f64>();
        expected += cox_munk_glints(&view, &pixels).sum::<f64>();
    }
    assert!(
        (rendered / expected - 1.0).abs() < 0.05,
        "summed glint {rendered}, Cox and Munk {expected}",
    );
}

/// The water's slopes at a point are the same seen from either side of the
/// world's longitude seam: on an 8 km world, views 10 m apart across the
/// seam agree where they overlap as closely as views 10 m apart elsewhere,
/// whose centres differ alike.
#[test]
fn water_slopes_join_across_the_longitude_seam() {
    let (width, height) = (960, 540);
    let shift = 360;
    let normals = |camera_x: f64| {
        capture_rgba(
            &format!("seam-{camera_x}"),
            &[
                "--diagnostic",
                "water_visual_normals",
                "--width",
                &width.to_string(),
                "--height",
                &height.to_string(),
                "--world-width",
                "8000",
                "--camera-x",
                &camera_x.to_string(),
                "--camera-y",
                "0",
                // 360 pixels span 10 m.
                "--view-height",
                "15",
            ],
        )
    };
    let overlap_difference = |left: &[u8], right: &[u8]| {
        let (left, right) = (left.as_chunks::<4>().0, right.as_chunks::<4>().0);
        mean((0..height).flat_map(|row| {
            (0..width - shift).flat_map(move |column| {
                let (a, b) = (
                    left[row * width + column + shift],
                    right[row * width + column],
                );
                (0..3).map(move |channel| f64::from(a[channel].abs_diff(b[channel])))
            })
        }))
    };
    let across = overlap_difference(&normals(3_995.0), &normals(-3_995.0));
    let within = overlap_difference(&normals(1_995.0), &normals(2_005.0));
    assert!(
        across <= 1.5 * within + 0.5,
        "mean channel difference {across} across the seam, {within} elsewhere",
    );
}
