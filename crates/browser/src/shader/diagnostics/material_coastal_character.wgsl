@fragment
fn fragment_material_coastal_character(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_material_coastal_character(initialize_playground_base(input));
}
