override CLOUD_SHADOW_OUTPUT: u32 = 0u;

@fragment
fn fragment_lighting_cloud_shadow(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if CLOUD_SHADOW_OUTPUT == 3u {
        return diagnostic_lighting_ambient_sky_occlusion(periodic_input);
    }
    initialize_playground_weather();
    initialize_playground_tide();
    initialize_playground_celestial();
    if CLOUD_SHADOW_OUTPUT == 0u {
        return diagnostic_lighting_projected_low_shadow(periodic_input);
    }
    if CLOUD_SHADOW_OUTPUT == 1u {
        return diagnostic_lighting_projected_upper_shadow(periodic_input);
    }
    return diagnostic_lighting_combined_transmittance(periodic_input);
}
