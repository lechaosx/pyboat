override PRESSURE_ANOMALY_OUTPUT: u32 = 0u;

@fragment
fn fragment_pressure_anomaly(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if PRESSURE_ANOMALY_OUTPUT == 0u {
        return diagnostic_pressure_broad_anomaly(periodic_input);
    }
    if PRESSURE_ANOMALY_OUTPUT == 1u {
        return diagnostic_pressure_full_anomaly(periodic_input);
    }
    if PRESSURE_ANOMALY_OUTPUT == 2u {
        return diagnostic_pressure_synoptic_anomaly(periodic_input);
    }
    return diagnostic_pressure_local_anomaly(periodic_input);
}
