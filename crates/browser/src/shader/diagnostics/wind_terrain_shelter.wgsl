@fragment
fn fragment_wind_terrain_shelter(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_wind_terrain_shelter(initialize_playground_base(input));
}
