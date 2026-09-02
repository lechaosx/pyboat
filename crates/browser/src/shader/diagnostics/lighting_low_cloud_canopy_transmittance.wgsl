@fragment
fn fragment_lighting_low_cloud_canopy_transmittance(
    input: VertexOutput,
) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    return diagnostic_lighting_low_cloud_canopy_transmittance(initialized.input);
}
