@fragment
fn fragment_water_tide_height(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_tide();
    return diagnostic_water_tide_height(periodic_input);
}
