@fragment
fn fragment_lighting_canopy_moon_scattering(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    return diagnostic_lighting_canopy_moon_scattering(initialized.input);
}
