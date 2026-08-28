use game::procedural_world_environment;
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

#[test]
fn default_world_usually_contains_active_severe_weather() {
    let geometry = WorldGeometry::default();
    let mut severe_worlds = 0;

    for case in 0..16_u64 {
        let seed = 0x5eed_cafe_dead_beef_u64.wrapping_add(case * 0x9e37_79b9);
        let tick = case * 9_876_543;
        let mut maximum_rain = 0.0_f32;
        for y in 0..32 {
            for x in 0..32 {
                let position_m = Vector::new(
                    (x as f32 / 32.0 - 0.5) * geometry.width_m(),
                    (y as f32 / 32.0 - 0.5) * geometry.height_m(),
                );
                maximum_rain = maximum_rain.max(
                    procedural_world_environment(geometry, seed, tick, position_m)
                        .weather
                        .rain_intensity,
                );
            }
        }
        severe_worlds += usize::from(maximum_rain >= 0.75);
    }

    assert!(
        severe_worlds >= 12,
        "only {severe_worlds} of 16 sampled worlds contained severe rain",
    );
}
