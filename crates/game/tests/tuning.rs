use pyboat_game::{dinghy_parameters, match_environment};
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
