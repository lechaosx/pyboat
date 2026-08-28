@fragment
fn fragment_water_visual_normals(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_weather();
    return diagnostic_water_visual_normals(periodic_input);
}
