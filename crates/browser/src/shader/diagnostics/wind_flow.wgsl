override WIND_FLOW_OUTPUT: u32 = 0u;

@fragment
fn fragment_wind_flow(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if WIND_FLOW_OUTPUT == 0u {
        return diagnostic_wind_upper(periodic_input);
    }
    if WIND_FLOW_OUTPUT == 1u {
        return diagnostic_wind_resolved_surface(periodic_input);
    }
    return diagnostic_wind_broad_synoptic(periodic_input);
}
