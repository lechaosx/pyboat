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
fn procedural_wind_is_seeded_bounded_and_continuous_across_space_and_time() {
    let seed = 0x5eed_u64;
    let position_m = Vector::new(12.0, -7.0);
    let wind = procedural_match_environment(seed, 600, position_m);

    assert_eq!(wind, procedural_match_environment(seed, 600, position_m));
    assert!((4.0..=8.0).contains(&wind.true_wind_velocity_m_s.length()));
    assert_eq!(wind.true_water_velocity_m_s, Vector::ZERO);

    let nearby = procedural_match_environment(seed, 601, position_m + Vector::new(0.1, 0.1));
    assert!((nearby.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() < 0.1);

    let distant = procedural_match_environment(seed, 600, position_m + Vector::new(64.0, 0.0));
    let later = procedural_match_environment(seed, 4200, position_m);
    let other_seed = procedural_match_environment(seed + 1, 600, position_m);
    assert!((distant.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
    assert!((later.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
    assert!((other_seed.true_wind_velocity_m_s - wind.true_wind_velocity_m_s).length() > 0.01);
}
