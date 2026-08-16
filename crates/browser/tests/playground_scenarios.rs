use std::collections::HashSet;

use game::{
    AtmospherePrimary, CelestialConditions, ClimateConditions, ProceduralWeather,
    celestial_lighting, current_air_temperature_c, derive_weather,
};
use rapier2d::prelude::Vector;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    slug: String,
    label: String,
    pressure_hpa: f32,
    pressure_gradient: f32,
    pressure_gradient_direction: f32,
    synoptic_pressure_gradient: f32,
    synoptic_pressure_gradient_direction: f32,
    annual_mean_air_temperature: f32,
    current_air_temperature_anomaly: f32,
    seasonal_temperature_range: f32,
    diurnal_temperature_range: f32,
    current_upper_air_temperature: f32,
    temperature_gradient: f32,
    temperature_gradient_direction: f32,
    current_water_vapor: f32,
    aerosol_optical_depth: f32,
    geographic_latitude: f32,
    season_phase: f32,
    day_phase: f32,
    moon_phase: f32,
    tidal_range: f32,
    tidal_phase_offset: f32,
}

impl Scenario {
    fn celestial_conditions(&self) -> CelestialConditions {
        CelestialConditions {
            geographic_latitude_degrees: self.geographic_latitude,
            season_phase: self.season_phase,
            day_phase: self.day_phase,
            moon_phase: self.moon_phase,
        }
    }

    fn current_air_temperature_c(&self) -> f32 {
        current_air_temperature_c(
            ClimateConditions {
                annual_mean_air_temperature_c: self.annual_mean_air_temperature,
                seasonal_temperature_range_c: self.seasonal_temperature_range,
                diurnal_temperature_range_c: self.diurnal_temperature_range,
            },
            self.celestial_conditions(),
        ) + self.current_air_temperature_anomaly
    }

    fn weather(&self) -> ProceduralWeather {
        derive_weather(AtmospherePrimary {
            pressure_hpa: self.pressure_hpa,
            pressure_gradient_hpa_per_m: directed_vector(
                self.pressure_gradient / 1000.0,
                self.pressure_gradient_direction,
            ),
            synoptic_pressure_gradient_hpa_per_m: directed_vector(
                self.synoptic_pressure_gradient / 1000.0,
                self.synoptic_pressure_gradient_direction,
            ),
            surface_air_temperature_c: self.current_air_temperature_c(),
            upper_air_temperature_c: self.current_upper_air_temperature,
            temperature_gradient_c_per_m: directed_vector(
                self.temperature_gradient / 1000.0,
                self.temperature_gradient_direction,
            ),
            water_vapor_g_kg: self.current_water_vapor,
        })
    }
}

fn directed_vector(magnitude: f32, direction_degrees: f32) -> Vector {
    let direction = direction_degrees.to_radians();
    magnitude * Vector::new(direction.cos(), direction.sin())
}

fn scenarios() -> Vec<Scenario> {
    serde_json::from_str(include_str!("../../../playground/scenarios.json"))
        .expect("playground scenarios should be valid JSON")
}

fn scenario<'a>(scenarios: &'a [Scenario], slug: &str) -> &'a Scenario {
    scenarios
        .iter()
        .find(|scenario| scenario.slug == slug)
        .unwrap_or_else(|| panic!("missing {slug} playground scenario"))
}

#[test]
fn playground_scenarios_have_stable_unique_human_labels() {
    let scenarios = scenarios();
    assert!(scenarios.len() >= 13);
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario.slug.as_str())
            .collect::<HashSet<_>>()
            .len(),
        scenarios.len()
    );
    assert!(scenarios.iter().all(|scenario| !scenario.label.is_empty()));
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.geographic_latitude.abs() <= 90.0)
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| (0.0..=1.0).contains(&scenario.season_phase))
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| (0.0..=1.0).contains(&scenario.day_phase))
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| (0.0..=1.0).contains(&scenario.moon_phase))
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| (0.0..=1.0).contains(&scenario.aerosol_optical_depth))
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.tidal_range >= 0.0 && scenario.tidal_phase_offset.abs() <= 0.5)
    );
}

#[test]
fn day_phase_scenarios_exercise_sun_twilight_and_moonlight() {
    let scenarios = scenarios();
    let midday = celestial_lighting(scenario(&scenarios, "clear-high").celestial_conditions());
    let twilight =
        celestial_lighting(scenario(&scenarios, "winter-twilight-stratus").celestial_conditions());
    let moonlight =
        celestial_lighting(scenario(&scenarios, "clear-full-moon").celestial_conditions());

    assert!(midday.sun_intensity > 0.8);
    assert!(twilight.sun_intensity < 0.35);
    assert!(twilight.ambient_intensity > moonlight.ambient_intensity);
    assert!(moonlight.sun_intensity < 0.01);
    assert!(moonlight.moon_intensity > 0.08);
}

#[test]
fn characteristic_scenarios_produce_distinct_weather_outcomes() {
    let scenarios = scenarios();
    let clear = scenario(&scenarios, "clear-high").weather();
    assert!(clear.cloudiness < 0.05);
    assert_eq!(clear.rain_intensity, 0.0);

    let fair = scenario(&scenarios, "fair-cumulus").weather();
    assert!((0.15..0.65).contains(&fair.cloudiness));
    assert!(fair.rain_intensity < 0.05);

    let stratus = scenario(&scenarios, "marine-stratus").weather();
    assert!(stratus.cloudiness > 0.65);
    assert!(stratus.instability < 0.1);
    assert!(stratus.rain_intensity < 0.05);

    let warm_front = scenario(&scenarios, "warm-front-rain").weather();
    assert!(warm_front.upper_cloudiness > 0.55);
    assert!((0.15..0.85).contains(&warm_front.rain_intensity));

    let squall = scenario(&scenarios, "cold-front-squall").weather();
    assert!(squall.rain_intensity > 0.65);
    assert!(squall.wind_shear_m_s.length() > 5.0);

    let convection = scenario(&scenarios, "tropical-convection").weather();
    assert!(convection.rain_intensity > 0.85);
    assert!(convection.instability > 0.9);

    let post_front = scenario(&scenarios, "post-front-clearing").weather();
    assert!((0.08..0.55).contains(&post_front.cloudiness));
    assert!(post_front.relative_humidity < 0.75);
    assert!(post_front.wind_velocity_m_s.length() > 8.0);
}
