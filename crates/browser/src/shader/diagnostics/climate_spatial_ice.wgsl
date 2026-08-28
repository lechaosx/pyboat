@fragment
fn fragment_climate_spatial_ice(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_climate_spatial_ice(initialize_playground_base(input));
}
