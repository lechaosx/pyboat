override CLOUD_LOW_OUTPUT: u32 = 0u;

@fragment
fn fragment_cloud_low_sample(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if CLOUD_LOW_OUTPUT == 0u {
        return diagnostic_cloud_low_coverage_threshold(periodic_input);
    }
    if CLOUD_LOW_OUTPUT == 1u {
        return diagnostic_cloud_low_amount(periodic_input);
    }
    if CLOUD_LOW_OUTPUT == 2u {
        return diagnostic_cloud_low_optical_depth(periodic_input);
    }
    if CLOUD_LOW_OUTPUT == 3u {
        return diagnostic_cloud_low_normals(periodic_input);
    }
    return diagnostic_cloud_low_height(periodic_input);
}
