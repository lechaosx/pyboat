@fragment
fn fragment_surface_air_temperature(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_surface_air_temperature(initialize_playground_base(input));
}
