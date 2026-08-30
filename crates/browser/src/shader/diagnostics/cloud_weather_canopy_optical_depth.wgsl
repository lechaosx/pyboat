@fragment
fn fragment_cloud_weather_canopy_optical_depth(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    return diagnostic_cloud_weather_canopy_optical_depth(periodic_input);
}
