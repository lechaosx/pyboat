use game::{
    OCEAN_WAVE_BANDS_PER_OCTAVE, OCEAN_WAVE_LONGEST_M, OceanWaveForcing, Swell, background_swell,
    ocean_wave_state, procedural_world_environment, procedural_world_environment_interpolated,
    swell_climate_knots, wind_sea, wind_sea_components,
};
use physics::glamx::Vec2 as Vector;
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
    let repeated_m = position_m + Vector::new(world.width_m() as f32, 0.0);

    let world_sample = procedural_world_environment(world, SEED, TICK, position_m);
    let sample = world_sample.ocean;
    assert_eq!(
        sample,
        procedural_world_environment(world, SEED, TICK, position_m).ocean
    );
    assert_eq!(
        world_sample.environment.true_water_velocity_m_s,
        (sample.current_velocity_m_s + sample.wave_orbital_velocity_m_s).as_dvec2(),
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
fn samples_do_not_depend_on_earlier_samples_on_the_same_thread() {
    let position_m = Vector::new(-14_900.0, -5_330.0);
    // Each call differs from the one before in one input: the tick fraction,
    // then the seed, then the world. Waves at the position show that each
    // sample reads the wave mode table.
    let calls = [
        (geometry(), SEED, 0.0),
        (geometry(), SEED, 0.5),
        (geometry(), SEED + 1, 0.5),
        (WorldGeometry::default(), SEED + 1, 0.5),
    ];
    for (world, seed, fraction) in calls {
        let sample =
            procedural_world_environment_interpolated(world, seed, TICK, fraction, position_m);
        let fresh = std::thread::spawn(move || {
            procedural_world_environment_interpolated(world, seed, TICK, fraction, position_m)
        })
        .join()
        .unwrap();
        assert!(sample.ocean.surface_displacement_m != 0.0);
        assert_eq!(sample, fresh);
    }
}

#[test]
fn currents_have_no_worldwide_prevailing_transport() {
    // These bounds catch inactive or strongly biased fields; current strength
    // remains uncalibrated against a hydrodynamic reference.
    let world = geometry();
    let mut sum = Vector::ZERO;
    let mut square_speed_sum = 0.0;
    let mut maximum_speed = 0.0_f32;
    let mut count = 0;

    for y_index in 0..32 {
        for x_index in 0..64 {
            let position_m = Vector::new(
                -0.5 * (world.width_m() as f32)
                    + (x_index as f32 + 0.5) * (world.width_m() as f32) / 64.0,
                -0.5 * (world.height_m() as f32)
                    + (y_index as f32 + 0.5) * (world.height_m() as f32) / 32.0,
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
    assert!(rms_speed > 0.01, "rms current {rms_speed}");
    assert!(maximum_speed > 0.1, "maximum current {maximum_speed}");
    assert!(
        mean.length() < 0.25 * rms_speed,
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
            let x_m = -0.5 * (world.width_m() as f32)
                + (x_index as f32 + 0.5) * (world.width_m() as f32) / 32.0;
            equatorial_energy +=
                procedural_world_environment(world, seed, TICK, Vector::new(x_m, 0.0))
                    .ocean
                    .current_velocity_m_s
                    .length_squared();
            subtropical_energy += procedural_world_environment(
                world,
                seed,
                TICK,
                Vector::new(x_m, 0.25 * (world.height_m() as f32)),
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
    let mut maximum_surface_wind_speed = 0.0_f32;
    let mut maximum_pressure_wind_speed = 0.0_f32;
    let mut maximum_swell_height_m = 0.0_f64;

    for y_index in 0..32 {
        for x_index in 0..64 {
            let position_m = Vector::new(
                -0.5 * (world.width_m() as f32)
                    + (x_index as f32 + 0.37) * (world.width_m() as f32) / 64.0,
                -0.5 * (world.height_m() as f32)
                    + (y_index as f32 + 0.61) * (world.height_m() as f32) / 32.0,
            );
            let sample = procedural_world_environment(world, SEED, TICK, position_m);
            let depth_m = sample.environment.water_depth_m as f32;
            let wind_speed_m_s = sample.environment.true_wind_velocity_m_s.length() as f32;
            assert!(
                sample.ocean.significant_wave_height_m.is_finite()
                    && sample.ocean.surface_displacement_m.is_finite()
                    && sample.ocean.wave_orbital_velocity_m_s.is_finite(),
                "{:?} at {position_m:?}",
                sample.ocean,
            );
            maximum_surface_wind_speed = maximum_surface_wind_speed.max(wind_speed_m_s);
            maximum_pressure_wind_speed = maximum_pressure_wind_speed.max(
                (sample.weather.wind_velocity_m_s - sample.weather.surface_gust_velocity_m_s)
                    .length(),
            );
            maximum_swell_height_m = maximum_swell_height_m
                .max(background_swell(world, SEED, TICK, 0.0, position_m).significant_height_m);
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
    // No sea outgrows the fully developed wind sea of the strongest resolved
    // wind beside the strongest swell.
    let ceiling_m = wind_sea(f64::from(maximum_pressure_wind_speed))
        .significant_height_m()
        .hypot(maximum_swell_height_m);
    assert!(
        maximum_height > 3.0 && f64::from(maximum_height) <= 1.01 * ceiling_m,
        "maximum significant wave height {maximum_height} m under resolved wind {maximum_pressure_wind_speed} m/s, gusty wind {maximum_surface_wind_speed} m/s and swell {maximum_swell_height_m} m, ceiling {ceiling_m} m",
    );
    // This is a numerical smoke ceiling, not a tuned orbital-speed target.
    assert!(maximum_orbital_speed < 12.0);
}

#[test]
fn wave_motion_has_no_persistent_drift_and_constructively_interferes() {
    let world = geometry();
    let mut position_m = Vector::ZERO;
    let mut severe_height_m = 0.0_f32;
    for y_index in 0..16 {
        for x_index in 0..32 {
            let candidate_m = Vector::new(
                -0.5 * (world.width_m() as f32)
                    + (x_index as f32 + 0.5) * (world.width_m() as f32) / 32.0,
                -0.5 * (world.height_m() as f32)
                    + (y_index as f32 + 0.5) * (world.height_m() as f32) / 16.0,
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
    // Crests above Hs are rare in a Gaussian sea, so the search holds each
    // clock over many points of a uniform storm sea, where the wave table
    // serves every sample.
    let storm = OceanWaveForcing {
        surface_wind_velocity_m_s: Vector::new(14.0, 11.0),
        swell: Swell::NONE,
        water_depth_m: 10_000.0,
    };
    for time_index in 0..32 {
        let tick = TICK + time_index * 900;
        for seed in SEED..SEED + 8 {
            for index in 0..2_048_u32 {
                let fraction = |step: f64| (f64::from(index) * step).fract() as f32;
                let candidate_m = Vector::new(
                    (fraction(0.618_033_988_75) - 0.5) * world.width_m() as f32,
                    (fraction(0.754_877_666_25) - 0.5) * world.height_m() as f32,
                );
                let ocean = ocean_wave_state(world, seed, tick, 0.0, candidate_m, storm);
                maximum_normalized_crest = maximum_normalized_crest
                    .max(ocean.surface_displacement_m.max(0.0) / ocean.significant_wave_height_m);
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

const GRAVITY_M_S2: f64 = 9.81;

/// Pierson and Moskowitz's (1964) spectrum of a sea peaking at `peak_frequency`,
/// α g² ω⁻⁵ exp(-5/4 (ω_p/ω)⁴) with α = 0.0081, per unit angular frequency.
fn pierson_moskowitz_density(frequency: f64, peak_frequency: f64) -> f64 {
    0.0081
        * GRAVITY_M_S2
        * GRAVITY_M_S2
        * frequency.powi(-5)
        * (-1.25 * (peak_frequency / frequency).powi(4)).exp()
}

/// The spectrum's variance between deep-water wavenumbers `lower` and `upper`,
/// by Simpson's rule over angular frequency.
fn spectral_variance(lower: f64, upper: f64, peak_wavenumber: f64) -> f64 {
    let peak_frequency = (GRAVITY_M_S2 * peak_wavenumber).sqrt();
    let (from, to) = ((GRAVITY_M_S2 * lower).sqrt(), (GRAVITY_M_S2 * upper).sqrt());
    let steps = 2_000;
    let step = (to - from) / steps as f64;
    (0..=steps)
        .map(|index| {
            let weight = if index == 0 || index == steps {
                1.0
            } else if index % 2 == 1 {
                4.0
            } else {
                2.0
            };
            weight * pierson_moskowitz_density(from + index as f64 * step, peak_frequency)
        })
        .sum::<f64>()
        * step
        / 3.0
}

/// The wave modes hold the wind sea between 0.36 and 3 times its peak
/// wavenumber.
const GAMEPLAY_WINDOW: [f64; 2] = [0.36, 3.0];

fn deep_forcing(wind_m_s: Vector) -> OceanWaveForcing {
    OceanWaveForcing {
        surface_wind_velocity_m_s: wind_m_s,
        swell: Swell::NONE,
        water_depth_m: 10_000.0,
    }
}

/// A fully developed sea has Pierson and Moskowitz's significant height,
/// 0.21 U²/g, and peak period, 2π U / (0.877 g), at the wind 19.5 m up, which
/// a neutral log profile with Charnock's roughness puts 5-7% above the 10 m
/// wind: within 3% of the heights and periods at 5, 10, 15 and 20 m/s.
#[test]
fn a_fully_developed_sea_has_pierson_and_moskowitzs_height_and_period() {
    for (wind_m_s, height_m, period_s) in [
        (5.0, 0.59, 3.82),
        (10.0, 2.40, 7.74),
        (15.0, 5.46, 11.68),
        (20.0, 9.80, 15.65),
    ] {
        let sea = wind_sea(wind_m_s);
        let actual_period_s =
            std::f64::consts::TAU / (GRAVITY_M_S2 * sea.peak_wavenumber_per_m).sqrt();
        assert!(
            (sea.significant_height_m() / height_m - 1.0).abs() < 0.03,
            "{wind_m_s} m/s: Hs {} m, expected {height_m} m",
            sea.significant_height_m()
        );
        assert!(
            (actual_period_s / period_s - 1.0).abs() < 0.03,
            "{wind_m_s} m/s: Tp {actual_period_s} s, expected {period_s} s"
        );
    }
}

/// Each band of wave modes holds the spectrum's variance over the part of its
/// wavenumbers inside the gameplay window, however few directions it visits.
#[test]
fn each_band_holds_its_spectral_variance() {
    let lowest_wavenumber = std::f64::consts::TAU / OCEAN_WAVE_LONGEST_M;
    for (wind_m_s, angle) in [(4.0_f32, 0.3_f32), (10.0, 2.0), (18.0, -2.7)] {
        let wind = Vector::new(angle.cos(), angle.sin()) * wind_m_s;
        let peak_wavenumber = wind_sea(f64::from(wind_m_s)).peak_wavenumber_per_m;
        let components = wind_sea_components(geometry(), SEED, wind);
        let mut bands: Vec<usize> = components.iter().map(|component| component.band).collect();
        bands.dedup();
        let total: f64 = components
            .iter()
            .map(|component| 0.5 * component.amplitude_m * component.amplitude_m)
            .sum();
        let window_variance = spectral_variance(
            GAMEPLAY_WINDOW[0] * peak_wavenumber,
            GAMEPLAY_WINDOW[1] * peak_wavenumber,
            peak_wavenumber,
        );
        assert!(
            (total / window_variance - 1.0).abs() < 1.0e-3,
            "{wind_m_s} m/s: modes hold {total} m², window {window_variance} m²"
        );
        for band in bands {
            let edges = [band, band + 1].map(|edge| {
                lowest_wavenumber * 2.0_f64.powf(edge as f64 / OCEAN_WAVE_BANDS_PER_OCTAVE as f64)
            });
            let expected = spectral_variance(
                edges[0].max(GAMEPLAY_WINDOW[0] * peak_wavenumber),
                edges[1].min(GAMEPLAY_WINDOW[1] * peak_wavenumber),
                peak_wavenumber,
            );
            let actual: f64 = components
                .iter()
                .filter(|component| component.band == band)
                .map(|component| 0.5 * component.amplitude_m * component.amplitude_m)
                .sum();
            assert!(
                (actual - expected).abs() <= 1.0e-3 * expected + 1.0e-12,
                "{wind_m_s} m/s, band {band}: {actual} m², spectrum {expected} m²"
            );
        }
    }
}

/// Under a uniform deep wind the sea surface varies about still water with
/// the variance the spectrum holds in the gameplay window, on every seed.
#[test]
fn a_uniform_deep_wind_raises_the_spectrums_variance_on_every_seed() {
    let wind_m_s = 10.0_f32;
    let peak_wavenumber = wind_sea(f64::from(wind_m_s)).peak_wavenumber_per_m;
    let expected = spectral_variance(
        GAMEPLAY_WINDOW[0] * peak_wavenumber,
        GAMEPLAY_WINDOW[1] * peak_wavenumber,
        peak_wavenumber,
    );
    for seed in SEED..SEED + 6 {
        let mut squares = 0.0;
        let mut count = 0;
        for index in 0..32_768_u32 {
            // Additive recurrences spread the samples over the world and the
            // clock without the lattice a grid would alias with the modes.
            let fraction = |step: f64| (f64::from(index) * step).fract() as f32;
            let position_m = Vector::new(
                (fraction(0.618_033_988_75) - 0.5) * geometry().width_m() as f32,
                (fraction(0.754_877_666_25) - 0.5) * geometry().height_m() as f32,
            );
            let state = ocean_wave_state(
                geometry(),
                seed,
                TICK + u64::from(index / 256) * 7_919,
                0.0,
                position_m,
                deep_forcing(Vector::new(wind_m_s, 0.0)),
            );
            squares += f64::from(state.surface_displacement_m).powi(2);
            count += 1;
        }
        let variance = squares / f64::from(count);
        assert!(
            (variance / expected - 1.0).abs() < 0.03,
            "seed {seed}: variance {variance} m², spectrum's window {expected} m²"
        );
    }
}

/// A point's surface under a uniform deep wind peaks in frequency within a
/// quarter octave of Pierson and Moskowitz's peak, on every seed: the
/// periodogram of its height, as a density in quarter-octave bins.
#[test]
fn a_uniform_deep_wind_sea_peaks_at_pierson_and_moskowitzs_period() {
    let wind_m_s = 12.0_f32;
    let peak_frequency =
        (GRAVITY_M_S2 * wind_sea(f64::from(wind_m_s)).peak_wavenumber_per_m).sqrt();
    let samples = 2_048_u64;
    let step_ticks = 15;
    let step_s = step_ticks as f64 / simulation::TICKS_PER_SECOND as f64;
    let resolution = std::f64::consts::TAU / (samples as f64 * step_s);
    let frequencies: Vec<f64> = (1..)
        .map(|index| index as f64 * resolution)
        .take_while(|frequency| *frequency < 4.0 * peak_frequency)
        .collect();
    for seed in SEED..SEED + 4 {
        let mut power = vec![0.0; frequencies.len()];
        for point in 0..4 {
            let position_m = Vector::new(point as f32 * 9_001.0 - 20_000.0, point as f32 * 3_001.0);
            let series: Vec<f64> = (0..samples)
                .map(|sample| {
                    f64::from(
                        ocean_wave_state(
                            geometry(),
                            seed,
                            TICK + sample * step_ticks,
                            0.0,
                            position_m,
                            deep_forcing(Vector::new(0.0, wind_m_s)),
                        )
                        .surface_displacement_m,
                    )
                })
                .collect();
            for (frequency, power) in frequencies.iter().zip(&mut power) {
                let (mut real, mut imaginary) = (0.0, 0.0);
                for (index, height) in series.iter().enumerate() {
                    let phase = frequency * step_s * index as f64;
                    real += height * phase.cos();
                    imaginary += height * phase.sin();
                }
                *power += real * real + imaginary * imaginary;
            }
        }
        let (peak, _) = (-24..=24)
            .map(|index| {
                let centre = peak_frequency * 2.0_f64.powf(index as f64 / 16.0);
                let bin = [centre * 2.0_f64.powf(-0.125), centre * 2.0_f64.powf(0.125)];
                let binned: f64 = frequencies
                    .iter()
                    .zip(&power)
                    .filter(|(frequency, _)| (bin[0]..bin[1]).contains(*frequency))
                    .map(|(_, power)| power)
                    .sum();
                (centre, binned / (bin[1] - bin[0]))
            })
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap();
        let octaves = (peak / peak_frequency).log2();
        assert!(
            octaves.abs() <= 0.25,
            "seed {seed}: spectral peak {octaves} octaves from Pierson and Moskowitz's"
        );
    }
}

/// The largest change in a point's surface height and slope between
/// neighbouring winds of a sweep that steps by `step` in speed, from 3 to
/// 20 m/s, or in direction, over a radian.
fn largest_step(step: f32, speed: bool) -> (f32, f32) {
    let position_m = Vector::new(-14_900.0, -5_330.0);
    let (mut height_step, mut slope_step) = (0.0_f32, 0.0_f32);
    let mut previous: Option<(f32, Vector)> = None;
    let count = if speed { 17.0 / step } else { 1.0 / step } as u32;
    for index in 0..=count {
        let wind = if speed {
            Vector::new(3.0 + index as f32 * step, 1.0)
        } else {
            Vector::new((index as f32 * step).cos(), (index as f32 * step).sin()) * 9.0
        };
        let state = ocean_wave_state(geometry(), SEED, TICK, 0.0, position_m, deep_forcing(wind));
        if let Some((height, slope)) = previous {
            height_step = height_step.max((state.surface_displacement_m - height).abs());
            slope_step = slope_step.max((state.surface_slope - slope).length());
        }
        previous = Some((state.surface_displacement_m, state.surface_slope));
    }
    (height_step, slope_step)
}

/// The surface changes continuously as the wind's speed moves the spectrum's
/// peak and as its direction turns: refining the sweep's step eightfold at
/// least halves the largest change between neighbouring winds, where a band
/// or direction entering or leaving with weight would keep it. A band enters
/// the window's short end with a variance growing linearly, so its amplitude,
/// and the largest change, falls only as the square root of the step there.
#[test]
fn the_surface_changes_continuously_with_the_wind() {
    for (step, speed) in [(2.0e-3, true), (4.0e-4, false)] {
        let coarse = largest_step(step, speed);
        let fine = largest_step(0.125 * step, speed);
        let kind = if speed { "speed" } else { "direction" };
        assert!(
            fine.0 <= 0.5 * coarse.0,
            "{kind}: height steps {} m, then {} m at an eighth of the step",
            coarse.0,
            fine.0
        );
        assert!(
            fine.1 <= 0.5 * coarse.1,
            "{kind}: slope steps {}, then {} at an eighth of the step",
            coarse.1,
            fine.1
        );
    }
}

/// Waves travel downwind: under a uniform deep wind, the mean of the
/// surface's height times its orbital velocity, the direction the waves
/// carry their energy, points within 5° of the wind, whichever way it blows;
/// and a tick later the surface has fallen where it sloped up downwind, as
/// crests moving downwind leave it, so the mean of the height's change times
/// the downwind slope is negative.
#[test]
fn a_uniform_wind_raises_waves_that_travel_downwind() {
    for angle in [0.4_f32, 2.5, -1.9] {
        let wind = Vector::new(angle.cos(), angle.sin());
        let mut flux = [0.0_f64; 2];
        let mut travel = 0.0_f64;
        for clock in 0..4_u64 {
            for index in 0..2_048_u32 {
                let fraction = |step: f64| (f64::from(index) * step).fract() as f32;
                let position_m = Vector::new(
                    (fraction(0.618_033_988_75) - 0.5) * geometry().width_m() as f32,
                    (fraction(0.754_877_666_25) - 0.5) * geometry().height_m() as f32,
                );
                let state_at = |tick| {
                    ocean_wave_state(
                        geometry(),
                        SEED,
                        tick,
                        0.0,
                        position_m,
                        deep_forcing(11.0 * wind),
                    )
                };
                let state = state_at(TICK + clock * 997);
                let later = state_at(TICK + clock * 997 + 1);
                let height_m = f64::from(state.surface_displacement_m);
                flux[0] += height_m * f64::from(state.wave_orbital_velocity_m_s.x);
                flux[1] += height_m * f64::from(state.wave_orbital_velocity_m_s.y);
                travel += f64::from(later.surface_displacement_m - state.surface_displacement_m)
                    * f64::from(state.surface_slope.dot(wind));
            }
        }
        let direction = Vector::new(flux[0] as f32, flux[1] as f32);
        let degrees = direction.angle_to(wind).to_degrees().abs();
        assert!(
            degrees < 5.0,
            "wind at {angle} rad: waves carry energy {degrees}° from it"
        );
        assert!(
            travel < 0.0,
            "wind at {angle} rad: the surface changes as waves travelling upwind change it, {travel}"
        );
    }
}

/// A swell of significant height `height_m` peaking at `period_s` and
/// travelling toward `direction_rad`.
fn swell(height_m: f64, period_s: f64, direction_rad: f64) -> Swell {
    let peak_frequency = std::f64::consts::TAU / period_s;
    Swell {
        significant_height_m: height_m,
        peak_wavenumber_per_m: peak_frequency * peak_frequency / GRAVITY_M_S2,
        direction_rad,
    }
}

/// The wave modes hold the swell between 0.7 and 2.1 times its peak
/// wavenumber.
const SWELL_WINDOW: [f64; 2] = [0.7, 2.1];

/// Ochi and Hubble's (1976, Proc. 15th ICCE, ch. 18) swell spectrum of
/// significant height `height_m` and modal frequency `peak_frequency`, of
/// their most probable shape λ = 3, per unit angular frequency:
/// Hs²/4 ((4λ+1)/4 ω_m⁴)^λ / Γ(λ) ω^-(4λ+1) exp(-(4λ+1)/4 (ω_m/ω)⁴).
fn ochi_hubble_swell_density(frequency: f64, height_m: f64, peak_frequency: f64) -> f64 {
    let shape = 3.0;
    let scale = (4.0 * shape + 1.0) / 4.0;
    0.25 * height_m * height_m * (scale * peak_frequency.powi(4)).powf(shape) / 2.0
        * frequency.powf(-(4.0 * shape + 1.0))
        * (-scale * (peak_frequency / frequency).powi(4)).exp()
}

/// The swell's variance between deep-water wavenumbers `lower` and `upper`,
/// by Simpson's rule over angular frequency.
fn swell_variance(lower: f64, upper: f64, swell: Swell) -> f64 {
    let peak_frequency = (GRAVITY_M_S2 * swell.peak_wavenumber_per_m).sqrt();
    let (from, to) = ((GRAVITY_M_S2 * lower).sqrt(), (GRAVITY_M_S2 * upper).sqrt());
    let steps = 2_000;
    let step = (to - from) / steps as f64;
    (0..=steps)
        .map(|index| {
            let weight = if index == 0 || index == steps {
                1.0
            } else if index % 2 == 1 {
                4.0
            } else {
                2.0
            };
            weight
                * ochi_hubble_swell_density(
                    from + index as f64 * step,
                    swell.significant_height_m,
                    peak_frequency,
                )
        })
        .sum::<f64>()
        * step
        / 3.0
}

/// The mean square of the surface's height and the mean of its height times
/// its orbital velocity, over points spread across the world and the clock,
/// under `forcing`, on `seed`.
fn surface_statistics(seed: u64, forcing: OceanWaveForcing) -> (f64, Vector) {
    let (mut squares, mut flux) = (0.0, [0.0_f64; 2]);
    let count = 32_768_u32;
    for index in 0..count {
        let fraction = |step: f64| (f64::from(index) * step).fract() as f32;
        let position_m = Vector::new(
            (fraction(0.618_033_988_75) - 0.5) * geometry().width_m() as f32,
            (fraction(0.754_877_666_25) - 0.5) * geometry().height_m() as f32,
        );
        let state = ocean_wave_state(
            geometry(),
            seed,
            TICK + u64::from(index / 256) * 7_919,
            0.0,
            position_m,
            forcing,
        );
        let height_m = f64::from(state.surface_displacement_m);
        squares += height_m * height_m;
        flux[0] += height_m * f64::from(state.wave_orbital_velocity_m_s.x);
        flux[1] += height_m * f64::from(state.wave_orbital_velocity_m_s.y);
    }
    (
        squares / f64::from(count),
        Vector::new(flux[0] as f32, flux[1] as f32),
    )
}

/// Where the wind is still, the sea is the swell's: its significant height,
/// the variance its spectrum holds in the wave modes' window on every seed,
/// and its energy carried within 5° of its direction.
#[test]
fn a_still_wind_leaves_the_swell_alone() {
    let swell = swell(2.0, 12.0, 0.7);
    let forcing = OceanWaveForcing {
        surface_wind_velocity_m_s: Vector::ZERO,
        swell,
        water_depth_m: 10_000.0,
    };
    let state = ocean_wave_state(
        geometry(),
        SEED,
        TICK,
        0.0,
        Vector::new(1_234.0, -5_678.0),
        forcing,
    );
    assert!(
        (f64::from(state.significant_wave_height_m) / swell.significant_height_m - 1.0).abs()
            < 1.0e-4,
        "Hs {} m under a still wind, swell {} m",
        state.significant_wave_height_m,
        swell.significant_height_m
    );
    let expected = swell_variance(
        SWELL_WINDOW[0] * swell.peak_wavenumber_per_m,
        SWELL_WINDOW[1] * swell.peak_wavenumber_per_m,
        swell,
    );
    for seed in SEED..SEED + 6 {
        let (variance, flux) = surface_statistics(seed, forcing);
        assert!(
            (variance / expected - 1.0).abs() < 0.03,
            "seed {seed}: variance {variance} m², swell's window {expected} m²"
        );
        let direction = Vector::new(
            swell.direction_rad.cos() as f32,
            swell.direction_rad.sin() as f32,
        );
        let degrees = flux.angle_to(direction).to_degrees().abs();
        assert!(
            degrees < 5.0,
            "seed {seed}: energy {degrees}° from the swell"
        );
    }
}

/// Each wave component takes the larger of the wind sea's and the swell's
/// spectra, so neither is counted twice: a weak swell along a strong wind,
/// which lies under the wind sea in every component the two share, leaves
/// the wind sea's surface; and a long swell against a moderate wind, which
/// shares none, adds its variance and its height in quadrature.
#[test]
fn each_component_takes_the_larger_of_wind_sea_and_swell() {
    let wind_m_s = Vector::new(15.0, 0.0);
    let under = OceanWaveForcing {
        surface_wind_velocity_m_s: wind_m_s,
        swell: swell(0.5, 11.7, 0.0),
        water_depth_m: 10_000.0,
    };
    let wind_height_m = wind_sea(15.0).significant_height_m();
    for index in 0..512_u32 {
        let position_m = Vector::new(index as f32 * 97.3 - 20_000.0, index as f32 * 31.1);
        let tick = TICK + u64::from(index) * 13;
        let with = ocean_wave_state(geometry(), SEED, tick, 0.0, position_m, under);
        let without = ocean_wave_state(
            geometry(),
            SEED,
            tick,
            0.0,
            position_m,
            deep_forcing(wind_m_s),
        );
        assert!(
            (with.surface_displacement_m - without.surface_displacement_m).abs() < 1.0e-5
                && (with.surface_slope - without.surface_slope).length() < 1.0e-6,
            "at {position_m:?}: {with:?} with the swell, {without:?} without"
        );
        assert!(
            (f64::from(with.significant_wave_height_m) / wind_height_m - 1.0).abs() < 2.0e-3,
            "Hs {} m beside the wind sea's {wind_height_m} m",
            with.significant_wave_height_m
        );
    }

    let wind_m_s = 8.0_f32;
    let against = OceanWaveForcing {
        surface_wind_velocity_m_s: Vector::new(wind_m_s, 0.0),
        swell: swell(2.0, 13.0, std::f64::consts::PI),
        water_depth_m: 10_000.0,
    };
    let peak_wavenumber = wind_sea(f64::from(wind_m_s)).peak_wavenumber_per_m;
    let expected = spectral_variance(
        GAMEPLAY_WINDOW[0] * peak_wavenumber,
        GAMEPLAY_WINDOW[1] * peak_wavenumber,
        peak_wavenumber,
    ) + swell_variance(
        SWELL_WINDOW[0] * against.swell.peak_wavenumber_per_m,
        SWELL_WINDOW[1] * against.swell.peak_wavenumber_per_m,
        against.swell,
    );
    let (variance, _) = surface_statistics(SEED, against);
    assert!(
        (variance / expected - 1.0).abs() < 0.03,
        "variance {variance} m², wind sea's and swell's windows {expected} m²"
    );
    let state = ocean_wave_state(geometry(), SEED, TICK, 0.0, Vector::ZERO, against);
    let expected_height_m = wind_sea(f64::from(wind_m_s))
        .significant_height_m()
        .hypot(against.swell.significant_height_m);
    assert!(
        (f64::from(state.significant_wave_height_m) / expected_height_m - 1.0).abs() < 1.0e-4,
        "Hs {} m, expected {expected_height_m} m",
        state.significant_wave_height_m
    );
}

/// The breaker limit, 0.78 of the depth, caps the sea's height counting the
/// components wind sea and swell share once: a swell along the wind at its
/// peak period over water deep enough for that height, though not for the
/// two seas' heights in quadrature, keeps its deep-water surface.
#[test]
fn the_breaker_limit_caps_the_height_shared_components_count_once_in() {
    let wind_m_s = 10.0_f32;
    let sea = wind_sea(f64::from(wind_m_s));
    let peak_period_s = std::f64::consts::TAU / (GRAVITY_M_S2 * sea.peak_wavenumber_per_m).sqrt();
    let aligned = |water_depth_m: f32| OceanWaveForcing {
        surface_wind_velocity_m_s: Vector::new(wind_m_s, 0.0),
        swell: swell(sea.significant_height_m(), peak_period_s, 0.0),
        water_depth_m,
    };
    let deep = ocean_wave_state(geometry(), SEED, TICK, 0.0, Vector::ZERO, aligned(10_000.0));
    let deep_height_m = f64::from(deep.significant_wave_height_m);
    let quadrature_height_m = sea.significant_height_m() * std::f64::consts::SQRT_2;
    assert!(
        quadrature_height_m > 1.1 * deep_height_m,
        "shared components bring Hs {deep_height_m} m only a little under {quadrature_height_m} m"
    );
    let depth_m = 1.05 * deep_height_m / 0.78;
    let shallow = ocean_wave_state(
        geometry(),
        SEED,
        TICK,
        0.0,
        Vector::ZERO,
        aligned(depth_m as f32),
    );
    assert!(
        (f64::from(shallow.significant_wave_height_m) / deep_height_m - 1.0).abs() < 1.0e-4
            && (shallow.surface_displacement_m - deep.surface_displacement_m).abs() < 1.0e-5,
        "over {depth_m} m: Hs {} m, height {} m; deep: Hs {deep_height_m} m, height {} m",
        shallow.significant_wave_height_m,
        shallow.surface_displacement_m,
        deep.surface_displacement_m
    );
}

/// The world position at `latitude_degrees` on the default world.
fn at_latitude(latitude_degrees: f32) -> Vector {
    Vector::new(
        3_000.0,
        latitude_degrees / 180.0 * WorldGeometry::default().height_m() as f32,
    )
}

/// The background swell holds the swell climate of NOAA's operational
/// WAVEWATCH III over 2018-2024, pooled by absolute latitude: over a year,
/// the mean of its height's square is the climate's at 10°, 30° and 50°, and
/// its mean peak period the climate's, in either hemisphere; its local
/// winter is rougher than its summer; and it repeats for a seed and differs
/// between seeds.
#[test]
fn the_background_swell_holds_the_swell_climate() {
    let world = WorldGeometry::default();
    let ticks_per_year = 365 * 86_400 * simulation::TICKS_PER_SECOND;
    let knots = swell_climate_knots();
    for latitude in [10.0_f32, 30.0, 50.0] {
        let climate = knots[2 * (latitude / 10.0) as usize];
        let (height_m, period_s) = (f64::from(climate[0]), f64::from(climate[2]));
        for hemisphere in [1.0, -1.0] {
            let position_m = at_latitude(hemisphere * latitude);
            let (mut squares, mut periods, mut count) = (0.0, 0.0, 0.0);
            for seed in 0..16 {
                for step in 0..1_000_u64 {
                    let swell = background_swell(
                        world,
                        seed,
                        step * ticks_per_year / 1_000,
                        0.0,
                        position_m,
                    );
                    squares += swell.significant_height_m.powi(2);
                    periods +=
                        std::f64::consts::TAU / (GRAVITY_M_S2 * swell.peak_wavenumber_per_m).sqrt();
                    count += 1.0;
                }
            }
            let rms_m = (squares / count).sqrt();
            let mean_period_s = periods / count;
            assert!(
                (rms_m / height_m - 1.0).abs() < 0.05,
                "latitude {}: rms Hs {rms_m} m, climate {height_m} m",
                hemisphere * latitude
            );
            assert!(
                (mean_period_s / period_s - 1.0).abs() < 0.03,
                "latitude {}: mean Tp {mean_period_s} s, climate {period_s} s",
                hemisphere * latitude
            );
        }
    }
    // Winters at 50°: northern ones around January, southern around July.
    let quarter_mean_square = |latitude: f32, start_day: u64| {
        let mut squares = 0.0;
        for seed in 0..16 {
            for day in 0..90 {
                let tick = (start_day + day) * 86_400 * simulation::TICKS_PER_SECOND;
                squares += background_swell(world, seed, tick, 0.0, at_latitude(latitude))
                    .significant_height_m
                    .powi(2);
            }
        }
        squares
    };
    for (latitude, winter, summer) in [(50.0, 0, 182), (-50.0, 182, 0)] {
        assert!(
            quarter_mean_square(latitude, winter) > 1.3 * quarter_mean_square(latitude, summer),
            "latitude {latitude}: winter no rougher than summer"
        );
    }
    let sample = |seed| background_swell(world, seed, TICK, 0.25, at_latitude(40.0));
    assert_eq!(sample(SEED), sample(SEED));
    assert_ne!(sample(SEED), sample(SEED + 1));
}

/// The largest change in a still sea's surface between neighbouring points
/// of a meridional sweep across `centre_y_m`, `count` steps of `step_m`
/// either side.
fn largest_swell_step(centre_y_m: f32, step_m: f32, count: i32) -> (f32, f32) {
    let world = WorldGeometry::default();
    let (mut height_step, mut slope_step) = (0.0_f32, 0.0_f32);
    let mut previous: Option<(f32, Vector)> = None;
    for index in -count..=count {
        let position_m = Vector::new(-7_100.0, centre_y_m + index as f32 * step_m);
        let forcing = OceanWaveForcing {
            surface_wind_velocity_m_s: Vector::ZERO,
            swell: background_swell(world, SEED, TICK, 0.0, position_m),
            water_depth_m: 10_000.0,
        };
        let state = ocean_wave_state(world, SEED, TICK, 0.0, position_m, forcing);
        if let Some((height, slope)) = previous {
            height_step = height_step.max((state.surface_displacement_m - height).abs());
            slope_step = slope_step.max((state.surface_slope - slope).length());
        }
        previous = Some((state.surface_displacement_m, state.surface_slope));
    }
    (height_step, slope_step)
}

/// The largest root-mean-square change of a still wind's sea of a 2 m swell
/// over 64 points between neighbouring peak periods `step_s` apart, over
/// `count` steps either side of `centre_s`.
fn largest_swell_period_step(centre_s: f64, step_s: f64, count: i32) -> f64 {
    let world = geometry();
    let surface = |period_s: f64| -> Vec<f32> {
        (0..64_u32)
            .map(|index| {
                let position_m = Vector::new(
                    index as f32 * 1_213.7 - 39_000.0,
                    index as f32 * 571.3 - 18_000.0,
                );
                let forcing = OceanWaveForcing {
                    surface_wind_velocity_m_s: Vector::ZERO,
                    swell: swell(2.0, period_s, 0.3),
                    water_depth_m: 10_000.0,
                };
                ocean_wave_state(world, SEED, TICK, 0.0, position_m, forcing).surface_displacement_m
            })
            .collect()
    };
    let mut largest = 0.0_f64;
    let mut previous = surface(centre_s - f64::from(count) * step_s);
    for index in -count + 1..=count {
        let next = surface(centre_s + f64::from(index) * step_s);
        let mean_square = previous
            .iter()
            .zip(&next)
            .map(|(before, after)| f64::from(after - before).powi(2))
            .sum::<f64>()
            / previous.len() as f64;
        largest = largest.max(mean_square.sqrt());
        previous = next;
    }
    largest
}

/// The swell's surface changes continuously with its period where its
/// window's lower edge crosses a band edge, near 10.71 s: refining a sweep's
/// step 64-fold cuts the largest change between neighbouring periods at
/// least fourfold, where a band entering or leaving with variance would keep
/// it, about 6 cm for 1.3% of a 2 m swell's variance. A band entering empty
/// grows as the square root of its width, so the change shrinks only as the
/// step's square root.
#[test]
fn the_swell_is_continuous_in_its_period_across_band_edges() {
    let coarse = largest_swell_period_step(10.71, 0.005, 8);
    let fine = largest_swell_period_step(10.71, 0.000_078_125, 512);
    assert!(coarse > 0.0, "no swell");
    assert!(
        fine <= 0.25 * coarse,
        "rms steps {coarse} m, then {fine} m at a 64th of the step"
    );
}

/// The swell changes continuously through the equator, where the
/// hemispheres' seasons and directions meet, and through a pole, across
/// which the world mirrors: refining a sweep's step eightfold cuts the
/// largest change between neighbouring points at least fourfold, where a
/// step in the surface would keep it.
#[test]
fn the_background_swell_is_continuous_through_the_equator_and_the_poles() {
    let pole_y_m = 0.5 * WorldGeometry::default().height_m() as f32;
    for centre_y_m in [0.0, pole_y_m] {
        let coarse = largest_swell_step(centre_y_m, 0.4, 16);
        let fine = largest_swell_step(centre_y_m, 0.05, 128);
        assert!(coarse.0 > 0.0, "no swell at y {centre_y_m} m");
        assert!(
            fine.0 <= 0.25 * coarse.0 && fine.1 <= 0.25 * coarse.1,
            "at y {centre_y_m} m: steps {coarse:?}, then {fine:?} at an eighth of the step"
        );
    }
}
