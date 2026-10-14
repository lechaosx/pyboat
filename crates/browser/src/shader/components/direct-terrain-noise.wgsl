fn terrain_broad_noise(
    field: u32,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
) -> TerrainNoiseSample {
    return direct_terrain_broad_noise(field, position_m, circle_position);
}
