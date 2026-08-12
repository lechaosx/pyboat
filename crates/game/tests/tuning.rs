use pyboat_game::{
    dinghy_parameters, match_environment, procedural_match_environment,
    procedural_terrain_elevation_m, procedural_unsheltered_wind_velocity_m_s,
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
                if has_outcrop && has_navigable_water {
                    mixed_coastal_patches += 1;
                    let patch_center_m =
                        Vector::new(patch_x as f32 * 256.0, patch_y as f32 * 256.0);
                    let has_deep_access = [
                        Vector::new(256.0, 0.0),
                        Vector::new(181.0, 181.0),
                        Vector::new(0.0, 256.0),
                        Vector::new(-181.0, 181.0),
                        Vector::new(-256.0, 0.0),
                        Vector::new(-181.0, -181.0),
                        Vector::new(0.0, -256.0),
                        Vector::new(181.0, -181.0),
                    ]
                    .into_iter()
                    .any(|offset_m| {
                        procedural_terrain_elevation_m(seed, patch_center_m + offset_m) <= -4.0
                    });
                    deep_access_patches += usize::from(has_deep_access);
                }
            }
        }
    }

    assert!(
        mixed_coastal_patches >= 64,
        "only {mixed_coastal_patches} mixed coastal patches"
    );
    assert!(maximum_slope > 0.15, "maximum slope was {maximum_slope}");
    relief_32_m.sort_by(f32::total_cmp);
    relief_128_m.sort_by(f32::total_cmp);
    let relief_32_m_p90 = relief_32_m[relief_32_m.len() * 9 / 10];
    let relief_128_m_p90 = relief_128_m[relief_128_m.len() * 9 / 10];
    let short_to_long_relief = relief_32_m_p90 / relief_128_m_p90;
    assert!(
        (0.5..=2.0).contains(&relief_32_m_p90),
        "32 m relief p90 was {relief_32_m_p90} m"
    );
    assert!(
        (0.75..=3.0).contains(&relief_128_m_p90),
        "128 m relief p90 was {relief_128_m_p90} m"
    );
    assert!(
        (0.6..=0.75).contains(&short_to_long_relief),
        "32 m relief p90 was {relief_32_m_p90} m versus {relief_128_m_p90} m at 128 m"
    );
    assert!(deep_access_patches > 0);
    assert!(
        deep_access_patches * 4 < mixed_coastal_patches,
        "{deep_access_patches} of {mixed_coastal_patches} reef patches had deep access"
    );
}
