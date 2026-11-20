// Passes that never draw water read no wave tiles.
fn wave_tile_slope(layer: u32, offset_m: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(0.0);
}

fn wave_tile_octaves() -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn wave_tile_resolved_octaves(layer: u32) -> vec2<f32> {
    return vec2<f32>(0.0);
}

fn wave_tile_wind_direction() -> vec2<f32> {
    return vec2<f32>(1.0, 0.0);
}
