use pyboat_game::{dinghy_parameters, match_environment, procedural_match_environment};
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
    assert_eq!(
        match_environment().true_wind_velocity_m_s,
        Vector::new(6.0, 0.0)
    );
    assert_eq!(match_environment().true_water_velocity_m_s, Vector::ZERO);
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
fn procedural_wind_has_uniform_direction_and_weibull_speed() {
    const SAMPLE_COUNT: usize = 16_384;
    let mut quadrant_counts = [0; 4];
    let mut speed_bins = [0; 31];
    let mut speed_sum = 0.0;
    let mut x_square_sum = 0.0;
    let mut y_square_sum = 0.0;
    let mut cross_sum = 0.0;
    let mut maximum_speed: f32 = 0.0;

    for seed in 0..SAMPLE_COUNT as u64 {
        let velocity = procedural_match_environment(seed, 12_345, Vector::new(12.0, -7.0))
            .true_wind_velocity_m_s;
        let quadrant = match (velocity.x >= 0.0, velocity.y >= 0.0) {
            (true, true) => 0,
            (false, true) => 1,
            (false, false) => 2,
            (true, false) => 3,
        };
        quadrant_counts[quadrant] += 1;
        let speed = velocity.length();
        speed_bins[(speed.floor() as usize).min(speed_bins.len() - 1)] += 1;
        speed_sum += speed;
        x_square_sum += velocity.x * velocity.x;
        y_square_sum += velocity.y * velocity.y;
        cross_sum += velocity.x * velocity.y;
        maximum_speed = maximum_speed.max(speed);
    }

    for count in quadrant_counts {
        assert!(count > SAMPLE_COUNT / 8);
    }
    let mean_speed = speed_sum / SAMPLE_COUNT as f32;
    assert!((7.6..=8.4).contains(&mean_speed));
    assert!((38.0..=43.0).contains(&(x_square_sum / SAMPLE_COUNT as f32)));
    assert!((38.0..=43.0).contains(&(y_square_sum / SAMPLE_COUNT as f32)));
    assert!((cross_sum / SAMPLE_COUNT as f32).abs() < 1.0);
    let mode_bin = speed_bins
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .unwrap()
        .0;
    assert!((5..=7).contains(&mode_bin));
    assert!(speed_bins[0] > 0);
    assert!(speed_bins[6] > speed_bins[12]);
    assert!(speed_bins[12] > speed_bins[18]);
    assert!(maximum_speed > 25.0);
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
