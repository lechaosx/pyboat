@fragment
fn fragment_water_physical_wave_normals(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_water_physical_wave_normals(initialize_playground_base(input));
}
