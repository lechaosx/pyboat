@fragment
fn fragment_terrain_coastal_reef(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_terrain_coastal_reef(initialize_playground_base(input));
}
