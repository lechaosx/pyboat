use game::procedural_world_environment;
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

const SEED: u64 = 0x5eed_cafe;
const TICK: u64 = 987_654;

fn geometry() -> WorldGeometry {
    WorldGeometry::new(80_000.0, 40_000.0).unwrap()
}

#[test]
fn ocean_state_is_deterministic_and_joins_across_longitude() {
    let world = geometry();
    let position_m = Vector::new(-39_999.75, 6_250.0);
    let repeated_m = position_m + Vector::new(world.width_m(), 0.0);

    let world_sample = procedural_world_environment(world, SEED, TICK, position_m);
    let sample = world_sample.ocean;
    assert_eq!(
        sample,
        procedural_world_environment(world, SEED, TICK, position_m).ocean
    );
    assert_eq!(
        world_sample.environment.true_water_velocity_m_s,
        sample.current_velocity_m_s + sample.wave_orbital_velocity_m_s,
    );
    assert_ne!(
        sample,
        procedural_world_environment(world, SEED, TICK + 1, position_m).ocean
    );
    let repeated = procedural_world_environment(world, SEED, TICK, repeated_m).ocean;

    assert!((sample.current_velocity_m_s - repeated.current_velocity_m_s).length() < 1.0e-3);
    assert!(
        (sample.wave_orbital_velocity_m_s - repeated.wave_orbital_velocity_m_s).length() < 1.0e-3
    );
    assert!((sample.significant_wave_height_m - repeated.significant_wave_height_m).abs() < 1.0e-4);
    assert!((sample.surface_displacement_m - repeated.surface_displacement_m).abs() < 1.0e-4);
    assert!((sample.surface_slope - repeated.surface_slope).length() < 1.0e-4);

    let late = procedural_world_environment(world, SEED, u64::MAX, position_m).ocean;
    assert!(late.current_velocity_m_s.is_finite());
    assert!(late.wave_orbital_velocity_m_s.is_finite());
    assert!(late.significant_wave_height_m.is_finite());
    assert!(late.surface_displacement_m.is_finite());
    assert!(late.surface_slope.is_finite());
}

#[test]
fn currents_have_no_worldwide_prevailing_transport() {
    let world = geometry();
    let mut sum = Vector::ZERO;
    let mut square_speed_sum = 0.0;
    let mut maximum_speed = 0.0_f32;
    let mut count = 0;

    for y_index in 0..32 {
        for x_index in 0..64 {
            let position_m = Vector::new(
                -0.5 * world.width_m() + (x_index as f32 + 0.5) * world.width_m() / 64.0,
                -0.5 * world.height_m() + (y_index as f32 + 0.5) * world.height_m() / 32.0,
            );
            let current = procedural_world_environment(world, SEED, TICK, position_m)
                .ocean
                .current_velocity_m_s;
            sum += current;
            square_speed_sum += current.length_squared();
            maximum_speed = maximum_speed.max(current.length());
            count += 1;
        }
    }

    let mean = sum / count as f32;
    let rms_speed = (square_speed_sum / count as f32).sqrt();
    assert!(rms_speed > 0.05, "rms current {rms_speed}");
    assert!(maximum_speed > 0.5, "maximum current {maximum_speed}");
    assert!(
        mean.length() < 0.1 * rms_speed,
        "world mean {mean:?}, rms {rms_speed}"
    );
}

#[test]
fn coriolis_suppresses_rotating_currents_at_the_equator() {
    let world = geometry();
    let mut equatorial_energy = 0.0;
    let mut subtropical_energy = 0.0;
    let mut samples = 0;

    for seed in 0..32 {
        for x_index in 0..32 {
            let x_m = -0.5 * world.width_m() + (x_index as f32 + 0.5) * world.width_m() / 32.0;
            equatorial_energy +=
                procedural_world_environment(world, seed, TICK, Vector::new(x_m, 0.0))
                    .ocean
                    .current_velocity_m_s
                    .length_squared();
            subtropical_energy += procedural_world_environment(
                world,
                seed,
                TICK,
                Vector::new(x_m, 0.25 * world.height_m()),
            )
            .ocean
            .current_velocity_m_s
            .length_squared();
            samples += 1;
        }
    }

    let equatorial_rms = (equatorial_energy / samples as f32).sqrt();
    let subtropical_rms = (subtropical_energy / samples as f32).sqrt();
    assert!(
        equatorial_rms < subtropical_rms,
        "equatorial RMS {equatorial_rms}, subtropical RMS {subtropical_rms}",
    );
}

#[test]
fn wind_sea_gains_energy_in_strong_wind_and_is_depth_limited() {
    let world = geometry();
    let mut calm_height_sum = 0.0;
    let mut calm_count = 0;
    let mut windy_height_sum = 0.0;
    let mut windy_count = 0;
    let mut shallow_count = 0;
    let mut maximum_height = 0.0_f32;
    let mut maximum_orbital_speed = 0.0_f32;

    for y_index in 0..32 {
        for x_index in 0..64 {
            let position_m = Vector::new(
                -0.5 * world.width_m() + (x_index as f32 + 0.37) * world.width_m() / 64.0,
                -0.5 * world.height_m() + (y_index as f32 + 0.61) * world.height_m() / 32.0,
            );
            let sample = procedural_world_environment(world, SEED, TICK, position_m);
            let depth_m = sample.environment.water_depth_m;
            let wind_speed_m_s = sample.environment.true_wind_velocity_m_s.length();
            maximum_height = maximum_height.max(sample.ocean.significant_wave_height_m);
            maximum_orbital_speed =
                maximum_orbital_speed.max(sample.ocean.wave_orbital_velocity_m_s.length());
            if depth_m > 50.0 && wind_speed_m_s < 4.0 {
                calm_height_sum += sample.ocean.significant_wave_height_m;
                calm_count += 1;
            }
            if depth_m > 50.0 && wind_speed_m_s > 12.0 {
                windy_height_sum += sample.ocean.significant_wave_height_m;
                windy_count += 1;
            }
            if (0.25..4.0).contains(&depth_m) {
                shallow_count += 1;
                assert!(sample.ocean.significant_wave_height_m <= 0.78 * depth_m + 1.0e-4);
            }
            if depth_m == 0.0 {
                assert_eq!(sample.ocean.significant_wave_height_m, 0.0);
                assert_eq!(sample.ocean.current_velocity_m_s, Vector::ZERO);
                assert_eq!(sample.ocean.wave_orbital_velocity_m_s, Vector::ZERO);
            }
        }
    }

    assert!(calm_count > 4, "only {calm_count} calm deep-water samples");
    assert!(
        windy_count > 4,
        "only {windy_count} windy deep-water samples"
    );
    assert!(shallow_count > 0);
    let calm_mean = calm_height_sum / calm_count as f32;
    let windy_mean = windy_height_sum / windy_count as f32;
    assert!(
        windy_mean > calm_mean + 0.5,
        "calm Hs {calm_mean}, windy Hs {windy_mean}"
    );
    assert!((3.0..15.0).contains(&maximum_height));
    assert!(maximum_orbital_speed < 8.0);
}

#[test]
fn wave_motion_has_no_persistent_drift_and_constructively_interferes() {
    let world = geometry();
    let mut position_m = Vector::ZERO;
    let mut severe_height_m = 0.0_f32;
    for y_index in 0..16 {
        for x_index in 0..32 {
            let candidate_m = Vector::new(
                -0.5 * world.width_m() + (x_index as f32 + 0.5) * world.width_m() / 32.0,
                -0.5 * world.height_m() + (y_index as f32 + 0.5) * world.height_m() / 16.0,
            );
            let height_m = procedural_world_environment(world, SEED, TICK, candidate_m)
                .ocean
                .significant_wave_height_m;
            if height_m > severe_height_m {
                severe_height_m = height_m;
                position_m = candidate_m;
            }
        }
    }
    assert!(
        severe_height_m > 3.0,
        "maximum sampled Hs {severe_height_m}"
    );
    let mut orbital_sum = Vector::ZERO;
    let mut displacement_sum = 0.0;
    let mut square_displacement_sum = 0.0;
    let mut maximum_normalized_crest = 0.0_f32;
    let mut maximum_rogue_crest_excess_m = 0.0_f32;
    let mut count = 0;

    for time_index in 0..4096 {
        let tick = TICK + time_index * 15;
        let ocean = procedural_world_environment(world, SEED, tick, position_m).ocean;
        if ocean.significant_wave_height_m <= 0.5 {
            continue;
        }
        orbital_sum += ocean.wave_orbital_velocity_m_s;
        displacement_sum += ocean.surface_displacement_m;
        square_displacement_sum += ocean.surface_displacement_m.powi(2);
        maximum_normalized_crest = maximum_normalized_crest
            .max(ocean.surface_displacement_m.max(0.0) / ocean.significant_wave_height_m);
        maximum_rogue_crest_excess_m = maximum_rogue_crest_excess_m.max(ocean.rogue_crest_excess_m);
        count += 1;
    }
    for seed in SEED..SEED + 8 {
        for y_index in 0..16 {
            for x_index in 0..32 {
                let candidate_m = Vector::new(
                    -0.5 * world.width_m() + (x_index as f32 + 0.5) * world.width_m() / 32.0,
                    -0.5 * world.height_m() + (y_index as f32 + 0.5) * world.height_m() / 16.0,
                );
                let ocean = procedural_world_environment(world, seed, TICK, candidate_m).ocean;
                maximum_rogue_crest_excess_m =
                    maximum_rogue_crest_excess_m.max(ocean.rogue_crest_excess_m);
            }
        }
    }

    assert!(count > 1_000, "only {count} open-water samples");
    let mean_orbital = orbital_sum / count as f32;
    let mean_displacement = displacement_sum / count as f32;
    let rms_displacement = (square_displacement_sum / count as f32).sqrt();
    assert!(mean_orbital.length() < 0.1);
    assert!(mean_displacement.abs() < 0.1 * rms_displacement);
    assert!(
        maximum_normalized_crest > 0.79,
        "maximum crest was only {maximum_normalized_crest} Hs"
    );
    assert!(
        maximum_rogue_crest_excess_m > 0.0,
        "no rogue crest emerged; maximum was {maximum_normalized_crest} Hs"
    );
}
