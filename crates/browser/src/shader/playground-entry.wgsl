@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    return final_scene_color(initialize_playground_scene(input));
}
