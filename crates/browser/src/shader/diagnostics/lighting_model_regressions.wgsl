override LIGHTING_MODEL_REGRESSION_OUTPUT: u32 = 0u;
override LIGHTING_MODEL_SURFACE_HEIGHT_M: f32 = 0.0;

@fragment
fn fragment_lighting_model_regressions(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 0u {
        return diagnostic_low_cloud_sun_lighting(LIGHTING_MODEL_SURFACE_HEIGHT_M);
    }
    return diagnostic_landform_sun_grade_moon_coupling();
}
