@fragment
fn fragment_water_precipitation_contact_normals(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_weather();
    return diagnostic_water_precipitation_contact_normals(periodic_input);
}
