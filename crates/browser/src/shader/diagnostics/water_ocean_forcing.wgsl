override OCEAN_FORCING_OUTPUT: u32 = 0u;

@fragment
fn fragment_water_ocean_forcing(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if OCEAN_FORCING_OUTPUT == 0u {
        return diagnostic_water_ocean_current(periodic_input);
    }
    if OCEAN_FORCING_OUTPUT == 2u {
        return diagnostic_water_wave_peak_period(periodic_input);
    }
    if OCEAN_FORCING_OUTPUT == 3u {
        return diagnostic_water_wave_direction(periodic_input);
    }
    return diagnostic_water_significant_wave_height(periodic_input);
}
