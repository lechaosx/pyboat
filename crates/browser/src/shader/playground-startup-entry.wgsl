@fragment
fn fragment_playground_startup(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = select(input.color, vec3<f32>(0.025, 0.09, 0.14), input.is_terrain > 0.5);
    let adaptation = EyeAdaptation(adapted_gain(1.0), vec3<f32>(1.0));
    return vec4<f32>(encode_display_color(tone_map_scene_color(
        adapted_unlit_color(linear_material_color(color), adaptation), adaptation, input.css_position_px,
    )), 1.0);
}
