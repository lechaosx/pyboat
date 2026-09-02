use std::{fs, io::BufReader, process::Command};

use game::{
    celestial_disc_visibility_at_height, celestial_lighting, celestial_lighting_at_height,
    procedural_solid_surface_elevation_m, procedural_terrain_elevation_m,
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
    for (coverage, scattering) in coverage.chunks_exact(4).zip(scattering.chunks_exact(4)) {
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
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, pixel)| (0.3..0.7).contains(&scalar_debug_amount(pixel)))
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
            (light.sun_intensity > 0.1 && light.moon_intensity > 0.005)
                .then_some((time, light.sun_intensity.min(8.0 * light.moon_intensity)))
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
            ("--light-balance", "0.15,0.15,0.15,0.15"),
            ("--light-contributions", "0,0,0,0"),
        ],
    );
    assert!(
        off.chunks_exact(4)
            .flat_map(|pixel| &pixel[..3])
            .all(|channel| *channel <= 1),
        "all disabled: maximum channel {}",
        off.chunks_exact(4)
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
            ("--light-balance", "0.15,0.15,0.15,0.15"),
            ("--light-contributions", gate_value.as_str()),
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
            ("--light-balance", "0.15,0.15,0.15,0.15"),
            ("--light-contributions", "1,1,1,1"),
        ],
    );
    let mean_error = all
        .chunks_exact(4)
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
            .chunks_exact(4)
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
            .chunks_exact(4)
            .zip(disabled.chunks_exact(4))
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
fn shadow_atlas_requires_two_distinct_reused_footprints() {
    let world = WorldGeometry::default();
    for (label, position_m, view_height_m) in [
        ("local", Vector::ZERO, 8_000.0),
        ("wide", Vector::ZERO, 24_000.0),
        (
            "north-pole",
            Vector::new(0.0, 0.46 * world.height_m()),
            12_000.0,
        ),
        (
            "south-pole",
            Vector::new(0.0, -0.46 * world.height_m()),
            12_000.0,
        ),
    ] {
        let world_time_seconds = (0..365 * 24)
            .map(|hour| hour as f64 * 3_600.0)
            .filter_map(|time| {
                let conditions = world_celestial_conditions(world, time, position_m);
                let surface = celestial_lighting_at_height(world, conditions, 0.0);
                let low = celestial_lighting_at_height(world, conditions, 900.0);
                (surface.sun_intensity > 0.12
                    && low.sun_intensity > 0.12
                    && low.moon_intensity == 0.0
                    && low.moon_direction[2] < -0.20)
                    .then_some((time, surface.sun_intensity.min(low.sun_intensity)))
            })
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap_or_else(|| panic!("{label}: no isolated Sun interval"))
            .0;
        let single = render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            view_height_m,
            "cloud_shadow_cache_tiles",
            128,
            128,
            &[
                ("--landscape-visibility", "1,1,1,0"),
                ("--cloud-shadows", "0,0,0.85,1"),
            ],
        );
        assert!(
            scalar_debug_amount(&single) < 0.01,
            "{label}: single footprint was cached",
        );
        let reused = render_controlled_image_at(
            world,
            position_m,
            world_time_seconds,
            view_height_m,
            "cloud_shadow_cache_tiles",
            128,
            128,
            &[
                ("--landscape-visibility", "1,1,1,0"),
                ("--cloud-shadows", "0,1,0.85,1"),
            ],
        );
        assert!(
            scalar_debug_amount(&reused) > 0.99,
            "{label}: two overlapping receiver footprints were not cached",
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
    let overview_size = 32usize;
    let overview_height_m = 16_000.0;
    let overview = render_diagnostic_image_with_view_at(
        world,
        Vector::new(0.0, -10_000.0),
        0.0,
        overview_height_m,
        "cloud_low_optical_depth",
        overview_size as u32,
        overview_size as u32,
    );
    let cloud_pixel = overview
        .chunks_exact(4)
        .position(|pixel| pixel[..3] != scalar_debug_pixel(0.0)[..3])
        .expect("representative view should contain a low cloud");
    let row = cloud_pixel / overview_size;
    let column = cloud_pixel % overview_size;
    let position_m = Vector::new(
        (column as f32 + 0.5) / overview_size as f32 * overview_height_m - 0.5 * overview_height_m,
        -10_000.0 + 0.5 * overview_height_m
            - (row as f32 + 0.5) / overview_size as f32 * overview_height_m,
    );
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

    assert_ne!(&near[..3], &scalar_debug_pixel(0.0)[..3]);
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
    let ambient_detail = normalized_mean_neighbor_scalar_difference(&ambient, 256, 64);
    let canopy_detail = normalized_mean_neighbor_scalar_difference(&canopy, 256, 64);

    assert!(canopy_detail > 0.0005, "{canopy_detail}");
    assert!(
        ambient_detail < 0.5 * canopy_detail,
        "ambient {ambient_detail}, canopy {canopy_detail}",
    );
}

#[test]
fn ambient_aerosol_is_lower_frequency_than_local_humidity() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(-12_000.0, -5_000.0);
    let aerosol = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        4_000.0,
        "lighting_aerosol_optical_depth",
        256,
        64,
    );
    let humidity = render_diagnostic_image_with_view_at(
        world,
        position_m,
        0.0,
        4_000.0,
        "weather_relative_humidity",
        256,
        64,
    );
    let aerosol_detail = normalized_mean_neighbor_scalar_difference(&aerosol, 256, 64);
    let humidity_detail = normalized_mean_neighbor_scalar_difference(&humidity, 256, 64);

    assert!(humidity_detail > 0.0005, "{humidity_detail}");
    assert!(
        aerosol_detail < 0.5 * humidity_detail,
        "aerosol {aerosol_detail}, humidity {humidity_detail}",
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
    for (seed, cloud_time_seconds) in [(SEED, 1_800.0), (1, 900.0), (0xdead_beef, 3_600.0)] {
        let canopy = render_diagnostic_image_with_seed_and_cloud_time(
            world,
            seed,
            Vector::ZERO,
            0.0,
            cloud_time_seconds,
            world.height_m(),
            "cloud_weather_canopy_optical_depth",
            width as u32,
            height as u32,
        );
        let ambient = render_diagnostic_image_with_seed_and_cloud_time(
            world,
            seed,
            Vector::ZERO,
            0.0,
            cloud_time_seconds,
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
        .chunks_exact(4)
        .zip(shadow.chunks_exact(4))
        .filter(|(sky, shadow)| {
            sky[..3] == scalar_debug_pixel(0.0)[..3] && scalar_debug_amount(shadow) > 0.05
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
    render_diagnostic_image_with_seed_and_cloud_time(
        world,
        SEED,
        position_m,
        world_time_seconds,
        cloud_time_seconds,
        view_height_m,
        diagnostic,
        width,
        height,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_diagnostic_image_with_seed_and_cloud_time(
    world: WorldGeometry,
    seed: u64,
    position_m: Vector,
    world_time_seconds: f64,
    cloud_time_seconds: f32,
    view_height_m: f32,
    diagnostic: &str,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!(
        "pyboat-spatial-surface-{}-{seed:016x}-{:016x}-{:08x}-{:08x}-{:08x}-{diagnostic}-{width}x{height}.png",
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
            &seed.to_string(),
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
        "--cloud-time",
        "0",
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

fn normalized_mean_neighbor_scalar_difference(bytes: &[u8], width: usize, height: usize) -> f32 {
    let (minimum, maximum) = bytes.chunks_exact(4).map(scalar_debug_amount).fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
    );
    let range = maximum - minimum;
    let mut difference = 0.0;
    let mut count = 0;
    for row in 0..height {
        for column in 1..width {
            let offset = 4 * (row * width + column);
            difference += (scalar_debug_amount(&bytes[offset..])
                - scalar_debug_amount(&bytes[offset - 4..]))
            .abs()
                / range;
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
    image.chunks_exact(4).map(linear_luminance).sum::<f32>() / (image.len() / 4) as f32
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
