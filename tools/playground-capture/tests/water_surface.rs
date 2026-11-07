//! Water reflects each beam by Fresnel's law from facets whose slopes follow
//! Cox and Munk's statistics, seen from the drone camera, and reflects the
//! clear sky by a radiance that carries its diffuse light.

use std::{
    f64::consts::PI,
    fs,
    io::BufReader,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use browser::DroneCamera;

/// Open ocean, the bench's open-ocean view centre, at the capture's default
/// world time.
const OPEN_OCEAN_M: [f64; 2] = [-14_900.0, -5_330.0];
const OPEN_OCEAN_WORLD_TIME_SECONDS: f64 = 37.0;
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

    /// The unit vector from a pixel's sea-level point to the camera.
    fn view_direction(&self, column: usize, row: usize) -> [f64; 3] {
        let (width, height) = (self.width as f64, self.height as f64);
        let offset_m = [
            (column as f64 + 0.5 - 0.5 * width) / height * self.view_height_m,
            (0.5 * height - row as f64 - 0.5) / height * self.view_height_m,
        ];
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

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0), |(sum, count), value| (sum + value, count + 1));
    sum / f64::from(count)
}

/// Beyond 500 m pixels no wave mode or decorative octave is resolved, so
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

/// At boat zoom the wave modes and decorative octaves resolve part of the
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

/// A lower tier resolves fewer decorative octaves and leaves their slopes to
/// the unresolved distribution, so its mean glint matches the high tier's: on
/// open ocean at 4 m/s, and in a 29 m/s storm whose wave modes leave the
/// decorative octaves none of Cox and Munk's total beyond the short waves'.
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

/// At a calm point of the surface wind the decorative octaves keep their
/// Phillips share of Cox and Munk's total there, spread down from the octaves'
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
