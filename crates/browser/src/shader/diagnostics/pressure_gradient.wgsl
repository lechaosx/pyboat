@fragment
fn fragment_pressure_gradient(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_pressure_gradient(initialize_playground_base(input));
}
