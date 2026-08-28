@fragment
fn fragment_climate_annual_temperature(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_climate_annual_temperature(initialize_playground_base(input));
}
