@fragment
fn fragment_cloud_weather_canopy_optical_depth(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_cloud_weather_canopy_optical_depth(initialize_playground_base(input));
}
