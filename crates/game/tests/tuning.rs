use std::collections::{HashMap, VecDeque};

use game::{
    AtmospherePrimary, CelestialConditions, ClimateConditions, TidalConditions, celestial_lighting,
    celestial_lighting_at_height, climate_surface, current_500_hpa_air_temperature_c,
    current_air_temperature_c, derive_weather, dinghy_parameters, match_environment, moon_phase,
    procedural_atmosphere_primary as procedural_atmosphere_primary_in_world,
    procedural_cloud_cover as procedural_cloud_cover_in_world,
    procedural_match_environment as procedural_match_environment_in_world,
    procedural_solid_surface_elevation_m as procedural_solid_surface_elevation_m_in_world,
    procedural_terrain_elevation_m as procedural_terrain_elevation_m_in_world,
    procedural_unsheltered_wind_velocity_m_s as procedural_unsheltered_wind_velocity_m_s_in_world,
    procedural_water_depth_m as procedural_water_depth_m_in_world,
    procedural_weather as procedural_weather_in_world, tide_height_m,
    world_celestial_conditions as world_celestial_conditions_in_world, world_climate_conditions,
};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

/// Seeds of a terrain population. Terrain statistics vary strongly between
/// worlds, so they are measured over a population. Bounds are set from the
/// statistic of the terrain the original thresholds were tuned on (commit
/// 2f3a2fa), measured with the same test code. Occupancy and region shares may
/// change by up to a factor of two, the change that turns sparse islands into a
/// patchwork or into isolated rocks. Relief at the 90th percentile may fall by up
/// to a quarter: these statistics lose about half their value when the relief
/// term they test is removed, so a quarter is about half of that term's
/// amplitude.
fn terrain_population_seeds(count: u64) -> impl Iterator<Item = u64> {
    (0..count).map(|index| 10 + 7_919 * index)
}

fn procedural_atmosphere_primary(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> AtmospherePrimary {
    procedural_atmosphere_primary_in_world(WorldGeometry::default(), match_seed, tick, position_m)
}

fn procedural_cloud_cover(match_seed: u64, tick: u64, position_m: Vector) -> f32 {
    procedural_cloud_cover_in_world(WorldGeometry::default(), match_seed, tick, position_m)
}

fn procedural_match_environment(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> simulation::Environment {
    procedural_match_environment_in_world(WorldGeometry::default(), match_seed, tick, position_m)
}

fn procedural_solid_surface_elevation_m(
    match_seed: u64,
    position_m: Vector,
    annual_mean_air_temperature_c: f32,
    water_temperature_c: f32,
) -> f32 {
    procedural_solid_surface_elevation_m_in_world(
        WorldGeometry::default(),
        match_seed,
        position_m,
        annual_mean_air_temperature_c,
        water_temperature_c,
    )
}

fn procedural_terrain_elevation_m(match_seed: u64, position_m: Vector) -> f32 {
    procedural_terrain_elevation_m_in_world(WorldGeometry::default(), match_seed, position_m)
}

fn procedural_unsheltered_wind_velocity_m_s(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> Vector {
    procedural_unsheltered_wind_velocity_m_s_in_world(
        WorldGeometry::default(),
        match_seed,
        tick,
        position_m,
    )
}

fn procedural_water_depth_m(match_seed: u64, world_time_seconds: f64, position_m: Vector) -> f32 {
    procedural_water_depth_m_in_world(
        WorldGeometry::default(),
        match_seed,
        world_time_seconds,
        position_m,
    )
}

fn procedural_weather(match_seed: u64, tick: u64, position_m: Vector) -> game::ProceduralWeather {
    procedural_weather_in_world(WorldGeometry::default(), match_seed, tick, position_m)
}

fn world_celestial_conditions(world_time_seconds: f64) -> CelestialConditions {
    world_celestial_conditions_in_world(
        WorldGeometry::default(),
        world_time_seconds,
        Vector::new(12.0, -7.0),
    )
}

fn derive_weather_at_surface_temperature(
    mut primary: AtmospherePrimary,
    surface_air_temperature_c: f32,
) -> game::ProceduralWeather {
    primary.surface_air_temperature_anomaly_c = 0.0;
    derive_weather(
        primary,
        ClimateConditions {
            annual_mean_air_temperature_c: surface_air_temperature_c,
            seasonal_temperature_range_c: 0.0,
            diurnal_temperature_range_c: 0.0,
            annual_mean_water_temperature_c: surface_air_temperature_c,
            seasonal_water_temperature_range_c: 0.0,
        },
        CelestialConditions {
            geographic_latitude_degrees: 45.0,
            celestial_latitude_degrees: 45.0,
            season_phase: 0.0,
            day_phase: 0.0,
            lunar_longitude_phase: 0.0,
        },
    )
}

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
    assert!(wind.true_water_velocity_m_s.is_finite());
    assert!(wind.true_water_velocity_m_s.length() > 0.01);

    let nearby = procedural_match_environment(seed, 601, position_m + Vector::new(0.1, 0.1));
    assert!((nearby.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() < 1.0);
    assert!((nearby.true_water_velocity_m_s - wind.true_water_velocity_m_s).length() < 1.0);

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
fn significant_gusts_require_mean_wind_or_moist_convection() {
    let primary = AtmospherePrimary {
        pressure_hpa: 1018.0,
        pressure_gradient_hpa_per_m: Vector::ZERO,
        synoptic_pressure_gradient_hpa_per_m: Vector::ZERO,
        surface_gust_signal: 1.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -4.0,
        local_temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 2.0,
    };

    let weather = derive_weather_at_surface_temperature(primary, 20.0);

    assert!(
        weather.wind_velocity_m_s.length() < 0.05,
        "calm stable air produced {:?} m/s wind",
        weather.wind_velocity_m_s,
    );
}

#[test]
fn mechanical_gusts_follow_and_scale_with_mean_surface_wind() {
    let weak = AtmospherePrimary {
        pressure_hpa: 1013.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0005, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.0005, 0.0),
        surface_gust_signal: 1.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -4.0,
        local_temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 2.0,
    };
    let strong = AtmospherePrimary {
        pressure_gradient_hpa_per_m: 2.0 * weak.pressure_gradient_hpa_per_m,
        synoptic_pressure_gradient_hpa_per_m: 2.0 * weak.synoptic_pressure_gradient_hpa_per_m,
        ..weak
    };
    let without_pulse = |primary: AtmospherePrimary| AtmospherePrimary {
        surface_gust_signal: 0.0,
        ..primary
    };
    let weak_weather = derive_weather_at_surface_temperature(weak, 20.0);
    let weak_mean = derive_weather_at_surface_temperature(without_pulse(weak), 20.0);
    let strong_weather = derive_weather_at_surface_temperature(strong, 20.0);
    let strong_mean = derive_weather_at_surface_temperature(without_pulse(strong), 20.0);
    let weak_gust = weak_weather.wind_velocity_m_s - weak_mean.wind_velocity_m_s;
    let strong_gust = strong_weather.wind_velocity_m_s - strong_mean.wind_velocity_m_s;

    assert!(weak_gust.dot(weak_mean.wind_velocity_m_s) > 0.0);
    assert!(strong_gust.dot(strong_mean.wind_velocity_m_s) > 0.0);
    assert!(strong_gust.length() > 1.8 * weak_gust.length());
}

#[test]
fn moist_convection_mixes_faster_upper_wind_into_surface_gusts() {
    let dry = AtmospherePrimary {
        pressure_hpa: 1013.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0005, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.0, -0.0015),
        surface_gust_signal: 1.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -4.0,
        local_temperature_gradient_c_per_m: Vector::new(0.0005, 0.0),
        water_vapor_g_kg: 2.0,
    };
    let convective = AtmospherePrimary {
        pressure_hpa: 995.0,
        air_temperature_500_hpa_c: -30.0,
        water_vapor_g_kg: 25.0,
        ..dry
    };
    let without_pulse = |primary: AtmospherePrimary| AtmospherePrimary {
        surface_gust_signal: 0.0,
        ..primary
    };
    let dry_weather = derive_weather_at_surface_temperature(dry, 30.0);
    let dry_mean = derive_weather_at_surface_temperature(without_pulse(dry), 30.0);
    let convective_weather = derive_weather_at_surface_temperature(convective, 30.0);
    let convective_mean = derive_weather_at_surface_temperature(without_pulse(convective), 30.0);
    let dry_gust = dry_weather.wind_velocity_m_s - dry_mean.wind_velocity_m_s;
    let convective_gust = convective_weather.wind_velocity_m_s - convective_mean.wind_velocity_m_s;

    assert!(convective_gust.length() > dry_gust.length());
    let upper_direction = convective_weather.upper_wind_velocity_m_s.normalize();
    assert!(
        convective_gust.dot(upper_direction) > dry_gust.dot(upper_direction),
        "convective gust {convective_gust:?} did not add momentum from upper wind {:?}",
        convective_weather.upper_wind_velocity_m_s,
    );
}

#[test]
fn surface_gust_activity_varies_between_weather_regions() {
    let world = WorldGeometry::default();
    let mut minimum_rms = f32::MAX;
    let mut maximum_rms = 0.0_f32;

    for region_y in 0..4 {
        for region_x in 0..8 {
            let center = Vector::new(
                (region_x as f32 / 8.0 - 0.4375) * world.width_m(),
                (region_y as f32 / 4.0 - 0.375) * world.height_m(),
            );
            let mut square_speed_sum = 0.0;
            for sample_y in 0..16 {
                for sample_x in 0..16 {
                    let offset = Vector::new(
                        (sample_x as f32 - 7.5) * 64.0,
                        (sample_y as f32 - 7.5) * 64.0,
                    );
                    let gust = procedural_weather_in_world(world, 0x5eed, 12_345, center + offset)
                        .surface_gust_velocity_m_s;
                    square_speed_sum += gust.length_squared();
                }
            }
            let rms = (square_speed_sum / 256.0).sqrt();
            minimum_rms = minimum_rms.min(rms);
            maximum_rms = maximum_rms.max(rms);
        }
    }

    assert!(
        maximum_rms > 1.5 * minimum_rms,
        "regional gust RMS range was {minimum_rms}..{maximum_rms} m/s",
    );
}

#[test]
fn resolved_pressure_is_dominated_by_broad_structure() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(12.0, 0.25 * world.height_m());
    let mut anomaly_square_sum = 0.0;
    let mut local_difference_square_sum = 0.0;

    for seed in 0..4_096 {
        let pressure_hpa =
            procedural_atmosphere_primary_in_world(world, seed, 12_345, position_m).pressure_hpa;
        let nearby_pressure_hpa = procedural_atmosphere_primary_in_world(
            world,
            seed,
            12_345,
            position_m + Vector::new(128.0, 0.0),
        )
        .pressure_hpa;
        anomaly_square_sum += (pressure_hpa - 1013.25).powi(2);
        local_difference_square_sum += (nearby_pressure_hpa - pressure_hpa).powi(2);
    }

    let anomaly_rms_hpa = (anomaly_square_sum / 4_096.0).sqrt();
    let local_difference_rms_hpa = (local_difference_square_sum / 4_096.0).sqrt();
    assert!(anomaly_rms_hpa > 0.5, "pressure RMS {anomaly_rms_hpa} hPa");
    assert!(
        local_difference_rms_hpa < 0.15 * anomaly_rms_hpa,
        "128 m pressure difference RMS {local_difference_rms_hpa} hPa vs anomaly RMS {anomaly_rms_hpa} hPa",
    );
}

#[test]
fn procedural_wind_has_a_local_prevailing_direction_without_a_worldwide_one() {
    let position_m = Vector::new(12.0, -7.0);
    let mut local_alignment_sum = 0.0;
    let mut regional_alignment_sum = 0.0;
    let mut local_samples = 0;
    let mut regional_samples = 0;

    for seed in 0..4_096 {
        let wind = procedural_unsheltered_wind_velocity_m_s(seed, 12_345, position_m);
        let local = procedural_unsheltered_wind_velocity_m_s(
            seed,
            12_345,
            position_m + Vector::new(128.0, 0.0),
        );
        let regional = procedural_unsheltered_wind_velocity_m_s(
            seed,
            12_345,
            position_m + Vector::new(32_000.0, 0.0),
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

    assert!(local_samples > 3_200, "local samples {local_samples}");
    assert!(
        regional_samples > 3_000,
        "regional samples {regional_samples}"
    );
    assert!(local_alignment_sum / local_samples as f32 > 0.85);
    assert!((regional_alignment_sum / regional_samples as f32).abs() < 0.25);
}

#[test]
fn one_world_contains_distinct_coherent_wind_systems() {
    let world = WorldGeometry::default();
    let mut direction_bins = [false; 8];
    let mut velocity_sum = Vector::ZERO;
    let mut speed_sum = 0.0;
    let mut minimum_speed = f32::MAX;
    let mut maximum_speed = 0.0_f32;
    let mut neighboring_alignment_sum = 0.0;
    let mut neighboring_alignment_count = 0;

    for y in 0..16 {
        for x in 0..16 {
            let position_m = Vector::new(
                (x as f32 / 16.0 - 0.5) * world.width_m(),
                (y as f32 / 16.0 - 0.5) * world.height_m(),
            );
            let wind = procedural_unsheltered_wind_velocity_m_s_in_world(
                world, 0x5eed, 12_345, position_m,
            );
            let speed = wind.length();
            velocity_sum += wind;
            speed_sum += speed;
            minimum_speed = minimum_speed.min(speed);
            maximum_speed = maximum_speed.max(speed);
            if speed >= 2.0 {
                let angle = wind.y.atan2(wind.x).rem_euclid(std::f32::consts::TAU);
                let bin = (8.0 * angle / std::f32::consts::TAU).floor() as usize % 8;
                direction_bins[bin] = true;

                let neighboring = procedural_unsheltered_wind_velocity_m_s_in_world(
                    world,
                    0x5eed,
                    12_345,
                    position_m + Vector::new(128.0, 0.0),
                );
                if neighboring.length() >= 2.0 {
                    neighboring_alignment_sum += wind.normalize().dot(neighboring.normalize());
                    neighboring_alignment_count += 1;
                }
            }
        }
    }

    assert!(
        direction_bins
            .into_iter()
            .filter(|occupied| *occupied)
            .count()
            >= 6
    );
    assert!(
        maximum_speed - minimum_speed > 12.0,
        "speed range was {}",
        maximum_speed - minimum_speed,
    );
    assert!(velocity_sum.length() < 0.35 * speed_sum);
    assert!(neighboring_alignment_count > 192);
    assert!(
        neighboring_alignment_sum / neighboring_alignment_count as f32 > 0.5,
        "128 m alignment was {}",
        neighboring_alignment_sum / neighboring_alignment_count as f32,
    );
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
    assert!(
        distant_change / 512.0 > 0.1,
        "mean distant density change was {}",
        distant_change / 512.0
    );
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
        storm_samples * 50 < samples,
        "{storm_samples} of {samples} samples were storm density"
    );
}

#[test]
fn pressure_gradient_drives_nearly_isobaric_surface_wind() {
    let primary = AtmospherePrimary {
        pressure_hpa: 1000.0,
        pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        surface_gust_signal: 0.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -15.0,
        local_temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 8.0,
    };

    let wind = derive_weather_at_surface_temperature(primary, 20.0).wind_velocity_m_s;

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
fn coriolis_reverses_pressure_driven_circulation_between_hemispheres() {
    let primary = AtmospherePrimary {
        pressure_hpa: 1000.0,
        pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.001, 0.0),
        surface_gust_signal: 0.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -15.0,
        local_temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 8.0,
    };
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: 20.0,
        seasonal_temperature_range_c: 0.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: 20.0,
        seasonal_water_temperature_range_c: 0.0,
    };
    let wind_at = |latitude| {
        derive_weather(
            primary,
            climate,
            CelestialConditions {
                geographic_latitude_degrees: latitude,
                celestial_latitude_degrees: latitude,
                season_phase: 0.0,
                day_phase: 0.0,
                lunar_longitude_phase: 0.0,
            },
        )
        .wind_velocity_m_s
    };

    let north = wind_at(45.0);
    let equator = wind_at(0.0);
    let south = wind_at(-45.0);

    assert!(north.y > 5.0, "northern wind was {north:?}");
    assert!(south.y < -5.0, "southern wind was {south:?}");
    assert!(equator.x < -5.0, "equatorial wind was {equator:?}");
    assert!((north.length() - south.length()).abs() < 1.0e-5);
    assert!((north.length() - equator.length()).abs() < 1.0e-5);
}

#[test]
fn horizontal_temperature_gradient_produces_vertical_wind_shear() {
    let calm_aloft = AtmospherePrimary {
        pressure_hpa: 1000.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0008, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.0008, 0.0),
        surface_gust_signal: 0.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -15.0,
        local_temperature_gradient_c_per_m: Vector::ZERO,
        water_vapor_g_kg: 10.0,
    };
    let sheared = AtmospherePrimary {
        local_temperature_gradient_c_per_m: Vector::new(0.0004, 0.0),
        ..calm_aloft
    };

    let calm_weather = derive_weather_at_surface_temperature(calm_aloft, 24.0);
    let sheared_weather = derive_weather_at_surface_temperature(sheared, 24.0);

    assert_eq!(
        calm_weather.wind_velocity_m_s,
        sheared_weather.wind_velocity_m_s
    );
    assert!(
        (sheared_weather.upper_wind_velocity_m_s - calm_weather.upper_wind_velocity_m_s).length()
            > 0.3
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
        surface_gust_signal: 0.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -20.0,
        local_temperature_gradient_c_per_m: Vector::new(0.0003, 0.0),
        water_vapor_g_kg: 4.0,
    };
    let moist = AtmospherePrimary {
        water_vapor_g_kg: 28.0,
        ..dry
    };

    let dry_weather = derive_weather_at_surface_temperature(dry, 28.0);
    let moist_weather = derive_weather_at_surface_temperature(moist, 28.0);

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
    assert!((-15.0..=15.0).contains(&primary.surface_air_temperature_anomaly_c));
    assert!((-80.0..=10.0).contains(&primary.air_temperature_500_hpa_c));
    assert!((0.0..=25.0).contains(&primary.water_vapor_g_kg));

    let nearby = procedural_atmosphere_primary(seed, tick + 1, position_m + Vector::new(1.0, 0.0));
    assert!((nearby.pressure_hpa - primary.pressure_hpa).abs() < 0.1);
    assert!(
        (nearby.surface_air_temperature_anomaly_c - primary.surface_air_temperature_anomaly_c)
            .abs()
            < 0.1
    );
    assert!((nearby.water_vapor_g_kg - primary.water_vapor_g_kg).abs() < 0.1);
}

#[test]
fn temperature_anomaly_probabilities_follow_observed_latitude_bands() {
    let world = WorldGeometry::default();
    let percentile = |latitude: f32, probability: f32| {
        let position_m = Vector::new(0.0, latitude / 180.0 * world.height_m());
        let mut samples = (0..2_048)
            .map(|seed| {
                procedural_atmosphere_primary_in_world(world, seed, 0, position_m)
                    .surface_air_temperature_anomaly_c
            })
            .collect::<Vec<_>>();
        samples.sort_by(f32::total_cmp);
        samples[((samples.len() - 1) as f32 * probability).round() as usize]
    };

    for (latitude, expected_tail_c) in [(10.0, 5.4), (40.0, 14.0), (50.0, 16.2), (70.0, 13.3)] {
        let cold = percentile(latitude, 0.01);
        let hot = percentile(latitude, 0.99);
        assert!((cold + expected_tail_c).abs() < 1.5, "{latitude}: {cold}");
        assert!((hot - expected_tail_c).abs() < 1.5, "{latitude}: {hot}");
    }
}

#[test]
fn temperate_temperature_extremes_have_observed_probabilities() {
    let world = WorldGeometry::default();
    let latitude = 50.0;
    let position_m = Vector::new(0.0, latitude / 180.0 * world.height_m());
    let climate = world_climate_conditions(latitude);
    let temperature_samples = |season_phase, day_phase| {
        (0..4_096)
            .map(|seed| {
                let anomaly = procedural_atmosphere_primary_in_world(world, seed, 0, position_m)
                    .surface_air_temperature_anomaly_c;
                current_air_temperature_c(
                    climate,
                    CelestialConditions {
                        geographic_latitude_degrees: latitude,
                        celestial_latitude_degrees: latitude,
                        season_phase,
                        day_phase,
                        lunar_longitude_phase: 0.0,
                    },
                ) + anomaly
            })
            .collect::<Vec<_>>()
    };
    let winter_highs = temperature_samples(0.09, 0.625);
    let summer_highs = temperature_samples(0.59, 0.625);
    let frozen_winter_days = winter_highs
        .iter()
        .filter(|temperature| **temperature < 0.0)
        .count();
    let hot_summer_days = summer_highs
        .iter()
        .filter(|temperature| **temperature >= 35.0)
        .count();
    let very_hot_summer_days = summer_highs
        .iter()
        .filter(|temperature| **temperature >= 40.0)
        .count();

    assert!((0.30..=0.50).contains(&(frozen_winter_days as f32 / winter_highs.len() as f32)));
    assert!((0.02..=0.08).contains(&(hot_summer_days as f32 / summer_highs.len() as f32)));
    assert!((1..=20).contains(&very_hot_summer_days));
}

#[test]
fn procedural_local_temperature_gradient_matches_the_equatorial_anomaly_derivative() {
    const SAMPLE_DISTANCE_M: f32 = 16.0;
    const SAMPLE_COUNT: u64 = 128;
    let mut error_sum = 0.0;

    for seed in 0..SAMPLE_COUNT {
        let position_m = Vector::new(371.0 * seed as f32 + 12.0, 0.0);
        let primary = procedural_atmosphere_primary(seed, 12_345, position_m);
        let numerical_gradient = Vector::new(
            procedural_atmosphere_primary(
                seed,
                12_345,
                position_m + Vector::new(SAMPLE_DISTANCE_M, 0.0),
            )
            .surface_air_temperature_anomaly_c
                - procedural_atmosphere_primary(
                    seed,
                    12_345,
                    position_m - Vector::new(SAMPLE_DISTANCE_M, 0.0),
                )
                .surface_air_temperature_anomaly_c,
            procedural_atmosphere_primary(
                seed,
                12_345,
                position_m + Vector::new(0.0, SAMPLE_DISTANCE_M),
            )
            .surface_air_temperature_anomaly_c
                - procedural_atmosphere_primary(
                    seed,
                    12_345,
                    position_m - Vector::new(0.0, SAMPLE_DISTANCE_M),
                )
                .surface_air_temperature_anomaly_c,
        ) / (2.0 * SAMPLE_DISTANCE_M);
        error_sum += (primary.local_temperature_gradient_c_per_m - numerical_gradient).length();
    }

    let mean_error = error_sum / SAMPLE_COUNT as f32;
    assert!(mean_error < 2.5e-7, "mean gradient error was {mean_error}");
}

#[test]
fn spatiotemporal_local_temperature_gradient_preserves_metre_scale_variation_after_years() {
    const SAMPLE_COUNT: u64 = 128;
    let tick = 1_u64 << 32;
    let position_m = Vector::new(12.0, 0.0);
    let displacement_m = Vector::new(1.0, 0.0);
    let mut derivative_error = 0.0;

    for seed in 0..SAMPLE_COUNT {
        let primary = procedural_atmosphere_primary(seed, tick, position_m);
        let nearby_temperature =
            procedural_atmosphere_primary(seed, tick, position_m + displacement_m)
                .surface_air_temperature_anomaly_c;
        let predicted_change = primary
            .local_temperature_gradient_c_per_m
            .dot(displacement_m);
        derivative_error +=
            (nearby_temperature - primary.surface_air_temperature_anomaly_c - predicted_change)
                .abs();
    }

    let mean_error = derivative_error / SAMPLE_COUNT as f32;
    assert!(
        mean_error < 1.0e-6,
        "mean nearby derivative error was {mean_error}"
    );
}

#[test]
fn surface_and_upper_air_share_an_air_mass_anomaly_without_becoming_identical() {
    const SAMPLE_COUNT: u64 = 512;
    let tick = 12_345;
    let conditions = world_celestial_conditions(tick as f64 / 60.0);
    let climatology = current_500_hpa_air_temperature_c(
        world_climate_conditions(conditions.geographic_latitude_degrees),
        conditions,
    );
    let mut surface_sum = 0.0;
    let mut upper_sum = 0.0;
    let mut surface_square_sum = 0.0;
    let mut upper_square_sum = 0.0;
    let mut product_sum = 0.0;

    for seed in 0..SAMPLE_COUNT {
        let primary = procedural_atmosphere_primary(seed, tick, Vector::new(12.0, -7.0));
        let surface = f64::from(primary.surface_air_temperature_anomaly_c);
        let upper = f64::from(primary.air_temperature_500_hpa_c - climatology);
        surface_sum += surface;
        upper_sum += upper;
        surface_square_sum += surface * surface;
        upper_square_sum += upper * upper;
        product_sum += surface * upper;
    }

    let count = SAMPLE_COUNT as f64;
    let covariance = product_sum - surface_sum * upper_sum / count;
    let surface_variance = surface_square_sum - surface_sum * surface_sum / count;
    let upper_variance = upper_square_sum - upper_sum * upper_sum / count;
    let correlation = covariance / libm::sqrt(surface_variance * upper_variance);

    assert!(
        (0.25..0.90).contains(&correlation),
        "correlation was {correlation}"
    );
}

#[test]
fn procedural_weather_applies_climate_and_celestial_temperature_to_the_weather_anomaly() {
    let seed = 0x5eed;
    let tick = 12_345;
    let position_m = Vector::new(12.0, -7.0);
    let primary = procedural_atmosphere_primary(seed, tick, position_m);
    let conditions = world_celestial_conditions(tick as f64 / 60.0);
    let climate = world_climate_conditions(conditions.geographic_latitude_degrees);
    let expected_surface_air_temperature_c =
        current_air_temperature_c(climate, conditions) + primary.surface_air_temperature_anomaly_c;
    let expected = derive_weather(primary, climate, conditions);

    assert_eq!(procedural_weather(seed, tick, position_m), expected);
    assert_eq!(
        expected.surface_air_temperature_c,
        expected_surface_air_temperature_c
    );
}

#[test]
fn celestial_cycle_places_equinox_sun_overhead_at_equatorial_noon() {
    let noon = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.25,
        day_phase: 0.5,
        lunar_longitude_phase: 0.25,
    });
    let midnight = celestial_lighting(CelestialConditions {
        day_phase: 0.0,
        ..CelestialConditions {
            geographic_latitude_degrees: 0.0,
            celestial_latitude_degrees: 0.0,
            season_phase: 0.25,
            day_phase: 0.5,
            lunar_longitude_phase: 0.25,
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
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.5,
        day_phase: 0.0,
        lunar_longitude_phase: 0.75,
    });
    let new_moon_midnight = celestial_lighting(CelestialConditions {
        lunar_longitude_phase: 0.25,
        ..CelestialConditions {
            geographic_latitude_degrees: 0.0,
            celestial_latitude_degrees: 0.0,
            season_phase: 0.5,
            day_phase: 0.0,
            lunar_longitude_phase: 0.75,
        }
    });

    assert!(full_moon_midnight.moon_direction[2] > 0.85);
    assert!(full_moon_midnight.moon_intensity > 0.09);
    assert_eq!(new_moon_midnight.moon_intensity, 0.0);
    assert!(full_moon_midnight.ambient_intensity > new_moon_midnight.ambient_intensity);
}

#[test]
fn upper_atmosphere_afterglow_outlasts_lower_twilight() {
    let ambient_at_solar_depression = |degrees: f32| {
        celestial_lighting(CelestialConditions {
            geographic_latitude_degrees: 0.0,
            celestial_latitude_degrees: 0.0,
            season_phase: 0.25,
            day_phase: 0.75 + degrees / 360.0,
            lunar_longitude_phase: 0.0,
        })
        .ambient_intensity
    };

    let civil_twilight = ambient_at_solar_depression(6.0);
    let nautical_twilight = ambient_at_solar_depression(12.0);
    let astronomical_night = ambient_at_solar_depression(18.0);

    assert!(civil_twilight > nautical_twilight);
    assert!(nautical_twilight > astronomical_night);
    assert!((astronomical_night - 0.035).abs() < 1e-6);
}

#[test]
fn direct_sun_uses_the_finite_disc_surface_horizon() {
    let angular_radius_rad = 0.00465_f32;
    let lighting = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.25,
        day_phase: 0.25 + 1.1 * angular_radius_rad / std::f32::consts::TAU,
        lunar_longitude_phase: 0.5,
    });

    assert!(lighting.sun_direction[2] > angular_radius_rad);
    assert!(lighting.sun_intensity > 0.35, "{}", lighting.sun_intensity);
}

#[test]
fn receiver_height_changes_lunar_parallax_and_solar_overlap() {
    let world = WorldGeometry::default();
    let horizon_conditions = CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.25,
        day_phase: 0.25,
        lunar_longitude_phase: 0.5,
    };
    let surface = celestial_lighting_at_height(world, horizon_conditions, 0.0);
    let canopy = celestial_lighting_at_height(world, horizon_conditions, 3_600.0);
    let below_datum = celestial_lighting_at_height(world, horizon_conditions, -900.0);
    assert!(
        surface
            .moon_direction
            .into_iter()
            .zip(below_datum.moon_direction)
            .any(|(surface, below)| (surface - below).abs() > 0.0001),
    );
    let direction_dot = surface
        .moon_direction
        .into_iter()
        .zip(canopy.moon_direction)
        .map(|(left, right)| left * right)
        .sum::<f32>()
        .clamp(-1.0, 1.0);
    let direction_change = direction_dot.acos();
    assert!(direction_change > 0.00452, "{direction_change}");

    let solar_node = 0.5104333;
    let eclipse_conditions = CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: (solar_node + 0.25) % 1.0,
        day_phase: 0.5 + 24.0 / 360.0,
        lunar_longitude_phase: solar_node,
    };
    let surface = celestial_lighting_at_height(world, eclipse_conditions, 0.0);
    let canopy = celestial_lighting_at_height(world, eclipse_conditions, 3_600.0);
    assert!(
        surface.solar_eclipse_fraction > 0.10,
        "{}",
        surface.solar_eclipse_fraction,
    );
    assert_eq!(canopy.solar_eclipse_fraction, 0.0);
}

#[test]
fn lunar_orbit_tilt_limits_eclipses_to_node_crossings() {
    let solar_node = 0.5104333;
    let off_node = (solar_node + 0.25) % 1.0;
    let new_moon_at_node = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: (solar_node + 0.25) % 1.0,
        day_phase: 0.5,
        lunar_longitude_phase: solar_node,
    });
    let new_moon_off_node = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: (off_node + 0.25) % 1.0,
        day_phase: 0.5,
        lunar_longitude_phase: off_node,
    });
    let full_moon_at_node = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: (solar_node + 0.25) % 1.0,
        day_phase: 0.0,
        lunar_longitude_phase: (solar_node + 0.5) % 1.0,
    });
    let full_moon_off_node = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: (off_node + 0.25) % 1.0,
        day_phase: 0.0,
        lunar_longitude_phase: (off_node + 0.5) % 1.0,
    });
    let direction_dot = |left: [f32; 3], right: [f32; 3]| {
        left.into_iter().zip(right).map(|(a, b)| a * b).sum::<f32>()
    };

    assert!(
        direction_dot(
            new_moon_off_node.sun_direction,
            new_moon_off_node.moon_direction
        ) < 0.999
    );
    assert!(new_moon_at_node.solar_eclipse_fraction > 0.9);
    assert_eq!(new_moon_off_node.solar_eclipse_fraction, 0.0);
    assert!(new_moon_at_node.sun_intensity < 0.25 * new_moon_off_node.sun_intensity);
    assert!(full_moon_at_node.lunar_umbra_fraction > 0.9);
    assert_eq!(full_moon_off_node.lunar_penumbra_fraction, 0.0);
    assert!(full_moon_at_node.moon_intensity < 0.25 * full_moon_off_node.moon_intensity);
    assert!(full_moon_at_node.moon_color[0] > full_moon_at_node.moon_color[2]);
    assert!(full_moon_off_node.moon_color[2] > full_moon_off_node.moon_color[0]);
}

#[test]
fn world_year_has_two_paired_eclipse_seasons() {
    let world = WorldGeometry::default();
    let mut solar_eclipse_months = 0;
    let mut lunar_eclipse_months = 0;

    for month in 0..13 {
        let new_moon_seconds = (21.0 + 28.0 * f64::from(month)) * 86_400.0;
        let mut solar_eclipse_visible = false;
        for latitude_index in -45..=45 {
            for longitude_index in 0..48 {
                let position_m = Vector::new(
                    world.width_m() * (longitude_index as f32 / 48.0 - 0.5),
                    world.height_m() * latitude_index as f32 / 90.0,
                );
                let lighting = celestial_lighting(world_celestial_conditions_in_world(
                    world,
                    new_moon_seconds,
                    position_m,
                ));
                solar_eclipse_visible |=
                    lighting.sun_direction[2] > 0.0 && lighting.solar_eclipse_fraction > 0.01;
            }
        }
        solar_eclipse_months += usize::from(solar_eclipse_visible);

        let full_moon_seconds = (7.0 + 28.0 * f64::from(month)) * 86_400.0;
        let lighting = celestial_lighting(world_celestial_conditions_in_world(
            world,
            full_moon_seconds,
            Vector::ZERO,
        ));
        lunar_eclipse_months += usize::from(lighting.lunar_penumbra_fraction > 0.01);
    }

    assert_eq!(solar_eclipse_months, 2);
    assert_eq!(lunar_eclipse_months, 2);
}

#[test]
fn first_lunar_eclipse_reaches_totality() {
    let conditions = world_celestial_conditions_in_world(
        WorldGeometry::default(),
        90.997_222_222_222_22 * 86_400.0,
        Vector::ZERO,
    );
    let lighting = celestial_lighting(conditions);

    assert!(
        lighting.lunar_umbra_fraction > 0.999,
        "{}",
        lighting.lunar_umbra_fraction,
    );
    assert!(lighting.moon_color[0] > lighting.moon_color[2]);
}

#[test]
fn celestial_lighting_uses_the_connected_hemisphere_after_a_pole() {
    let world = WorldGeometry::default();
    let world_time_seconds = (0.25 - 0.0021) * 365.0 * 86_400.0;
    let ascending_y_m = 0.25 * world.height_m();
    let descending_y_m = 0.75 * world.height_m();
    let longitude_m = (0..360)
        .map(|index| world.width_m() * (index as f32 / 360.0 - 0.5))
        .max_by(|left, right| {
            let sun_z = |longitude_m| {
                celestial_lighting(world_celestial_conditions_in_world(
                    world,
                    world_time_seconds,
                    Vector::new(longitude_m, ascending_y_m),
                ))
                .sun_direction[2]
            };
            sun_z(*left).total_cmp(&sun_z(*right))
        })
        .unwrap();
    let ascending = celestial_lighting(world_celestial_conditions_in_world(
        world,
        world_time_seconds,
        Vector::new(longitude_m, ascending_y_m),
    ));
    let descending = celestial_lighting(world_celestial_conditions_in_world(
        world,
        world_time_seconds,
        Vector::new(longitude_m, descending_y_m),
    ));

    assert!(ascending.sun_direction[2] > 0.6);
    assert!(
        descending.sun_direction[2] < -0.6,
        "connected hemisphere Sun direction: {:?}",
        descending.sun_direction,
    );
}

#[test]
fn lunar_position_advances_independently_while_phase_is_derived() {
    let start = world_celestial_conditions_in_world(WorldGeometry::default(), 0.0, Vector::ZERO);
    let one_synodic_cycle_later = world_celestial_conditions_in_world(
        WorldGeometry::default(),
        28.0 * 86_400.0,
        Vector::ZERO,
    );

    assert!((moon_phase(start) - 0.25).abs() < 0.0001);
    assert!((moon_phase(one_synodic_cycle_later) - moon_phase(start)).abs() < 0.0001);
    let longitude_change = (one_synodic_cycle_later.lunar_longitude_phase
        - start.lunar_longitude_phase)
        .rem_euclid(1.0);
    assert!(longitude_change > 0.05);
}

#[test]
fn solar_year_and_lunar_month_advance_independently() {
    let start = world_celestial_conditions_in_world(WorldGeometry::default(), 0.0, Vector::ZERO);
    let one_month_later = world_celestial_conditions_in_world(
        WorldGeometry::default(),
        28.0 * 86_400.0,
        Vector::ZERO,
    );
    let one_year_later = world_celestial_conditions_in_world(
        WorldGeometry::default(),
        365.0 * 86_400.0,
        Vector::ZERO,
    );

    assert!((one_month_later.season_phase - start.season_phase - 28.0 / 365.0).abs() < 0.0001);
    assert!((moon_phase(one_month_later) - moon_phase(start)).abs() < 0.0001);
    assert!((one_year_later.season_phase - start.season_phase).abs() < 0.0001);
    let annual_lunar_advance = (moon_phase(one_year_later) - moon_phase(start)).rem_euclid(1.0);
    assert!((annual_lunar_advance - 1.0 / 28.0).abs() < 0.0001);
}

#[test]
fn celestial_geometry_repeats_after_twenty_eight_years() {
    let world = WorldGeometry::default();
    let position_m = Vector::new(0.0, 10_000.0);
    let time_seconds = 11.75 * 86_400.0;
    let conditions_at = |offset_days| {
        world_celestial_conditions_in_world(
            world,
            time_seconds + offset_days * 86_400.0,
            position_m,
        )
    };
    let direction_difference = |left: [f32; 3], right: [f32; 3]| {
        left.into_iter()
            .zip(right)
            .map(|(left, right)| (left - right).abs())
            .sum::<f32>()
    };
    let start_conditions = conditions_at(0.0);
    let next_month_conditions = conditions_at(28.0);
    let start = celestial_lighting(start_conditions);
    let next_month = celestial_lighting(next_month_conditions);
    let next_year = celestial_lighting(conditions_at(365.0));
    let next_full_cycle = celestial_lighting(conditions_at(28.0 * 365.0));

    assert!((moon_phase(start_conditions) - moon_phase(next_month_conditions)).abs() < 0.0001);
    assert!(direction_difference(start.sun_direction, next_month.sun_direction) > 0.1);
    assert!(direction_difference(start.moon_direction, next_month.moon_direction) > 0.1);
    assert!(direction_difference(start.sun_direction, next_year.sun_direction) < 0.0001);
    assert!(direction_difference(start.moon_direction, next_year.moon_direction) > 0.01);
    assert!(direction_difference(start.sun_direction, next_full_cycle.sun_direction) < 0.0001);
    assert!(direction_difference(start.moon_direction, next_full_cycle.moon_direction) < 0.0001);
}

#[test]
fn moonlight_strength_follows_moon_elevation() {
    let overhead_conditions = CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.5,
        day_phase: 0.0,
        lunar_longitude_phase: 0.75,
    };
    let overhead = celestial_lighting(overhead_conditions);
    let low = celestial_lighting(CelestialConditions {
        day_phase: 0.20,
        ..overhead_conditions
    });

    assert!(overhead.moon_direction[2] > low.moon_direction[2]);
    assert!(overhead.moon_intensity > 1.3 * low.moon_intensity);
}

#[test]
fn diffuse_moonlight_crosses_the_horizon_more_gradually_than_direct_moonlight() {
    let base = CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.5,
        day_phase: 0.0,
        lunar_longitude_phase: 0.75,
    };
    let lighting_at_elevation = |elevation_degrees: f32| {
        let target_z = elevation_degrees.to_radians().sin();
        (0..=20_000)
            .map(|step| {
                celestial_lighting(CelestialConditions {
                    day_phase: step as f32 / 20_000.0,
                    ..base
                })
            })
            .min_by(|left, right| {
                (left.moon_direction[2] - target_z)
                    .abs()
                    .total_cmp(&(right.moon_direction[2] - target_z).abs())
            })
            .unwrap()
    };
    let below = lighting_at_elevation(-1.0);
    let above = lighting_at_elevation(1.0);
    let high = lighting_at_elevation(8.0);

    assert_eq!(below.moon_intensity, 0.0);
    assert!(below.moon_ambient_intensity > 0.0);
    assert!(below.moon_ambient_intensity < above.moon_ambient_intensity);
    assert!(above.moon_ambient_intensity < high.moon_ambient_intensity);
    assert!(
        above.moon_ambient_intensity - below.moon_ambient_intensity
            < 0.5 * (high.moon_ambient_intensity - below.moon_ambient_intensity),
    );
}

#[test]
fn seasonal_solar_declination_changes_high_latitude_noon_elevation() {
    let summer = celestial_lighting(CelestialConditions {
        geographic_latitude_degrees: 55.0,
        celestial_latitude_degrees: 55.0,
        season_phase: 0.5,
        day_phase: 0.5,
        lunar_longitude_phase: 0.25,
    });
    let winter = celestial_lighting(CelestialConditions {
        season_phase: 0.0,
        ..CelestialConditions {
            geographic_latitude_degrees: 55.0,
            celestial_latitude_degrees: 55.0,
            season_phase: 0.5,
            day_phase: 0.5,
            lunar_longitude_phase: 0.25,
        }
    });

    assert!(summer.sun_direction[2] > winter.sun_direction[2] + 0.5);
    assert!(summer.ambient_intensity > winter.ambient_intensity);
}

#[test]
fn climate_cycle_has_opposite_seasons_and_a_lagged_daily_temperature_peak() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: 15.0,
        seasonal_temperature_range_c: 20.0,
        diurnal_temperature_range_c: 8.0,
        annual_mean_water_temperature_c: 15.0,
        seasonal_water_temperature_range_c: 20.0,
    };
    let northern_summer = current_air_temperature_c(
        climate,
        CelestialConditions {
            geographic_latitude_degrees: 55.0,
            celestial_latitude_degrees: 55.0,
            season_phase: 0.59,
            day_phase: 0.625,
            lunar_longitude_phase: 0.0,
        },
    );
    let northern_winter = current_air_temperature_c(
        climate,
        CelestialConditions {
            geographic_latitude_degrees: 55.0,
            celestial_latitude_degrees: 55.0,
            season_phase: 0.09,
            day_phase: 0.625,
            lunar_longitude_phase: 0.0,
        },
    );
    let southern_summer = current_air_temperature_c(
        climate,
        CelestialConditions {
            geographic_latitude_degrees: -55.0,
            celestial_latitude_degrees: -55.0,
            season_phase: 0.09,
            day_phase: 0.625,
            lunar_longitude_phase: 0.0,
        },
    );
    let summer_before_dawn = current_air_temperature_c(
        climate,
        CelestialConditions {
            geographic_latitude_degrees: 55.0,
            celestial_latitude_degrees: 55.0,
            season_phase: 0.59,
            day_phase: 0.125,
            lunar_longitude_phase: 0.0,
        },
    );

    assert!(northern_summer > northern_winter + 19.0);
    assert!((northern_summer - southern_summer).abs() < 0.01);
    assert!(northern_summer > summer_before_dawn + 7.9);
}

#[test]
fn upper_air_climatology_has_muted_seasons_and_no_diurnal_cycle() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: 15.0,
        seasonal_temperature_range_c: 20.0,
        diurnal_temperature_range_c: 8.0,
        annual_mean_water_temperature_c: 15.0,
        seasonal_water_temperature_range_c: 20.0,
    };
    let conditions = |season_phase, day_phase| CelestialConditions {
        geographic_latitude_degrees: 55.0,
        celestial_latitude_degrees: 55.0,
        season_phase,
        day_phase,
        lunar_longitude_phase: 0.0,
    };
    let summer = current_500_hpa_air_temperature_c(climate, conditions(0.59, 0.625));
    let summer_night = current_500_hpa_air_temperature_c(climate, conditions(0.59, 0.125));
    let winter = current_500_hpa_air_temperature_c(climate, conditions(0.09, 0.625));
    let surface_summer = current_air_temperature_c(climate, conditions(0.59, 0.625));
    let surface_winter = current_air_temperature_c(climate, conditions(0.09, 0.625));

    assert!((summer - summer_night).abs() < f32::EPSILON);
    assert!(summer > winter);
    assert!(summer - winter < 0.6 * (surface_summer - surface_winter));
    assert!(summer - winter > 0.4 * (surface_summer - surface_winter));
}

#[test]
fn geographic_latitude_suppresses_equatorial_temperature_seasons() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: 15.0,
        seasonal_temperature_range_c: 20.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: 15.0,
        seasonal_water_temperature_range_c: 20.0,
    };
    let temperature = |geographic_latitude_degrees, season_phase| {
        current_air_temperature_c(
            climate,
            CelestialConditions {
                geographic_latitude_degrees,
                celestial_latitude_degrees: geographic_latitude_degrees,
                season_phase,
                day_phase: 0.5,
                lunar_longitude_phase: 0.0,
            },
        )
    };
    let equatorial_range = (temperature(0.0, 0.59) - temperature(0.0, 0.09)).abs();
    let high_latitude_range = (temperature(70.0, 0.59) - temperature(70.0, 0.09)).abs();

    assert!(equatorial_range < 0.25 * high_latitude_range);
}

#[test]
fn seasonal_temperature_is_continuous_across_the_equator() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: 15.0,
        seasonal_temperature_range_c: 20.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: 15.0,
        seasonal_water_temperature_range_c: 20.0,
    };
    let temperature = |geographic_latitude_degrees| {
        current_air_temperature_c(
            climate,
            CelestialConditions {
                geographic_latitude_degrees,
                celestial_latitude_degrees: geographic_latitude_degrees,
                season_phase: 0.59,
                day_phase: 0.5,
                lunar_longitude_phase: 0.0,
            },
        )
    };

    assert!((temperature(1.0) - temperature(-1.0)).abs() < 0.05);
}

#[test]
fn aligned_sun_and_moon_produce_a_larger_tidal_range_than_quarter_phase() {
    let conditions = |day_phase, moon_phase| CelestialConditions {
        geographic_latitude_degrees: 45.0,
        celestial_latitude_degrees: 45.0,
        season_phase: 0.25,
        day_phase,
        lunar_longitude_phase: moon_phase,
    };
    let tide = TidalConditions {
        reference_range_m: 2.0,
        phase_offset: 0.0,
    };
    let range = |moon_phase| {
        tide_height_m(tide, conditions(0.5, moon_phase))
            - tide_height_m(tide, conditions(0.75, moon_phase))
    };

    assert!(range(0.0) > 1.9);
    assert!(range(0.0) > 2.5 * range(0.25).abs());
    assert!((range(0.0) - range(0.5)).abs() < 0.01);
}

#[test]
fn stateless_surface_climate_distinguishes_snow_polar_and_high_elevation_ice() {
    let cold = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: -30.0,
            seasonal_temperature_range_c: 18.0,
            diurnal_temperature_range_c: 4.0,
            annual_mean_water_temperature_c: -30.0,
            seasonal_water_temperature_range_c: 18.0,
        },
        CelestialConditions {
            geographic_latitude_degrees: 75.0,
            celestial_latitude_degrees: 75.0,
            season_phase: 0.09,
            day_phase: 0.5,
            lunar_longitude_phase: 0.0,
        },
        8.0,
        0.0,
        0.0,
        600.0,
    );
    let warm = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: 22.0,
            seasonal_temperature_range_c: 8.0,
            diurnal_temperature_range_c: 4.0,
            annual_mean_water_temperature_c: 22.0,
            seasonal_water_temperature_range_c: 8.0,
        },
        CelestialConditions {
            geographic_latitude_degrees: 15.0,
            celestial_latitude_degrees: 15.0,
            season_phase: 0.59,
            day_phase: 0.5,
            lunar_longitude_phase: 0.0,
        },
        8.0,
        0.0,
        0.0,
        600.0,
    );
    let ordinary_polar_cold = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: -8.0,
            seasonal_temperature_range_c: 18.0,
            diurnal_temperature_range_c: 4.0,
            annual_mean_water_temperature_c: -8.0,
            seasonal_water_temperature_range_c: 18.0,
        },
        CelestialConditions {
            geographic_latitude_degrees: 75.0,
            celestial_latitude_degrees: 75.0,
            season_phase: 0.09,
            day_phase: 0.5,
            lunar_longitude_phase: 0.0,
        },
        8.0,
        0.0,
        0.0,
        600.0,
    );
    let normal_pole = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: -18.0,
            seasonal_temperature_range_c: 44.0,
            diurnal_temperature_range_c: 3.0,
            annual_mean_water_temperature_c: -18.0,
            seasonal_water_temperature_range_c: 44.0,
        },
        CelestialConditions {
            geographic_latitude_degrees: 90.0,
            celestial_latitude_degrees: 90.0,
            season_phase: 0.09,
            day_phase: 0.5,
            lunar_longitude_phase: 0.0,
        },
        4.0,
        0.0,
        0.0,
        -3_000.0,
    );

    assert!(cold.seasonal_snow_cover > 0.8);
    assert!(cold.polar_ice_closure < 0.05);
    assert!(cold.solid_ice_cover > 0.8);
    assert!(ordinary_polar_cold.polar_ice_closure < 0.05);
    assert!(ordinary_polar_cold.solid_ice_cover > 0.8);
    assert!(normal_pole.polar_ice_closure > 0.99);
    assert!(normal_pole.solid_ice_cover > 0.99);
    assert!(warm.seasonal_snow_cover < 0.05);
    assert!(warm.polar_ice_closure < 0.05);
    assert!(warm.solid_ice_cover < 0.05);
}

#[test]
fn perennial_ice_does_not_follow_instantaneous_water_vapor() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: -8.0,
        seasonal_temperature_range_c: 18.0,
        diurnal_temperature_range_c: 4.0,
        annual_mean_water_temperature_c: -8.0,
        seasonal_water_temperature_range_c: 18.0,
    };
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 75.0,
        celestial_latitude_degrees: 75.0,
        season_phase: 0.09,
        day_phase: 0.5,
        lunar_longitude_phase: 0.0,
    };
    let dry = climate_surface(climate, conditions, 0.0, 0.0, 0.0, 600.0);
    let humid = climate_surface(climate, conditions, 25.0, 0.0, 0.0, 600.0);

    assert_eq!(dry.solid_ice_cover, humid.solid_ice_cover);
}

#[test]
fn sea_ice_responds_to_seasonal_water_temperature() {
    let climate = world_climate_conditions(75.0);
    let conditions = |season_phase| CelestialConditions {
        geographic_latitude_degrees: 75.0,
        celestial_latitude_degrees: 75.0,
        season_phase,
        day_phase: 0.5,
        lunar_longitude_phase: 0.0,
    };
    let summer = climate_surface(climate, conditions(0.675), 8.0, 0.0, 0.0, -1_000.0);
    let winter = climate_surface(climate, conditions(0.175), 8.0, 0.0, 0.0, -1_000.0);

    assert!(winter.polar_ice_closure > summer.polar_ice_closure + 0.5);
}

#[test]
fn snow_cover_and_sea_ice_can_occur_independently() {
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 90.0,
        celestial_latitude_degrees: 90.0,
        season_phase: 0.0,
        day_phase: 0.625,
        lunar_longitude_phase: 0.0,
    };
    let frozen_water_climate = ClimateConditions {
        annual_mean_air_temperature_c: -4.0,
        seasonal_temperature_range_c: 0.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: -4.0,
        seasonal_water_temperature_range_c: 0.0,
    };
    let bare_warm_land = climate_surface(frozen_water_climate, conditions, 0.0, 24.0, 0.0, 0.0);
    let frozen_sea = climate_surface(frozen_water_climate, conditions, 0.0, 24.0, 0.0, -1_000.0);
    assert!(frozen_sea.polar_ice_closure > 0.95);
    assert!(
        bare_warm_land.seasonal_snow_cover < 0.05,
        "warm land snow {}",
        bare_warm_land.seasonal_snow_cover
    );

    let open_water_climate = ClimateConditions {
        annual_mean_air_temperature_c: 4.0,
        seasonal_temperature_range_c: 0.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: 4.0,
        seasonal_water_temperature_range_c: 0.0,
    };
    let snowy_cold_land = climate_surface(open_water_climate, conditions, 8.0, -12.0, 1.0, 0.0);
    let open_sea = climate_surface(open_water_climate, conditions, 8.0, -12.0, 1.0, -1_000.0);
    assert!(snowy_cold_land.seasonal_snow_cover > 0.8);
    assert!(open_sea.polar_ice_closure < 0.05);
}

#[test]
fn sea_ice_is_not_a_direct_function_of_ocean_depth() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: -8.0,
        seasonal_temperature_range_c: 0.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: -8.0,
        seasonal_water_temperature_range_c: 0.0,
    };
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 75.0,
        celestial_latitude_degrees: 75.0,
        season_phase: 0.0,
        day_phase: 0.5,
        lunar_longitude_phase: 0.0,
    };
    let shallow = climate_surface(climate, conditions, 8.0, 0.0, 0.0, -10.0);
    let deep = climate_surface(climate, conditions, 8.0, 0.0, 0.0, -3_000.0);

    assert_eq!(shallow.polar_ice_closure, deep.polar_ice_closure);
}

#[test]
fn acute_air_temperature_modulates_snow_without_becoming_ground_temperature() {
    let climate = ClimateConditions {
        annual_mean_air_temperature_c: -1.0,
        seasonal_temperature_range_c: 0.0,
        diurnal_temperature_range_c: 0.0,
        annual_mean_water_temperature_c: -1.0,
        seasonal_water_temperature_range_c: 0.0,
    };
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 60.0,
        celestial_latitude_degrees: 60.0,
        season_phase: 0.0,
        day_phase: 0.5,
        lunar_longitude_phase: 0.0,
    };
    let cold = climate_surface(climate, conditions, 8.0, -12.0, 0.0, 0.0);
    let mild = climate_surface(climate, conditions, 8.0, 0.0, 0.0, 0.0);
    let warm = climate_surface(climate, conditions, 8.0, 12.0, 0.0, 0.0);

    assert!(cold.seasonal_snow_cover > mild.seasonal_snow_cover);
    assert!(mild.seasonal_snow_cover > warm.seasonal_snow_cover);
    assert!(cold.ground_temperature_c - warm.ground_temperature_c > -12.0);
    assert!(cold.ground_temperature_c - warm.ground_temperature_c < 0.0);
}

#[test]
fn winter_snow_cover_continues_increasing_toward_the_pole() {
    let snow_cover = |latitude| {
        let conditions = CelestialConditions {
            geographic_latitude_degrees: latitude,
            celestial_latitude_degrees: latitude,
            season_phase: 0.13,
            day_phase: 0.125,
            lunar_longitude_phase: 0.0,
        };
        climate_surface(
            world_climate_conditions(latitude),
            conditions,
            8.0,
            0.0,
            0.0,
            0.0,
        )
        .seasonal_snow_cover
    };
    let temperate = snow_cover(45.0);
    let subpolar = snow_cover(60.0);
    let polar = snow_cover(75.0);

    assert!(
        subpolar > temperate + 0.25,
        "temperate {temperate}, subpolar {subpolar}, polar {polar}"
    );
    assert!(polar >= subpolar);
}

#[test]
fn snow_moisture_is_relative_to_local_air_temperature() {
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 0.0,
        celestial_latitude_degrees: 0.0,
        season_phase: 0.0,
        day_phase: 0.625,
        lunar_longitude_phase: 0.0,
    };
    let cold_air = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: -8.333,
            seasonal_temperature_range_c: 0.0,
            diurnal_temperature_range_c: 0.0,
            annual_mean_water_temperature_c: -8.333,
            seasonal_water_temperature_range_c: 0.0,
        },
        conditions,
        2.0,
        -6.667,
        0.0,
        0.0,
    );
    let warm_air = climate_surface(
        ClimateConditions {
            annual_mean_air_temperature_c: -13.333,
            seasonal_temperature_range_c: 0.0,
            diurnal_temperature_range_c: 0.0,
            annual_mean_water_temperature_c: -13.333,
            seasonal_water_temperature_range_c: 0.0,
        },
        conditions,
        2.0,
        13.333,
        0.0,
        0.0,
    );

    assert!((cold_air.ground_temperature_c - warm_air.ground_temperature_c).abs() < 0.001);
    assert!(cold_air.air_temperature_c < warm_air.air_temperature_c - 10.0);
    assert!(cold_air.seasonal_snow_cover > warm_air.seasonal_snow_cover + 0.30);
}

#[test]
fn temperate_january_snow_can_coexist_with_open_water() {
    let climate = world_climate_conditions(45.0);
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 45.0,
        celestial_latitude_degrees: 45.0,
        season_phase: 0.09,
        day_phase: 0.125,
        lunar_longitude_phase: 0.0,
    };
    let land = climate_surface(climate, conditions, 7.0, -6.0, 1.0, 0.0);
    let sea = climate_surface(climate, conditions, 7.0, -6.0, 1.0, -100.0);

    assert!(land.seasonal_snow_cover > 0.5);
    assert!(sea.polar_ice_closure < 0.05);
}

#[test]
fn temperate_winter_climate_retains_snow_beyond_sea_ice() {
    let latitude = 49.0;
    let conditions = CelestialConditions {
        geographic_latitude_degrees: latitude,
        celestial_latitude_degrees: latitude,
        season_phase: 0.09,
        day_phase: 0.125,
        lunar_longitude_phase: 0.0,
    };
    let climate = world_climate_conditions(latitude);
    let land = climate_surface(climate, conditions, 7.0, 0.0, 0.0, 0.0);
    let sea = climate_surface(climate, conditions, 7.0, 0.0, 0.0, -100.0);
    let farther_temperate_snow = climate_surface(
        world_climate_conditions(45.0),
        CelestialConditions {
            geographic_latitude_degrees: 45.0,
            celestial_latitude_degrees: 45.0,
            ..conditions
        },
        7.0,
        0.0,
        0.0,
        0.0,
    );

    assert!(land.seasonal_snow_cover > 0.60);
    assert!(farther_temperate_snow.seasonal_snow_cover > 0.35);
    assert!(sea.polar_ice_closure < 0.05);
}

#[test]
fn winter_sea_ice_has_a_polar_core_without_a_zonal_edge_at_68_degrees() {
    let surface = |latitude| {
        climate_surface(
            world_climate_conditions(latitude),
            CelestialConditions {
                geographic_latitude_degrees: latitude,
                celestial_latitude_degrees: latitude,
                season_phase: 0.175,
                day_phase: 0.5,
                lunar_longitude_phase: 0.0,
            },
            4.0,
            0.0,
            0.0,
            -100.0,
        )
    };

    assert!(surface(68.0).polar_ice_closure < 0.05);
    assert!(surface(72.0).polar_ice_closure > 0.95);
}

#[test]
fn cold_precipitation_adds_fresh_snow() {
    let climate = world_climate_conditions(45.0);
    let conditions = CelestialConditions {
        geographic_latitude_degrees: 45.0,
        celestial_latitude_degrees: 45.0,
        season_phase: 0.09,
        day_phase: 0.125,
        lunar_longitude_phase: 0.0,
    };
    let dry = climate_surface(climate, conditions, 7.0, -4.0, 0.0, 0.0);
    let snowing = climate_surface(climate, conditions, 7.0, -4.0, 1.0, 0.0);

    assert!(snowing.seasonal_snow_cover > dry.seasonal_snow_cover + 0.25);
}

#[test]
fn temperate_winter_snow_retreats_during_the_warm_part_of_the_day() {
    let climate = world_climate_conditions(45.0);
    let conditions = |day_phase| CelestialConditions {
        geographic_latitude_degrees: 45.0,
        celestial_latitude_degrees: 45.0,
        season_phase: 0.09,
        day_phase,
        lunar_longitude_phase: 0.0,
    };
    let cold_night = climate_surface(climate, conditions(0.125), 7.0, -2.0, 0.6, 0.0);
    let warm_afternoon = climate_surface(climate, conditions(0.625), 7.0, -2.0, 0.6, 0.0);

    assert!(cold_night.seasonal_snow_cover > warm_afternoon.seasonal_snow_cover + 0.15);
}

#[test]
fn day_night_temperature_changes_precipitation() {
    let climate = world_climate_conditions(45.0);
    let conditions = |day_phase| CelestialConditions {
        geographic_latitude_degrees: 45.0,
        celestial_latitude_degrees: 45.0,
        season_phase: 0.09,
        day_phase,
        lunar_longitude_phase: 0.0,
    };
    let primary = AtmospherePrimary {
        pressure_hpa: 990.0,
        pressure_gradient_hpa_per_m: Vector::new(0.0015, 0.0),
        synoptic_pressure_gradient_hpa_per_m: Vector::new(0.0005, 0.0),
        surface_gust_signal: 0.0,
        surface_air_temperature_anomaly_c: 0.0,
        air_temperature_500_hpa_c: -30.0,
        local_temperature_gradient_c_per_m: Vector::new(0.0005, 0.0),
        water_vapor_g_kg: 4.0,
    };
    let night = derive_weather(primary, climate, conditions(0.125));
    let afternoon = derive_weather(primary, climate, conditions(0.625));

    assert!(afternoon.surface_air_temperature_c > night.surface_air_temperature_c + 4.0);
    assert!(night.rain_intensity > afternoon.rain_intensity + 0.5);
}

#[test]
fn evolving_moisture_does_not_breathe_with_the_daily_temperature_cycle() {
    let position_m = Vector::new(0.0, 16_000.0);
    let half_day_ticks = 12 * 60 * 60 * 60;
    let mut afternoon_temperature_c = 0.0;
    let mut night_temperature_c = 0.0;
    let mut afternoon_water_vapor_g_kg = 0.0;
    let mut night_water_vapor_g_kg = 0.0;
    let sample_count = 128;

    for seed in 0..sample_count {
        let afternoon = procedural_weather(seed, 0, position_m);
        let night = procedural_weather(seed, half_day_ticks, position_m);
        afternoon_temperature_c += afternoon.surface_air_temperature_c;
        night_temperature_c += night.surface_air_temperature_c;
        afternoon_water_vapor_g_kg += afternoon.primary.water_vapor_g_kg;
        night_water_vapor_g_kg += night.primary.water_vapor_g_kg;
    }

    let samples = sample_count as f32;
    afternoon_temperature_c /= samples;
    night_temperature_c /= samples;
    afternoon_water_vapor_g_kg /= samples;
    night_water_vapor_g_kg /= samples;

    assert!(
        afternoon_temperature_c > night_temperature_c + 3.0,
        "afternoon {afternoon_temperature_c}, night {night_temperature_c}"
    );
    assert!(
        (afternoon_water_vapor_g_kg - night_water_vapor_g_kg).abs()
            < 0.10 * afternoon_water_vapor_g_kg
    );
}

#[test]
fn astronomical_tide_changes_authoritative_water_depth() {
    let seed = 0x5eed;
    let position_m = (-32..=32)
        .flat_map(|x| (-32..=32).map(move |y| Vector::new(x as f32 * 128.0, y as f32 * 128.0)))
        .find(|position_m| procedural_terrain_elevation_m(seed, *position_m) < -3.0)
        .expect("representative region should contain deep water");
    let high_tide_depth = procedural_match_environment(seed, 0, position_m).water_depth_m;
    let six_hours_in_ticks = 6 * 60 * 60 * 60;
    let low_tide_depth =
        procedural_match_environment(seed, six_hours_in_ticks, position_m).water_depth_m;

    assert!((high_tide_depth - low_tide_depth).abs() > 0.1);
}

#[test]
fn solid_ice_extends_land_across_coastal_water() {
    let seed = 0x5eed;
    let position_m = (-128..=128)
        .flat_map(|x| (-128..=128).map(move |y| Vector::new(x as f32 * 32.0, y as f32 * 32.0)))
        .find(|position_m| {
            (-12.0..=-8.0).contains(&procedural_terrain_elevation_m(seed, *position_m))
        })
        .expect("representative region should contain a shallow coast");
    let bedrock_elevation_m = procedural_terrain_elevation_m(seed, position_m);
    let ice_elevation_m = procedural_solid_surface_elevation_m(seed, position_m, -20.0, -20.0);

    assert!(bedrock_elevation_m < 0.0);
    assert!(ice_elevation_m > 0.0);
}

#[test]
fn extreme_cold_closes_deep_water_with_solid_ice() {
    let seed = 0x5eed;
    let position_m = (-32..=32)
        .flat_map(|x| (-32..=32).map(move |y| Vector::new(x as f32 * 128.0, y as f32 * 128.0)))
        .find(|position_m| procedural_terrain_elevation_m(seed, *position_m) < -1_000.0)
        .expect("representative region should contain deep ocean");
    let ice_elevation_m = procedural_solid_surface_elevation_m(seed, position_m, -30.0, -30.0);

    assert!(ice_elevation_m > 0.0);
}

#[test]
fn normal_polar_climate_closes_water_across_the_pole() {
    let world = WorldGeometry::default();
    let seed = 0x5eed;
    let latitude = -90.0;
    let annual_mean = world_climate_conditions(latitude).annual_mean_air_temperature_c;
    let mut water_samples = 0;

    for longitude_index in 0..512 {
        let position = Vector::new(
            world.width_m() * (longitude_index as f32 / 512.0 - 0.5),
            -0.5 * world.height_m(),
        );
        if procedural_terrain_elevation_m_in_world(world, seed, position) >= 0.0 {
            continue;
        }
        water_samples += 1;
        assert!(
            procedural_solid_surface_elevation_m_in_world(
                world,
                seed,
                position,
                annual_mean,
                annual_mean,
            ) > 0.0,
            "open water at polar longitude index {longitude_index}",
        );
    }

    assert!(water_samples > 128);
}

#[test]
fn cold_surface_water_freezes_over_shallow_and_deep_ocean() {
    let seed = 0x5eed;
    let shallow_position_m = (-128..=128)
        .flat_map(|x| (-128..=128).map(move |y| Vector::new(x as f32 * 32.0, y as f32 * 32.0)))
        .find(|position_m| {
            (-12.0..=-8.0).contains(&procedural_terrain_elevation_m(seed, *position_m))
        })
        .expect("representative region should contain shallow water");
    let deep_position_m = (-32..=32)
        .flat_map(|x| (-32..=32).map(move |y| Vector::new(x as f32 * 128.0, y as f32 * 128.0)))
        .find(|position_m| procedural_terrain_elevation_m(seed, *position_m) < -1_000.0)
        .expect("representative region should contain deep ocean");

    let shallow_ice = procedural_solid_surface_elevation_m(seed, shallow_position_m, -20.0, -20.0);
    let deep_ice = procedural_solid_surface_elevation_m(seed, deep_position_m, -20.0, -20.0);

    assert!(shallow_ice > 0.0);
    assert!(deep_ice > 0.0);
}

#[test]
fn polar_ice_front_has_frozen_and_open_deep_water_at_one_latitude() {
    let world = WorldGeometry::default();
    let seed = 0x5eed;
    let mixed_latitude = (65..90).find(|latitude| {
        let latitude = *latitude as f32;
        let climate = world_climate_conditions(latitude);
        let water_temperature_c = climate_surface(
            climate,
            CelestialConditions {
                geographic_latitude_degrees: latitude,
                celestial_latitude_degrees: latitude,
                season_phase: 0.09,
                day_phase: 0.5,
                lunar_longitude_phase: 0.0,
            },
            4.0,
            0.0,
            0.0,
            -1_000.0,
        )
        .water_temperature_c;
        let mut frozen_deep_water = 0;
        let mut open_deep_water = 0;

        for longitude_index in 0..512 {
            let position = Vector::new(
                world.width_m() * (longitude_index as f32 / 512.0 - 0.5),
                world.height_m() * latitude / 180.0,
            );
            let bedrock = procedural_terrain_elevation_m_in_world(world, seed, position);
            if bedrock > -1_000.0 {
                continue;
            }
            let solid = procedural_solid_surface_elevation_m_in_world(
                world,
                seed,
                position,
                climate.annual_mean_air_temperature_c,
                water_temperature_c,
            );
            if solid >= 0.0 {
                frozen_deep_water += 1;
            } else {
                open_deep_water += 1;
            }
        }

        frozen_deep_water >= 8 && open_deep_water >= 8
    });

    assert!(
        mixed_latitude.is_some(),
        "no spatially varied polar edge found"
    );
}

#[test]
fn warm_surface_water_does_not_close_the_deep_ocean() {
    let seed = 0x5eed;
    let position_m = (-32..=32)
        .flat_map(|x| (-32..=32).map(move |y| Vector::new(x as f32 * 128.0, y as f32 * 128.0)))
        .find(|position_m| procedural_terrain_elevation_m(seed, *position_m) < -1_000.0)
        .expect("representative region should contain deep ocean");
    let ice_elevation_m = procedural_solid_surface_elevation_m(seed, position_m, -5.0, 8.0);

    assert!(ice_elevation_m < 0.0);
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
    assert!(instability_range.0 < 0.25, "{instability_range:?}");
    assert!(instability_range.1 > 0.75, "{instability_range:?}");
    assert!(high_cloud_samples > 0);
    assert!(high_cloud_samples < 512);
    assert!(
        upper_cloud_wind_change < full_wind_change,
        "upper/full wind change ratio was {}",
        upper_cloud_wind_change / full_wind_change,
    );
}

#[test]
fn aggregate_cloud_cover_is_locally_coherent_and_regionally_varied() {
    let origin_m = Vector::new(12.0, -7.0);
    let nearby_m = origin_m + Vector::new(256.0, 0.0);
    let regional_m = origin_m + Vector::new(8_192.0, 0.0);
    let mut nearby_difference = 0.0;
    let mut regional_difference = 0.0;

    for seed in 0..512 {
        let origin = procedural_weather(seed, 12_345, origin_m).upper_cloudiness;
        nearby_difference +=
            (procedural_weather(seed, 12_345, nearby_m).upper_cloudiness - origin).abs();
        regional_difference +=
            (procedural_weather(seed, 12_345, regional_m).upper_cloudiness - origin).abs();
    }

    let nearby_mean_difference = nearby_difference / 512.0;
    let regional_mean_difference = regional_difference / 512.0;
    assert!(
        regional_mean_difference > 0.05,
        "{regional_mean_difference}",
    );
    assert!(
        nearby_mean_difference < 0.25 * regional_mean_difference,
        "nearby {nearby_mean_difference}, regional {regional_mean_difference}",
    );
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
    assert!(
        upper_speed_sum > sea_speed_sum,
        "upper/sea speed ratio was {}",
        upper_speed_sum / sea_speed_sum,
    );
    let average_alignment = alignment_sum / samples as f32;
    assert!(average_alignment > 0.85, "{average_alignment}");
    assert!(sheared_samples > samples * 9 / 10);
}

#[test]
fn procedural_terrain_is_seeded_and_continuous() {
    let seed = 0x5eed_u64;
    let position_m = (-16..=16)
        .flat_map(|x| (-16..=16).map(move |y| Vector::new(x as f32 * 256.0, y as f32 * 256.0)))
        .find(|position_m| procedural_terrain_elevation_m(seed, *position_m).abs() < 100.0)
        .unwrap();
    let elevation_m = procedural_terrain_elevation_m(seed, position_m);

    assert_eq!(
        elevation_m,
        procedural_terrain_elevation_m(seed, position_m)
    );
    assert!(elevation_m.is_finite());
    let nearby_delta_m = (procedural_terrain_elevation_m(seed, position_m + Vector::new(0.1, 0.1))
        - elevation_m)
        .abs();
    assert!(
        nearby_delta_m < 0.2,
        "10 cm sample delta was {nearby_delta_m} m"
    );
    assert!(
        (procedural_terrain_elevation_m(seed, position_m + Vector::new(64.0, 0.0)) - elevation_m)
            .abs()
            > 0.01
    );
    assert!((procedural_terrain_elevation_m(seed + 1, position_m) - elevation_m).abs() > 0.01);
    assert_eq!(
        procedural_match_environment(seed, 600, position_m).water_depth_m,
        procedural_water_depth_m(seed, 10.0, position_m)
    );
}

#[test]
fn procedural_terrain_forms_sparse_clusters_of_large_islands() {
    // The calibrated terrain covers this share of the population's samples with land.
    const CALIBRATED_LAND_OCCUPANCY: f32 = 0.0342;
    let mut minimum_elevation_m = f32::MAX;
    let mut maximum_elevation_m = f32::MIN;
    let mut land_samples = 0;
    let mut samples = 0;
    let mut land_samples_with_distant_land = 0;

    for seed in terrain_population_seeds(16) {
        let land_samples_before_seed = land_samples;
        let samples_before_seed = samples;
        for x in -64..=64 {
            for y in -64..=64 {
                let position_m = Vector::new(x as f32 * 256.0, y as f32 * 256.0);
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
    let land_occupancy = land_samples as f32 / samples as f32;
    assert!(
        (0.5 * CALIBRATED_LAND_OCCUPANCY..=2.0 * CALIBRATED_LAND_OCCUPANCY)
            .contains(&land_occupancy),
        "land occupied {land_occupancy} of the population's samples"
    );
    assert!(
        land_samples_with_distant_land * 2 > land_samples,
        "only {land_samples_with_distant_land} of {land_samples} land samples remained land 512 m away"
    );
}

#[test]
fn procedural_terrain_water_is_overwhelmingly_connected_to_open_sea() {
    const SIDE: usize = 129;
    let mut total_water_samples = 0;
    let mut enclosed_water_samples = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        let mut water = vec![false; SIDE * SIDE];
        for x in 0..SIDE {
            for y in 0..SIDE {
                let position_m = Vector::new(
                    (x as f32 - (SIDE / 2) as f32) * 256.0,
                    (y as f32 - (SIDE / 2) as f32) * 256.0,
                );
                water[y * SIDE + x] = procedural_terrain_elevation_m(seed, position_m) < 0.0;
            }
        }

        let mut open_sea = vec![false; SIDE * SIDE];
        let mut frontier = VecDeque::new();
        for coordinate in 0..SIDE {
            for index in [coordinate, (SIDE - 1) * SIDE + coordinate] {
                if water[index] && !open_sea[index] {
                    open_sea[index] = true;
                    frontier.push_back(index);
                }
            }
            for index in [coordinate * SIDE, coordinate * SIDE + SIDE - 1] {
                if water[index] && !open_sea[index] {
                    open_sea[index] = true;
                    frontier.push_back(index);
                }
            }
        }

        while let Some(index) = frontier.pop_front() {
            let x = index % SIDE;
            let y = index / SIDE;
            if x > 0 {
                let neighbor = index - 1;
                if water[neighbor] && !open_sea[neighbor] {
                    open_sea[neighbor] = true;
                    frontier.push_back(neighbor);
                }
            }
            if x + 1 < SIDE {
                let neighbor = index + 1;
                if water[neighbor] && !open_sea[neighbor] {
                    open_sea[neighbor] = true;
                    frontier.push_back(neighbor);
                }
            }
            if y > 0 {
                let neighbor = index - SIDE;
                if water[neighbor] && !open_sea[neighbor] {
                    open_sea[neighbor] = true;
                    frontier.push_back(neighbor);
                }
            }
            if y + 1 < SIDE {
                let neighbor = index + SIDE;
                if water[neighbor] && !open_sea[neighbor] {
                    open_sea[neighbor] = true;
                    frontier.push_back(neighbor);
                }
            }
        }

        total_water_samples += water.iter().filter(|sample| **sample).count();
        enclosed_water_samples += water
            .iter()
            .zip(open_sea.iter())
            .filter(|(is_water, is_open_sea)| **is_water && !**is_open_sea)
            .count();
    }

    assert!(total_water_samples > 0);
    assert!(
        enclosed_water_samples * 200 < total_water_samples,
        "{enclosed_water_samples} of {total_water_samples} water samples were enclosed"
    );
}

#[test]
fn procedural_coasts_naturally_mix_sharp_outcrops_and_navigable_water() {
    // Relief ceilings are 90th percentiles from NOAA BlueTopo 4 m grids
    // BH5475HC (rocky Maine, 2026-06-01) and BH4TB55V (Florida Keys, 2026-05-20),
    // sampled in the same -4..1 m band. They bound roughness, not shoreline shape.
    // Source: https://www.nauticalcharts.noaa.gov/data/bluetopo.html
    // Intertidal and beach relief may fall by up to a quarter from the
    // calibrated terrain's 90th percentiles.
    const CALIBRATED_INTERTIDAL_RELIEF_16_M_P90: f32 = 0.44;
    const CALIBRATED_BEACH_RELIEF_16_M_P90: f32 = 0.57;
    let mut mixed_coastal_patches = 0;
    let mut deep_access_patches = 0;
    let mut maximum_slope: f32 = 0.0;
    let mut relief_32_m = Vec::new();
    let mut relief_128_m = Vec::new();
    let mut underwater_relief_16_m = Vec::new();
    let mut intertidal_relief_16_m = Vec::new();
    let mut beach_relief_16_m = Vec::new();
    let mut shallow_patch_relief_16_m_p90 = Vec::new();

    for seed in terrain_population_seeds(16) {
        for patch_x in (-24..=24).step_by(2) {
            for patch_y in (-24..=24).step_by(2) {
                let mut has_outcrop = false;
                let mut has_navigable_water = false;
                let mut deep_water = [[false; 8]; 8];
                let mut shallow_water_samples = 0;
                let mut shallow_patch_relief_16_m = Vec::new();
                for (sample_x, deep_water_column) in deep_water.iter_mut().enumerate() {
                    for (sample_y, is_deep_water) in deep_water_column.iter_mut().enumerate() {
                        let position_m = Vector::new(
                            patch_x as f32 * 256.0 + (sample_x as f32 - 3.5) * 32.0,
                            patch_y as f32 * 256.0 + (sample_y as f32 - 3.5) * 32.0,
                        );
                        let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                        let nearby_underwater_elevation_m = procedural_terrain_elevation_m(
                            seed,
                            position_m + Vector::new(16.0, 0.0),
                        );
                        if (-4.0..=-1.4).contains(&elevation_m)
                            && (-4.0..=-1.4).contains(&nearby_underwater_elevation_m)
                        {
                            let relief_m = (nearby_underwater_elevation_m - elevation_m).abs();
                            underwater_relief_16_m.push(relief_m);
                            shallow_patch_relief_16_m.push(relief_m);
                        }
                        if (-1.3..=0.0).contains(&elevation_m)
                            && (-1.3..=0.0).contains(&nearby_underwater_elevation_m)
                        {
                            intertidal_relief_16_m
                                .push((nearby_underwater_elevation_m - elevation_m).abs());
                        }
                        if (0.0..=1.5).contains(&elevation_m)
                            && (0.0..=1.5).contains(&nearby_underwater_elevation_m)
                        {
                            beach_relief_16_m
                                .push((nearby_underwater_elevation_m - elevation_m).abs());
                        }
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
                        *is_deep_water = elevation_m <= -4.0;
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
                if shallow_patch_relief_16_m.len() >= 8 {
                    shallow_patch_relief_16_m.sort_by(f32::total_cmp);
                    shallow_patch_relief_16_m_p90
                        .push(shallow_patch_relief_16_m[shallow_patch_relief_16_m.len() * 9 / 10]);
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
    underwater_relief_16_m.sort_by(f32::total_cmp);
    intertidal_relief_16_m.sort_by(f32::total_cmp);
    beach_relief_16_m.sort_by(f32::total_cmp);
    shallow_patch_relief_16_m_p90.sort_by(f32::total_cmp);
    let relief_32_m_p90 = relief_32_m[relief_32_m.len() * 9 / 10];
    let relief_128_m_p90 = relief_128_m[relief_128_m.len() * 9 / 10];
    assert!(
        relief_32_m_p90 <= 1.94,
        "32 m relief p90 was {relief_32_m_p90} m"
    );
    let underwater_relief_16_m_p90 = underwater_relief_16_m[underwater_relief_16_m.len() * 9 / 10];
    assert!(
        underwater_relief_16_m_p90 >= 0.5,
        "16 m underwater relief p90 lost detail at {underwater_relief_16_m_p90} m"
    );
    let intertidal_relief_16_m_p90 = intertidal_relief_16_m[intertidal_relief_16_m.len() * 9 / 10];
    assert!(
        intertidal_relief_16_m_p90 >= 0.75 * CALIBRATED_INTERTIDAL_RELIEF_16_M_P90,
        "16 m intertidal relief p90 lost detail at {intertidal_relief_16_m_p90} m"
    );
    let beach_relief_16_m_p90 = beach_relief_16_m[beach_relief_16_m.len() * 9 / 10];
    assert!(
        beach_relief_16_m_p90 >= 0.75 * CALIBRATED_BEACH_RELIEF_16_M_P90,
        "16 m beach relief p90 lost detail at {beach_relief_16_m_p90} m"
    );
    let calm_shallow_patch_relief_m =
        shallow_patch_relief_16_m_p90[shallow_patch_relief_16_m_p90.len() / 10];
    let rugged_shallow_patch_relief_m =
        shallow_patch_relief_16_m_p90[shallow_patch_relief_16_m_p90.len() * 9 / 10];
    assert!(
        rugged_shallow_patch_relief_m >= 3.2 * calm_shallow_patch_relief_m,
        "shallow relief only varied from {calm_shallow_patch_relief_m} to {rugged_shallow_patch_relief_m} m"
    );
    assert!(
        relief_128_m_p90 <= 2.92,
        "128 m relief p90 was {relief_128_m_p90} m"
    );
    assert!(deep_access_patches > 0);
    assert!(
        deep_access_patches * 2 < mixed_coastal_patches,
        "{deep_access_patches} of {mixed_coastal_patches} reef patches had deep access"
    );
}

#[test]
fn procedural_coasts_mix_gradual_shelves_and_steep_basin_drops() {
    let mut mid_depth_pairs = 0;
    let mut gradual_pairs = 0;
    let mut steep_pairs = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for sample_x in -32..=32 {
            for sample_y in -32..=32 {
                let position_m = Vector::new(sample_x as f32 * 64.0, sample_y as f32 * 64.0);
                let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                let nearby_elevation_m =
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(64.0, 0.0));
                if (-200.0..=-8.0).contains(&elevation_m)
                    && (-200.0..=-8.0).contains(&nearby_elevation_m)
                {
                    let relief_m = (nearby_elevation_m - elevation_m).abs();
                    mid_depth_pairs += 1;
                    gradual_pairs += usize::from(relief_m <= 8.0);
                    steep_pairs += usize::from(relief_m >= 32.0);
                }
            }
        }
    }

    assert!(
        mid_depth_pairs >= 100,
        "only {mid_depth_pairs} mid-depth pairs"
    );
    assert!(
        gradual_pairs * 4 >= mid_depth_pairs,
        "only {gradual_pairs} of {mid_depth_pairs} mid-depth pairs were gradual"
    );
    assert!(steep_pairs > 0, "no steep basin drops remained");
}

#[test]
fn mid_depth_shelves_rarely_form_abrupt_local_kinks() {
    let mut mid_depth_triples = 0;
    let mut abrupt_kinks = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for sample_x in -96..=96 {
            for sample_y in -96..=96 {
                let position_m = Vector::new(sample_x as f32 * 32.0, sample_y as f32 * 32.0);
                for direction in [Vector::new(32.0, 0.0), Vector::new(0.0, 32.0)] {
                    let before_m = procedural_terrain_elevation_m(seed, position_m - direction);
                    let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                    let after_m = procedural_terrain_elevation_m(seed, position_m + direction);
                    if !(-200.0..=-8.0).contains(&before_m)
                        || !(-200.0..=-8.0).contains(&elevation_m)
                        || !(-200.0..=-8.0).contains(&after_m)
                    {
                        continue;
                    }
                    let local_curvature_m = (before_m - 2.0 * elevation_m + after_m).abs();
                    mid_depth_triples += 1;
                    abrupt_kinks += usize::from(local_curvature_m >= 24.0);
                }
            }
        }
    }

    assert!(
        mid_depth_triples >= 100,
        "only {mid_depth_triples} mid-depth triples"
    );
    assert!(
        abrupt_kinks * 20 < mid_depth_triples,
        "{abrupt_kinks} of {mid_depth_triples} mid-depth triples formed abrupt kinks"
    );
}

#[test]
fn visible_outer_shelves_retain_local_bottom_relief() {
    let mut shelf_samples = 0;
    let mut structured_samples = 0;

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for sample_x in -96..=96 {
            for sample_y in -96..=96 {
                let position_m = Vector::new(sample_x as f32 * 32.0, sample_y as f32 * 32.0);
                let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                if !(-20.0..=-8.0).contains(&elevation_m) {
                    continue;
                }
                let before_m =
                    procedural_terrain_elevation_m(seed, position_m - Vector::new(8.0, 0.0));
                let after_m =
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(8.0, 0.0));
                if !(-20.0..=-8.0).contains(&before_m) || !(-20.0..=-8.0).contains(&after_m) {
                    continue;
                }
                let local_curvature_m = (before_m - 2.0 * elevation_m + after_m).abs();
                shelf_samples += 1;
                structured_samples += usize::from(local_curvature_m >= 0.10);
            }
        }
    }

    assert!(
        shelf_samples >= 100,
        "only {shelf_samples} outer-shelf samples"
    );
    assert!(
        structured_samples * 6 >= shelf_samples,
        "only {structured_samples} of {shelf_samples} outer-shelf samples retained local relief"
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
fn procedural_gentle_land_retains_zoomed_surface_relief() {
    // The calibrated terrain's 90th-percentile 8 m relief on gentle land. It
    // varies from 0.39 to 0.80 m between four-seed groups, so it is measured
    // over 128 seeds.
    const CALIBRATED_RELIEF_8_M_P90: f32 = 0.427;
    let mut relief_8_m = Vec::new();

    for seed in terrain_population_seeds(128) {
        for x in -32..=32 {
            for y in -32..=32 {
                let position_m = Vector::new(x as f32 * 256.0, y as f32 * 256.0);
                let left_32_m =
                    procedural_terrain_elevation_m(seed, position_m - Vector::new(32.0, 0.0));
                let right_32_m =
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(32.0, 0.0));
                if (2.0..=100.0).contains(&left_32_m)
                    && (2.0..=100.0).contains(&right_32_m)
                    && (right_32_m - left_32_m).abs() <= 1.0
                {
                    let left_4_m =
                        procedural_terrain_elevation_m(seed, position_m - Vector::new(4.0, 0.0));
                    let right_4_m =
                        procedural_terrain_elevation_m(seed, position_m + Vector::new(4.0, 0.0));
                    relief_8_m.push((right_4_m - left_4_m).abs());
                }
            }
        }
    }

    assert!(relief_8_m.len() >= 100);
    relief_8_m.sort_by(f32::total_cmp);
    let relief_8_m_p90 = relief_8_m[relief_8_m.len() * 9 / 10];
    assert!(
        relief_8_m_p90 >= 0.75 * CALIBRATED_RELIEF_8_M_P90,
        "8 m gentle-land relief p90 lost detail at {relief_8_m_p90} m"
    );
}

#[test]
fn procedural_rough_terrain_retains_metre_scale_relief() {
    let mut metre_scale_residuals_m = Vec::new();

    for seed in [0, 1, 0x5eed, 0xdead_beef] {
        for x in -64..=64 {
            for y in -64..=64 {
                let position_m = Vector::new(x as f32 * 128.0 + 0.37, y as f32 * 128.0 + 0.19);
                let elevation_m = procedural_terrain_elevation_m(seed, position_m);
                if !(-8.0..=200.0).contains(&elevation_m) {
                    continue;
                }
                let left_4_m =
                    procedural_terrain_elevation_m(seed, position_m - Vector::new(4.0, 0.0));
                let right_4_m =
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(4.0, 0.0));
                if (right_4_m - left_4_m).abs() < 0.5 {
                    continue;
                }
                let left_1_m =
                    procedural_terrain_elevation_m(seed, position_m - Vector::new(0.5, 0.0));
                let right_1_m =
                    procedural_terrain_elevation_m(seed, position_m + Vector::new(0.5, 0.0));
                metre_scale_residuals_m.push((elevation_m - 0.5 * (left_1_m + right_1_m)).abs());
            }
        }
    }

    assert!(metre_scale_residuals_m.len() >= 100);
    metre_scale_residuals_m.sort_by(f32::total_cmp);
    let residual_p90_m = metre_scale_residuals_m[metre_scale_residuals_m.len() * 9 / 10];
    assert!(
        residual_p90_m >= 0.06,
        "1 m rough-terrain relief residual p90 was {residual_p90_m} m"
    );
}

#[test]
fn procedural_islands_mix_gentle_beaches_with_steep_cliff_coasts() {
    // Shares of coastal 2 km regions on the calibrated terrain; mixed and
    // beach-dominant regions stay at least half as common. Cliff-dominant
    // regions are few, so the population only has to contain them.
    const CALIBRATED_MIXED_SHARE: f32 = 0.59;
    const CALIBRATED_BEACH_SHARE: f32 = 0.22;
    let mut gentle_coast_points = 0;
    let mut cliff_coast_points = 0;
    let mut regional_coast_points = HashMap::<(u64, i32, i32), [usize; 2]>::new();

    for seed in terrain_population_seeds(16) {
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
    let cliff_dominant_regions = regional_coast_points
        .values()
        .filter(|counts| {
            let total = counts[0] + counts[1];
            counts[1] >= 8 && counts[1] * 4 >= total * 3
        })
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
    let coastal_regions = regional_coast_points
        .values()
        .filter(|counts| counts[0] + counts[1] >= 8)
        .count() as f32;
    assert!(
        cliff_dominant_regions > 0,
        "no coastal region was cliff-dominant"
    );
    for (label, regions, calibrated_share) in [
        ("mixed", mixed_regions, CALIBRATED_MIXED_SHARE),
        ("beach-dominant", beach_regions, CALIBRATED_BEACH_SHARE),
    ] {
        let share = regions as f32 / coastal_regions;
        assert!(
            share >= 0.5 * calibrated_share,
            "{label} regions were {share} of {coastal_regions} coastal regions"
        );
    }
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
fn procedural_seafloor_has_ocean_scale_basins_and_rare_abyssal_trenches() {
    let mut water_samples = 0;
    let mut basin_samples = 0;
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
                    basin_samples += usize::from(elevation_m <= -500.0);
                    trench_samples += usize::from(elevation_m <= -4_500.0);
                }
            }
        }
    }

    assert!(
        basin_samples * 2 > water_samples,
        "only {basin_samples} of {water_samples} water samples reached ocean-basin depth"
    );
    assert!(
        trench_samples > 0,
        "no sampled seafloor reached abyssal-trench depth"
    );
    assert!(
        trench_samples * 20 < water_samples,
        "{trench_samples} of {water_samples} water samples were trench depth"
    );
}
