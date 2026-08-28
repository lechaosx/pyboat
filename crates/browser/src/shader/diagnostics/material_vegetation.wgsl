@fragment
fn fragment_material_vegetation(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_material_vegetation(initialize_playground_base(input));
}
