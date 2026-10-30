@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let adaptation = field_adaptation(input.css_position_px);
    return vec4<f32>(encode_display_color(tone_map_scene_color(color.rgb, adaptation, input.css_position_px)), color.a);
}

@fragment
fn fragment_playground_illumination(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    return adapting_light(scene_illumination(
        initialized.input.world_position_m,
        initialized.atmosphere,
        initialized.surface_circle_position,
    ));
}
