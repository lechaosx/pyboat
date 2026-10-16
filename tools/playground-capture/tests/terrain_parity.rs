use std::{fs, io::BufReader, process::Command};

use game::{
    broad_landform_elevation_m, celestial_disc_visibility_at_height, celestial_lighting,
    celestial_lighting_at_height, procedural_solid_surface_elevation_m,
    procedural_terrain_elevation_m, procedural_world_environment, world_celestial_conditions,
};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed;

#[test]
fn physical_waves_advance_with_fractional_world_ticks() {
    let world = WorldGeometry::default();
    let position_m = (0..64)
        .flat_map(|y_index| {
            (0..64).map(move |x_index| {
                Vector::new(
                    world.width_m() * (x_index as f32 / 64.0 - 0.5),
                    0.5 * world.height_m() * (y_index as f32 / 64.0 - 0.5),
                )
            })
        })
        .find(|position_m| procedural_terrain_elevation_m(world, SEED, *position_m) < -20.0)
        .unwrap();
    let before = render_diagnostic_image_with_view_at(
        world,
        position_m,
        12_345.0,
        80.0,
        "water_physical_wave_normals",
        32,
        32,
    );
    let after = render_diagnostic_image_with_view_at(
        world,
        position_m,
        12_345.0 + 0.75 / 60.0,
        80.0,
        "water_physical_wave_normals",
        32,
        32,
    );

    assert_ne!(before, after);
}

#[test]
fn direct_sun_and_moon_light_extinguish_at_the_geometric_horizon() {
    let world = WorldGeometry::default();
    let position_m = (-16..=16)
        .map(|index| Vector::new(index as f32 * 500.0, 0.0))
        .find(|position| procedural_terrain_elevation_m(world, SEED, *position) < -20.0)
        .unwrap();
    for source in 0..2 {
        let light_at =
            |time| celestial_lighting(world_celestial_conditions(world, time, position_m));
        let elevation = |light: game::CelestialLighting| {
            if source == 0 {
                light.sun_direction[2]
            } else {
                light.moon_direction[2]
            }
        };
        let (mut below, mut above) = (0..28 * 24)
            .map(|hour| (hour as f64 * 3_600.0, (hour + 1) as f64 * 3_600.0))
            .find(|(start, end)| {
                let end_light = light_at(*end);
                elevation(light_at(*start)) < 0.0
                    && elevation(end_light) > 0.1
                    // Daylight exposure can leave moonlight below one 8-bit level.
                    && (source == 0
                        || (end_light.moon_intensity > 0.02 && end_light.sun_direction[2] < 0.0))
            })
            .unwrap();
        let visible_time = above;
        for _ in 0..40 {
            let midpoint = (below + above) / 2.0;
            if elevation(light_at(midpoint)) < 0.0 {
                below = midpoint;
            } else {
                above = midpoint;
            }
        }
        let gates = if source == 0 { "1,0,0,0" } else { "0,1,0,0" };
        let controls = [
            ("--light-contributions", gates),
            ("--cloud-shadows", "0,0,0,0"),
            ("--layer-material-colors", "0,0,0,0"),
            ("--landform-lighting", "0,0,320,0"),
        ];
        let visible = render_controlled_image_at(
            world,
            position_m,
            visible_time,
            1.0,
            "scene",
            1,
            1,
            &controls,
        );
        assert!(
            visible[..3].iter().any(|channel| *channel > 0),
            "source {source} did not illuminate the surface"
        );
        let grazing = render_controlled_image_at(
            world,
            position_m,
            (below + above) / 2.0,
            1.0,
            "scene",
            1,
            1,
            &controls,
        );
        assert_eq!(&grazing[..3], &[0, 0, 0], "source {source} at the horizon");
    }
}

#[test]
fn adapted_view_vision_reduces_chroma_everywhere() {
    let world = WorldGeometry::default();
    let mut images = Vec::new();
    for position in [Vector::ZERO, Vector::new(0.5 * world.width_m(), 0.0)] {
        for diagnostic in [
            "lighting_night_vision_unadapted",
            "lighting_night_vision_adapted",
        ] {
            images.push(render_diagnostic_image_with_view_at(
                world, position, 0.0, 1.0, diagnostic, 1, 1,
            ));
        }
    }
    assert_eq!(images[0], images[2]);
    assert_eq!(images[1], images[3]);
    let chroma = |color: &[u8]| {
        let channels: Vec<_> = color[..3]
            .iter()
            .copied()
            .map(srgb_channel_to_linear)
            .collect();
        channels.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - channels.iter().copied().fold(f32::INFINITY, f32::min)
    };
    assert!(chroma(&images[3]) < chroma(&images[2]));
}

#[test]
fn night_vision_transitions_from_color_to_rod_sensitivity() {
    let world = WorldGeometry::default();
    let render = |name: &str| {
        render_diagnostic_image_with_view_at(world, Vector::ZERO, 0.0, 1.0, name, 1, 1)
    };
    for material in ["gray", "white"] {
        let cones = render(&format!("vision_{material}_cones"));
        for vision in ["mesopic", "rods"] {
            assert_eq!(cones, render(&format!("vision_{material}_{vision}")));
        }
    }
    let red = render("vision_red_cones");
    assert_eq!(&red[1..3], &[0, 0]);
    assert!((srgb_channel_to_linear(red[0]) - 0.5).abs() < 0.005);
    let blue = render("vision_blue_cones");
    assert_eq!(&blue[..2], &[0, 0]);
    assert!((srgb_channel_to_linear(blue[2]) - 0.5).abs() < 0.005);
    for material in ["red", "blue"] {
        let cones = render(&format!("vision_{material}_cones"));
        let mesopic = render(&format!("vision_{material}_mesopic"));
        let rods = render(&format!("vision_{material}_rods"));
        let spread =
            |pixel: &[u8]| pixel[..3].iter().max().unwrap() - pixel[..3].iter().min().unwrap();
        assert!(spread(&cones) > spread(&mesopic));
        assert!(spread(&mesopic) > spread(&rods));
        assert_eq!(rods[0], rods[1]);
        assert_eq!(rods[1], rods[2]);
    }
    assert!(
        linear_luminance(&render("vision_blue_rods"))
            > linear_luminance(&render("vision_red_rods"))
    );
    assert!(linear_luminance(&render("vision_red_rods")) < linear_luminance(&red));
}

#[test]
fn night_vision_noise_is_unbiased_bounded_and_continuous() {
    let world = WorldGeometry::default();
    let render = |name: &str| {
        render_diagnostic_image_with_view_at(world, Vector::ZERO, 0.0, 1.0, name, 128, 128)
    };
    let day = render("vision_noise_day");
    let dark = render("vision_noise_dark");
    let bright = render("vision_noise_bright");
    let later = render("vision_noise_later");
    assert!(
        day.as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel == &day[..4])
    );
    assert!(dark != day, "night signal has no variation");
    assert_eq!(dark, render("vision_noise_dark"));
    assert!(dark != later, "noise does not evolve");
    assert_eq!(dark, render("vision_noise_period"));
    for image in [&dark, &bright, &later] {
        let values: Vec<_> = image
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                assert_eq!(pixel[0], pixel[1]);
                assert_eq!(pixel[1], pixel[2]);
                let value = srgb_channel_to_linear(pixel[0]);
                assert!((value - 0.5).abs() <= 0.065);
                value
            })
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!((mean - 0.5).abs() < 0.005, "biased noise: {mean}");
    }
    let variance = |image: &[u8]| {
        image
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| (srgb_channel_to_linear(pixel[0]) - 0.5).powi(2))
            .sum::<f32>()
            / (image.len() / 4) as f32
    };
    assert!(variance(&dark) > variance(&bright));
    let before_wrap = render("vision_noise_before_wrap");
    let after_wrap = render("vision_noise_after_wrap");
    assert!(
        before_wrap
            .iter()
            .zip(&after_wrap)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    assert!(
        render("vision_noise_black")
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3] == [0, 0, 0])
    );
}

#[test]
fn nighttime_eye_adaptation_brightens_daylight_elsewhere_in_the_view() {
    let world = WorldGeometry::default();
    let size = 64usize;
    // Cloud parallax is camera-relative; compare ground lighting independently of cloud bodies.
    let controls = [("--cloud-rendering", "0,0,0,0")];
    let day_center = render_controlled_image_at(
        world,
        Vector::ZERO,
        0.0,
        world.height_m(),
        "scene",
        size as u32,
        size as u32,
        &controls,
    );
    let night_center = render_controlled_image_at(
        world,
        Vector::new(0.5 * world.width_m(), 0.0),
        0.0,
        world.height_m(),
        "scene",
        size as u32,
        size as u32,
        &controls,
    );
    let mut compared_day_pixels = 0;
    let mut brighter_night_pixels = 0;
    let mut day_luminance_sum = 0.0;
    let mut night_adapted_day_luminance_sum = 0.0;
    let mut day_chroma_sum = 0.0;
    let mut night_adapted_day_chroma_sum = 0.0;
    let relative_chroma = |color: &[u8]| {
        let channels: Vec<_> = color[..3]
            .iter()
            .copied()
            .map(srgb_channel_to_linear)
            .collect();
        let maximum = channels.iter().copied().fold(0.0, f32::max);
        let minimum = channels.iter().copied().fold(1.0, f32::min);
        if maximum == 0.0 {
            0.0
        } else {
            (maximum - minimum) / maximum
        }
    };
    for row in 0..size {
        for column in 0..size {
            let position = Vector::new(
                world.width_m() * ((column as f32 + 0.5) / size as f32 - 0.5),
                world.height_m() * (0.5 - (row as f32 + 0.5) / size as f32),
            );
            let light = celestial_lighting(world_celestial_conditions(world, 0.0, position));
            let day_offset = 4 * (row * size + column);
            let night_offset = 4 * (row * size + (column + size / 2) % size);
            if light.sun_direction[2] > 0.2 {
                day_luminance_sum += linear_luminance(&day_center[day_offset..day_offset + 4]);
                night_adapted_day_luminance_sum +=
                    linear_luminance(&night_center[night_offset..night_offset + 4]);
                day_chroma_sum += relative_chroma(&day_center[day_offset..day_offset + 4]);
                night_adapted_day_chroma_sum +=
                    relative_chroma(&night_center[night_offset..night_offset + 4]);
                compared_day_pixels += 1;
            } else if light.sun_direction[2] < -0.3 {
                brighter_night_pixels += usize::from((0..3).any(|channel| {
                    night_center[night_offset + channel] > day_center[day_offset + channel]
                }));
            }
        }
    }
    assert!(compared_day_pixels > 0);
    assert!(
        night_adapted_day_luminance_sum - day_luminance_sum > compared_day_pixels as f32 / 255.0
    );
    assert!(night_adapted_day_chroma_sum < day_chroma_sum);
    assert!(brighter_night_pixels > 0);
}

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
    for position_m in [
        Vector::new(1_234.5, -6_789.25),
        Vector::new(1_234.5, 0.75 * world.height_m()),
    ] {
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
}

#[test]
fn rendered_broad_landform_matches_authoritative_rust() {
    let world = WorldGeometry::default();
    let mut compared = 0;
    for index in 0..400 {
        let position_m = Vector::new(
            (index % 20) as f32 * 1_610.0 - 16_000.0,
            (index / 20) as f32 * 1_630.0 - 16_000.0,
        );
        let expected_m = broad_landform_elevation_m(world, SEED, position_m);
        // The diagnostic resolves about 4 cm within 8 m of sea level.
        if !(-7.0..=7.0).contains(&expected_m) {
            continue;
        }
        let expected = signed_debug_pixel(expected_m / 8.0);
        let actual = render_diagnostic_pixel(world, position_m, "terrain_broad_landform");
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 2,
                "broad landform at {position_m:?}: {actual:?} != {expected:?} ({expected_m} m)",
            );
        }
        compared += 1;
    }
    assert!(
        compared >= 4,
        "only {compared} positions were near sea level"
    );
}

#[test]
fn rendered_instability_matches_authoritative_rust_upper_air() {
    let world = WorldGeometry::default();
    let mut compared = 0;
    for index in 0..64 {
        let position_m = Vector::new(
            (index % 8) as f32 * 3_700.0 - 13_000.0,
            (index / 8) as f32 * 3_900.0 - 15_000.0,
        );
        let expected = procedural_world_environment(world, SEED, 0, position_m)
            .weather
            .instability;
        // Saturated instability hides the 500 hPa temperature it derives from.
        if !(0.1..=0.9).contains(&expected) {
            continue;
        }
        let actual = scalar_debug_amount(&render_diagnostic_pixel(
            world,
            position_m,
            "weather_instability",
        ));
        assert!(
            (actual - expected).abs() < 0.05,
            "instability at {position_m:?}: rendered {actual}, Rust {expected}",
        );
        compared += 1;
    }
    assert!(
        compared >= 4,
        "only {compared} positions had unsaturated instability"
    );
}

#[test]
fn rendered_surface_temperature_is_continuous_across_a_pole() {
    let world = WorldGeometry::default();
    let pole_y_m = 0.5 * world.height_m();
    let before = render_diagnostic_pixel(
        world,
        Vector::new(0.0, pole_y_m - 0.000001 * world.height_m()),
        "surface_air_temperature",
    );
    let after = render_diagnostic_pixel(
        world,
        Vector::new(0.0, pole_y_m + 0.000001 * world.height_m()),
        "surface_air_temperature",
    );

    for channel in 0..3 {
        assert!(before[channel].abs_diff(after[channel]) <= 1);
    }
}

#[test]
fn rendered_solar_eclipse_matches_authoritative_celestial_geometry() {
    let world = WorldGeometry::default();
    let world_time_seconds = 105.0 * 86_400.0;
    let position_m = Vector::new(9_599.998, -23_822.223);
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
fn rendered_surface_sun_amount_uses_receiver_elevation() {
    let world = WorldGeometry::default();
    let position_m = (0..64)
        .flat_map(|y_index| {
            (0..64).map(move |x_index| {
                Vector::new(
                    world.width_m() * (x_index as f32 / 64.0 - 0.5),
                    world.height_m() * (y_index as f32 / 128.0 - 0.25),
                )
            })
        })
        .max_by(|left, right| {
            procedural_terrain_elevation_m(world, SEED, *left)
                .total_cmp(&procedural_terrain_elevation_m(world, SEED, *right))
        })
        .unwrap();
    let elevation_m = procedural_terrain_elevation_m(world, SEED, position_m);
    assert!(elevation_m > 0.0);
    let (world_time_seconds, expected) = (0..24 * 60)
        .map(|minute| minute as f64 * 60.0)
        .find_map(|world_time_seconds| {
            let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
            let datum = celestial_lighting(conditions);
            let elevated = celestial_lighting_at_height(world, conditions, elevation_m);
            (datum.sun_intensity == 0.0 && elevated.sun_intensity > 0.10)
                .then_some((world_time_seconds, elevated.sun_intensity))
        })
        .expect("elevated terrain should see the Sun beyond the datum horizon");

    let actual = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_surface_sun_amount",
        1,
        1,
    );
    let expected = scalar_debug_pixel(expected);
    for channel in 0..3 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 4,
            "{actual:?} != {expected:?} at elevation {elevation_m}",
        );
    }
}

#[test]
fn rendered_cloud_layer_sun_visibility_uses_the_world_horizon() {
    let world = WorldGeometry::default();
    let world_time_seconds = 0.0;
    let position_m = (0..4_096)
        .map(|index| Vector::new(world.width_m() * (index as f32 / 4_096.0 - 0.5), 0.0))
        .find(|position_m| {
            let sun_z = celestial_lighting(world_celestial_conditions(
                world,
                world_time_seconds,
                *position_m,
            ))
            .sun_direction[2];
            (-0.205..=-0.195).contains(&sun_z)
        })
        .expect("the equator should cross the selected twilight elevation");
    let sun_z = celestial_lighting(world_celestial_conditions(
        world,
        world_time_seconds,
        position_m,
    ))
    .sun_direction[2];

    for (diagnostic, height_m) in [
        ("lighting_low_cloud_sun_visibility", 900.0),
        ("lighting_canopy_sun_visibility", 3_600.0),
    ] {
        let actual = render_diagnostic_image_with_view_at(
            world,
            position_m,
            world_time_seconds,
            1.0,
            diagnostic,
            1,
            1,
        );
        let expected = scalar_debug_pixel(celestial_disc_visibility_at_height(
            world, height_m, sun_z, 0.00465,
        ));
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 4,
                "{diagnostic}: {actual:?} != {expected:?} at sun z {sun_z}",
            );
        }
    }
}

#[test]
fn full_moon_canopy_scattering_tracks_canopy_coverage() {
    let world = WorldGeometry::default();
    let position_m = Vector::ZERO;
    let world_time_seconds = (0..28 * 24)
        .map(|hour| hour as f64 * 3_600.0)
        .max_by(|left, right| {
            let left = celestial_lighting(world_celestial_conditions(world, *left, position_m));
            let right = celestial_lighting(world_celestial_conditions(world, *right, position_m));
            left.moon_intensity.total_cmp(&right.moon_intensity)
        })
        .unwrap();
    let width = 64;
    let height = 32;
    let coverage = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "weather_sky_fraction",
        width,
        height,
    );
    let scattering = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "lighting_canopy_moon_scattering",
        width,
        height,
    );
    let mut clear_pixels = 0;
    let mut scattering_pixels = 0;
    for (coverage, scattering) in coverage
        .as_chunks::<4>()
        .0
        .iter()
        .zip(scattering.as_chunks::<4>().0.iter())
    {
        let coverage = scalar_debug_amount(coverage);
        let scattering = scalar_debug_amount(scattering);
        if coverage <= 0.002 {
            clear_pixels += 1;
            assert!(scattering <= 0.002, "clear canopy scattered {scattering}");
        }
        scattering_pixels += usize::from(scattering > 0.01);
    }
    assert!(clear_pixels > 0);
    assert!(scattering_pixels > 0);
}

#[test]
fn canopy_transfer_preserves_clear_sky_at_zero_thickness_and_attenuates_thick_canopy() {
    let world = WorldGeometry::default();
    let position_m = Vector::ZERO;
    let world_time_seconds = (0..24 * 60)
        .map(|minute| minute as f64 * 60.0)
        .max_by(|left, right| {
            let left = celestial_lighting(world_celestial_conditions(world, *left, position_m));
            let right = celestial_lighting(world_celestial_conditions(world, *right, position_m));
            left.sun_intensity.total_cmp(&right.sun_intensity)
        })
        .unwrap();
    let clear = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "lighting_canopy_transfer_clear",
        1,
        1,
    );
    let zero = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "lighting_canopy_transfer_zero",
        1,
        1,
    );
    assert_rgb_close(&clear, &zero, 1);

    let thick = render_diagnostic_image_at(
        world,
        position_m,
        world_time_seconds,
        "lighting_canopy_transfer_thick",
        1,
        1,
    );
    assert!(
        linear_luminance(&thick) < linear_luminance(&zero),
        "zero {:?}, thick {:?}",
        &zero[..3],
        &thick[..3],
    );
}

#[test]
fn scene_light_contribution_gates_compose_independently() {
    let world = WorldGeometry::default();
    let canopy = render_diagnostic_image(world, "lighting_ambient_sky_occlusion", 64, 64);
    let mixed_canopy_pixel = canopy
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| (0.3..0.7).contains(&scalar_debug_amount(*pixel)))
        .min_by_key(|(pixel_index, _)| (pixel_index / 64).abs_diff(32))
        .map(|(pixel_index, _)| pixel_index)
        .expect("the world should contain mixed canopy conditions");
    let row = mixed_canopy_pixel / 64;
    let column = mixed_canopy_pixel % 64;
    let position_m = Vector::new(
        world.width_m() * ((column as f32 + 0.5) / 64.0 - 0.5),
        world.height_m() * (0.5 - (row as f32 + 0.5) / 64.0),
    );
    let world_time_seconds = (0..28 * 24)
        .map(|hour| hour as f64 * 3_600.0)
        .filter_map(|time| {
            let light = celestial_lighting(world_celestial_conditions(world, time, position_m));
            (light.sun_direction[2] > 0.1
                && light.moon_direction[2] > 0.1
                && light.sun_intensity > 0.1
                && light.moon_intensity > 0.005)
                .then_some((time, light.sun_direction[2].min(light.moon_direction[2])))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .expect("Sun and Moon should share the sky during the lunar month")
        .0;
    let off = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        2_000.0,
        "scene",
        48,
        48,
        &[
            ("--light-balance", "1,1,1,1"),
            ("--light-contributions", "0,0,0,0"),
            ("--cloud-shadows", "0,0,0.85,0"),
        ],
    );
    assert!(
        off.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| &pixel[..3])
            .all(|channel| *channel <= 1),
        "all disabled: maximum channel {}",
        off.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| &pixel[..3])
            .copied()
            .max()
            .unwrap(),
    );

    let mut isolated = Vec::new();
    for contribution in 0..4 {
        let mut gates = [0; 4];
        gates[contribution] = 1;
        let gate_value = format!("{},{},{},{}", gates[0], gates[1], gates[2], gates[3]);
        let controls = [
            ("--light-balance", "1,1,1,1"),
            ("--light-contributions", gate_value.as_str()),
            ("--cloud-shadows", "0,0,0.85,0"),
        ];
        let image = render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            2_000.0,
            "scene",
            48,
            48,
            &controls,
        );
        let luminance = mean_linear_luminance(&image);
        assert!(
            luminance > 0.000_01,
            "contribution {contribution}: {luminance}"
        );
        isolated.push(image);
    }
    let all = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        2_000.0,
        "scene",
        48,
        48,
        &[
            ("--light-balance", "1,1,1,1"),
            ("--light-contributions", "1,1,1,1"),
            ("--cloud-shadows", "0,0,0.85,0"),
        ],
    );
    let mean_error = all
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .flat_map(|(pixel_index, pixel)| {
            (0..3).map({
                let isolated = &isolated;
                move |channel| {
                    let expected = isolated
                        .iter()
                        .map(|image| srgb_channel_to_linear(image[4 * pixel_index + channel]))
                        .sum::<f32>();
                    (srgb_channel_to_linear(pixel[channel]) - expected).abs()
                }
            })
        })
        .sum::<f32>()
        / (48.0 * 48.0 * 3.0);
    assert!(mean_error < 0.01, "composition error {mean_error}");
}

#[test]
fn canopy_shadow_control_disables_attenuation_on_lower_clouds() {
    let world = WorldGeometry::default();
    let disabled = render_controlled_image_at(
        world,
        Vector::ZERO,
        0.0,
        world.height_m(),
        "lighting_low_cloud_canopy_transmittance",
        128,
        64,
        &[("--cloud-shadows", "1,0,0.85,1")],
    );
    assert!(
        disabled
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| scalar_debug_amount(pixel) > 0.99),
    );
    let enabled = render_controlled_image_at(
        world,
        Vector::ZERO,
        0.0,
        world.height_m(),
        "lighting_low_cloud_canopy_transmittance",
        128,
        64,
        &[("--cloud-shadows", "1,1,0.85,1")],
    );
    assert!(
        enabled
            .as_chunks::<4>()
            .0
            .iter()
            .zip(disabled.as_chunks::<4>().0.iter())
            .any(|(enabled, disabled)| {
                scalar_debug_amount(enabled) + 0.02 < scalar_debug_amount(disabled)
            }),
        "enabled canopy shadows produced no lower-cloud attenuation",
    );
}

#[test]
fn low_cloud_sunlight_uses_receiver_eclipse_with_surface_horizon_visibility() {
    let world = WorldGeometry::default();
    let world_time_seconds = 105.0 * 86_400.0;
    let (position_m, datum, low_cloud) = (0..128)
        .flat_map(|x_index| {
            (0..64).map(move |y_index| {
                Vector::new(
                    world.width_m() * (x_index as f32 / 128.0 - 0.5),
                    world.height_m() * (y_index as f32 / 64.0 - 0.5),
                )
            })
        })
        .filter_map(|position_m| {
            let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
            let datum = celestial_lighting_at_height(world, conditions, 0.0);
            let low_cloud = celestial_lighting_at_height(world, conditions, 900.0);
            (datum.sun_intensity > 0.05
                && low_cloud.solar_eclipse_fraction > datum.solar_eclipse_fraction + 0.05)
                .then_some((position_m, datum, low_cloud))
        })
        .max_by(|left, right| {
            (left.2.solar_eclipse_fraction - left.1.solar_eclipse_fraction)
                .total_cmp(&(right.2.solar_eclipse_fraction - right.1.solar_eclipse_fraction))
        })
        .expect("the eclipse should differ between datum and low-cloud receivers");
    let datum_unoccluded = datum.sun_intensity / (1.0 - datum.solar_eclipse_fraction);
    let expected_surface = datum_unoccluded * (1.0 - low_cloud.solar_eclipse_fraction);
    let expected_elevated = (low_cloud.sun_intensity - expected_surface).max(0.0);
    let actual = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_low_cloud_sun_lighting",
        1,
        1,
    );
    for (channel, expected) in [expected_surface, low_cloud.sun_intensity, expected_elevated]
        .into_iter()
        .enumerate()
    {
        let actual = srgb_channel_to_linear(actual[channel]);
        assert!(
            (actual - expected).abs() < 0.015,
            "channel {channel}: {actual} != {expected}; datum {datum:?}, low {low_cloud:?}",
        );
    }
}

#[test]
fn ambient_uses_rendered_receiver_eclipse_and_direct_partition() {
    let world = WorldGeometry::default();
    let world_time_seconds = 105.0 * 86_400.0;
    let (position_m, datum, surface) = (0..128)
        .flat_map(|x_index| {
            (0..64).map(move |y_index| {
                Vector::new(
                    world.width_m() * (x_index as f32 / 128.0 - 0.5),
                    world.height_m() * (y_index as f32 / 64.0 - 0.5),
                )
            })
        })
        .filter_map(|position_m| {
            let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
            let datum = celestial_lighting_at_height(world, conditions, 0.0);
            let surface = celestial_lighting_at_height(world, conditions, 600.0);
            (datum.sun_direction[2] > 0.2
                && datum.sun_intensity > 0.05
                && surface.solar_eclipse_fraction > datum.solar_eclipse_fraction + 0.05)
                .then_some((position_m, datum, surface))
        })
        .max_by(|left, right| {
            (left.2.solar_eclipse_fraction - left.1.solar_eclipse_fraction)
                .total_cmp(&(right.2.solar_eclipse_fraction - right.1.solar_eclipse_fraction))
        })
        .expect("the eclipse should differ at an elevated surface receiver");
    let datum_ambient = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_receiver_ambient_datum",
        1,
        1,
    );
    let surface_ambient = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_receiver_ambient_elevated",
        1,
        1,
    );
    assert!(
        linear_luminance(&surface_ambient) + 0.01 < linear_luminance(&datum_ambient),
        "datum {:?}, surface {:?}; datum light {datum:?}, surface light {surface:?}",
        &datum_ambient[..3],
        &surface_ambient[..3],
    );

    let transmitted = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_receiver_atmospheric_transmittance_elevated",
        1,
        1,
    );
    let expected_share = (1.0
        - 0.38
            * surface.sun_intensity.clamp(0.0, 1.0).sqrt()
            * srgb_channel_to_linear(transmitted[0])
        - 0.16
            * surface.moon_intensity.clamp(0.0, 1.0).sqrt()
            * srgb_channel_to_linear(transmitted[1]))
    .max(0.0);
    let actual_share = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_receiver_ambient_share_elevated",
        1,
        1,
    );
    assert!(
        (scalar_debug_amount(&actual_share) - expected_share).abs() < 0.01,
        "ambient share {} != {expected_share}; datum {datum:?}, surface {surface:?}",
        scalar_debug_amount(&actual_share),
    );
}

#[test]
fn atmospheric_twilight_bands_do_not_shift_with_surface_horizon() {
    let world = WorldGeometry::default();
    let position_m = Vector::ZERO;
    let world_time_seconds = (0..24 * 12)
        .map(|step| step as f64 * 300.0)
        .find(|time| {
            let light = celestial_lighting(world_celestial_conditions(world, *time, position_m));
            (-0.12..-0.08).contains(&light.sun_direction[2])
        })
        .expect("the Sun should cross the lower twilight band each day");
    let datum = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_ambient_horizon_datum",
        1,
        1,
    );
    let elevated = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_ambient_horizon_elevated",
        1,
        1,
    );
    assert_rgb_close(&datum, &elevated, 1);
}

#[test]
fn daylight_amplitude_uses_receiver_normalized_solar_elevation() {
    let world = WorldGeometry::default();
    let position_m = Vector::ZERO;
    let world_time_seconds = (0..24 * 60)
        .map(|minute| minute as f64 * 60.0)
        .filter_map(|time| {
            let light = celestial_lighting(world_celestial_conditions(world, time, position_m));
            ((0.06..0.10).contains(&light.sun_direction[2]) && light.solar_eclipse_fraction == 0.0)
                .then_some((time, light.moon_intensity))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .expect("the Sun should cross the daylight transition each day")
        .0;
    let datum = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_ambient_horizon_datum",
        1,
        1,
    );
    let elevated = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_ambient_horizon_elevated",
        1,
        1,
    );
    assert!(
        linear_luminance(&elevated) > linear_luminance(&datum) + 0.02,
        "datum {:?}, elevated {:?}",
        &datum[..3],
        &elevated[..3],
    );
}

#[test]
fn landform_sun_grade_is_independent_of_moon_visibility() {
    let world = WorldGeometry::default();
    let position_m = Vector::ZERO;
    let world_time_seconds = (0..28 * 24)
        .map(|hour| hour as f64 * 3_600.0)
        .filter_map(|time| {
            let light = celestial_lighting(world_celestial_conditions(world, time, position_m));
            (light.sun_intensity > 0.1 && light.moon_intensity > 0.005)
                .then_some((time, light.sun_intensity.min(8.0 * light.moon_intensity)))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .expect("Sun and Moon should share the sky during the lunar month")
        .0;
    let coupling = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_landform_sun_grade_moon_coupling",
        1,
        1,
    );
    assert!(
        scalar_debug_amount(&coupling) < 0.002,
        "Moon changed the Sun terrain grade by {}",
        scalar_debug_amount(&coupling),
    );
}

#[test]
fn landform_shadows_use_source_specific_apparent_radii() {
    let world = WorldGeometry::default();
    let position_m = representative_shore(world);
    let image = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        2_000.0,
        "lighting_landform_shadow_disc_radii",
        256,
        256,
    );

    let baseline = scalar_debug_pixel(0.0);
    let maximum = image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .max()
        .unwrap();
    assert!(
        maximum > baseline[0].saturating_add(1),
        "Sun and Moon landform penumbrae were identical: {maximum} <= {}",
        baseline[0],
    );
}

#[test]
fn landform_shadows_are_continuous_across_the_local_tangent() {
    let world = WorldGeometry::default();
    let position_m = representative_shore(world);
    let image = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        2_000.0,
        "lighting_landform_shadow_tangent_continuity",
        256,
        256,
    );

    let maximum = image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| scalar_debug_amount(pixel))
        .fold(0.0, f32::max);
    assert!(maximum < 0.02, "shadow discontinuity was {maximum}");
}

#[test]
fn landform_penumbra_is_centered_on_half_occlusion() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        1.0,
        "lighting_landform_shadow_penumbra_center",
        1,
        1,
    );
    let amount = scalar_debug_amount(&image);
    assert!(
        (amount - 0.5).abs() < 0.02,
        "penumbra centre occlusion was {amount}",
    );
}

#[test]
fn partially_visible_disc_uses_a_ray_above_the_horizon() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        1.0,
        "lighting_landform_shadow_partial_disc_ray",
        1,
        1,
    );
    assert!(
        scalar_debug_amount(&image) > 0.98,
        "partial-disc shadow ray entered the planet",
    );
}

#[test]
fn projected_low_cloud_receives_light_at_its_world_position() {
    let world = WorldGeometry::default();
    let camera_m = Vector::ZERO;
    let ground_m = Vector::new(16_000.0, 0.0);
    let projected_m = Vector::new(14_800.0, 0.0);
    let (world_time_seconds, expected) = (0..96)
        .map(|quarter_hour| quarter_hour as f64 * 900.0)
        .map(|time| {
            let ground = celestial_lighting_at_height(
                world,
                world_celestial_conditions(world, time, ground_m),
                900.0,
            );
            let projected = celestial_lighting_at_height(
                world,
                world_celestial_conditions(world, time, projected_m),
                900.0,
            );
            (time, ground, projected)
        })
        .max_by(|left, right| {
            (left.1.sun_intensity - left.2.sun_intensity)
                .abs()
                .total_cmp(&(right.1.sun_intensity - right.2.sun_intensity).abs())
        })
        .map(|(time, ground, projected)| {
            assert!((ground.sun_intensity - projected.sun_intensity).abs() > 0.03);
            (time, projected.sun_intensity)
        })
        .unwrap();
    let image = render_diagnostic_image_with_view_at(
        world,
        camera_m,
        world_time_seconds,
        16_000.0,
        "lighting_projected_low_cloud_receiver",
        3,
        1,
    );
    let actual = srgb_channel_to_linear(image[4 * 2]);
    assert!((actual - expected).abs() < 0.015, "{actual} != {expected}");
}

#[test]
fn projected_low_cloud_direct_color_is_independent_of_background_position() {
    let world = WorldGeometry::default();
    let projected_position_m = Vector::new(14_800.0, 0.0);
    let world_time_seconds = 0.065 * 86_400.0;
    let offset_view = render_diagnostic_image_with_view_at(
        world,
        Vector::ZERO,
        world_time_seconds,
        16_000.0,
        "lighting_projected_low_cloud_direct_color",
        3,
        1,
    );
    let centered_view = render_diagnostic_image_with_view_at(
        world,
        projected_position_m,
        world_time_seconds,
        1.0,
        "lighting_projected_low_cloud_direct_color",
        1,
        1,
    );
    assert_rgb_close(&offset_view[8..], &centered_view, 2);
}

#[test]
fn below_datum_water_elevation_changes_cloud_projection() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        1.0,
        "lighting_below_datum_cloud_projection",
        1,
        1,
    );
    assert!(scalar_debug_amount(&image) > 0.98);
}

#[test]
fn resolvable_cloud_disc_samples_spread_across_both_axes() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        1.0,
        "lighting_cloud_disc_transverse_spread",
        1,
        1,
    );
    let clipped_to_full_spread = 2.0 * scalar_debug_amount(&image);
    assert!(
        clipped_to_full_spread > 0.8 && clipped_to_full_spread < 1.2,
        "clipped/full transverse spread ratio was {clipped_to_full_spread}",
    );
}

#[test]
fn cloud_shadow_softness_responds_to_celestial_disc_radius() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        8_000.0,
        "lighting_cloud_shadow_disc_radius",
        256,
        256,
    );
    let maximum = image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| scalar_debug_amount(pixel))
        .fold(0.0, f32::max);
    assert!(
        maximum > 0.02,
        "disc radius did not change cloud shadows: {maximum}"
    );
}

#[test]
fn low_cloud_canopy_shadow_softness_responds_to_celestial_disc_radius() {
    let image = render_diagnostic_image_with_view_at(
        WorldGeometry::default(),
        Vector::ZERO,
        0.0,
        8_000.0,
        "lighting_low_cloud_canopy_shadow_disc_radius",
        256,
        256,
    );
    let maximum = image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| scalar_debug_amount(pixel))
        .fold(0.0, f32::max);
    assert!(
        maximum > 0.02,
        "disc radius did not change canopy shadows on low clouds: {maximum}"
    );
}

#[test]
fn landform_cast_shadows_reach_open_water() {
    let world = WorldGeometry::default();
    let position_m = representative_shore(world);
    let width = 192;
    let height = 192;
    let elevation = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        2_000.0,
        "terrain_elevation",
        width,
        height,
    );
    let mut shadowed_water_pixels = 0;
    let mut low_sun_times = [None, None];
    for quarter_hour in 0..192 {
        let world_time_seconds = quarter_hour as f64 * 900.0;
        let light = celestial_lighting(world_celestial_conditions(
            world,
            world_time_seconds,
            position_m,
        ));
        if (0.0..0.7).contains(&light.sun_intensity) && light.sun_intensity > 0.0 {
            let direction_index = usize::from(light.sun_direction[0] >= 0.0);
            if low_sun_times[direction_index]
                .is_none_or(|(_, intensity)| light.sun_intensity < intensity)
            {
                low_sun_times[direction_index] = Some((world_time_seconds, light.sun_intensity));
            }
        }
    }
    assert!(low_sun_times.iter().all(Option::is_some));
    for (world_time_seconds, _) in low_sun_times.into_iter().flatten() {
        let controls = [
            ("--cloud-shadows", "0,0,0.85,0"),
            ("--layer-material-colors", "0,0,0,0"),
            ("--light-contributions", "1,0,0,0"),
        ];
        let unshadowed = render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            2_000.0,
            "scene",
            width,
            height,
            &[
                controls[0],
                controls[1],
                controls[2],
                ("--landform-lighting", "0,0,320,0"),
            ],
        );
        let shadowed = render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            2_000.0,
            "scene",
            width,
            height,
            &[
                controls[0],
                controls[1],
                controls[2],
                ("--landform-lighting", "0,1,320,0"),
            ],
        );
        for ((elevation, unshadowed), shadowed) in elevation
            .as_chunks::<4>()
            .0
            .iter()
            .zip(unshadowed.as_chunks::<4>().0.iter())
            .zip(shadowed.as_chunks::<4>().0.iter())
        {
            if elevation[2] > elevation[0].saturating_add(2) {
                shadowed_water_pixels +=
                    usize::from((0..3).any(|channel| unshadowed[channel] > shadowed[channel]));
            }
        }
    }
    assert!(
        shadowed_water_pixels > 0,
        "landform shadows did not darken any open-water pixels",
    );
}

#[test]
fn cloud_projection_stays_continuous_across_mirrored_latitude_boundaries() {
    let world = WorldGeometry::default();
    let projected_offset = |y_m| {
        let pixel = render_diagnostic_image_with_view_at(
            world,
            Vector::new(0.0, y_m),
            0.0,
            1.0,
            "lighting_cloud_projection",
            1,
            1,
        );
        [
            srgb_channel_to_linear(pixel[0]) - 0.5,
            srgb_channel_to_linear(pixel[1]) - 0.5,
        ]
    };
    let before_pole = projected_offset(0.499 * world.height_m());
    let after_pole = projected_offset(0.501 * world.height_m());

    assert!(
        (before_pole[0] - after_pole[0]).abs() < 0.01
            && (before_pole[1] - after_pole[1]).abs() < 0.01,
        "projected offset jumped at the pole: {before_pole:?}, {after_pole:?}",
    );
}

#[test]
fn cloud_projection_stays_continuous_within_a_latitude_band() {
    let world = WorldGeometry::default();
    let projected_offset = |y_m| {
        let pixel = render_diagnostic_image_with_view_at(
            world,
            Vector::new(0.0, y_m),
            0.0,
            1.0,
            "lighting_cloud_projection",
            1,
            1,
        );
        [
            srgb_channel_to_linear(pixel[0]) - 0.5,
            srgb_channel_to_linear(pixel[1]) - 0.5,
        ]
    };
    let before_branch = projected_offset(20_830.0);
    let after_branch = projected_offset(20_850.0);

    assert!(
        (before_branch[0] - after_branch[0]).abs() < 0.01
            && (before_branch[1] - after_branch[1]).abs() < 0.01,
        "projected offset jumped within one latitude band: {before_branch:?}, {after_branch:?}",
    );
}

#[test]
fn cloud_projection_has_the_same_world_scale_in_each_direction() {
    let world = WorldGeometry::default();
    let difference = render_diagnostic_image_with_view_at(
        world,
        Vector::ZERO,
        0.0,
        1.0,
        "lighting_cloud_projection_isotropy",
        1,
        1,
    );

    assert!(
        scalar_debug_amount(&difference) < 0.01,
        "east/west and north/south projection scales differ: {:?}",
        &difference[..3],
    );
}

#[test]
fn cloud_shadow_diagnostics_use_receiver_local_light() {
    let world = WorldGeometry::default();
    let world_time_seconds = 105.0 * 86_400.0;
    let position_m = (0..128)
        .flat_map(|x_index| {
            (0..64).map(move |y_index| {
                Vector::new(
                    world.width_m() * (x_index as f32 / 128.0 - 0.5),
                    world.height_m() * (y_index as f32 / 64.0 - 0.5),
                )
            })
        })
        .filter_map(|position_m| {
            let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
            let datum = celestial_lighting_at_height(world, conditions, 0.0);
            let elevated = celestial_lighting_at_height(world, conditions, 600.0);
            let difference = (elevated.solar_eclipse_fraction - datum.solar_eclipse_fraction).abs();
            (datum.sun_direction[2] > 0.2 && datum.sun_intensity > 0.05 && difference > 0.05)
                .then_some((position_m, difference))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .expect("the eclipse should differ at an elevated surface receiver")
        .0;
    let difference = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "lighting_cloud_shadow_receiver",
        1,
        1,
    );

    assert!(
        scalar_debug_amount(&difference) < 0.01,
        "shadow diagnostic used different projection and weighting receivers: {:?}",
        &difference[..3],
    );
}

#[test]
fn world_light_direction_stays_continuous_across_the_pole() {
    let world = WorldGeometry::default();
    let sample = |y_m| {
        let pixel = render_diagnostic_image_with_view_at(
            world,
            Vector::new(0.0, y_m),
            0.0,
            1.0,
            "lighting_world_direction",
            1,
            1,
        );
        [
            srgb_channel_to_linear(pixel[0]),
            srgb_channel_to_linear(pixel[1]),
            srgb_channel_to_linear(pixel[2]),
        ]
    };
    let before_pole = sample(0.499 * world.height_m());
    let after_pole = sample(0.501 * world.height_m());

    for channel in 0..3 {
        assert!(
            (before_pole[channel] - after_pole[channel]).abs() < 0.01,
            "world light direction jumped at the pole: {before_pole:?}, {after_pole:?}",
        );
    }
}

#[test]
fn rendered_lighting_uses_the_connected_hemisphere_after_a_pole() {
    let world = WorldGeometry::default();
    let world_time_seconds = (0.25 - 0.0021) * 365.0 * 86_400.0;
    let ascending_y_m = 0.25 * world.height_m();
    let descending_y_m = 0.75 * world.height_m();
    let longitude_m = (0..360)
        .map(|index| world.width_m() * (index as f32 / 360.0 - 0.5))
        .max_by(|left, right| {
            let sun_z = |longitude_m| {
                celestial_lighting(world_celestial_conditions(
                    world,
                    world_time_seconds,
                    Vector::new(longitude_m, ascending_y_m),
                ))
                .sun_direction[2]
            };
            sun_z(*left).total_cmp(&sun_z(*right))
        })
        .unwrap();
    let sample_z = |y_m| {
        let pixel = render_diagnostic_image_with_view_at(
            world,
            Vector::new(longitude_m, y_m),
            world_time_seconds,
            1.0,
            "lighting_world_direction",
            1,
            1,
        );
        2.0 * srgb_channel_to_linear(pixel[2]) - 1.0
    };

    assert!(sample_z(ascending_y_m) > 0.6);
    assert!(sample_z(descending_y_m) < -0.6);
}

#[test]
fn receiver_height_moon_geometry_matches_authoritative_rust() {
    let world = WorldGeometry::default();
    let world_time_seconds = 105.0 * 86_400.0;
    let position_m = Vector::new(9_599.998, -23_822.223);
    let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
    let mut expected_directions = Vec::new();
    let mut expected_eclipses = Vec::new();
    for (diagnostic, height_m) in [
        ("lighting_receiver_moon_datum", 0.0),
        ("lighting_receiver_moon_low_cloud", 900.0),
        ("lighting_receiver_moon_canopy", 3_600.0),
    ] {
        let expected = celestial_lighting_at_height(world, conditions, height_m);
        let actual = render_diagnostic_image_with_view_at(
            world,
            position_m,
            world_time_seconds,
            1.0,
            diagnostic,
            1,
            1,
        );
        for channel in 0..3 {
            let expected_channel =
                linear_channel_byte(0.5 + 0.5 * expected.moon_direction[channel]);
            assert!(
                actual[channel].abs_diff(expected_channel) <= 3,
                "{diagnostic} direction: {actual:?} != {:?}",
                expected.moon_direction,
            );
        }
        let expected_eclipse = (255.0 * expected.solar_eclipse_fraction).round() as u8;
        assert!(
            actual[3].abs_diff(expected_eclipse) <= 2,
            "{diagnostic} eclipse: {} != {}",
            actual[3],
            expected.solar_eclipse_fraction,
        );
        expected_directions.push(expected.moon_direction);
        expected_eclipses.push(expected.solar_eclipse_fraction);
    }
    assert!(
        expected_directions[0]
            .iter()
            .zip(expected_directions[2])
            .any(|(datum, canopy)| (datum - canopy).abs() > 0.001),
    );
    assert!((expected_eclipses[0] - expected_eclipses[2]).abs() > 0.01);
}

#[test]
fn diffuse_moon_horizon_matches_authoritative_rust() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(0.0, 0.42 * world.height_m());
    let world_time_seconds = (0..28 * 24 * 4)
        .map(|quarter_hour| quarter_hour as f64 * 900.0)
        .filter_map(|time| {
            let lighting = celestial_lighting(world_celestial_conditions(world, time, position_m));
            (lighting.moon_ambient_intensity > 0.005)
                .then_some((time, lighting.moon_direction[2].abs()))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .expect("the lunar month should contain diffuse Moon twilight")
        .0;
    let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
    for (diagnostic, height_m) in [
        ("lighting_receiver_moon_ambient_datum", 0.0),
        ("lighting_receiver_moon_ambient_low_cloud", 900.0),
        ("lighting_receiver_moon_ambient_canopy", 3_600.0),
    ] {
        let expected = celestial_lighting_at_height(world, conditions, height_m);
        let actual = render_diagnostic_image_with_view_at(
            world,
            position_m,
            world_time_seconds,
            1.0,
            diagnostic,
            1,
            1,
        );
        let expected = scalar_debug_pixel(expected.moon_ambient_intensity);
        assert_rgb_close(&actual, &expected, 3);
    }
}

/// Times when a source at the view centre has just become or is just about to
/// stop being bright enough for shadow taps: the Sun or Moon at the surface
/// contrast gate, or at the low-cloud base horizon. Footprints reach farthest there.
fn grazing_light_times(world: WorldGeometry, position_m: Vector) -> Vec<f64> {
    let step_seconds = 60.0;
    let lit = |world_time_seconds: f64| {
        let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
        let surface = celestial_lighting_at_height(world, conditions, 0.0);
        let low_cloud = celestial_lighting_at_height(world, conditions, 900.0);
        [
            surface.sun_direction[2] > 0.04,
            surface.moon_direction[2] > 0.04,
            low_cloud.sun_intensity > 0.0,
            low_cloud.moon_intensity > 0.0,
        ]
    };
    let start_seconds = 9_072_240.0;
    let mut times = Vec::new();
    let mut previous = lit(start_seconds);
    for step in 1..=(2 * 86_400 / 60) {
        let world_time_seconds = start_seconds + f64::from(step) * step_seconds;
        let current = lit(world_time_seconds);
        for (now, before) in current.into_iter().zip(previous) {
            if now != before {
                times.push(if now {
                    world_time_seconds
                } else {
                    world_time_seconds - step_seconds
                });
            }
        }
        previous = current;
    }
    times
}

fn clamped_lookup_pixels(image: &[u8]) -> usize {
    image
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] == [255, 0, 255])
        .count()
}

#[test]
fn occluder_field_lookups_stay_inside_their_grids() {
    let world = WorldGeometry::default();
    let views = [
        (Vector::new(2_400.0, 2_000.0), 1_500.0),
        (Vector::new(-3_700.0, -13_400.0), 2_400.0),
        (Vector::new(-4_000.0, 0.0), 20_000.0),
        (Vector::new(0.5 * world.width_m() - 200.0, 0.0), 4_000.0),
        (Vector::new(0.0, 0.46 * world.height_m()), 2_400.0),
        (Vector::new(800.0, 0.0), 300.0),
    ];
    for (position_m, view_height_m) in views {
        let times = grazing_light_times(world, position_m);
        assert!(times.len() >= 4, "{position_m:?}: {times:?}");
        for world_time_seconds in times {
            let image = render_diagnostic_image_with_view_at(
                world,
                position_m,
                world_time_seconds,
                view_height_m,
                "scene_occluder_coverage",
                96,
                54,
            );
            assert_eq!(
                clamped_lookup_pixels(&image),
                0,
                "{position_m:?} with a {view_height_m} m view at {world_time_seconds} s",
            );
        }
    }
}

#[test]
fn coverage_view_marks_lookups_outside_their_grids() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(800.0, 0.0);
    let night_seconds = (0..96)
        .map(|quarter_hour| f64::from(quarter_hour) * 900.0)
        .find(|&world_time_seconds| {
            let conditions = world_celestial_conditions(world, world_time_seconds, position_m);
            celestial_lighting_at_height(world, conditions, 900.0).sun_direction[2] < -0.6
        })
        .expect("the view centre should have a deep night");
    // Forced branches take Sun shadow taps at night, outside the minimal
    // grid an unlit source gets.
    let image = render_diagnostic_image_with_view_at(
        world,
        position_m,
        night_seconds,
        300.0,
        "scene_occluder_coverage_all_branches",
        96,
        54,
    );

    assert!(clamped_lookup_pixels(&image) > 96 * 54 / 2);
}

#[test]
fn grid_scene_matches_direct_scene_across_longitude_seam() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(0.5 * world.width_m() - 200.0, 0.0);
    let seam_column = ((0.5 * world.width_m() - position_m.x) / (4_000.0 * 256.0 / 128.0) * 256.0
        + 128.0) as usize;
    let render_direct = |world_time_seconds, arguments: &[(&str, &str)]| {
        render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            4_000.0,
            "scene_direct",
            256,
            128,
            arguments,
        )
    };
    // The comparison needs a morning whose clouds or their shadows cross the seam.
    let mut best_occluder_maximum = 0;
    let (world_time_seconds, direct) = (0..16)
        .map(|day| (day as f64 + 0.25) * 86_400.0)
        .find_map(|world_time_seconds| {
            let direct = render_direct(world_time_seconds, &[]);
            let without_occluders = render_direct(
                world_time_seconds,
                &[
                    ("--cloud-rendering", "0,0,1,1"),
                    ("--cloud-shadows", "0,0,0.85,0"),
                ],
            );
            let occluder_maximum = direct
                .as_chunks::<4>()
                .0
                .chunks(256)
                .zip(without_occluders.as_chunks::<4>().0.chunks(256))
                .flat_map(|(direct, bare)| {
                    direct[seam_column - 8..seam_column + 8]
                        .iter()
                        .zip(&bare[seam_column - 8..seam_column + 8])
                })
                .map(|(direct, bare)| {
                    (0..3)
                        .map(|channel| direct[channel].abs_diff(bare[channel]))
                        .max()
                        .unwrap()
                })
                .max()
                .unwrap();
            best_occluder_maximum = best_occluder_maximum.max(occluder_maximum);
            (occluder_maximum > 3).then_some((world_time_seconds, direct))
        })
        .unwrap_or_else(|| {
            panic!(
                "clouds and their shadows never reach the seam; the largest difference was {best_occluder_maximum}"
            )
        });
    let grid = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        4_000.0,
        "scene",
        256,
        128,
    );
    let differences = direct
        .as_chunks::<4>()
        .0
        .iter()
        .zip(grid.as_chunks::<4>().0.iter())
        .map(|(direct, grid)| {
            (0..3)
                .map(|channel| direct[channel].abs_diff(grid[channel]) as f32)
                .sum::<f32>()
                / 3.0
        })
        .collect::<Vec<_>>();
    let mean = differences.iter().sum::<f32>() / differences.len() as f32;
    // Away from the seam, coarsened low-cloud grids soften small cloud edges.
    let seam_maximum = differences
        .chunks(256)
        .flat_map(|row| &row[seam_column - 8..seam_column + 8])
        .copied()
        .fold(0.0, f32::max);

    assert!(mean < 0.1, "mean grid/direct byte difference: {mean}");
    assert!(
        seam_maximum < 3.0,
        "maximum grid/direct byte difference beside the seam: {seam_maximum}"
    );
}

#[test]
fn incrementally_filled_caches_render_like_fresh_ones() {
    let coast_m = rugged_coast_m(WorldGeometry::default());
    let (camera_x, camera_y) = (coast_m.x.to_string(), coast_m.y.to_string());
    let end = [
        "--camera-x",
        &camera_x,
        "--camera-y",
        &camera_y,
        "--view-height",
        "240",
    ];
    // Pans within one finest-level window while zooming out through coarser
    // levels and back, so the end view reuses texels from every step. The last
    // step is narrower, so the end view zooms out and reads the baked level a
    // fresh view reads; the check below covers a zoom in.
    let prior_views = [
        (-840.0, -160.0, 240.0),
        (-590.0, -110.0, 240.0),
        (-340.0, -50.0, 600.0),
        (-240.0, -10.0, 2400.0),
        (-90.0, 0.0, 600.0),
        (-40.0, -5.0, 200.0),
    ]
    .map(|(x_m, y_m, view_height_m)| {
        format!("{},{},{view_height_m}", coast_m.x + x_m, coast_m.y + y_m)
    });
    let prior_views = prior_views
        .iter()
        .flat_map(|view| ["--prior-view", view.as_str()])
        .collect::<Vec<_>>();
    let fresh = render_capture("cache-fresh", &end);
    let incremental = render_capture("cache-incremental", &[&end[..], &prior_views].concat());
    let unfilled = render_capture(
        "cache-unfilled",
        &[&end[..], &prior_views, &["--skip-prior-fills"]].concat(),
    );

    let incremental_differences = pixel_differences(&fresh, &incremental);
    assert!(
        incremental_differences
            .iter()
            .all(|&difference| difference <= 1),
        "incremental fills differ from a fresh fill by up to {}",
        incremental_differences.iter().max().unwrap(),
    );
    let unfilled_pixels = pixel_differences(&fresh, &unfilled)
        .iter()
        .filter(|&&difference| difference > 16)
        .count();
    assert!(
        unfilled_pixels > fresh.len() / 4 / 10,
        "unfilled cache texels left only {unfilled_pixels} pixels visibly wrong",
    );

    // A zoom in may read a level one rung coarser than a fresh view reads.
    let zoomed_in = render_capture(
        "cache-zoomed-in",
        &[&end[..], &prior_views[..prior_views.len() - 2]].concat(),
    );
    let quarter_pixel_m = 0.25 * 240.0 / 144.0;
    let panned_x = (coast_m.x + quarter_pixel_m).to_string();
    let direct = ["--diagnostic", "scene_direct"];
    assert_flips_within_a_quarter_pixel_pan(
        "a zoom in",
        &zoomed_in,
        &fresh,
        &render_capture("cache-direct", &[&end[..], &direct].concat()),
        &render_capture(
            "cache-panned",
            &[
                &[
                    "--camera-x",
                    &panned_x,
                    "--camera-y",
                    &camera_y,
                    "--view-height",
                    "240",
                ][..],
                &direct,
            ]
            .concat(),
        ),
    );
}

#[test]
fn baked_scene_matches_direct_scene_at_boat_zoom_shorelines_across_the_seam() {
    let world = WorldGeometry::default();
    let coast_m = rugged_coast_m(world);
    let reef_y_m = coast_m.y;
    let reef_shore_x_m = (0..4_000)
        .map(|step| coast_m.x - 1_000.0 + 0.5 * step as f32)
        .find(|&x_m| {
            let elevation =
                |x_m| procedural_terrain_elevation_m(world, SEED, Vector::new(x_m, reef_y_m));
            elevation(x_m).signum() != elevation(x_m + 0.5).signum()
        })
        .expect("a shoreline should cross the reef coast row");
    // Boat-zoom pixels, as a 240 m view 1080 pixels high has: the reef
    // shoreline, a broader reef coast view, and a shoreline across the seam.
    for camera_m in [
        (reef_shore_x_m, reef_y_m),
        (coast_m.x, reef_y_m),
        seam_shore_m(world),
    ] {
        assert_baked_shorelines_match_direct((480, 270), 240.0 / 1080.0, camera_m, &[]);
    }
}

#[test]
fn baked_scene_flips_no_more_pixels_than_a_quarter_pixel_pan_at_overview_zooms() {
    let world = WorldGeometry::default();
    let coast_m = rugged_coast_m(world);
    // Pixels of 8 km and 16 km views 1080 pixels high, over the coast and the
    // seam.
    for view_height_m in [8_000.0, 16_000.0] {
        let pixel_m = view_height_m / 1080.0;
        for (camera_x_m, camera_y_m) in [(coast_m.x, coast_m.y), seam_shore_m(world)] {
            let render = |label, diagnostic, camera_x_m: f32| {
                render_capture(
                    label,
                    &[
                        "--width",
                        "480",
                        "--height",
                        "270",
                        "--camera-x",
                        &camera_x_m.to_string(),
                        "--camera-y",
                        &camera_y_m.to_string(),
                        "--view-height",
                        &(pixel_m * 270.0).to_string(),
                        "--diagnostic",
                        diagnostic,
                        "--cloud-rendering",
                        "0,0,1,1",
                        "--cloud-shadows",
                        "0,0,0.85,0",
                    ],
                )
            };
            let direct = render("overview-direct", "scene_direct", camera_x_m);
            assert_flips_within_a_quarter_pixel_pan(
                &format!("{view_height_m} m view at ({camera_x_m}, {camera_y_m})"),
                &render("overview-baked", "scene", camera_x_m),
                &direct,
                &direct,
                &render(
                    "overview-panned",
                    "scene_direct",
                    camera_x_m + 0.25 * pixel_m,
                ),
            );
        }
    }
}

/// Pixels that change by more than 32 levels.
fn flipped_pixels(left: &[u8], right: &[u8]) -> usize {
    pixel_differences(left, right)
        .into_iter()
        .filter(|&difference| difference > 32)
        .count()
}

/// Relief finer than a pixel aliases differently in each reconstruction, as in
/// a sub-pixel pan, while a misplaced shoreline or reef edge flips whole runs
/// of pixels; so `reconstructed` may flip against `reference` no more pixels
/// than panning `direct` by a quarter pixel flips.
fn assert_flips_within_a_quarter_pixel_pan(
    label: &str,
    reconstructed: &[u8],
    reference: &[u8],
    direct: &[u8],
    panned: &[u8],
) {
    let reconstructed_flips = flipped_pixels(reconstructed, reference);
    let pan_flips = flipped_pixels(panned, direct);
    assert!(
        reconstructed_flips <= pan_flips,
        "{label}: {reconstructed_flips} pixels flip, a quarter-pixel pan flips {pan_flips}",
    );
}

/// A point on the longitude seam where a shoreline crosses it.
fn seam_shore_m(world: WorldGeometry) -> (f32, f32) {
    let seam_x_m = 0.5 * world.width_m() - 1.0;
    let seam_shore_y_m = (0..60_000)
        .map(|step| 10.0 * step as f32)
        .find(|&y_m| {
            let elevation =
                |y_m| procedural_terrain_elevation_m(world, SEED, Vector::new(seam_x_m, y_m));
            elevation(y_m).signum() != elevation(y_m + 10.0).signum()
        })
        .expect("a shoreline should cross the longitude seam");
    (seam_x_m, seam_shore_y_m)
}

#[test]
fn baked_scene_matches_direct_scene_at_shorelines_within_the_touch_budget() {
    let coast_m = rugged_coast_m(WorldGeometry::default());
    // A 2048x2732 tablet's scene at the touch render scale of 0.75, whose
    // bake coarsens to fit the budget.
    assert_baked_shorelines_match_direct(
        (1536, 2049),
        240.0 / 2049.0,
        (coast_m.x, coast_m.y),
        &["--touch-budget"],
    );
}

#[test]
fn baked_sea_ice_differs_from_direct_less_than_a_half_pixel_pan() {
    // The bench's northern summer ice edge and the floes beyond it, and the
    // coldest season's ice edge, where the bake's freeze mask ends, in pixels
    // of boat-zoom and 2.4 km views 1080 pixels high.
    for (world_time, (camera_x_m, camera_y_m), view_height_m) in [
        ("15595200", (7_100.0, 29_055.0), 240.0),
        ("15595200", (7_100.0, 29_600.0), 240.0),
        ("15595200", (7_100.0, 29_000.0), 2_400.0),
        ("5529600", (4_878.0, 23_000.0), 240.0),
        ("5529600", (4_878.0, 23_000.0), 2_400.0),
    ] {
        let pixel_m = view_height_m / 1080.0;
        let render = |label, diagnostic, camera_x_m: f32, arguments: &[&str]| {
            render_capture(
                label,
                &[
                    &[
                        "--width",
                        "480",
                        "--height",
                        "270",
                        "--camera-x",
                        &camera_x_m.to_string(),
                        "--camera-y",
                        &camera_y_m.to_string(),
                        "--view-height",
                        &(pixel_m * 270.0).to_string(),
                        "--world-time",
                        world_time,
                        "--diagnostic",
                        diagnostic,
                        "--cloud-rendering",
                        "0,0,1,1",
                        "--cloud-shadows",
                        "0,0,0.85,0",
                    ],
                    arguments,
                ]
                .concat(),
            )
        };
        // Without its colour, sea ice renders black.
        let sea_ice: Vec<bool> = render(
            "sea-ice-mask",
            "scene_direct",
            camera_x_m,
            &["--solid-material-colors", "1,1,1,0"],
        )
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[..3] == [0, 0, 0])
        .collect();
        // Relief finer than a pixel aliases differently in each reconstruction,
        // as in a sub-pixel pan, while lost or misplaced plate detail shifts
        // every ice pixel.
        let direct = render("sea-ice-direct", "scene_direct", camera_x_m, &[]);
        let baked = mean_difference(
            &render("sea-ice-baked", "scene", camera_x_m, &[]),
            &direct,
            &sea_ice,
        );
        let panned = mean_difference(
            &render(
                "sea-ice-panned",
                "scene_direct",
                camera_x_m + 0.5 * pixel_m,
                &[],
            ),
            &direct,
            &sea_ice,
        );
        let label =
            format!("{view_height_m} m view at ({camera_x_m}, {camera_y_m}), {world_time} s");
        assert!(
            sea_ice.iter().filter(|&&ice| ice).count() > sea_ice.len() / 4,
            "{label}: too little sea ice",
        );
        assert!(
            baked <= panned,
            "{label}: baked sea ice differs by {baked}, a half-pixel pan by {panned}",
        );
    }
}

/// The mean over `selected` pixels of each one's largest RGB channel difference.
fn mean_difference(left: &[u8], right: &[u8], selected: &[bool]) -> f64 {
    let (sum, count) = pixel_differences(left, right)
        .into_iter()
        .zip(selected)
        .filter(|&(_, &selected)| selected)
        .fold((0.0, 0), |(sum, count), (difference, _)| {
            (sum + f64::from(difference), count + 1)
        });
    sum / f64::from(count)
}

/// Removes every material colour except water's, leaving land, ice, and lower
/// clouds black.
const WATER_ONLY: [&str; 6] = [
    "--land-material-colors",
    "0,0,0,0",
    "--solid-material-colors",
    "0,0,0,0",
    "--layer-material-colors",
    "0,0,0,0",
];

/// Pixels that are water in one image and land or ice in the other.
fn land_water_flips(left: &[u8], right: &[u8]) -> usize {
    let is_water = |pixel: &[u8; 4]| pixel[..3].iter().any(|&channel| channel > 0);
    left.as_chunks::<4>()
        .0
        .iter()
        .zip(right.as_chunks::<4>().0)
        .filter(|(left, right)| is_water(left) != is_water(right))
        .count()
}

/// Renders the shoreline at `camera_m` with baked and directly evaluated
/// terrain, and asserts that baking moves far fewer pixels between land and
/// water than shifting the view by one pixel of `pixel_m` does. This guards
/// against gross breakage only: the rendered coast needs no closer match.
fn assert_baked_shorelines_match_direct(
    (width, height): (u32, u32),
    pixel_m: f32,
    (camera_x_m, camera_y_m): (f32, f32),
    arguments: &[&str],
) {
    let render = |label, diagnostic, camera_x_m: f32| {
        render_capture(
            label,
            &[
                &[
                    "--width",
                    &width.to_string(),
                    "--height",
                    &height.to_string(),
                    "--camera-x",
                    &camera_x_m.to_string(),
                    "--camera-y",
                    &camera_y_m.to_string(),
                    "--view-height",
                    &(pixel_m * height as f32).to_string(),
                    "--diagnostic",
                    diagnostic,
                    "--cloud-rendering",
                    "0,0,1,1",
                    "--cloud-shadows",
                    "0,0,0.85,0",
                ],
                &WATER_ONLY[..],
                arguments,
            ]
            .concat(),
        )
    };
    let baked = render("shore-baked", "scene", camera_x_m);
    let direct = render("shore-direct", "scene_direct", camera_x_m);
    let shifted = render("shore-shifted", "scene_direct", camera_x_m + pixel_m);

    let label = format!("{pixel_m} m pixels at ({camera_x_m}, {camera_y_m})");
    let shift_flips = land_water_flips(&shifted, &direct);
    let baked_flips = land_water_flips(&baked, &direct);
    assert!(
        shift_flips >= 200,
        "{label}: a one-pixel shift moves only {shift_flips} pixels between land and water",
    );
    assert!(
        baked_flips * 5 <= shift_flips,
        "{label}: baking moves {baked_flips} pixels between land and water, a one-pixel shift {shift_flips}",
    );
}

/// The first view centre, scanning 500 m apart outward from the origin, that is
/// daylit at 37 s and whose 240 m-high view has a shoreline that reefs and rocks
/// break up: its middle row crosses the shoreline at least twelve times at 2 m
/// spacing.
fn rugged_coast_m(world: WorldGeometry) -> Vector {
    let half_width_m = 0.5 * 240.0 * 16.0 / 9.0;
    let crossings = |center_m: Vector, step_m: f32| {
        let count = (2.0 * half_width_m / step_m) as usize;
        (0..=count)
            .map(|index| {
                let x_m = center_m.x - half_width_m + step_m * index as f32;
                procedural_terrain_elevation_m(world, SEED, Vector::new(x_m, center_m.y)) >= 0.0
            })
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
    };
    (0..32_i32)
        .flat_map(|ring| {
            (-ring..=ring).flat_map(move |y_index| {
                (-ring..=ring)
                    .filter(move |x_index| x_index.abs() == ring || y_index.abs() == ring)
                    .map(move |x_index| Vector::new(500.0 * x_index as f32, 500.0 * y_index as f32))
            })
        })
        .find(|&center_m| {
            celestial_lighting(world_celestial_conditions(world, 37.0, center_m)).sun_direction[2]
                > 0.5
                && crossings(center_m, 36.0) > 0
                && crossings(center_m, 2.0) >= 12
        })
        .expect("the world should have a rugged coast")
}

/// Renders a 256x144 capture of the default world at a daytime clock.
fn render_capture(label: &str, arguments: &[&str]) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-{label}-{}-{:016x}.png",
        std::process::id(),
        arguments
            .join(" ")
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            }),
    ));
    let capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"))
        .args([
            "--width",
            "256",
            "--height",
            "144",
            "--seed",
            &SEED.to_string(),
            "--world-time",
            "37",
            "--output",
            output.to_str().unwrap(),
        ])
        .args(arguments)
        .output()
        .expect("headless capture should start");
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

/// Each pixel's largest RGB channel difference.
fn pixel_differences(left: &[u8], right: &[u8]) -> Vec<u8> {
    left.as_chunks::<4>()
        .0
        .iter()
        .zip(right.as_chunks::<4>().0)
        .map(|(left, right)| {
            (0..3)
                .map(|channel| left[channel].abs_diff(right[channel]))
                .max()
                .unwrap()
        })
        .collect()
}

#[test]
fn rendered_sea_ice_matches_the_authoritative_solid_surface() {
    let world = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let season_phase = 0.09;
    let world_time_seconds = world_time_for_season_phase(season_phase);
    let tick = (world_time_seconds * simulation::TICKS_PER_SECOND as f64) as u64;
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
fn hidden_low_cloud_color_row_matches_zero_cloud_opacity() {
    let world = WorldGeometry::default();
    let (world_time_seconds, position_m) = opaque_low_cloud();
    let row_hidden = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "scene",
        1,
        1,
        &[("--layer-material-colors", "0,1,1,1")],
    );
    let opacity_disabled = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "scene",
        1,
        1,
        &[("--cloud-rendering", "1,0,1,0")],
    );
    assert_rgb_close(&row_hidden, &opacity_disabled, 1);
}

/// An early-afternoon world time on the first day whose 16 km overview around
/// `LOW_CLOUD_OVERVIEW_M` shows an opaque low cloud, and that cloud's position.
/// Tests share one search, whose overview captures would otherwise collide.
fn opaque_low_cloud() -> (f64, Vector) {
    static LOW_CLOUD: std::sync::OnceLock<(f64, Vector)> = std::sync::OnceLock::new();
    *LOW_CLOUD.get_or_init(search_opaque_low_cloud)
}

fn search_opaque_low_cloud() -> (f64, Vector) {
    let overview_size = 64usize;
    let overview_height_m = 16_000.0;
    let (world_time_seconds, cloud_pixel) = (0..48)
        .map(|day| f64::from(day) * 86_400.0 + 123.456)
        .find_map(|world_time_seconds| {
            let overview = render_diagnostic_image_with_view_at(
                WorldGeometry::default(),
                LOW_CLOUD_OVERVIEW_M,
                world_time_seconds,
                overview_height_m,
                "cloud_low_optical_depth",
                overview_size as u32,
                overview_size as u32,
            );
            overview
                .as_chunks::<4>()
                .0
                .iter()
                .position(|pixel| scalar_debug_amount(pixel) > 0.2)
                .map(|cloud_pixel| (world_time_seconds, cloud_pixel))
        })
        .expect("some afternoon should show an opaque low cloud");
    let row = cloud_pixel / overview_size;
    let column = cloud_pixel % overview_size;
    let position_m = LOW_CLOUD_OVERVIEW_M
        + Vector::new(
            (column as f32 + 0.5) / overview_size as f32 * overview_height_m
                - 0.5 * overview_height_m,
            0.5 * overview_height_m - (row as f32 + 0.5) / overview_size as f32 * overview_height_m,
        );
    (world_time_seconds, position_m)
}

const LOW_CLOUD_OVERVIEW_M: Vector = Vector::new(0.0, -10_000.0);

#[test]
fn low_cloud_parallax_moves_visible_bodies_without_moving_ground_shadows() {
    let world = WorldGeometry::default();
    let position_m = LOW_CLOUD_OVERVIEW_M;
    let (world_time_seconds, _) = opaque_low_cloud();
    let without_parallax = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        16_000.0,
        "scene",
        65,
        65,
        &[("--cloud-rendering", "1,1,1,0")],
    );
    let with_parallax = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        16_000.0,
        "scene",
        65,
        65,
        &[("--cloud-rendering", "1,1,1,1")],
    );
    assert_ne!(without_parallax, with_parallax);
    let center_pixel = 4 * (32 * 65 + 32);
    assert_eq!(
        &without_parallax[center_pixel..center_pixel + 4],
        &with_parallax[center_pixel..center_pixel + 4],
    );

    let shadows_without_parallax = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        16_000.0,
        "scene",
        64,
        64,
        &[
            ("--cloud-rendering", "1,1,1,0"),
            ("--layer-material-colors", "0,1,1,1"),
        ],
    );
    let shadows_with_parallax = render_controlled_image_at(
        world,
        position_m,
        world_time_seconds,
        16_000.0,
        "scene",
        64,
        64,
        &[
            ("--cloud-rendering", "1,1,1,1"),
            ("--layer-material-colors", "0,1,1,1"),
        ],
    );
    assert_eq!(shadows_without_parallax, shadows_with_parallax);
}

#[test]
fn low_cloud_optical_depth_is_independent_of_pixel_footprint() {
    let world = WorldGeometry::default();
    let (world_time_seconds, position_m) = opaque_low_cloud();
    let near = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        1.0,
        "cloud_low_optical_depth",
        1,
        1,
    );
    let far = render_diagnostic_image_with_view_at(
        world,
        position_m,
        world_time_seconds,
        200.0,
        "cloud_low_optical_depth",
        1,
        1,
    );

    assert_ne!(&near[..3], &scalar_debug_pixel(0.0)[..3]);
    assert_eq!(&near[..3], &far[..3]);
}

#[test]
fn ambient_sky_occlusion_is_lower_frequency_than_canopy_optical_depth() {
    assert_regional_field_has_half_the_fine_variance_share(
        "lighting_ambient_sky_occlusion",
        "cloud_weather_canopy_optical_depth",
    );
}

#[test]
fn ambient_aerosol_is_lower_frequency_than_local_humidity() {
    assert_regional_field_has_half_the_fine_variance_share(
        "lighting_aerosol_optical_depth",
        "weather_relative_humidity",
    );
}

/// Over five 4 km-high views, the regional field keeps under half the local
/// field's share of variance below about 1 km. The local field must keep at
/// least 0.5% of its variance there, ten times the share that 8-bit
/// quantization of a 0.3-wide range leaves, so the comparison is not between
/// two smooth ramps.
fn assert_regional_field_has_half_the_fine_variance_share(regional: &str, local: &str) {
    let world = WorldGeometry::default();
    let mut regional_fine_fraction = 0.0;
    let mut local_fine_fraction = 0.0;
    let positions_m = [
        Vector::new(-12_000.0, -5_000.0),
        Vector::new(9_000.0, 3_000.0),
        Vector::new(-3_000.0, 14_000.0),
        Vector::new(20_000.0, -15_000.0),
        Vector::new(0.0, -24_000.0),
    ];
    for position_m in positions_m {
        let render = |diagnostic| {
            render_diagnostic_image_with_view_at(
                world, position_m, 0.0, 4_000.0, diagnostic, 256, 64,
            )
        };
        regional_fine_fraction += fine_scale_variance_fraction(
            &render(regional),
            256,
            64,
            &format!("{regional} at {position_m:?}"),
        );
        local_fine_fraction += fine_scale_variance_fraction(
            &render(local),
            256,
            64,
            &format!("{local} at {position_m:?}"),
        );
    }
    let view_count = positions_m.len() as f32;

    assert!(
        local_fine_fraction >= 0.005 * view_count,
        "{local} kept only {local_fine_fraction} of its variance below 1 km over {view_count} views",
    );
    assert!(
        regional_fine_fraction < 0.5 * local_fine_fraction,
        "fine-scale variance fraction: {regional} {regional_fine_fraction}, {local} {local_fine_fraction}",
    );
}

/// The share of a scalar image's variance left after subtracting a 17-pixel box
/// blur, about 1 km in the 4 km-high views above, over pixels whose blur lies
/// inside the image. Water vapour and canopy texture still vary at that scale,
/// while regional drivers start at 8 km wavelengths.
fn fine_scale_variance_fraction(bytes: &[u8], width: usize, height: usize, view: &str) -> f32 {
    const RADIUS: usize = 8;
    let values = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| scalar_debug_amount(pixel))
        .collect::<Vec<_>>();
    let interior = |index: usize| {
        let (row, column) = (index / width, index % width);
        (RADIUS..height - RADIUS).contains(&row) && (RADIUS..width - RADIUS).contains(&column)
    };
    let interior_count = (width - 2 * RADIUS) * (height - 2 * RADIUS);
    let mean = (0..values.len())
        .filter(|&index| interior(index))
        .map(|index| values[index])
        .sum::<f32>()
        / interior_count as f32;
    let mut total_variance = 0.0;
    let mut fine_variance = 0.0;
    for row in RADIUS..height - RADIUS {
        for column in RADIUS..width - RADIUS {
            let mut blurred = 0.0;
            for blur_row in row - RADIUS..=row + RADIUS {
                for blur_column in column - RADIUS..=column + RADIUS {
                    blurred += values[blur_row * width + blur_column];
                }
            }
            blurred /= ((2 * RADIUS + 1) * (2 * RADIUS + 1)) as f32;
            let value = values[row * width + column];
            total_variance += (value - mean) * (value - mean);
            fine_variance += (value - blurred) * (value - blurred);
        }
    }
    assert!(total_variance > 0.0, "{view} has no variance");
    fine_variance / total_variance
}

#[test]
fn ambient_sky_occlusion_follows_existing_weather_conditions() {
    let world = WorldGeometry::default();
    let width = 32usize;
    let height = 32usize;
    // One world's weather at one time spans few independent systems, so the
    // correlation is pooled over a population of seeds.
    let mut occlusion = Vec::new();
    let mut cloud_condition = Vec::new();
    for seed in [SEED, 1, 2, 3, 4, 5, 6, 7] {
        let ambient = render_diagnostic_image_with_seed(
            world,
            seed,
            Vector::ZERO,
            0.0,
            world.height_m(),
            "lighting_ambient_sky_occlusion",
            width as u32,
            height as u32,
        );
        for row in 0..height {
            for column in 0..width {
                let offset = 4 * (row * width + column);
                occlusion.push(
                    (ambient[offset] as f32
                        + ambient[offset + 1] as f32
                        + ambient[offset + 2] as f32)
                        / (3.0 * 255.0),
                );
                let position_m = Vector::new(
                    (column as f32 + 0.5) / width as f32 * world.width_m() - 0.5 * world.width_m(),
                    (0.5 - (row as f32 + 0.5) / height as f32) * world.height_m(),
                );
                let weather = procedural_world_environment(world, seed, 0, position_m).weather;
                cloud_condition.push(
                    weather
                        .upper_cloudiness
                        .max(0.5 * weather.cloudiness + 0.5 * weather.rain_intensity),
                );
            }
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
    for diagnostic in [
        "cloud_low_optical_depth",
        "cloud_weather_canopy_optical_depth",
    ] {
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
fn broad_pressure_lift_spans_settled_and_lifting_regions() {
    let world = WorldGeometry::default();
    let lift = render_diagnostic_image(world, "weather_broad_pressure_lift", 128, 128);

    assert!(channel_range(&lift, 0) > 0.65);
}

#[test]
fn broad_atmosphere_emerges_into_distinct_weather_regimes() {
    let world = WorldGeometry::default();
    let width = 128usize;
    let height = 128usize;
    let cover = render_diagnostic_image(world, "weather_sky_fraction", width as u32, height as u32);
    let thickness = render_diagnostic_image(
        world,
        "weather_canopy_optical_thickness",
        width as u32,
        height as u32,
    );
    let debug_amount = |image: &[u8], offset: usize| {
        let srgb = image[offset] as f32 / 255.0;
        let linear = if srgb <= 0.040_45 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        };
        ((linear - 0.015) / 0.905).clamp(0.0, 1.0)
    };
    let mut clear = 0;
    let mut mixed = 0;
    let mut overcast = 0;
    let mut storm = 0;
    for block_row in 0..8 {
        for block_column in 0..8 {
            let mut cover_sum = 0.0;
            let mut thickness_sum = 0.0;
            for row in block_row * 16..(block_row + 1) * 16 {
                for column in block_column * 16..(block_column + 1) * 16 {
                    let offset = 4 * (row * width + column);
                    cover_sum += debug_amount(&cover, offset);
                    thickness_sum += debug_amount(&thickness, offset);
                }
            }
            let block_cover = cover_sum / 256.0;
            let block_thickness = thickness_sum / 256.0;
            clear += usize::from(block_cover < 0.06);
            mixed += usize::from(block_cover > 0.18 && block_cover < 0.42);
            overcast += usize::from(block_cover > 0.68);
            storm += usize::from(block_thickness > 0.68);
        }
    }

    assert!(clear >= 4, "clear blocks {clear}");
    assert!(mixed >= 4, "mixed blocks {mixed}");
    assert!(overcast >= 2, "overcast blocks {overcast}");
    assert!(storm >= 1, "storm blocks {storm}");
}

#[test]
fn canopy_coverage_tracks_ambient_sky_fraction() {
    let world = WorldGeometry::default();
    let width = 128usize;
    let height = 128usize;
    for (seed, world_time_seconds) in [(SEED, 1_800.0), (1, 900.0), (0xdead_beef, 3_600.0)] {
        let canopy = render_diagnostic_image_with_seed(
            world,
            seed,
            Vector::ZERO,
            world_time_seconds,
            world.height_m(),
            "cloud_weather_canopy_optical_depth",
            width as u32,
            height as u32,
        );
        let ambient = render_diagnostic_image_with_seed(
            world,
            seed,
            Vector::ZERO,
            world_time_seconds,
            world.height_m(),
            "lighting_ambient_sky_occlusion",
            width as u32,
            height as u32,
        );
        let mut canopy_cover = Vec::new();
        let mut sky_fraction = Vec::new();
        for block_row in 0..8 {
            for block_column in 0..8 {
                let mut occupied = 0.0;
                let mut ambient_sum = 0.0;
                for row in block_row * 16..(block_row + 1) * 16 {
                    for column in block_column * 16..(block_column + 1) * 16 {
                        let offset = 4 * (row * width + column);
                        occupied += f32::from(scalar_debug_amount(&canopy[offset..]) > 0.05);
                        ambient_sum += scalar_debug_amount(&ambient[offset..]);
                    }
                }
                canopy_cover.push(occupied / 256.0);
                sky_fraction.push(ambient_sum / 256.0);
            }
        }

        let correlation = pearson_correlation(&canopy_cover, &sky_fraction);
        let mean_error = canopy_cover
            .iter()
            .zip(&sky_fraction)
            .map(|(cover, target)| (cover - target).abs())
            .sum::<f32>()
            / canopy_cover.len() as f32;
        let target_mean = sky_fraction.iter().sum::<f32>() / sky_fraction.len() as f32;
        let cover_mean = canopy_cover.iter().sum::<f32>() / canopy_cover.len() as f32;
        assert!(
            correlation > 0.75,
            "seed {seed:#x}: correlation {correlation}"
        );
        assert!(
            (cover_mean - target_mean).abs() < 0.05,
            "seed {seed:#x}: cover {cover_mean}, target {target_mean}",
        );
        assert!(mean_error < 0.10, "seed {seed:#x}: mean error {mean_error}");
    }
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
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after_sunset.as_chunks::<4>().0.iter())
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
fn projected_upper_clouds_can_shadow_locally_clear_ground() {
    let world = WorldGeometry::default();
    let width = 128usize;
    let height = 64usize;
    let world_time_seconds = world_time_for_season_phase(0.5021);
    let sky = render_diagnostic_image_at(
        world,
        Vector::ZERO,
        world_time_seconds,
        "lighting_ambient_sky_occlusion",
        width as u32,
        height as u32,
    );
    let shadow = render_diagnostic_image_at(
        world,
        Vector::ZERO,
        world_time_seconds,
        "lighting_projected_upper_shadow",
        width as u32,
        height as u32,
    );
    let shadowed_clear_ground = sky
        .as_chunks::<4>()
        .0
        .iter()
        .zip(shadow.as_chunks::<4>().0.iter())
        .filter(|(sky, shadow)| {
            sky[..3] == scalar_debug_pixel(0.0)[..3] && scalar_debug_amount(*shadow) > 0.05
        })
        .count();

    assert!(
        shadowed_clear_ground > 0,
        "no projected canopy crossed a clear-sky boundary"
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
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > pixel[2].saturating_add(8)),
        "pressure diagnostic contains no high-pressure red"
    );
    assert!(
        pressure
            .as_chunks::<4>()
            .0
            .iter()
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
    let epoch_phase =
        world_celestial_conditions(WorldGeometry::default(), 0.0, Vector::ZERO).season_phase;
    (season_phase - f64::from(epoch_phase)).rem_euclid(1.0) * 86_400.0 * 365.0
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
    render_diagnostic_image_with_seed(
        world,
        SEED,
        position_m,
        world_time_seconds,
        view_height_m,
        diagnostic,
        width,
        height,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_diagnostic_image_with_seed(
    world: WorldGeometry,
    seed: u64,
    position_m: Vector,
    world_time_seconds: f64,
    view_height_m: f32,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-spatial-surface-{}-{seed:016x}-{:016x}-{:08x}-{:08x}-{diagnostic}-{width}x{height}.png",
        std::process::id(),
        world_time_seconds.to_bits(),
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
            &seed.to_string(),
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

#[allow(clippy::too_many_arguments)]
fn render_controlled_image_at(
    world: WorldGeometry,
    position_m: Vector,
    world_time_seconds: f64,
    view_height_m: f32,
    diagnostic: &str,
    width: u32,
    height: u32,
    controls: &[(&str, &str)],
) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-controlled-{}-{:016x}-{:08x}-{:08x}-{diagnostic}-{width}x{height}.png",
        std::process::id(),
        world_time_seconds.to_bits(),
        position_m.x.to_bits(),
        position_m.y.to_bits(),
    ));
    let mut capture = Command::new(env!("CARGO_BIN_EXE_playground-capture"));
    capture.args([
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
        "--world-time",
        &world_time_seconds.to_string(),
        "--diagnostic",
        diagnostic,
        "--output",
        output.to_str().unwrap(),
    ]);
    for (option, value) in controls {
        capture.args([option, value]);
    }
    let capture = capture
        .output()
        .expect("headless controlled capture should start");
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
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[channel])
        .min()
        .unwrap();
    let maximum = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[channel])
        .max()
        .unwrap();
    f32::from(maximum - minimum) / 255.0
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

fn linear_luminance(pixel: &[u8]) -> f32 {
    0.2126 * srgb_channel_to_linear(pixel[0])
        + 0.7152 * srgb_channel_to_linear(pixel[1])
        + 0.0722 * srgb_channel_to_linear(pixel[2])
}

fn mean_linear_luminance(image: &[u8]) -> f32 {
    image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| linear_luminance(pixel))
        .sum::<f32>()
        / (image.len() / 4) as f32
}

fn assert_rgb_close(left: &[u8], right: &[u8], tolerance: u8) {
    for channel in 0..3 {
        assert!(
            left[channel].abs_diff(right[channel]) <= tolerance,
            "{left:?} != {right:?}",
        );
    }
}

fn scalar_debug_amount(pixel: &[u8]) -> f32 {
    let srgb = pixel[0] as f32 / 255.0;
    let linear = if srgb <= 0.040_45 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    };
    ((linear - 0.015) / 0.905).clamp(0.0, 1.0)
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
