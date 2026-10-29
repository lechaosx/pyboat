@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    if occluder_field_inspection_mode() == 1u {
        return vec4<f32>(encode_display_color(occluder_field_inspection_grids(input.position.xy)), 1.0);
    }
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let gain = field_adapted_gain(input.css_position_px);
    var displayed = tone_map_scene_color(color.rgb, gain, input.css_position_px);
    if occluder_field_inspection_mode() == 2u && occluder_field_lookups_outside {
        displayed = vec3<f32>(1.0, 0.0, 1.0);
    }
    return vec4<f32>(encode_display_color(displayed), color.a);
}

@fragment
fn fragment_playground_illumination(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    return vec4<f32>(adapting_light_stops(scene_illumination(
        initialized.input.world_position_m,
        initialized.atmosphere,
        initialized.surface_circle_position,
    )), 0.0, 0.0, 1.0);
}
