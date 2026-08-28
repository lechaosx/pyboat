override CLIMATE_SURFACE_OUTPUT: u32 = 0u;

@fragment
fn fragment_climate_surface(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_weather();
    if CLIMATE_SURFACE_OUTPUT == 0u {
        return diagnostic_climate_solid_ice(periodic_input);
    }
    if CLIMATE_SURFACE_OUTPUT == 1u {
        return diagnostic_climate_autumn_vegetation(periodic_input);
    }
    if CLIMATE_SURFACE_OUTPUT == 2u {
        return diagnostic_climate_spatial_snow(periodic_input);
    }
    return diagnostic_climate_composition(periodic_input);
}
