@fragment
fn fragment_lighting_aerosol_optical_depth(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_lighting_aerosol_optical_depth(initialize_playground_base(input));
}
