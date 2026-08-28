override WIND_SURFACE_OUTPUT: u32 = 0u;

@fragment
fn fragment_wind_surface_motion(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if WIND_SURFACE_OUTPUT == 0u {
        return diagnostic_wind_surface(periodic_input);
    }
    if WIND_SURFACE_OUTPUT == 1u {
        return diagnostic_wind_shear(periodic_input);
    }
    return diagnostic_wind_surface_gust(periodic_input);
}
