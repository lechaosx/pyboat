struct PlaygroundSceneFragment {
    @location(0) color: vec4<f32>,
    @location(1) surface_height_m: f32,
};

struct PlaygroundVisionFragment {
    @location(0) color: vec4<f32>,
    @location(1) surface_height_m: f32,
    @location(2) luminance: vec4<f32>,
};

@fragment
fn fragment_playground_vision(input: VertexOutput) -> PlaygroundVisionFragment {
    let initialized = initialize_playground_scene(input);
    let color = final_scene_color(initialized.input, initialized.atmosphere, initialized.surface_circle_position);
    let adaptation = field_adaptation(input.css_position_px);
    return PlaygroundVisionFragment(
        vec4<f32>(tone_map_scene_color(color.rgb, adaptation, input.css_position_px), color.a),
        drawn_surface_height_m,
        vision_auxiliary(color.rgb, color.a),
    );
}

@fragment
fn fragment_playground_scene(input: VertexOutput) -> PlaygroundSceneFragment {
    let initialized = initialize_playground_scene(input);
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let adaptation = field_adaptation(input.css_position_px);
    return PlaygroundSceneFragment(vec4<f32>(encode_display_color(tone_map_scene_color(color.rgb, adaptation, input.css_position_px)), color.a), drawn_surface_height_m);
}

@fragment
fn fragment_playground_illumination(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    return vec4<f32>(scene_illumination(
        initialized.input.world_position_m,
        initialized.atmosphere,
        initialized.surface_circle_position,
    ), 1.0);
}
