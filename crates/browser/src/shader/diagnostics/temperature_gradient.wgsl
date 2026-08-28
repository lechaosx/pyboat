@fragment
fn fragment_temperature_gradient(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_temperature_gradient(initialize_playground_base(input));
}
