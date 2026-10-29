@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let gain = adapted_gain(clear_sky_intensity(celestial_receiver_light_at_height(0.0)));
    return vec4<f32>(encode_display_color(tone_map_scene_color(color.rgb, gain, input.css_position_px)), color.a);
}
