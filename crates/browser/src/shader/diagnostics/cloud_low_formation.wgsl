@fragment
fn fragment_cloud_low_formation(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_weather();
    return diagnostic_cloud_low_formation(periodic_input);
}
