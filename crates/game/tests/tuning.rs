use std::collections::HashMap;

use pyboat_game::{
    AtmospherePrimary, CelestialConditions, ClimateConditions, celestial_lighting,
    current_low_temperature_c, derive_weather, dinghy_parameters, match_environment,
    procedural_atmosphere_primary, procedural_cloud_cover, procedural_match_environment,
    procedural_terrain_elevation_m, procedural_unsheltered_wind_velocity_m_s, procedural_weather,
};
use rapier2d::prelude::Vector;

#[test]
fn shared_dinghy_and_environment_preserve_the_accepted_tuning() {
    let parameters = dinghy_parameters();
    let total_mass_kg = parameters.hull.mass_kg
        + parameters.sail_mass_kg
        + parameters.boom_mass_kg
        + parameters.rudder_mass_kg
        + parameters.tiller_mass_kg;

    assert_eq!(total_mass_kg, 250.0);
    assert_eq!(parameters.hull.length_m, 4.0);
    assert_eq!(parameters.hull.beam_m, 1.5);
    assert_eq!(parameters.sail_area_m2, 8.0);
    assert_eq!(parameters.keel_area_m2, 0.6);
    assert_eq!(parameters.rudder_area_m2, 0.12);
    assert_eq!(parameters.draft_m, 0.6);
    assert_eq!(parameters.ground_static_friction_coefficient, 1.0);
    assert_eq!(parameters.ground_kinetic_friction_coefficient, 0.6);
    assert_eq!(
        match_environment().true_wind_velocity_m_s,
        Vector::new(6.0, 0.0)
    );
    assert_eq!(match_environment().true_water_velocity_m_s, Vector::ZERO);
    assert_eq!(match_environment().water_depth_m, f32::INFINITY);
}

#[test]
fn procedural_wind_is_seeded_and_continuous_across_space_and_time() {
    let seed = 0x5eed_u64;
    let position_m = Vector::new(12.0, -7.0);
    let wind = procedural_match_environment(seed, 600, position_m);

    assert_eq!(wind, procedural_match_environment(seed, 600, position_m));
    assert!(wind.true_wind_velocity_m_s.length().is_finite());
    assert_eq!(wind.true_water_velocity_m_s, Vector::ZERO);

    let nearby = procedural_match_environment(seed, 601, position_m + Vector::new(0.1, 0.1));
    assert!((nearby.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() < 1.0);

    let distant = procedural_match_environment(seed, 600, position_m + Vector::new(64.0, 0.0));
    let later = procedural_match_environment(seed, 4200, position_m);
    let other_seed = procedural_match_environment(seed + 1, 600, position_m);
    assert!((distant.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
    assert!((later.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
    assert!((other_seed.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
}

#[test]
fn procedural_wind_is_spatially_and_temporally_coherent() {
    let position_m = Vector::new(12.0, -7.0);
    let mut spatial_change_sum = 0.0;
    let mut one_second_change_sum = 0.0;
    for seed in 0..512 {
        let wind = procedural_match_environment(seed, 12_345, position_m).true_wind_velocity_m_s;
        let adjacent =
            procedural_match_environment(seed, 12_345, position_m + Vector::new(3.0, 0.0))
                .true_wind_velocity_m_s;
        let one_second_later =
            procedural_match_environment(seed, 12_405, position_m).true_wind_velocity_m_s;
        spatial_change_sum += (adjacent - wind).length();
        one_second_change_sum += (one_second_later - wind).length();
    }
    assert!(spatial_change_sum / 512.0 < 0.75);
    assert!(one_second_change_sum / 512.0 < 1.25);

    let seed = 0x5eed_u64;
    let mut minimum_speed = f32::MAX;
    let mut maximum_speed: f32 = 0.0;
    for tick in (0..=120 * 60).step_by(60) {
        let speed = procedural_match_environment(seed, tick, position_m)
            .true_wind_velocity_m_s
            .length();
        minimum_speed = minimum_speed.min(speed);
        maximum_speed = maximum_speed.max(speed);
    }
    assert!(maximum_speed - minimum_speed > 1.0);
}

#[test]
fn procedural_wind_has_a_local_prevailing_direction_without_a_worldwide_one() {
    let position_m = Vector::new(12.0, -7.0);
    let mut local_alignment_sum = 0.0;
    let mut regional_alignment_sum = 0.0;
    let mut local_samples = 0;
    let mut regional_samples = 0;

    for seed in 0..1_024 {
        let wind = procedural_unsheltered_wind_velocity_m_s(seed, 12_345, position_m);
        let local = procedural_unsheltered_wind_velocity_m_s(
            seed,
            12_345,
            position_m + Vector::new(2_048.0, 0.0),
        );
        let regional = procedural_unsheltered_wind_velocity_m_s(
            seed,
            12_345,
            position_m + Vector::new(65_536.0, 0.0),
        );
        if wind.length() >= 3.0 && local.length() >= 3.0 {
            local_alignment_sum += wind.normalize().dot(local.normalize());
            local_samples += 1;
        }
        if wind.length() >= 3.0 && regional.length() >= 3.0 {
            regional_alignment_sum += wind.normalize().dot(regional.normalize());
            regional_samples += 1;
        }
    }

    assert!(local_samples > 800);
    assert!(regional_samples > 800);
    assert!(local_alignment_sum / local_samples as f32 > 0.85);
    assert!((regional_alignment_sum / regional_samples as f32).abs() < 0.25);
}

#[test]
fn procedural_cloud_cover_is_seeded_and_continuous_in_spacetime() {
    let position_m = Vector::new(12.0, -7.0);
    let mut nearby_change = 0.0;
    let mut one_second_change = 0.0;
    let mut distant_change = 0.0;
    let mut one_minute_change = 0.0;
    let mut other_seed_change = 0.0;

    for seed in 0..512 {
        let cover = procedural_cloud_cover(seed, 12_345, position_m);
        assert_eq!(cover, procedural_cloud_cover(seed, 12_345, position_m));
        assert!((0.0..=1.0).contains(&cover));
        nearby_change += (procedural_cloud_cover(seed, 12_345, position_m + Vector::new(1.0, 0.0))
            - cover)
            .abs();
        one_second_change += (procedural_cloud_cover(seed, 12_405, position_m) - cover).abs();
        distant_change +=
            (procedural_cloud_cover(seed, 12_345, position_m + Vector::new(8_192.0, 0.0)) - cover)
                .abs();
        one_minute_change += (procedural_cloud_cover(seed, 15_945, position_m) - cover).abs();
        other_seed_change +=
            (procedural_cloud_cover(seed + 10_000, 12_345, position_m) - cover).abs();
    }

    assert!(nearby_change / 512.0 < 0.02);
    assert!(one_second_change / 512.0 < 0.1);
    assert!(distant_change / 512.0 > 0.1);
    assert!(
        one_minute_change / 512.0 > 0.03,
        "mean one-minute density change was {}",
        one_minute_change / 512.0
    );
    assert!(other_seed_change / 512.0 > 0.1);
}

#[test]
fn procedural_cloud_cover_has_mostly_clear_skies_and_rare_storms() {
    let mut clear_samples = 0;
    let mut cloudy_samples = 0;
    let mut storm_samples = 0;
    let mut samples = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -64..=64 {
            for y in -64..=64 {
                let density = procedural_cloud_cover(
                    seed,
                    12_345,
                    Vector::new(x as f32 * 256.0, y as f32 * 256.0),
                );
                clear_samples += usize::from(density <= 0.05);
                cloudy_samples += usize::from(density >= 0.2);
                storm_samples += usize::from(density >= 0.85);
                samples += 1;
            }
        }
    }

    assert!(
        clear_samples * 2 > samples,
        "only {clear_samples} of {samples} samples were clear"
    );
    assert!(
        cloudy_samples * 10 > samples,
        "only {cloudy_samples} of {samples} samples were cloudy"
    );
    assert!(
        cloudy_samples * 5 < samples * 2,
        "{cloudy_samples} of {samples} samples were cloudy"
    );
    assert!(
        storm_samples > 0,
        "no sampled cloud density reached storm strength"
    );
    assert!(
        storm_samples * 100 < samples,
        "{storm_samples} of {samples} samples were storm density"
    );
}

#[test]
fn pressure_gradient_drives_nearly_isobaric_surface_wind() {
    let primary = AtmospherePrimary {
        pressure_hpa: 1000.0,
        pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        low_temperature_c: 20.0,
        upper_temperature_c: -15.0,
        temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 8.0,
    };

    let wind = derive_weather(primary).wind_velocity_m_s;

    assert!(wind.y > 5.0, "surface wind was {wind:?}");
    assert!(
        wind.x < 0.0,
        "surface wind did not cross toward low pressure: {wind:?}"
    );
    assert!(
        wind.x.abs() < wind.y * 0.2,
        "surface wind was not nearly isobaric: {wind:?}"
    );
}

#[test]
fn horizontal_temperature_gradient_produces_vertical_wind_shear() {
    let calm_aloft = AtmospherePrimary {
        pressure_hpa: 1000.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0008, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.0008, 0.0),
        low_temperature_c: 24.0,
        upper_temperature_c: -15.0,
        temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 10.0,
    };
    let sheared = AtmospherePrimary {
        temperature_gradient_c_per_m: Vector::new(0.0004, 0.0),
        ..calm_aloft
    };

    let calm_weather = derive_weather(calm_aloft);
    let sheared_weather = derive_weather(sheared);

    assert_eq!(
        calm_weather.wind_velocity_m_s,
        sheared_weather.wind_velocity_m_s
    );
    assert!(
        (sheared_weather.upper_wind_velocity_m_s - calm_weather.upper_wind_velocity_m_s).length()
            > 2.0
    );
    assert_eq!(
        sheared_weather.wind_shear_m_s,
        sheared_weather.upper_wind_velocity_m_s - sheared_weather.wind_velocity_m_s
    );
}

#[test]
fn moisture_increases_humidity_clouds_and_rain_without_overriding_the_result() {
    let dry = AtmospherePrimary {
        pressure_hpa: 990.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0012, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        low_temperature_c: 28.0,
        upper_temperature_c: -20.0,
        temperature_gradient_c_per_m: Vector::new(0.0003, 0.0),
        water_vapor_g_kg: 4.0,
    };
    let moist = AtmospherePrimary {
        water_vapor_g_kg: 18.0,
        ..dry
    };

    let dry_weather = derive_weather(dry);
    let moist_weather = derive_weather(moist);

    assert!(moist_weather.relative_humidity > dry_weather.relative_humidity);
    assert!(moist_weather.cloudiness > dry_weather.cloudiness);
    assert!(moist_weather.rain_intensity > dry_weather.rain_intensity);
}

#[test]
fn procedural_primary_atmosphere_is_seeded_and_locally_continuous() {
    let seed = 0x5eed;
    let tick = 12_345;
    let position_m = Vector::new(12.0, -7.0);
    let primary = procedural_atmosphere_primary(seed, tick, position_m);

    assert_eq!(
        primary,
        procedural_atmosphere_primary(seed, tick, position_m)
    );
    assert!(primary.pressure_hpa.is_finite());
    assert!((-45.0..=45.0).contains(&primary.low_temperature_c));
    assert!((-80.0..=10.0).contains(&primary.upper_temperature_c));
    assert!((0.0..=25.0).contains(&primary.water_vapor_g_kg));

    let nearby = procedural_atmosphere_primary(seed, tick + 1, position_m + Vector::new(1.0, 0.0));
    assert!((nearby.pressure_hpa - primary.pressure_hpa).abs() < 0.1);
    assert!((nearby.low_temperature_c - primary.low_temperature_c).abs() < 0.1);
    assert!((nearby.water_vapor_g_kg - primary.water_vapor_g_kg).abs() < 0.1);
}

#[test]
fn celestial_cycle_places_equinox_sun_overhead_at_equatorial_noon() {
    let noon = celestial_lighting(CelestialConditions {
        latitude_degrees: 0.0,
        day_of_year: 80.0,
        solar_time_hours: 12.0,
        moon_phase: 0.0,
    });
    let midnight = celestial_lighting(CelestialConditions {
        solar_time_hours: 0.0,
        ..CelestialConditions {
            latitude_degrees: 0.0,
            day_of_year: 80.0,
            solar_time_hours: 12.0,
            moon_phase: 0.0,
        }
    });

    assert!(noon.sun_direction[2] > 0.99);
    assert!(noon.sun_intensity > 0.99);
    assert!(midnight.sun_direction[2] < -0.99);
    assert_eq!(midnight.sun_intensity, 0.0);
}

#[test]
fn full_moon_opposes_the_sun_while_new_moon_contributes_no_light() {
    let full_moon_midnight = celestial_lighting(CelestialConditions {
        latitude_degrees: 0.0,
        day_of_year: 80.0,
        solar_time_hours: 0.0,
        moon_phase: 0.5,
    });
    let new_moon_midnight = celestial_lighting(CelestialConditions {
        moon_phase: 0.0,
        ..CelestialConditions {
            latitude_degrees: 0.0,
            day_of_year: 80.0,
            solar_time_hours: 0.0,
            moon_phase: 0.5,
        }
    });

    assert!(full_moon_midnight.moon_direction[2] > 0.99);
    assert!(full_moon_midnight.moon_intensity > 0.09);
    assert_eq!(new_moon_midnight.moon_intensity, 0.0);
    assert!(full_moon_midnight.ambient_intensity > new_moon_midnight.ambient_intensity);
}

#[test]
fn seasonal_solar_declination_changes_high_latitude_noon_elevation() {
    let summer = celestial_lighting(CelestialConditions {
        latitude_degrees: 55.0,
        day_of_year: 172.0,
        solar_time_hours: 12.0,
        moon_phase: 0.25,
    });
    let winter = celestial_lighting(CelestialConditions {
        day_of_year: 355.0,
        ..CelestialConditions {
            latitude_degrees: 55.0,
            day_of_year: 172.0,
            solar_time_hours: 12.0,
            moon_phase: 0.25,
        }
    });

    assert!(summer.sun_direction[2] > winter.sun_direction[2] + 0.5);
    assert!(summer.ambient_intensity > winter.ambient_intensity);
}

#[test]
fn climate_cycle_has_opposite_seasons_and_a_lagged_daily_temperature_peak() {
    let climate = ClimateConditions {
        annual_mean_low_temperature_c: 15.0,
        seasonal_temperature_range_c: 20.0,
        diurnal_temperature_range_c: 8.0,
    };
    let northern_summer = current_low_temperature_c(
        climate,
        CelestialConditions {
            latitude_degrees: 55.0,
            day_of_year: 205.0,
            solar_time_hours: 15.0,
            moon_phase: 0.0,
        },
    );
    let northern_winter = current_low_temperature_c(
        climate,
        CelestialConditions {
            latitude_degrees: 55.0,
            day_of_year: 22.0,
            solar_time_hours: 15.0,
            moon_phase: 0.0,
        },
    );
    let southern_summer = current_low_temperature_c(
        climate,
        CelestialConditions {
            latitude_degrees: -55.0,
            day_of_year: 22.0,
            solar_time_hours: 15.0,
            moon_phase: 0.0,
        },
    );
    let summer_before_dawn = current_low_temperature_c(
        climate,
        CelestialConditions {
            latitude_degrees: 55.0,
            day_of_year: 205.0,
            solar_time_hours: 3.0,
            moon_phase: 0.0,
        },
    );

    assert!(northern_summer > northern_winter + 19.0);
    assert!((northern_summer - southern_summer).abs() < 0.01);
    assert!(northern_summer > summer_before_dawn + 7.9);
}

#[test]
fn procedural_weather_composes_shared_wind_clouds_and_rain() {
    let position_m = Vector::new(12.0, -7.0);
    let mut clear_samples = 0;
    let mut rainy_samples = 0;
    let mut instability_range = (1.0_f32, 0.0_f32);
    let mut high_cloud_samples = 0;
    let mut full_wind_change = 0.0;
    let mut upper_cloud_wind_change = 0.0;

    for seed in 0..1_024 {
        let weather = procedural_weather(seed, 12_345, position_m);
        assert_eq!(weather, procedural_weather(seed, 12_345, position_m));
        assert_eq!(
            weather.wind_velocity_m_s,
            procedural_unsheltered_wind_velocity_m_s(seed, 12_345, position_m)
        );
        let later_weather = procedural_weather(seed, 12_405, position_m);
        full_wind_change += (later_weather.wind_velocity_m_s - weather.wind_velocity_m_s).length();
        upper_cloud_wind_change +=
            (later_weather.upper_wind_velocity_m_s - weather.upper_wind_velocity_m_s).length();
        assert_eq!(
            weather.cloudiness,
            procedural_cloud_cover(seed, 12_345, position_m)
        );
        assert!((0.0..=1.5).contains(&weather.relative_humidity));
        assert!((0.0..=1.0).contains(&weather.lift));
        assert!((0.0..=1.0).contains(&weather.instability));
        assert!((0.0..=1.0).contains(&weather.upper_cloudiness));
        assert!((0.0..=1.0).contains(&weather.rain_intensity));
        if weather.rain_intensity > 0.0 {
            assert!(weather.cloudiness > 0.7);
        }
        clear_samples += usize::from(weather.rain_intensity == 0.0);
        rainy_samples += usize::from(weather.rain_intensity > 0.0);
        instability_range.0 = instability_range.0.min(weather.instability);
        instability_range.1 = instability_range.1.max(weather.instability);
        high_cloud_samples += usize::from(weather.upper_cloudiness > 0.0);
    }

    assert!(clear_samples > rainy_samples);
    assert!(rainy_samples > 0);
    assert!(instability_range.0 < 0.25);
    assert!(instability_range.1 > 0.75);
    assert!(high_cloud_samples > 0);
    assert!(high_cloud_samples < 512);
    assert!(upper_cloud_wind_change < full_wind_change * 0.5);
}

#[test]
fn procedural_weather_upper_wind_is_seeded_coherent_and_faster_aloft() {
    let position_m = Vector::new(12.0, -7.0);
    let mut sea_speed_sum = 0.0;
    let mut upper_speed_sum = 0.0;
    let mut alignment_sum = 0.0;
    let mut samples = 0;
    let mut sheared_samples = 0;

    for seed in 0..1_024 {
        let weather = procedural_weather(seed, 12_345, position_m);
        assert_eq!(weather, procedural_weather(seed, 12_345, position_m));

        let sea = weather.wind_velocity_m_s;
        let upper = weather.upper_wind_velocity_m_s;
        if sea.length() < 1.0 || upper.length() < 1.0 {
            continue;
        }

        sea_speed_sum += sea.length();
        upper_speed_sum += upper.length();
        alignment_sum += sea.normalize().dot(upper.normalize());
        sheared_samples += usize::from((sea - upper).length() > 0.01);
        samples += 1;
    }

    assert!(samples > 900);
    assert!(upper_speed_sum > sea_speed_sum);
    let average_alignment = alignment_sum / samples as f32;
    assert!(average_alignment > 0.9, "{average_alignment}");
    assert!(sheared_samples > samples * 9 / 10);
}

#[test]
fn procedural_terrain_is_seeded_and_continuous() {
    let seed = 0x5eed_u64;
    let position_m = Vector::new(12.0, -7.0);
    let elevation_m = procedural_terrain_elevation_m(seed, position_m);

    assert_eq!(
        elevation_m,
        procedural_terrain_elevation_m(seed, position_m)
    );
    assert!(elevation_m.is_finite());
    assert!(
        (procedural_terrain_elevation_m(seed, position_m + Vector::new(0.1, 0.1)) - elevation_m)
            .abs()
            < 0.2
    );
    assert!(
        (procedural_terrain_elevation_m(seed, position_m + Vector::new(64.0, 0.0)) - elevation_m)
            .abs()
            > 0.01
    );
    assert!((procedural_terrain_elevation_m(seed + 1, position_m) - elevation_m).abs() > 0.01);
    assert_eq!(
        procedural_match_environment(seed, 600, position_m).water_depth_m,
        (-elevation_m).max(0.0)
    );
}

#[test]
fn procedural_terrain_forms_sparse_clusters_of_large_islands() {
    let mut minimum_elevation_m = f32::MAX;
    let mut maximum_elevation_m = f32::MIN;
    let mut land_samples = 0;
    let mut samples = 0;
    let mut land_samples_with_distant_land = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        let land_samples_before_seed = land_samples;
        let samples_before_seed = samples;
        for x in -128..=128 {
            for y in -128..=128 {
                let position_m = Vector::new(x as f32 * 128.0, y as f32 * 128.0);
                let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                minimum_elevation_m = minimum_elevation_m.min(elevation_m);
                maximum_elevation_m = maximum_elevation_m.max(elevation_m);
                if elevation_m >= 0.0 {
                    land_samples += 1;
                    land_samples_with_distant_land += usize::from(
                        procedural_terrain_elevation_m(seed, position_m + Vector::new(512.0, 0.0))
                            >= 0.0,
                    );
                }
                samples += 1;
            }
        }
        let seed_land_samples = land_samples - land_samples_before_seed;
        let seed_samples = samples - samples_before_seed;
        assert!(
            seed_land_samples < seed_samples / 5,
            "seed {seed} had {seed_land_samples} land samples in a {seed_samples}-sample sea region"
        );
    }

    assert!(minimum_elevation_m < -4.0);
    assert!(maximum_elevation_m > 2.0);
    assert!(
        (samples / 20..=samples * 3 / 20).contains(&land_samples),
        "land occupied {} of {samples} samples",
        land_samples
    );
    assert!(
        land_samples_with_distant_land * 2 > land_samples,
        "only {land_samples_with_distant_land} of {land_samples} land samples remained land 512 m away"
    );
}

#[test]
fn procedural_coasts_naturally_mix_sharp_outcrops_and_navigable_water() {
    let mut mixed_coastal_patches = 0;
    let mut deep_access_patches = 0;
    let mut maximum_slope: f32 = 0.0;
    let mut relief_32_m = Vec::new();
    let mut relief_128_m = Vec::new();

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for patch_x in -24..=24 {
            for patch_y in -24..=24 {
                let mut has_outcrop = false;
                let mut has_navigable_water = false;
                let mut deep_water = [[false; 8]; 8];
                let mut shallow_water_samples = 0;
                for sample_x in 0..8 {
                    for sample_y in 0..8 {
                        let position_m = Vector::new(
                            patch_x as f32 * 256.0 + (sample_x as f32 - 3.5) * 32.0,
                            patch_y as f32 * 256.0 + (sample_y as f32 - 3.5) * 32.0,
                        );
                        let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                        for (distance_m, relief) in
                            [(32.0, &mut relief_32_m), (128.0, &mut relief_128_m)]
                        {
                            let nearby_elevation_m = procedural_terrain_elevation_m(
                                seed,
                                position_m + Vector::new(distance_m, 0.0),
                            );
                            if (-4.0..=1.0).contains(&elevation_m)
                                && (-4.0..=1.0).contains(&nearby_elevation_m)
                            {
                                relief.push((nearby_elevation_m - elevation_m).abs());
                            }
                        }
                        has_outcrop |= elevation_m >= 0.25;
                        has_navigable_water |= elevation_m <= -1.5;
                        deep_water[sample_x][sample_y] = elevation_m <= -4.0;
                        shallow_water_samples += usize::from((-2.0..=-0.2).contains(&elevation_m));
                        if elevation_m >= 0.0 {
                            let gradient = Vector::new(
                                procedural_terrain_elevation_m(
                                    seed,
                                    position_m + Vector::new(2.0, 0.0),
                                ) - procedural_terrain_elevation_m(
                                    seed,
                                    position_m - Vector::new(2.0, 0.0),
                                ),
                                procedural_terrain_elevation_m(
                                    seed,
                                    position_m + Vector::new(0.0, 2.0),
                                ) - procedural_terrain_elevation_m(
                                    seed,
                                    position_m - Vector::new(0.0, 2.0),
                                ),
                            ) / 4.0;
                            maximum_slope = maximum_slope.max(gradient.length());
                        }
                    }
                }
                if has_outcrop && has_navigable_water && shallow_water_samples >= 8 {
                    mixed_coastal_patches += 1;
                    let has_deep_access = (0..8).any(|across| {
                        (0..8).all(|along| deep_water[across][along])
                            || (0..8).all(|along| deep_water[along][across])
                    });
                    deep_access_patches += usize::from(has_deep_access);
                }
            }
        }
    }

    assert!(
        mixed_coastal_patches >= 48,
        "only {mixed_coastal_patches} mixed coastal patches"
    );
    assert!(maximum_slope > 0.15, "maximum slope was {maximum_slope}");
    relief_32_m.sort_by(f32::total_cmp);
    relief_128_m.sort_by(f32::total_cmp);
    let relief_32_m_p90 = relief_32_m[relief_32_m.len() * 9 / 10];
    let relief_128_m_p90 = relief_128_m[relief_128_m.len() * 9 / 10];
    assert!(
        relief_32_m_p90 <= 1.94,
        "32 m relief p90 was {relief_32_m_p90} m"
    );
    assert!(
        relief_128_m_p90 <= 2.92,
        "128 m relief p90 was {relief_128_m_p90} m"
    );
    assert!(deep_access_patches > 0);
    assert!(
        deep_access_patches * 4 < mixed_coastal_patches,
        "{deep_access_patches} of {mixed_coastal_patches} reef patches had deep access"
    );
}

#[test]
fn procedural_shorelines_do_not_fragment_at_reef_scale() {
    let mut coastal_transects = 0;
    let mut shoreline_crossings = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for patch_x in -24..=24 {
            for patch_y in -24..=24 {
                let start_m = Vector::new(patch_x as f32 * 256.0 - 128.0, patch_y as f32 * 256.0);
                let mut previous_is_land = procedural_terrain_elevation_m(seed, start_m) >= 0.0;
                let mut crossings = 0;
                for sample in 1..=64 {
                    let is_land = procedural_terrain_elevation_m(
                        seed,
                        start_m + Vector::new(sample as f32 * 4.0, 0.0),
                    ) >= 0.0;
                    crossings += usize::from(is_land != previous_is_land);
                    previous_is_land = is_land;
                }
                if crossings > 0 {
                    coastal_transects += 1;
                    shoreline_crossings += crossings;
                }
            }
        }
    }

    assert!(coastal_transects > 0);
    assert!(
        shoreline_crossings <= coastal_transects * 2,
        "{shoreline_crossings} crossings fragmented {coastal_transects} coastal transects"
    );
}

#[test]
fn procedural_islands_mix_gentle_beaches_with_steep_cliff_coasts() {
    let mut gentle_coast_points = 0;
    let mut cliff_coast_points = 0;
    let mut regional_coast_points = HashMap::<(u64, i32, i32), [usize; 2]>::new();

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -192_i32..=192 {
            for y in -192_i32..=192 {
                let position_m = Vector::new(x as f32 * 32.0, y as f32 * 32.0);
                let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                if !(0.0..=1.0).contains(&elevation_m) {
                    continue;
                }
                let cardinal_directions = [
                    Vector::new(1.0, 0.0),
                    Vector::new(0.0, 1.0),
                    Vector::new(-1.0, 0.0),
                    Vector::new(0.0, -1.0),
                ];
                let touches_water = cardinal_directions.iter().any(|direction| {
                    procedural_terrain_elevation_m(seed, position_m + direction * 16.0) < 0.0
                });
                let belongs_to_island = cardinal_directions.iter().any(|direction| {
                    procedural_terrain_elevation_m(seed, position_m + direction * 64.0) >= 0.0
                });
                if !touches_water || !belongs_to_island {
                    continue;
                }
                let gradient = Vector::new(
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(2.0, 0.0))
                        - procedural_terrain_elevation_m(seed, position_m - Vector::new(2.0, 0.0)),
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(0.0, 2.0))
                        - procedural_terrain_elevation_m(seed, position_m - Vector::new(0.0, 2.0)),
                ) / 4.0;
                let regional_counts = regional_coast_points
                    .entry((seed, x.div_euclid(64), y.div_euclid(64)))
                    .or_default();
                if gradient.length() >= 0.08 {
                    cliff_coast_points += 1;
                    regional_counts[1] += 1;
                } else {
                    gentle_coast_points += 1;
                    regional_counts[0] += 1;
                }
            }
        }
    }

    let coast_points = gentle_coast_points + cliff_coast_points;
    assert!(coast_points > 0);
    assert!(
        cliff_coast_points * 10 >= coast_points,
        "only {cliff_coast_points} of {coast_points} coast points were cliffs"
    );
    assert!(
        gentle_coast_points * 2 > coast_points,
        "only {gentle_coast_points} of {coast_points} coast points were beaches"
    );
    let beachless_regions = regional_coast_points
        .values()
        .filter(|counts| counts[1] >= 8 && counts[0] == 0)
        .count();
    let mixed_regions = regional_coast_points
        .values()
        .filter(|counts| {
            let total = counts[0] + counts[1];
            total >= 8 && counts[0] * 4 >= total && counts[1] * 4 >= total
        })
        .count();
    let beach_regions = regional_coast_points
        .values()
        .filter(|counts| counts[0] >= 8 && counts[1] * 10 < counts[0] + counts[1])
        .count();
    assert!(
        beachless_regions > 0,
        "no sampled coastal region was beachless"
    );
    assert!(
        mixed_regions > 0,
        "no sampled coastal region mixed cliffs and beaches"
    );
    assert!(
        beach_regions > 0,
        "no sampled coastal region was beach-dominant"
    );
}

#[test]
fn procedural_cliffs_continue_below_the_waterline() {
    let mut cliff_crossings = 0;
    let mut submerged_cliff_crossings = 0;
    let cardinal_directions = [
        Vector::new(1.0, 0.0),
        Vector::new(0.0, 1.0),
        Vector::new(-1.0, 0.0),
        Vector::new(0.0, -1.0),
    ];

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -128..=128 {
            for y in -128..=128 {
                let shore_m = Vector::new(x as f32 * 64.0, y as f32 * 64.0);
                let shore_elevation_m = procedural_terrain_elevation_m(seed, shore_m);
                if !(0.0..=2.0).contains(&shore_elevation_m) {
                    continue;
                }

                for direction in cardinal_directions {
                    let water_elevation_m =
                        procedural_terrain_elevation_m(seed, shore_m + direction * 32.0);
                    if water_elevation_m >= 0.0 {
                        continue;
                    }
                    let inland_elevation_m =
                        procedural_terrain_elevation_m(seed, shore_m - direction * 32.0);
                    let offshore_elevation_m =
                        procedural_terrain_elevation_m(seed, shore_m + direction * 64.0);
                    let land_slope = (inland_elevation_m - shore_elevation_m) / 32.0;
                    if land_slope < 0.08 {
                        continue;
                    }

                    cliff_crossings += 1;
                    let submerged_slope = (water_elevation_m - offshore_elevation_m) / 32.0;
                    submerged_cliff_crossings += usize::from(submerged_slope >= 0.08);
                }
            }
        }
    }

    assert!(cliff_crossings > 0, "no sampled shoreline crossed a cliff");
    assert!(
        submerged_cliff_crossings * 2 >= cliff_crossings,
        "only {submerged_cliff_crossings} of {cliff_crossings} cliffs continued underwater"
    );
}

#[test]
fn procedural_coasts_often_reach_navigable_depth_within_256_metres() {
    let mut sea_facing_shores = 0;
    let mut nearby_navigable_water = 0;
    let cardinal_directions = [
        Vector::new(1.0, 0.0),
        Vector::new(0.0, 1.0),
        Vector::new(-1.0, 0.0),
        Vector::new(0.0, -1.0),
    ];

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -128..=128 {
            for y in -128..=128 {
                let shore_m = Vector::new(x as f32 * 64.0, y as f32 * 64.0);
                let shore_elevation_m = procedural_terrain_elevation_m(seed, shore_m);
                if !(0.0..=2.0).contains(&shore_elevation_m) {
                    continue;
                }

                for direction in cardinal_directions {
                    if procedural_terrain_elevation_m(seed, shore_m + direction * 32.0) >= 0.0 {
                        continue;
                    }
                    sea_facing_shores += 1;
                    nearby_navigable_water += usize::from(
                        procedural_terrain_elevation_m(seed, shore_m + direction * 256.0) <= -4.0,
                    );
                }
            }
        }
    }

    assert!(sea_facing_shores > 0);
    assert!(
        nearby_navigable_water * 2 >= sea_facing_shores,
        "only {nearby_navigable_water} of {sea_facing_shores} shoreline transects reached 4 m depth within 256 m"
    );
}

#[test]
fn procedural_seafloor_has_rare_deep_trenches() {
    let mut water_samples = 0;
    let mut trench_samples = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -128..=128 {
            for y in -128..=128 {
                let elevation_m = procedural_terrain_elevation_m(
                    seed,
                    Vector::new(x as f32 * 128.0, y as f32 * 128.0),
                );
                if elevation_m < 0.0 {
                    water_samples += 1;
                    trench_samples += usize::from(elevation_m <= -100.0);
                }
            }
        }
    }

    assert!(
        trench_samples > 0,
        "no sampled seafloor reached 100 m depth"
    );
    assert!(
        trench_samples * 20 < water_samples,
        "{trench_samples} of {water_samples} water samples were trench depth"
    );
}
