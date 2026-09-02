override LIGHTING_SOLAR_OUTPUT: u32 = 0u;
override LIGHTING_RECEIVER_HEIGHT_M: f32 = 0.0;

@fragment
fn fragment_lighting_solar(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_celestial();
    if LIGHTING_SOLAR_OUTPUT == 4u {
        initialize_playground_tide();
        return diagnostic_lighting_surface_sun_amount(periodic_input);
    }
    if LIGHTING_SOLAR_OUTPUT == 5u {
        return diagnostic_lighting_receiver_moon(LIGHTING_RECEIVER_HEIGHT_M);
    }
    if LIGHTING_SOLAR_OUTPUT == 0u {
        return diagnostic_lighting_solar_elevation(periodic_input);
    }
    if LIGHTING_SOLAR_OUTPUT == 1u {
        return diagnostic_lighting_solar_eclipse_coverage(periodic_input);
    }
    if LIGHTING_SOLAR_OUTPUT == 2u {
        return diagnostic_lighting_low_cloud_sun_visibility(periodic_input);
    }
    return diagnostic_lighting_canopy_sun_visibility(periodic_input);
}
