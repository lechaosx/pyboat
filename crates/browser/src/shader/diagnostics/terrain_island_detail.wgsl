@fragment
fn fragment_terrain_island_detail(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_terrain_island_detail(initialize_playground_base(input));
}
