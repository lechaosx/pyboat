use game::{
    RENDER_PRESSURE_GRID_SIDE, procedural_atmosphere_primary, procedural_world_environment,
    render_pressure_grid,
};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

#[test]
fn render_pressure_grid_nodes_match_the_authoritative_field() {
    let geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let seed = 0x5eed_cafe_dead_beef;
    let tick = 1_234_567;
    let grid = render_pressure_grid(geometry, seed, tick);

    assert_eq!(
        grid.len(),
        RENDER_PRESSURE_GRID_SIDE * RENDER_PRESSURE_GRID_SIDE
    );
    for y in 0..RENDER_PRESSURE_GRID_SIDE {
        for x in 0..RENDER_PRESSURE_GRID_SIDE {
            let position_m = Vector::new(
                x as f32 * geometry.width_m() / RENDER_PRESSURE_GRID_SIDE as f32,
                y as f32 * geometry.height_m() / RENDER_PRESSURE_GRID_SIDE as f32,
            );
            let primary = procedural_atmosphere_primary(geometry, seed, tick, position_m);
            let sample = grid[y * RENDER_PRESSURE_GRID_SIDE + x];

            assert!((sample[0] - primary.pressure_hpa).abs() < 1.0e-4);
            assert!((sample[1] - primary.pressure_gradient_hpa_per_m.x).abs() < 1.0e-7);
            assert!((sample[2] - primary.pressure_gradient_hpa_per_m.y).abs() < 1.0e-7);
            assert_eq!(sample[3], 0.0);
        }
    }
}

#[test]
fn default_world_usually_contains_active_severe_weather() {
    let geometry = WorldGeometry::default();
    let mut severe_worlds = 0;

    for case in 0..16_u64 {
        let seed = 0x5eed_cafe_dead_beef_u64.wrapping_add(case * 0x9e37_79b9);
        let tick = case * 9_876_543;
        let mut maximum_rain = 0.0_f32;
        for y in 0..16 {
            for x in 0..16 {
                let position_m = Vector::new(
                    (x as f32 / 16.0 - 0.5) * geometry.width_m(),
                    (y as f32 / 16.0 - 0.5) * geometry.height_m(),
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
