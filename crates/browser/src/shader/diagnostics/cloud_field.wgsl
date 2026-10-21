override CLOUD_FIELD_OUTPUT: u32 = 0u;

@fragment
fn fragment_cloud_field(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if CLOUD_FIELD_OUTPUT == 2u {
        return diagnostic_cumulus_along_two_lights(periodic_input);
    }
    return diagnostic_cloud_field_overhead_shadow(periodic_input, CLOUD_FIELD_OUTPUT);
}
